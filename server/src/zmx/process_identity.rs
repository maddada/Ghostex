use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ProcessRow {
    command: String,
    pid: i64,
    ppid: i64,
    terminal_name: Option<String>,
}

impl ProcessRow {
    /// A row read from the kernel rather than parsed from `ps` (process_table_macos.rs); the
    /// terminal name goes through the same normalization a `ps` tty column does.
    #[cfg(target_os = "macos")]
    pub(crate) fn new(pid: i64, ppid: i64, terminal_name: Option<String>, command: String) -> Self {
        Self {
            command,
            pid,
            ppid,
            terminal_name: terminal_name
                .as_deref()
                .and_then(normalize_process_terminal_name),
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    pub(crate) fn pid_for_test(&self) -> i64 {
        self.pid
    }

    #[cfg(all(test, target_os = "macos"))]
    pub(crate) fn ppid_for_test(&self) -> i64 {
        self.ppid
    }

    #[cfg(all(test, target_os = "macos"))]
    pub(crate) fn command_for_test(&self) -> &str {
        &self.command
    }
}

#[derive(Clone, Debug)]
struct ProcessIdentityCandidate {
    confidence: i64,
    depth: i64,
    /// Depth of the first agent process of this candidate's launcher chain.
    chain_depth: i64,
    identity: ZmxProcessIdentity,
}

/// The agent process that owns a path down the tree: the last process of its launcher chain.
#[derive(Clone, Debug)]
struct ProcessTreeAgentOwner {
    agent_id: String,
    pid: i64,
    chain_depth: i64,
}

#[derive(Clone, Debug)]
struct ProcessIdentityObservation {
    confidence: i64,
    identity: ZmxProcessIdentity,
}

const GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT: usize = 12;
const GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE: i64 = 300;
const GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE: i64 = 275;
const GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS: i64 = 25;

pub(crate) fn parse_process_rows(ps_output: &str) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let remaining = parts.collect::<Vec<_>>();
        let has_terminal_column = remaining
            .first()
            .copied()
            .is_some_and(looks_like_process_terminal_name);
        let terminal_name = remaining
            .first()
            .copied()
            .filter(|_| has_terminal_column)
            .and_then(normalize_process_terminal_name);
        let command_start = usize::from(has_terminal_column);
        rows.push(ProcessRow {
            command: remaining[command_start..].join(" "),
            pid,
            ppid,
            terminal_name,
        });
    }
    rows
}

/// PID of the daemon that owns `session_name`, read from the process table.
///
/// CDXC:ZmxWireGeneration 2026-08-23: `zmx run <name> -d` keeps its argv across
/// daemonization, so the owning process identifies itself without answering any
/// IPC. `zmx list` reports the same PID but pays a per-session probe that a
/// pre-wire-break daemon cannot complete, which is exactly the case this exists
/// for.
#[cfg(not(windows))]
pub(crate) fn find_zmx_daemon_process_id(ps_output: &str, session_name: &str) -> Option<i64> {
    zmx_daemon_processes(ps_output)
        .into_iter()
        .find_map(|(pid, name)| (name == session_name).then_some(pid))
}

/// Every `zmx run <name>` daemon in the process table as (pid, session name), whichever zmx binary started it.
#[cfg(not(windows))]
pub(crate) fn zmx_daemon_processes(ps_output: &str) -> Vec<(i64, String)> {
    parse_process_rows(ps_output)
        .into_iter()
        .filter_map(|row| {
            let mut arguments = row.command.split_whitespace();
            let executable = arguments.next()?;
            if Path::new(executable).file_stem()?.to_str()? != "zmx" || arguments.next()? != "run" {
                return None;
            }
            let name = arguments.next()?.to_string();
            (row.pid > 0).then_some((row.pid, name))
        })
        .collect()
}

/// CDXC:Zmx 2026-11-03 WHY: macOS `ps` writes `??` for a process without a terminal and Linux procps writes `?`. Only `??` was known, so on Linux every daemon row kept its `?` as the first word of its command, no `zmx run <name>` daemon was ever found, and the chat's repaint step reported "the session's zmx daemon is not running" for live sessions (GitHub #400).
fn looks_like_process_terminal_name(value: &str) -> bool {
    value == "??"
        || value == "?"
        || value == "-"
        || value.starts_with("tty")
        || value.starts_with("pts/")
        || value.starts_with("/dev/")
}

