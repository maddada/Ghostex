//! A Codex session's own sections: the MCP servers it loads, the AGENTS.md files it loaded, and the
//! background terminals it started. Its account email is read with the rest of Environment.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! Codex 0.164 writes no list of the MCP servers a session started (its rollout's tool records hold only part of them in code mode), so Connectors asks Codex itself with `codex mcp list --json`, the read-only answer of the servers a session in that folder with that CODEX_HOME loads (its plugins' servers included), next to the Ghostex Connectors workspace every Codex launch gets. The rollout's `world_state` record carries `agents_md` (`directory` and the loaded text: the global AGENTS.md, then `--- project-doc ---` and the project's), which names the files. Background terminals are `exec_command` calls whose answer carries a `session_id` and no exit code, and they end in a `CommandExecution` item with that `process_id` (tested 2026-10-25 on Codex 0.164 in code mode, where `update_plan` is not offered, so Codex sessions have no task list). Codex has no way to be asked to stop one, so the rows have no stop button.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::server::AppState;
use crate::session_chat::{read_transcript_file_version, TranscriptFileVersion};
use crate::session_chat_fleet_transcript::timestamp;

/// Rollouts kept scanned in memory.
const CACHED_ROLLOUTS: usize = 16;
/// A record longer than this is skipped unparsed (command output can be large).
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
/// Finished background terminals kept for the panel's fold.
const FINISHED_KEPT: usize = 20;
const MAX_TITLE_CHARS: usize = 160;
/// How long `codex mcp list` may take, and how long its answer is reused.
const MCP_LIST_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_LIST_FRESH_FOR: Duration = Duration::from_secs(60);
/// What Codex puts between the global AGENTS.md and the project's in the text it loads.
const PROJECT_DOC_SEPARATOR: &str = "--- project-doc ---";
/// The MCP server Ghostex Connectors adds to every Codex launch (connectors/agent/launch.rs).
const CONNECTORS_SERVER: &str = "ghostex-connectors";

#[derive(Clone, Debug)]
struct Run {
    id: String,
    title: String,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    status: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct RolloutFacts {
    /// `(directory, text)` of the last `agents_md` Codex recorded.
    agents_md: Option<(Option<String>, String)>,
    /// The commands of `exec` calls still waiting for their answer, by call id.
    pending: HashMap<String, (Vec<String>, i64)>,
    running: Vec<Run>,
    finished: Vec<Run>,
    last_at: i64,
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\u{2026}", text[..end].trim_end()),
        None => text.to_string(),
    }
}

