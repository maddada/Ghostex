use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::Value;

use crate::platform::shell::command_shell;

use super::repository::*;
use super::slug::*;
use super::types::*;

const PORTLESS_LISTENER_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES: usize = 64 * 1024;

pub(crate) fn list_portless_listener_candidate_sessions(
    db: &Connection,
) -> Result<Vec<PortlessListenerCandidateSession>> {
    let mut statement = db
        .prepare(
            r#"
            SELECT
              sessions.projectId,
              sessions.sessionId,
              sessions.zmxName,
              sessions.lifecycleState,
              sessions.launchSettingsJson,
              sessions.runtimeSettingsJson,
              projects.worktreeJson
            FROM sessions
            INNER JOIN projects ON projects.projectId = sessions.projectId
            ORDER BY sessions.projectId ASC, sessions.sessionId ASC
            "#,
        )
        .with_context(|| "prepare Portless listener candidate sessions")?;
    let rows = statement
        .query_map([], |row| {
            Ok(RawPortlessListenerCandidateSession {
                project_id: row.get("projectId")?,
                session_id: row.get("sessionId")?,
                zmx_name: row.get("zmxName")?,
                lifecycle_state: row.get("lifecycleState")?,
                launch_settings_json: row.get("launchSettingsJson")?,
                runtime_settings_json: row.get("runtimeSettingsJson")?,
                worktree_json: row.get("worktreeJson")?,
            })
        })
        .with_context(|| "query Portless listener candidate sessions")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .with_context(|| "read Portless listener candidate sessions")?;

    rows.into_iter()
        .filter_map(
            |row| match PortlessListenerCandidateSession::from_raw(row) {
                Ok(Some(session)) => Some(Ok(session)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect()
}

pub(crate) fn compute_portless_owned_listeners_for_sessions(
    sessions: &[PortlessListenerCandidateSession],
    zmx_list_output: &str,
    ps_output: &str,
    listener_output: &str,
) -> Vec<PortlessOwnedListener> {
    if sessions.is_empty() {
        return Vec::new();
    }

    let session_names = sessions
        .iter()
        .map(|session| session.zmx_name.clone())
        .collect::<Vec<_>>();
    let root_pids_by_zmx_name = parse_portless_zmx_root_pids(zmx_list_output, &session_names);
    let process_rows = parse_portless_process_rows(ps_output);
    let live_pids = process_rows
        .iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    let children_by_parent_pid = group_portless_processes_by_parent_pid(&process_rows);
    let mut owner_by_pid = HashMap::<i64, PortlessProcessOwner>::new();

    for (session_index, session) in sessions.iter().enumerate() {
        let Some(root_pid) = root_pids_by_zmx_name.get(&session.zmx_name).copied() else {
            continue;
        };
        if !live_pids.contains(&root_pid) {
            continue;
        }
        for (pid, depth) in collect_portless_process_tree_pids(root_pid, &children_by_parent_pid) {
            owner_by_pid
                .entry(pid)
                .and_modify(|owner| {
                    if depth < owner.depth {
                        owner.session_index = session_index;
                        owner.depth = depth;
                    }
                })
                .or_insert(PortlessProcessOwner {
                    depth,
                    session_index,
                });
        }
    }

    let mut seen = HashSet::<(u32, u16)>::new();
    let mut owned = Vec::new();
    for listener in parse_portless_tcp_listener_rows(listener_output) {
        if !seen.insert((listener.pid, listener.port)) {
            continue;
        }
        let Some(owner) = owner_by_pid.get(&(listener.pid as i64)) else {
            continue;
        };
        let session = &sessions[owner.session_index];
        owned.push(PortlessOwnedListener {
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            zmx_name: session.zmx_name.clone(),
            worktree_parent_project_id: session.worktree_parent_project_id.clone(),
            port: listener.port,
            pid: listener.pid,
        });
    }

    owned.sort_by(|left, right| {
        left.project_id
            .cmp(&right.project_id)
            .then_with(|| left.session_id.cmp(&right.session_id))
            .then_with(|| left.port.cmp(&right.port))
            .then_with(|| left.pid.cmp(&right.pid))
    });
    owned
}

pub(crate) fn build_portless_listener_snapshot_command(zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
printf '%s\n' '__GHOSTEX_ZMX_LIST__'
"$zmx_bin" list
printf '%s\n' '__GHOSTEX_PS__'
ps -axo pid=,ppid=,command=
printf '%s\n' '__GHOSTEX_LSOF_LISTEN__'
if [ -x /usr/sbin/lsof ]; then
  /usr/sbin/lsof -nP -iTCP -sTCP:LISTEN -F pcn 2>/dev/null || true
elif [ -x /usr/bin/lsof ]; then
  /usr/bin/lsof -nP -iTCP -sTCP:LISTEN -F pcn 2>/dev/null || true
elif [ -x /usr/sbin/ss ]; then
  /usr/sbin/ss -H -ltnp 2>/dev/null || true
elif [ -x /usr/bin/ss ]; then
  /usr/bin/ss -H -ltnp 2>/dev/null || true
fi
"#,
        portless_shell_quote(zmx_executable_path)
    )
    .trim()
    .to_string()
}

pub(crate) fn parse_portless_listener_snapshot_sections(
    stdout: &str,
) -> PortlessListenerSnapshotSections {
    let zmx_marker = "__GHOSTEX_ZMX_LIST__";
    let ps_marker = "__GHOSTEX_PS__";
    let listener_marker = "__GHOSTEX_LSOF_LISTEN__";
    let Some(zmx_index) = stdout.find(zmx_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    let Some(ps_index) = stdout.find(ps_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    let Some(listener_index) = stdout.find(listener_marker) else {
        return PortlessListenerSnapshotSections::default();
    };
    if ps_index <= zmx_index || listener_index <= ps_index {
        return PortlessListenerSnapshotSections::default();
    }
    PortlessListenerSnapshotSections {
        listener_output: stdout[listener_index + listener_marker.len()..]
            .trim()
            .to_string(),
        ps_output: stdout[ps_index + ps_marker.len()..listener_index]
            .trim()
            .to_string(),
        zmx_list_output: stdout[zmx_index + zmx_marker.len()..ps_index]
            .trim()
            .to_string(),
    }
}

pub(crate) fn run_portless_listener_snapshot_command(
    script: &str,
) -> Result<PortlessSnapshotCommandOutput> {
    let shell = command_shell();
    let mut child = Command::new(&shell.executable)
        .args(shell.script_args(script))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "start Portless listener snapshot")?;
    let stdout = child
        .stdout
        .take()
        .with_context(|| "open Portless listener snapshot stdout")?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| "open Portless listener snapshot stderr")?;
    let terminate = Arc::new(AtomicBool::new(false));
    let stdout_terminate = terminate.clone();
    let stderr_terminate = terminate.clone();
    let stdout_thread = thread::spawn(move || {
        read_portless_capped_output(
            stdout,
            PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES,
            stdout_terminate,
        )
    });
    let stderr_thread = thread::spawn(move || {
        read_portless_capped_output(
            stderr,
            PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES,
            stderr_terminate,
        )
    });

    let started = Instant::now();
    let mut timed_out = false;
    let mut terminate_started: Option<Instant> = None;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| "wait for Portless listener snapshot")?
        {
            let (stdout, stdout_truncated) = stdout_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stdout reader failed."))?;
            let (_, stderr_truncated) = stderr_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stderr reader failed."))?;
            let mut exit_code = status.code().unwrap_or(1);
            if timed_out {
                exit_code = 124;
            } else if stdout_truncated || stderr_truncated {
                exit_code = 125;
            }
            return Ok(PortlessSnapshotCommandOutput {
                exit_code,
                stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
                stdout_truncated,
            });
        }

        let should_terminate = terminate.load(Ordering::SeqCst)
            || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
        if should_terminate {
            timed_out = timed_out || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
            if terminate_started.is_none() {
                terminate_started = Some(Instant::now());
                signal_portless_snapshot_child(&mut child, false);
            } else if terminate_started
                .map(|instant| instant.elapsed() >= Duration::from_millis(1_000))
                .unwrap_or(false)
            {
                signal_portless_snapshot_child(&mut child, true);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_portless_capped_output<R: Read>(
    mut reader: R,
    limit: usize,
    terminate: Arc<AtomicBool>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if read > remaining {
            if remaining > 0 {
                output.extend_from_slice(&buffer[..remaining]);
            }
            truncated = true;
            terminate.store(true, Ordering::SeqCst);
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    (output, truncated)
}

fn signal_portless_snapshot_child(child: &mut std::process::Child, force: bool) {
    #[cfg(unix)]
    unsafe {
        libc::kill(
            child.id() as i32,
            if force { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn parse_portless_zmx_root_pids(
    zmx_list_output: &str,
    session_names: &[String],
) -> HashMap<String, i64> {
    let wanted = session_names.iter().cloned().collect::<HashSet<_>>();
    let mut root_pids = HashMap::new();
    for line in zmx_list_output.lines() {
        let Some(name) = parse_portless_zmx_list_name(line) else {
            continue;
        };
        if !wanted.contains(&name) {
            continue;
        }
        if let Some(pid) = parse_portless_zmx_list_pid(line) {
            root_pids.insert(name, pid);
        }
    }
    root_pids
}

fn parse_portless_zmx_list_name(line: &str) -> Option<String> {
    for part in line.split_whitespace() {
        let Some(value) = part
            .strip_prefix("name=")
            .or_else(|| part.strip_prefix("→name="))
        else {
            continue;
        };
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_portless_zmx_list_pid(line: &str) -> Option<i64> {
    for part in line.split_whitespace() {
        let Some(value) = part.strip_prefix("pid=") else {
            continue;
        };
        let pid = value.parse::<i64>().ok()?;
        if pid > 0 {
            return Some(pid);
        }
    }
    None
}

fn parse_portless_process_rows(ps_output: &str) -> Vec<PortlessProcessRow> {
    let mut rows = Vec::new();
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        rows.push(PortlessProcessRow { pid, ppid });
    }
    rows
}

fn group_portless_processes_by_parent_pid(
    processes: &[PortlessProcessRow],
) -> HashMap<i64, Vec<i64>> {
    let mut grouped = HashMap::<i64, Vec<i64>>::new();
    for process_row in processes {
        grouped
            .entry(process_row.ppid)
            .or_default()
            .push(process_row.pid);
    }
    grouped
}

fn collect_portless_process_tree_pids(
    root_pid: i64,
    children_by_parent_pid: &HashMap<i64, Vec<i64>>,
) -> Vec<(i64, usize)> {
    let mut collected = Vec::new();
    let mut queue = std::collections::VecDeque::from([(root_pid, 0_usize)]);
    let mut seen = HashSet::new();
    while let Some((pid, depth)) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        collected.push((pid, depth));
        if let Some(children) = children_by_parent_pid.get(&pid) {
            for child in children {
                queue.push_back((*child, depth + 1));
            }
        }
    }
    collected
}

pub(crate) fn parse_portless_tcp_listener_rows(
    listener_output: &str,
) -> Vec<PortlessTcpListenerRow> {
    let mut listeners = Vec::new();
    let mut current_pid: Option<u32> = None;
    for raw_line in listener_output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(field) = line.chars().next() else {
            continue;
        };
        if !matches!(field, 'p' | 'n') {
            if let Some(row) = parse_portless_ss_listener_row(line) {
                listeners.push(row);
            }
            continue;
        }
        let value = &line[field.len_utf8()..];
        match field {
            'p' => current_pid = parse_positive_u32(value),
            'n' => {
                let Some(pid) = current_pid else {
                    continue;
                };
                let Some(port) = parse_portless_tcp_listener_port(value) else {
                    continue;
                };
                listeners.push(PortlessTcpListenerRow { pid, port });
            }
            _ => {}
        }
    }
    listeners
}

fn parse_portless_ss_listener_row(line: &str) -> Option<PortlessTcpListenerRow> {
    let pid = parse_portless_ss_listener_pid(line)?;
    let before_process = line.split(" users:").next().unwrap_or(line);
    let port = before_process
        .split_whitespace()
        .filter_map(parse_portless_tcp_listener_port)
        .next()?;
    Some(PortlessTcpListenerRow { pid, port })
}

fn parse_portless_ss_listener_pid(line: &str) -> Option<u32> {
    let value = line.split("pid=").nth(1)?;
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    parse_positive_u32(&digits)
}

fn parse_portless_tcp_listener_port(endpoint: &str) -> Option<u16> {
    let endpoint = endpoint
        .trim()
        .strip_prefix("TCP ")
        .unwrap_or_else(|| endpoint.trim())
        .split(" (")
        .next()
        .unwrap_or("")
        .trim();
    if endpoint.is_empty() {
        return None;
    }

    let raw_port = if endpoint.starts_with('[') {
        let host_end = endpoint.find("]:")?;
        &endpoint[host_end + 2..]
    } else {
        let separator_index = endpoint.rfind(':')?;
        &endpoint[separator_index + 1..]
    };
    let port = raw_port.trim().parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

fn parse_positive_u32(value: &str) -> Option<u32> {
    let parsed = value.trim().parse::<u64>().ok()?;
    if parsed == 0 || parsed > u32::MAX as u64 {
        return None;
    }
    Some(parsed as u32)
}

fn parse_worktree_parent_project_id_for_listener(
    project_id: &str,
    worktree_json: &str,
) -> Result<Option<String>> {
    let value: Value = serde_json::from_str(worktree_json).with_context(|| {
        format!("parse Portless listener worktree metadata for project {project_id}")
    })?;
    let Some(worktree) = value.as_object() else {
        return Ok(None);
    };
    let Some(parent_project_id) = trimmed_json_string(worktree.get("parentProjectId")) else {
        return Ok(None);
    };
    validate_stable_key("parentProjectId", &parent_project_id)?;
    Ok(Some(parent_project_id))
}

fn read_settings_text(settings_json: &str, key: &str) -> Option<String> {
    serde_json::from_str::<Value>(settings_json)
        .ok()
        .and_then(|value| {
            value
                .as_object()
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn is_portless_listener_eligible_session(
    lifecycle_state: &str,
    launch_settings_json: &str,
    runtime_settings_json: &str,
) -> bool {
    lifecycle_state == "running"
        && read_settings_text(runtime_settings_json, "sessionPersistenceProvider").as_deref()
            == Some("zmx")
        && read_settings_text(launch_settings_json, "surface").as_deref() != Some("commands")
        && read_settings_text(runtime_settings_json, "surface").as_deref() != Some("commands")
}

fn portless_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Default)]
pub(crate) struct PortlessListenerSnapshotSections {
    pub(crate) listener_output: String,
    pub(crate) ps_output: String,
    pub(crate) zmx_list_output: String,
}

pub(crate) struct PortlessSnapshotCommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stdout_truncated: bool,
}

struct RawPortlessListenerCandidateSession {
    project_id: String,
    session_id: String,
    zmx_name: String,
    lifecycle_state: String,
    launch_settings_json: String,
    runtime_settings_json: String,
    worktree_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PortlessListenerCandidateSession {
    project_id: String,
    session_id: String,
    zmx_name: String,
    worktree_parent_project_id: Option<String>,
}

impl PortlessListenerCandidateSession {
    fn from_raw(row: RawPortlessListenerCandidateSession) -> Result<Option<Self>> {
        if !is_portless_listener_eligible_session(
            &row.lifecycle_state,
            &row.launch_settings_json,
            &row.runtime_settings_json,
        ) {
            return Ok(None);
        }
        let zmx_name = row.zmx_name.trim();
        if zmx_name.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            project_id: row.project_id.clone(),
            session_id: row.session_id,
            zmx_name: zmx_name.to_string(),
            worktree_parent_project_id: parse_worktree_parent_project_id_for_listener(
                &row.project_id,
                &row.worktree_json,
            )?,
        }))
    }
}

#[derive(Clone, Debug)]
struct PortlessProcessRow {
    pid: i64,
    ppid: i64,
}

#[derive(Clone, Debug)]
struct PortlessProcessOwner {
    depth: usize,
    session_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortlessTcpListenerRow {
    pub(crate) pid: u32,
    pub(crate) port: u16,
}