fn normalize_process_terminal_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || matches!(trimmed, "??" | "?" | "-") {
        return None;
    }
    let name = Path::new(trimmed).file_name()?.to_str()?;
    if name.is_empty()
        || name.len() > 128
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return None;
    }
    Some(name.to_string())
}

pub(crate) fn group_processes_by_parent_pid(
    processes: &[ProcessRow],
) -> HashMap<i64, Vec<ProcessRow>> {
    let mut grouped = HashMap::<i64, Vec<ProcessRow>>::new();
    for process_row in processes {
        grouped
            .entry(process_row.ppid)
            .or_default()
            .push(process_row.clone());
    }
    grouped
}

pub(crate) fn resolve_process_tree_agent_identity(
    root_pid: i64,
    processes: &[ProcessRow],
    children_by_parent_pid: &HashMap<i64, Vec<ProcessRow>>,
) -> Option<ZmxProcessIdentity> {
    let rows_by_pid = processes
        .iter()
        .map(|process_row| (process_row.pid, process_row))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::<ProcessIdentityCandidate>::new();
    /*
    CDXC:SessionIdentity 2026-09-02 WHY:
    The first agent process found on a path down from the zmx root owns the
    terminal; every process below it was spawned BY that agent. A different
    agent CLI down there is one of its tool invocations (`grok models`,
    `cursor-agent --list-models` run from Claude's Bash tool, observed live
    2026-09-02), not the session's agent, and the deepest-wins ordering below
    let it replace the real identity: the row flipped to that agent, the
    transcript was then decoded with the wrong reader, and the chat view sat on
    "Loading conversation…" for good.
    CDXC:SessionIdentity 2026-10-05 WHY:
    The same holds for the SAME agent: a `claude -p` or `cswap run … claude` an agent starts from its Bash tool, or another session's tree hung below this one by a reused Windows pid, is a deeper `claude --resume <other id>` that deepest-wins took as the session's agent, so the session got another conversation's id, title and final message (thread G0hhy took coordinator G4snt's on 2026-10-04). A same-agent process only continues the owner's launcher chain when it is the owner's DIRECT child (`node codex` → native `codex`, which deepest-wins exists for); anything further down is the agent's own tool run and never a candidate. Separate chains rank by how close their first agent process is to the session's shell, so the session's own agent wins over anything grafted deeper.
    SEE-ALSO: .dependencies/wmx/src/process_snapshot.rs (drops reused-pid parent links on Windows).
    */
    let mut queue = VecDeque::from([(0_i64, root_pid, None::<ProcessTreeAgentOwner>)]);
    let mut seen = HashSet::<i64>::new();
    while let Some((depth, pid, owner)) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        // Claude's background daemon, a headless Claude and everything they run belong to no
        // pane's agent chain.
        if rows_by_pid.get(&pid).is_some_and(|row| {
            is_claude_daemon_process(&row.command) || is_headless_claude_process(&row.command)
        }) {
            continue;
        }
        let mut owner = owner;
        if let Some(row) = rows_by_pid.get(&pid) {
            if let Some(mut observation) = resolve_process_command_agent_identity(&row.command) {
                if let Some(agent_id) = observation.identity.agent_id.clone() {
                    let chain_depth = match owner.as_ref() {
                        None => Some(depth),
                        Some(current)
                            if current.agent_id == agent_id && current.pid == row.ppid =>
                        {
                            Some(current.chain_depth)
                        }
                        Some(_) => None,
                    };
                    if let Some(chain_depth) = chain_depth {
                        owner = Some(ProcessTreeAgentOwner {
                            agent_id,
                            pid: row.pid,
                            chain_depth,
                        });
                        observation.identity.process_id = Some(row.pid);
                        observation.identity.terminal_name = row.terminal_name.clone();
                        candidates.push(ProcessIdentityCandidate {
                            confidence: observation.confidence,
                            depth,
                            chain_depth,
                            identity: observation.identity,
                        });
                    }
                }
            }
        }
        if let Some(children) = children_by_parent_pid.get(&pid) {
            for child in children {
                queue.push_back((depth + 1, child.pid, owner.clone()));
            }
        }
    }
    candidates.sort_by(|left, right| {
        let chain = left.chain_depth.cmp(&right.chain_depth);
        if chain != std::cmp::Ordering::Equal {
            return chain;
        }
        let confidence =
            score_process_identity_candidate(right).cmp(&score_process_identity_candidate(left));
        if confidence != std::cmp::Ordering::Equal {
            return confidence;
        }
        let id_score = right
            .identity
            .agent_session_id
            .is_some()
            .cmp(&left.identity.agent_session_id.is_some());
        if id_score != std::cmp::Ordering::Equal {
            return id_score;
        }
        right.depth.cmp(&left.depth)
    });
    candidates
        .into_iter()
        .next()
        .map(|candidate| candidate.identity)
}