/// The string literal at the start of `text` (`"…"`, `'…'` or `` `…` ``), unescaped.
fn string_literal(text: &str) -> Option<String> {
    let quote = text
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let mut out = String::new();
    let mut chars = text[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c if c == quote => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// The commands of the `tools.exec_command(…)` calls in a code-mode `exec` script, in order: the
/// string argument, or the `cmd` of the object argument.
fn exec_commands(script: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for (at, _) in script.match_indices("exec_command(") {
        let rest = script[at + "exec_command(".len()..].trim_start();
        let command = if rest.starts_with(['"', '\'', '`']) {
            string_literal(rest)
        } else {
            let window: String = rest.chars().take(600).collect();
            window.find("cmd").and_then(|key| {
                let after = window[key + 3..]
                    .trim_start_matches(['"', '\''])
                    .trim_start();
                string_literal(after.strip_prefix(':')?.trim_start())
            })
        };
        if let Some(command) = command.map(|command| command.trim().to_string()) {
            if !command.is_empty() {
                commands.push(command);
            }
        }
    }
    commands
}

/// The session ids of the terminals an `exec` answer left running (a result with a `session_id`
/// and no `exit_code`), in order.
fn running_session_ids(output: &Value) -> Vec<String> {
    output
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .filter_map(|text| serde_json::from_str::<Value>(text.trim()).ok())
        .filter(|result| result.get("exit_code").is_none())
        .filter_map(|result| result.get("session_id").and_then(Value::as_i64))
        .map(|id| id.to_string())
        .collect()
}

impl RolloutFacts {
    fn observe(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let wanted = [
            "\"world_state\"",
            "\"custom_tool_call",
            "\"CommandExecution\"",
            "\"session_meta\"",
        ];
        if !wanted.iter().any(|marker| text.contains(marker)) {
            return;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if let Some(at) = timestamp(&record) {
            self.last_at = at;
        }
        let payload = &record["payload"];
        match (record["type"].as_str(), payload["type"].as_str()) {
            // A new or resumed Codex starts with no terminals: the old process's died with it.
            (Some("session_meta"), _) => {
                self.pending.clear();
                self.running.clear();
            }
            (Some("world_state"), _) => {
                if let Some(agents_md) = payload.pointer("/state/agents_md") {
                    self.agents_md = Some((
                        agents_md["directory"].as_str().map(str::to_string),
                        agents_md["text"].as_str().unwrap_or_default().to_string(),
                    ));
                }
            }
            (Some("response_item"), Some("custom_tool_call")) if payload["name"] == "exec" => {
                let commands = exec_commands(payload["input"].as_str().unwrap_or_default());
                if let (Some(call), false) = (payload["call_id"].as_str(), commands.is_empty()) {
                    self.pending
                        .insert(call.to_string(), (commands, self.last_at));
                }
            }
            (Some("response_item"), Some("custom_tool_call_output")) => {
                let Some((commands, at)) = payload["call_id"]
                    .as_str()
                    .and_then(|call| self.pending.remove(call))
                else {
                    return;
                };
                for (index, id) in running_session_ids(&payload["output"])
                    .into_iter()
                    .enumerate()
                {
                    let title = commands
                        .get(index)
                        .or(commands.last())
                        .map(|command| {
                            cut(command.lines().next().unwrap_or(command), MAX_TITLE_CHARS)
                        })
                        .unwrap_or_default();
                    self.running.retain(|run| run.id != id);
                    self.running.push(Run {
                        id,
                        title,
                        started_at_ms: at,
                        ended_at_ms: None,
                        status: None,
                    });
                }
            }
            (Some("event_msg"), Some("item_completed"))
                if payload.pointer("/item/type").and_then(Value::as_str)
                    == Some("CommandExecution") =>
            {
                let item = &payload["item"];
                let Some(id) = item["process_id"].as_str() else {
                    return;
                };
                let Some(at) = self.running.iter().position(|run| run.id == id) else {
                    return;
                };
                let mut run = self.running.remove(at);
                run.ended_at_ms = payload["completed_at_ms"].as_i64().or(Some(self.last_at));
                run.status = Some(
                    match (item["status"].as_str(), item["exit_code"].as_i64()) {
                        (Some("completed"), Some(0) | None) => "completed",
                        (Some("completed"), Some(_)) | (Some("failed"), _) => "failed",
                        _ => "stopped",
                    }
                    .to_string(),
                );
                self.finished.insert(0, run);
                self.finished.truncate(FINISHED_KEPT);
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct CacheEntry {
    version: TranscriptFileVersion,
    consumed_to: u64,
    facts: RolloutFacts,
    used: u64,
}

/// The facts of the rollout at `path`, reading only what was appended since the last call.
fn scan_rollout(path: &Path) -> std::io::Result<RolloutFacts> {
    static CACHE: OnceLock<Mutex<(u64, HashMap<PathBuf, CacheEntry>)>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    let previous = cache
        .lock()
        .ok()
        .and_then(|cache| cache.1.get(path).cloned());
    let mut entry = previous
        .filter(|entry| {
            entry.version.identity == version.identity && entry.version.size <= version.size
        })
        .unwrap_or_else(|| CacheEntry {
            version: version.clone(),
            consumed_to: 0,
            facts: RolloutFacts::default(),
            used: 0,
        });
    if entry.consumed_to < version.size {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(entry.consumed_to))?;
        let mut reader = BufReader::new(file.take(version.size - entry.consumed_to));
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 || line.last() != Some(&b'\n') {
                break;
            }
            if read <= MAX_RECORD_BYTES {
                entry.facts.observe(&line);
            }
            entry.consumed_to += read as u64;
        }
    }
    entry.version = version;
    let facts = entry.facts.clone();
    if let Ok(mut cache) = cache.lock() {
        cache.0 += 1;
        entry.used = cache.0;
        if cache.1.len() >= CACHED_ROLLOUTS && !cache.1.contains_key(path) {
            if let Some(oldest) = cache
                .1
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
            {
                cache.1.remove(&oldest);
            }
        }
        cache.1.insert(path.to_path_buf(), entry);
    }
    Ok(facts)
}

/// The AGENTS.md files behind Codex's loaded text: the global one (`$CODEX_HOME`) when the text
/// has its part, then each project folder's from the repository root down to `directory`.
fn memory_files(directory: Option<&str>, text: &str, codex_home: &Path) -> Vec<Value> {
    let pick = |folder: &Path| {
        ["AGENTS.override.md", "AGENTS.md"]
            .iter()
            .map(|name| folder.join(name))
            .find(|path| path.is_file())
    };
    let (global, project) = match text.split_once(PROJECT_DOC_SEPARATOR) {
        Some((global, project)) => (!global.trim().is_empty(), !project.trim().is_empty()),
        None if directory.is_some() => (false, !text.trim().is_empty()),
        None => (!text.trim().is_empty(), false),
    };
    let mut files = Vec::new();
    if global {
        if let Some(path) = pick(codex_home) {
            files.push(
                json!({"path": path.to_string_lossy(), "kind": "User", "tokens": Value::Null}),
            );
        }
    }
    if let (true, Some(directory)) = (project, directory) {
        let directory = Path::new(directory);
        let mut folders = Vec::new();
        for folder in directory.ancestors() {
            folders.push(folder);
            if folder.join(".git").exists() {
                break;
            }
        }
        if !folders
            .last()
            .is_some_and(|root| root.join(".git").exists())
        {
            folders.truncate(1);
        }
        for folder in folders.into_iter().rev() {
            if let Some(path) = pick(folder) {
                files.push(
                    json!({"path": path.to_string_lossy(), "kind": "Project", "tokens": Value::Null}),
                );
            }
        }
    }
    files
}

fn run_value(run: &Run) -> Value {
    let at = |ms: i64| {
        chrono::DateTime::from_timestamp_millis(ms)
            .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    };
    json!({
        "id": run.id,
        "kind": "shell",
        "title": run.title,
        "command": run.title,
        "status": run.status,
        "startedAt": at(run.started_at_ms),
        "endedAt": run.ended_at_ms.and_then(at),
    })
}

/// The program that runs Codex for this session: the live process's (with its script for a
/// `node codex.js` install), else the `codex` on this computer.
fn codex_program(
    live: Option<&(Vec<String>, Vec<(String, String)>)>,
    home: &Path,
) -> Option<Vec<String>> {
    if let Some((arguments, _)) = live {
        let first = arguments.first()?;
        let stem = Path::new(first)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if matches!(stem, "node" | "bun") {
            if let Some(script) = arguments.get(1).filter(|script| !script.starts_with('-')) {
                return Some(vec![first.clone(), script.clone()]);
            }
        } else if Path::new(first).is_absolute() {
            return Some(vec![first.clone()]);
        }
    }
    crate::accounts::helpers::probe_executable(home, "codex")
        .map(|path| vec![path.to_string_lossy().into_owned()])
}

/// `codex mcp list --json` for a session in `folder`, run with the live session's environment.
/// Blocking; cached a minute.
fn mcp_servers(
    program: &[String],
    environment: &[(String, String)],
    folder: Option<&Path>,
) -> Vec<Value> {
    type Cache = Mutex<HashMap<String, (Instant, Vec<Value>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let codex_home = environment
        .iter()
        .find(|(name, _)| name == "CODEX_HOME")
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();
    let key = format!(
        "{}\u{0}{codex_home}\u{0}{}",
        program.join(" "),
        folder
            .map(|folder| folder.to_string_lossy())
            .unwrap_or_default()
    );
    if let Some((_, servers)) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|(at, _)| at.elapsed() < MCP_LIST_FRESH_FOR)
    {
        return servers;
    }
    let servers = list_mcp_servers(program, environment, folder).unwrap_or_default();
    if let Ok(mut cache) = cache.lock() {
        cache.retain(|_, (at, _)| at.elapsed() < MCP_LIST_FRESH_FOR);
        cache.insert(key, (Instant::now(), servers.clone()));
    }
    servers
}

fn list_mcp_servers(
    program: &[String],
    environment: &[(String, String)],
    folder: Option<&Path>,
) -> Option<Vec<Value>> {
    let mut command = Command::new(program.first()?);
    command.args(&program[1..]).args(["mcp", "list", "--json"]);
    if !environment.is_empty() {
        command.env_clear();
        command.envs(environment.iter().map(|(name, value)| (name, value)));
    }
    if let Some(folder) = folder.filter(|folder| folder.is_dir()) {
        command.current_dir(folder);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + MCP_LIST_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let text = reader.join().ok()?;
    let listed: Value = serde_json::from_str(text.trim()).ok()?;
    Some(
        listed
            .as_array()?
            .iter()
            .filter(|server| server["enabled"] == true)
            .filter_map(|server| {
                let name = server["name"].as_str()?;
                (name != CONNECTORS_SERVER).then(|| {
                    json!({
                        "key": name,
                        "name": name,
                        "tools": Value::Null,
                        "status": if server["auth_status"] == "not_logged_in" { "needsAuth" } else { "connected" },
                        "scope": Value::Null,
                    })
                })
            })
            .collect(),
    )
}

/// Fills Connectors, Memory files and Background tasks for a Codex session. Blocking.
pub(super) fn read_codex_details(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    folder: Option<&Path>,
    result: &mut Value,
) {
    let home = state.paths.home_dir.clone();
    let live = super::agents::live_launch(session, &home);
    let environment = live
        .as_ref()
        .map(|(_, environment)| environment.clone())
        .unwrap_or_default();
    let codex_home = environment
        .iter()
        .find(|(name, _)| name == "CODEX_HOME")
        .map(|(_, value)| PathBuf::from(value))
        .unwrap_or_else(|| home.join(".codex"));

    let workspace = super::connectors::workspace_group(state, project_id, session_id);
    let servers = codex_program(live.as_ref(), &home)
        .map(|program| mcp_servers(&program, &environment, folder))
        .unwrap_or_default();
    if workspace.is_some() || !servers.is_empty() {
        result["connectors"] = json!({"workspace": workspace, "servers": servers});
    }

    let transcript = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    )
    .ok()
    .and_then(|resolution| resolution.transcript().1.map(Path::to_path_buf));
    let Some(facts) = transcript
        .as_deref()
        .and_then(|path| scan_rollout(path).ok())
    else {
        return;
    };
    if let Some((directory, text)) = facts.agents_md.as_ref() {
        let files = memory_files(directory.as_deref(), text, &codex_home);
        if !files.is_empty() {
            result["memoryFiles"] = Value::Array(files);
        }
    }
    if !facts.running.is_empty() || !facts.finished.is_empty() {
        result["background"] = json!({
            "running": facts.running.iter().map(run_value).collect::<Vec<_>>(),
            "finished": facts.finished.iter().map(run_value).collect::<Vec<_>>(),
        });
    }
}