fn score_process_identity_candidate(candidate: &ProcessIdentityCandidate) -> i64 {
    candidate.confidence
        + if candidate.identity.agent_session_id.is_some() {
            GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS
        } else {
            0
        }
}

fn resolve_process_command_agent_identity(command: &str) -> Option<ProcessIdentityObservation> {
    let tokens = split_process_command_prefix(command, GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT);
    if tokens.is_empty() || should_ignore_process_command_for_agent_identity(command, &tokens) {
        return None;
    }
    resolve_agent_process_invocation(&tokens, 0, GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE)
        .or_else(|| resolve_wrapped_agent_process_invocation(command, &tokens))
}

/// CDXC:SessionIdentity 2026-10-16 WHY:
/// The first time a terminal's Claude opens the agents view, Claude 2.1.296 starts its per-user daemon as a child of that `claude` (`claude daemon run`), so the daemon's pty hosts, its pre-warmed spares and every conversation it runs (`claude --session-id <id>` under `--bg-pty-host`) hang below that zmx pane. The scan followed the same-agent chain into them and took a spare's `--session-id` as the session's conversation: on 2026-10-16 the session moved to f0903321 by its hooks and 1.6 seconds later to the spare 580e4309, which never holds a message. Which daemon conversation a terminal shows comes from its hooks (server/src/agent_hooks/claude_daemon.rs), so the scan stops at the daemon.
fn is_claude_daemon_process(command: &str) -> bool {
    let tokens = split_process_command_prefix(command, GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT);
    if normalize_process_executable_name(tokens.first().map(String::as_str)).as_deref()
        != Some("claude")
    {
        return false;
    }
    let args = tokens.get(1..).unwrap_or(&[]);
    args.first().map(String::as_str) == Some("daemon")
        || args.iter().any(|token| {
            matches!(
                token.as_str(),
                "--bg-pty-host" | "bg-pty-host" | "--bg-spare" | "bg-spare"
            )
        })
}

/// How far into a command line the scan looks for Claude's print-mode flags; a quoted prompt is one
/// token, so an app's long flag list still fits.
const HEADLESS_CLAUDE_FLAG_TOKEN_LIMIT: usize = 64;

/// CDXC:SessionIdentity 2026-10-30 WHY:
/// A program started from a Ghostex terminal (a desktop app, a script) that spawns `claude -p` or an Agent SDK Claude put that Claude below the pane's zmx with no agent above it, so the scan made a plain terminal session a Claude session for as long as the run lasted and kept it that way (throwaway G11n7, 2026-10-30), and `claude -p --resume <id>` would hand the session that id. A session's own Claude is interactive; print mode (`-p`, `--print`) and the SDK's stream-json flags only work headless. The hooks drop the same runs (nested_agent.rs `hook_agent_runs_headless`).
fn is_headless_claude_process(command: &str) -> bool {
    if !resolve_process_command_agent_identity(command).is_some_and(|observation| {
        matches!(
            observation.identity.agent_id.as_deref(),
            Some("claude" | "openclaude")
        )
    }) {
        return false;
    }
    split_process_command_prefix(command, HEADLESS_CLAUDE_FLAG_TOKEN_LIMIT)
        .iter()
        .skip(1)
        .any(|token| {
            let token = token.trim_matches(['"', '\'']);
            matches!(token, "-p" | "--print")
                || token.starts_with("--output-format")
                || token.starts_with("--input-format")
        })
}

fn should_ignore_process_command_for_agent_identity(command: &str, tokens: &[String]) -> bool {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str));
    if executable_name.as_deref().is_some_and(|name| {
        matches!(
            name,
            "gte" | "node_repl" | "prompt-editor" | "skycomputeruseclient"
        )
    }) {
        return true;
    }
    let lower_command = command.to_ascii_lowercase();
    ["skycomputeruseclient", "/node_repl", " node_repl"]
        .iter()
        .any(|marker| lower_command.contains(marker))
}

/// CDXC:SessionIdentity 2026-09-28 WHY:
/// Hermes runs as a Python interpreter: its installer's launcher is `python3 -I -c "<bootstrap>"` and a pip install is `python3 …/bin/hermes`. Left unrecognised, the first agent binary below it owned the row, and Hermes's Claude subscription provider spawns the native `claude` for every turn, so a Hermes row turned into a Claude session mid-conversation, dropped its Hermes conversation id, and its chat lost the history (observed live 2026-09-28).
fn resolve_wrapped_agent_process_invocation(
    command: &str,
    tokens: &[String],
) -> Option<ProcessIdentityObservation> {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str))?;
    let is_python = executable_name.starts_with("python");
    if is_python && command.contains("import hermes_bootstrap") {
        return Some(ProcessIdentityObservation {
            confidence: GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
            identity: ZmxProcessIdentity {
                agent_id: Some("hermes-agent".to_string()),
                ..ZmxProcessIdentity::default()
            },
        });
    }
    if executable_name == "env" || is_python {
        if let Some(invocation_index) = find_env_wrapped_command_index(tokens) {
            return resolve_agent_process_invocation(
                tokens,
                invocation_index,
                GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
            );
        }
    }
    if !matches!(executable_name.as_str(), "bun" | "node") {
        return None;
    }
    /*
    CDXC:SessionIdentity 2026-10-17 WHY:
    On native Windows Cursor CLI runs as `cmd /c cursor-agent.cmd` → `powershell -File cursor-agent.ps1` → `node.exe …\cursor-agent\versions\<version>\index.js --resume <id>`, and none of them is named cursor-agent (on macOS and Linux its launcher `exec -a`s the name onto node). Unrecognised, the session had no running agent as far as the process scan knew, so chat Interrupt & Send never interrupted: the message went into Cursor's own follow-up queue behind the running turn (agent flow tests, Windows, 2026-10-17). Node running the script of Cursor's versioned install is Cursor.
    */
    if let Some(index) = tokens.iter().take(8).skip(1).position(|token| {
        normalize_process_token(Some(token)).is_some_and(|path| {
            let path = path.replace('\\', "/");
            path.ends_with("/index.js") && path.contains("/cursor-agent/versions/")
        })
    }) {
        let index = index + 1;
        return Some(ProcessIdentityObservation {
            confidence: GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
            identity: ZmxProcessIdentity {
                agent_id: Some("cursor".to_string()),
                agent_session_id: extract_agent_process_session_id("cursor", tokens, index),
                ..ZmxProcessIdentity::default()
            },
        });
    }
    for index in 1..tokens.len().min(8) {
        if let Some(observation) = resolve_agent_process_invocation(
            tokens,
            index,
            GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
        ) {
            return Some(observation);
        }
    }
    None
}

fn resolve_agent_process_invocation(
    tokens: &[String],
    executable_index: usize,
    confidence: i64,
) -> Option<ProcessIdentityObservation> {
    let agent_id = infer_agent_id_from_process_executable(tokens, executable_index)?;
    let agent_session_id = extract_agent_process_session_id(&agent_id, tokens, executable_index);
    Some(ProcessIdentityObservation {
        confidence,
        identity: ZmxProcessIdentity {
            agent_id: Some(agent_id),
            agent_session_id,
            agent_session_path: None,
            process_id: None,
            terminal_name: None,
            open_file_paths: None,
        },
    })
}

#[cfg(test)]
pub(crate) fn read_codex_process_session_identity(
    process_id: Option<i64>,
) -> Option<(String, String)> {
    let process_id = process_id.filter(|process_id| *process_id > 0)?;
    codex_process_session_identity_from_paths(process_open_file_paths(process_id))
}

pub(crate) fn codex_process_session_identity_from_paths(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Option<(String, String)> {
    let mut identities = HashMap::<String, (String, PathBuf)>::new();
    let mut predecessors = HashSet::new();
    let mut inherited_rollouts = HashSet::new();
    for target in paths {
        let Some(agent_session_id) = codex_session_id_from_transcript_path(&target) else {
            continue;
        };
        let meta = crate::session_chat_successor::read_codex_session_meta(&target)?;
        if meta.is_subagent {
            continue;
        }
        if meta.session_id != agent_session_id {
            return None;
        }
        if let Some(parent) = meta.forked_from_id {
            predecessors.insert(parent);
        }
        if let Some(base) = meta.history_base {
            inherited_rollouts.insert(base.thread_id);
        }
        identities
            .entry(meta.rollout_id)
            .or_insert((agent_session_id, target));
    }
    // Codex may retain the previous rollout handle after rewinding. Among its
    // open root rollouts, only an unambiguous descendant can be the active branch.
    identities.retain(|rollout, (id, _)| {
        !predecessors.contains(id) && !inherited_rollouts.contains(rollout)
    });
    if identities.len() != 1 {
        return None;
    }
    identities
        .into_iter()
        .next()
        .map(|(_, (agent_session_id, path))| {
            (agent_session_id, path.to_string_lossy().into_owned())
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn process_open_file_paths(process_id: i64) -> Vec<PathBuf> {
    let fd_dir = PathBuf::from(format!("/proc/{process_id}/fd"));
    fs::read_dir(fd_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .collect()
}

/// `struct proc_fileinfo` from `<sys/proc_info.h>`, which the libc crate does not declare. Only its layout is used.
#[cfg(target_os = "macos")]
#[repr(C)]
#[allow(dead_code)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath`, the `PROC_PIDFDVNODEPATHINFO` reply.
#[cfg(target_os = "macos")]
#[repr(C)]
#[allow(dead_code)]
struct VnodeFdInfoWithPath {
    pfi: ProcFileInfo,
    pvip: libc::vnode_info_path,
}

#[cfg(target_os = "macos")]
const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;

/// CDXC:SessionIdentity 2026-10-23 WHY:
/// The paths of the files the process holds open, read from the kernel's file table as Linux reads `/proc/<pid>/fd`. This ran `lsof` (about 60ms of CPU and 200ms of wall time) for every live Codex process on each two-second identity snapshot. Every reader of these paths looks for a Codex rollout, which Codex keeps open for append, so the descriptors are the whole answer; `lsof` also listed the working directory, the executable and its libraries, which no reader wants.
#[cfg(target_os = "macos")]
pub(crate) fn process_open_file_paths(process_id: i64) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let Ok(pid) = i32::try_from(process_id) else {
        return Vec::new();
    };
    let entry_size = std::mem::size_of::<libc::proc_fdinfo>();
    // Both calls fill plain C output buffers; only complete replies are read.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return Vec::new();
    }
    // Room for descriptors opened between the two calls.
    let mut descriptors: Vec<libc::proc_fdinfo> =
        Vec::with_capacity(needed as usize / entry_size + 32);
    let Ok(capacity) = i32::try_from(descriptors.capacity() * entry_size) else {
        return Vec::new();
    };
    let listed = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            descriptors.as_mut_ptr() as *mut libc::c_void,
            capacity,
        )
    };
    if listed <= 0 {
        return Vec::new();
    }
    unsafe { descriptors.set_len(listed as usize / entry_size) };
    let reply_size = std::mem::size_of::<VnodeFdInfoWithPath>() as i32;
    descriptors
        .iter()
        .filter(|descriptor| descriptor.proc_fdtype == libc::PROX_FDTYPE_VNODE as u32)
        .filter_map(|descriptor| {
            let mut info: VnodeFdInfoWithPath = unsafe { std::mem::zeroed() };
            let read = unsafe {
                libc::proc_pidfdinfo(
                    pid,
                    descriptor.proc_fd,
                    PROC_PIDFDVNODEPATHINFO,
                    &mut info as *mut _ as *mut libc::c_void,
                    reply_size,
                )
            };
            if read != reply_size {
                return None;
            }
            let bytes = info
                .pvip
                .vip_path
                .iter()
                .flatten()
                .map(|byte| *byte as u8)
                .take_while(|byte| *byte != 0)
                .collect::<Vec<_>>();
            let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
            path.is_absolute().then_some(path)
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn process_open_file_paths(process_id: i64) -> Vec<PathBuf> {
    crate::platform::process_files::open_paths_isolated(&[process_id])
        .remove(&process_id)
        .unwrap_or_default()
}

/// `process_open_file_paths` for several processes; on Windows one helper reads them all.
#[cfg(windows)]
pub(crate) fn process_open_file_paths_many(process_ids: &[i64]) -> HashMap<i64, Vec<PathBuf>> {
    crate::platform::process_files::open_paths_isolated(process_ids)
}

#[cfg(not(windows))]
pub(crate) fn process_open_file_paths_many(process_ids: &[i64]) -> HashMap<i64, Vec<PathBuf>> {
    process_ids
        .iter()
        .map(|pid| (*pid, process_open_file_paths(*pid)))
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) fn process_open_file_paths(_process_id: i64) -> Vec<PathBuf> {
    Vec::new()
}

pub(crate) fn codex_session_id_from_transcript_path(path: &Path) -> Option<String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
        || !path
            .ancestors()
            .filter_map(Path::file_name)
            .filter_map(|name| name.to_str())
            .any(|name| name == "sessions")
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if !stem.starts_with("rollout-") || stem.len() < 36 {
        return None;
    }
    crate::session_chat_successor::codex_rollout_session_id(stem)
        .and_then(|id| normalize_codex_session_id(&id))
}

/// CDXC:SessionIdentity 2026-09-14 WHY:
/// OMP leaves `agent/terminal-sessions/<tty>` behind after exit, so a new process on a recycled TTY can inherit another project's identity before OMP rewrites the record.
/// Only accept the exact record written during this process's lifetime with its current cwd; never guess from transcript recency or another session in the cwd.
pub(crate) fn read_omp_terminal_session_identity(
    home_dir: &Path,
    terminal_name: Option<&str>,
    process_id: Option<i64>,
) -> Option<(String, String)> {
    let terminal_name = normalize_process_terminal_name(terminal_name?)?;
    let (cwd, started_at) = super::process_context::process_context(process_id?)?;
    let agent_dir = omp_agent_directory(home_dir);
    read_omp_terminal_session_identity_from_agent_dir(
        &agent_dir,
        &terminal_name,
        home_dir,
        &cwd,
        started_at,
    )
}

pub(crate) fn read_omp_terminal_session_identity_from_agent_dir(
    agent_dir: &Path,
    terminal_name: &str,
    home_dir: &Path,
    process_cwd: &Path,
    process_started_at: std::time::SystemTime,
) -> Option<(String, String)> {
    let record_path = agent_dir.join("terminal-sessions").join(terminal_name);
    use std::io::Read;
    let mut file = fs::File::open(&record_path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file()
        || metadata.len() > 16 * 1024
        || metadata.modified().ok()? < process_started_at
    {
        return None;
    }
    let mut record = String::new();
    (&mut file)
        .take(16 * 1024 + 1)
        .read_to_string(&mut record)
        .ok()?;
    let after = file.metadata().ok()?;
    if record.len() > 16 * 1024
        || metadata.modified().ok()? != after.modified().ok()?
        || metadata.len() != after.len()
    {
        return None;
    }
    let mut lines = record.lines();
    let cwd = expand_home_path(lines.next()?.trim(), home_dir);
    if fs::canonicalize(cwd).ok()? != fs::canonicalize(process_cwd).ok()? {
        return None;
    }
    let transcript_path = expand_home_path(lines.next()?.trim(), home_dir);
    if transcript_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("jsonl")
        || !transcript_path.is_file()
    {
        return None;
    }
    let stem = transcript_path.file_stem()?.to_str()?.trim();
    let agent_session_id = stem.rsplit_once('_').map(|(_, id)| id).unwrap_or(stem);
    if agent_session_id.is_empty() || agent_session_id.chars().count() > 256 {
        return None;
    }
    Some((
        agent_session_id.to_string(),
        transcript_path.to_string_lossy().into_owned(),
    ))
}

fn omp_agent_directory(home_dir: &Path) -> PathBuf {
    if let Some(agent_dir) = std::env::var("PI_CODING_AGENT_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return expand_home_path(&agent_dir, home_dir);
    }
    let config_dir = std::env::var("PI_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home_path(&value, home_dir))
        .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

fn expand_home_path(value: &str, home_dir: &Path) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return home_dir.to_path_buf();
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return home_dir.join(relative);
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        home_dir.join(path)
    }
}

fn infer_agent_id_from_process_executable(
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let executable_name =
        normalize_process_executable_name(tokens.get(executable_index).map(String::as_str))?;
    if executable_name == "cli"
        && tokens
            .get(executable_index)?
            .ends_with("/@oh-my-pi/pi-coding-agent/dist/cli.js")
    {
        return Some("omp".to_string());
    }
    // dsh-tui's launcher starts its terminal app as a second node process running the profile's
    // `host-entry.js`, which is the process in the foreground while DeepSeek Harness runs.
    if executable_name == "host-entry"
        && tokens
            .get(executable_index)?
            .replace('\\', "/")
            .ends_with("/dsh-adapter/host-entry.js")
    {
        return Some("dsh".to_string());
    }
    if executable_name == "acli"
        && normalize_process_token(tokens.get(executable_index + 1).map(String::as_str)).as_deref()
            == Some("rovodev")
        && normalize_process_token(tokens.get(executable_index + 2).map(String::as_str)).as_deref()
            == Some("run")
    {
        return Some("rovodev".to_string());
    }
    Some(
        match executable_name.as_str() {
            "agy" => "antigravity",
            "amp" => "amp",
            "claude" => "claude",
            "codebuddy" => "codebuddy",
            "codex" => "codex",
            "commandcode" => "command-code",
            "copilot" => "copilot",
            "cursor-agent" => "cursor",
            "mastracode" => "mastra",
            "devin" => "devin",
            "droid" => "droid",
            "empryo" | "em" => "empryo",
            "freebuff" => "freebuff",
            "dsh-tui" | "dst" => "dsh",
            "gemini" => "gemini",
            "grok" => "grok",
            "hermes" => "hermes-agent",
            "kimi" => "kimi",
            "kiro-cli" => "kiro",
            "omp" => "omp",
            "openclaude" => "openclaude",
            "opencode" => "opencode",
            "pi" => "pi",
            "qodercli" => "qoder",
            "zcode" | "zcode-cli" => "zcode",
            "rovodev" => "rovodev",
            _ => return None,
        }
        .to_string(),
    )
}

fn extract_agent_process_session_id(
    agent_id: &str,
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let args = tokens.get(executable_index + 1..).unwrap_or(&[]);
    /*
    CDXC:SessionFork 2026-09-02:
    A fork launch (`codex fork <id>`, `claude --resume <id> --fork-session`)
    names the PARENT conversation in argv; the forked conversation gets a brand
    new id that only the agent's hook or transcript can report. Reading the
    parent id as this terminal's own identity made every list/snapshot poll
    overwrite the hook-reported fork id with the parent's, so the fork and its
    parent shared one id: the parent's chat follower then adopted the fork's
    transcript as its own "successor" and both rows showed the same chat and
    title (observed live 2026-09-01). A fork argv therefore yields no id at all.
    */
    if agent_id == "codex" {
        for index in 0..args.len().saturating_sub(1) {
            let token = normalize_process_token(args.get(index).map(String::as_str));
            if token.as_deref() == Some("fork") {
                return None;
            }
            if token.as_deref() == Some("resume") {
                return normalize_agent_process_session_id(
                    agent_id,
                    args.get(index + 1).map(String::as_str),
                );
            }
        }
        return None;
    }
    if matches!(agent_id, "claude" | "cursor") {
        /*
        CDXC:SessionFork 2026-09-21 WHY:
        An in-app Claude fork leaves the terminal's `claude --resume <parent>` as a viewer and runs the conversation in a daemon-hosted `claude --session-id <fork> --fork-session --resume <parent>` below it. `--session-id` names the process's own conversation even on a fork launch, so it is read before the fork rule; ignoring it made the viewer's stale parent id the terminal's identity and the chat Subagents card refused the session as "a different conversation".
        */
        if agent_id == "claude" {
            if let Some(id) = read_agent_process_flag_value(agent_id, args, "--session-id") {
                return Some(id);
            }
        }
        if args.iter().any(|arg| {
            let token = arg.trim_matches(['"', '\'']);
            token == "--fork-session" || token.starts_with("--fork-session=")
        }) {
            return None;
        }
        return read_agent_process_flag_value(agent_id, args, "--resume");
    }
    if agent_id == "zcode" {
        return read_agent_process_flag_value(agent_id, args, "--resume");
    }
    if agent_id == "freebuff" {
        return read_agent_process_flag_value(agent_id, args, "--continue");
    }
    if agent_id == "dsh" {
        return read_agent_process_flag_value(agent_id, args, "--resume")
            .filter(|id| crate::session_chat_dsh::is_safe_dsh_session_id(id));
    }
    if agent_id == "empryo" {
        return ["--session", "--resume", "-s"]
            .into_iter()
            .find_map(|flag| read_agent_process_flag_value(agent_id, args, flag));
    }
    if agent_id == "opencode" {
        return read_agent_process_flag_value(agent_id, args, "--session")
            .or_else(|| read_agent_process_flag_value(agent_id, args, "-s"));
    }
    if matches!(agent_id, "pi" | "omp") {
        return read_agent_process_flag_value(agent_id, args, "--session").or_else(|| {
            (agent_id == "pi")
                .then(|| read_agent_process_flag_value(agent_id, args, "--session-id"))
                .flatten()
        });
    }
    if agent_id == "kiro" {
        return read_agent_process_flag_value(agent_id, args, "--resume-id");
    }
    None
}

fn read_agent_process_flag_value(agent_id: &str, args: &[String], flag: &str) -> Option<String> {
    for index in 0..args.len() {
        let arg = args.get(index)?;
        if arg == flag {
            return normalize_agent_process_session_id(
                agent_id,
                args.get(index + 1).map(String::as_str),
            );
        }
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return normalize_agent_process_session_id(agent_id, Some(value));
        }
    }
    None
}

fn normalize_agent_process_session_id(agent_id: &str, value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().trim_end_matches([';', '&', '|']).to_string();
    if normalized.is_empty() {
        return None;
    }
    if agent_id == "codex" {
        normalize_codex_session_id(&normalized).or(Some(normalized))
    } else {
        Some(normalized)
    }
}

fn find_env_wrapped_command_index(tokens: &[String]) -> Option<usize> {
    for index in 1..tokens.len() {
        let token = tokens.get(index)?;
        if token.starts_with('-') || is_environment_assignment(token) {
            continue;
        }
        return Some(index);
    }
    None
}

fn split_process_command_prefix(command: &str, max_tokens: usize) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for char in command.chars() {
        if escaped {
            current.push(char);
            escaped = false;
            continue;
        }
        if char == '\\' && !cfg!(windows) {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if char == active_quote {
                quote = None;
            } else {
                current.push(char);
            }
            continue;
        }
        if matches!(char, '"' | '\'') {
            quote = Some(char);
            continue;
        }
        if char.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() >= max_tokens {
                    return tokens;
                }
            }
            continue;
        }
        current.push(char);
    }
    if !current.is_empty() && tokens.len() < max_tokens {
        tokens.push(current);
    }
    tokens
}

fn normalize_process_executable_name(token: Option<&str>) -> Option<String> {
    let normalized = normalize_process_token(token)?;
    let basename = normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized.as_str())
        .to_string();
    for extension in [".cjs", ".cmd", ".exe", ".js", ".mjs", ".ts"] {
        if let Some(stripped) = basename.strip_suffix(extension) {
            return (!stripped.is_empty()).then(|| stripped.to_string());
        }
    }
    Some(basename)
}

fn normalize_process_token(token: Option<&str>) -> Option<String> {
    let text = token?.trim().to_ascii_lowercase();
    let text = text.trim_matches(['"', '\'', '`']).to_string();
    (!text.is_empty()).then_some(text)
}

fn is_environment_assignment(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut seen_equals = false;
    for char in chars {
        if char == '=' {
            seen_equals = true;
            break;
        }
        if !(char.is_ascii_alphanumeric() || char == '_') {
            return false;
        }
    }
    seen_equals
}

fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value).then(|| value.to_ascii_lowercase())
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}
