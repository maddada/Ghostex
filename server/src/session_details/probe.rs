//! Claude's own context breakdown for a session, read from a silent read-only copy of it.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! The live Claude runs in a terminal, so Ghostex cannot send it the `get_context_usage` control request Claude Desktop sends through its SDK. A second Claude started headless on the same conversation can be asked instead: `claude -p --input-format stream-json --resume <id> --fork-session --no-session-persistence` answers `initialize`, `get_context_usage` and `mcp_status` in about two seconds without calling the model, and writes nothing (tested 2026-10-25: the resumed transcript kept its size and time, no new file appeared). It is started the way the live one was: its executable, its environment (account folder, task tools) and the flags that change what is loaded (`--mcp-config`, `--plugin-dir`, `--agents`, the system-prompt files, `--model`), with every hook off so no session-start or stop hook fires for it.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q8): the breakdown refreshes "after each turn ends, while the panel is open, plus a refresh button", and "make the update really slow and only happen for the currently shown session". So nothing here runs on a timer: only a panel that is open asks, an automatic ask runs at most every `AUTO_INTERVAL` and only once the transcript grew and the agent is idle, a refresh press at most every `REFRESH_SPACING`, and one probe runs at a time on the whole computer.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

/// How often an open panel's automatic ask may start a probe.
const AUTO_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// How soon after a failed automatic probe another automatic one may start.
const FAILED_RETRY: Duration = Duration::from_secs(3 * 60);
/// How often the refresh button may start one.
const REFRESH_SPACING: Duration = Duration::from_secs(15);
/// How long one probe may take, MCP servers starting included.
const PROBE_TIMEOUT: Duration = Duration::from_secs(45);
/// Sessions whose breakdown is remembered.
const REMEMBERED: usize = 32;
/// Longest stderr line of Claude's kept for the error.
const MAX_STDERR_CHARS: usize = 300;
/// How long a probe that stopped early waits for the rest of Claude's stderr.
const STDERR_GRACE: Duration = Duration::from_secs(2);

/// What an ask wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProbeMode {
    /// The panel's own reads: a probe starts only when the slow rule allows.
    Auto,
    /// The refresh button.
    Refresh,
}

/// The session the probe copies.
pub(super) struct ProbeTarget {
    pub agent_session_id: String,
    pub cwd: Option<PathBuf>,
    /// The session's zmx name, which leads to the live Claude process whose program, environment
    /// and flags the copy reuses. Looked up only when a probe starts.
    pub zmx_name: Option<String>,
    /// The model the live session reports (statusline), used when its flags name none.
    pub model: Option<String>,
    pub transcript_len: u64,
    /// The conversation has its first message (`TranscriptFacts::conversation_started`); before
    /// that Claude has nothing to resume, so no probe starts.
    pub resumable: bool,
    pub working: bool,
    pub home: PathBuf,
}

/// The remembered breakdown of one session, as the panel reads it.
#[derive(Clone, Debug, Default)]
pub(super) struct ProbeSnapshot {
    /// The compact breakdown (`compact_breakdown`), once a probe has succeeded.
    pub breakdown: Option<Value>,
    /// The servers `mcp_status` reported, by name, with their status.
    pub mcp_status: Option<Value>,
    pub taken_at: Option<chrono::DateTime<chrono::Utc>>,
    pub running: bool,
    pub error: Option<String>,
    /// When the refresh button may start the next probe.
    pub refresh_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Default)]
struct Entry {
    snapshot: ProbeSnapshot,
    taken_at: Option<Instant>,
    attempted_at: Option<Instant>,
    transcript_len: Option<u64>,
    used: Option<Instant>,
}

fn probe_entries() -> &'static Mutex<HashMap<String, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(Mutex::default)
}

/// One probe at a time on the whole computer.
static PROBING: AtomicBool = AtomicBool::new(false);

fn due(entry: &Entry, target: &ProbeTarget, mode: ProbeMode, now: Instant) -> bool {
    if entry.snapshot.running || !target.resumable {
        return false;
    }
    let since = |at: Option<Instant>| at.map(|at| now.duration_since(at));
    match mode {
        ProbeMode::Refresh => since(entry.attempted_at).is_none_or(|age| age >= REFRESH_SPACING),
        ProbeMode::Auto => match entry.taken_at {
            // The first breakdown of a session is read straight away, unless the last try failed
            // a moment ago.
            None => since(entry.attempted_at).is_none_or(|age| age >= FAILED_RETRY),
            Some(taken) => {
                !target.working
                    && entry.transcript_len != Some(target.transcript_len)
                    && now.duration_since(taken) >= AUTO_INTERVAL
                    && since(entry.attempted_at).is_none_or(|age| age >= FAILED_RETRY)
            }
        },
    }
}

/// The remembered breakdown of `target`, after starting a probe when `mode` and the slow rule
/// allow one. Never waits for the probe: its answer is in the next read.
pub(super) fn request(target: ProbeTarget, mode: ProbeMode) -> ProbeSnapshot {
    let now = Instant::now();
    let mut entries = probe_entries()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if entries.len() >= REMEMBERED && !entries.contains_key(&target.agent_session_id) {
        if let Some(oldest) = entries
            .iter()
            .filter(|(_, entry)| !entry.snapshot.running)
            .min_by_key(|(_, entry)| entry.used)
            .map(|(id, _)| id.clone())
        {
            entries.remove(&oldest);
        }
    }
    let entry = entries.entry(target.agent_session_id.clone()).or_default();
    entry.used = Some(now);
    if due(entry, &target, mode, now)
        && PROBING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        entry.snapshot.running = true;
        entry.attempted_at = Some(now);
        entry.snapshot.refresh_at = Some(
            chrono::Utc::now() + chrono::Duration::from_std(REFRESH_SPACING).unwrap_or_default(),
        );
        let snapshot = entry.snapshot.clone();
        drop(entries);
        std::thread::spawn(move || {
            let outcome = run_probe(&target);
            PROBING.store(false, Ordering::SeqCst);
            let mut known = probe_entries()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let entry = known.entry(target.agent_session_id.clone()).or_default();
            entry.snapshot.running = false;
            match outcome {
                Ok((breakdown, mcp_status)) => {
                    entry.snapshot.breakdown = Some(breakdown);
                    entry.snapshot.mcp_status = mcp_status;
                    entry.snapshot.taken_at = Some(chrono::Utc::now());
                    entry.snapshot.error = None;
                    entry.taken_at = Some(Instant::now());
                    entry.transcript_len = Some(target.transcript_len);
                }
                Err(error) => entry.snapshot.error = Some(error),
            }
        });
        return snapshot;
    }
    entry.snapshot.clone()
}

/// Flags of the live Claude that change what its context holds, with how many values each takes.
enum Arity {
    Switch,
    One,
    /// Every following argument up to the next flag (`--mcp-config a.json b.json`).
    List,
}

fn mirrored_flag(name: &str) -> Option<Arity> {
    Some(match name {
        "--strict-mcp-config" => Arity::Switch,
        "--model"
        | "--agents"
        | "--agent"
        | "--system-prompt"
        | "--system-prompt-file"
        | "--append-system-prompt"
        | "--append-system-prompt-file" => Arity::One,
        "--mcp-config" | "--add-dir" | "--plugin-dir" | "--allowedTools" | "--allowed-tools"
        | "--disallowedTools" | "--disallowed-tools" | "--tools" => Arity::List,
        _ => return None,
    })
}

/// The live process's program (with a script argument for a `node cli.js` install), its mirrored
/// flags, and the `--settings` it was started with.
fn split_live_arguments(arguments: &[String]) -> (Vec<String>, Vec<String>, Option<String>) {
    let mut program = Vec::new();
    let mut rest = arguments.iter().peekable();
    if let Some(first) = rest.next() {
        program.push(first.clone());
        let stem = Path::new(first)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if matches!(stem, "node" | "bun" | "deno") {
            if let Some(script) = rest.next_if(|argument| !argument.starts_with('-')) {
                program.push(script.clone());
            }
        }
    }
    let mut flags = Vec::new();
    let mut settings = None;
    while let Some(argument) = rest.next() {
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (argument.as_str(), None),
        };
        if name == "--settings" {
            settings = inline.or_else(|| rest.next().cloned());
            continue;
        }
        let Some(arity) = mirrored_flag(name) else {
            continue;
        };
        flags.push(name.to_string());
        match (arity, inline) {
            (Arity::Switch, _) => {}
            (_, Some(value)) => flags.push(value),
            (Arity::One, None) => {
                if let Some(value) = rest.next() {
                    flags.push(value.clone());
                }
            }
            (Arity::List, None) => {
                while let Some(value) = rest.next_if(|value| !value.starts_with("--")) {
                    flags.push(value.clone());
                }
            }
        }
    }
    (program, flags, settings)
}

/// The live `--settings` (inline JSON or a file) with every hook off.
fn probe_settings(live: Option<&str>) -> String {
    let mut settings = live
        .and_then(|value| {
            serde_json::from_str::<Value>(value).ok().or_else(|| {
                std::fs::read_to_string(value)
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            })
        })
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    settings.insert("disableAllHooks".to_string(), Value::Bool(true));
    Value::Object(settings).to_string()
}

/// The live Claude process of the session, when it is running.
fn live_process_id(target: &ProbeTarget) -> Option<i64> {
    let name = target.zmx_name.as_ref()?;
    crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(name), &target.home)
        .ok()?
        .get(name)?
        .process_id
}

fn build_command(target: &ProbeTarget) -> Result<Command, String> {
    let live = live_process_id(target).and_then(crate::accounts::process_launch);
    let (program, mut flags, live_settings) = live
        .as_ref()
        .map(|live| split_live_arguments(&live.arguments))
        .unwrap_or_default();
    let program = match program.first() {
        Some(executable) if Path::new(executable).is_absolute() => program,
        _ => vec![
            crate::accounts::helpers::probe_executable(&target.home, "claude")
                .ok_or_else(|| "Claude Code isn't installed on this computer.".to_string())?
                .to_string_lossy()
                .into_owned(),
        ],
    };
    if !flags.iter().any(|flag| flag == "--model") {
        if let Some(model) = target.model.as_deref().filter(|model| !model.is_empty()) {
            flags.push("--model".to_string());
            flags.push(model.to_string());
        }
    }
    let mut command = Command::new(&program[0]);
    command.args(&program[1..]);
    command.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--resume",
        &target.agent_session_id,
        "--fork-session",
        "--settings",
        &probe_settings(live_settings.as_deref()),
    ]);
    command.args(&flags);
    if let Some(live) = live.as_ref().filter(|live| !live.environment.is_empty()) {
        command.env_clear();
        command.envs(live.environment.iter().map(|(name, value)| (name, value)));
    }
    // The copy is nobody's child session and must not reach an IDE the live one talks to.
    for key in crate::zmx::agent_cli_session_identity_environment_keys() {
        command.env_remove(key);
    }
    if let Some(cwd) = target.cwd.as_ref().filter(|cwd| cwd.is_dir()) {
        command.current_dir(cwd);
    } else {
        command.current_dir(&target.home);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    Ok(command)
}

/// Runs one probe: `(compact breakdown, mcp status)` or the reason in words.
fn run_probe(target: &ProbeTarget) -> Result<(Value, Option<Value>), String> {
    let mut command = build_command(target)?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("Claude Code couldn't start: {error}"))?;
    // Claude says why it stopped on stderr ("No conversation found with session ID: …"); its last
    // line goes into the error the panel shows.
    let (stderr_sender, stderr_last) = mpsc::channel::<String>();
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut last = String::new();
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else {
                    break;
                };
                if !line.trim().is_empty() {
                    last = line.trim().chars().take(MAX_STDERR_CHARS).collect();
                }
            }
            let _ = stderr_sender.send(last);
        });
    }
    let result = (|| {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Claude Code couldn't be asked.".to_string())?;
        for (id, subtype) in [
            ("init", "initialize"),
            ("ctx", "get_context_usage"),
            ("mcp", "mcp_status"),
        ] {
            let request = json!({"type": "control_request", "request_id": id, "request": {"subtype": subtype}});
            writeln!(stdin, "{request}")
                .map_err(|error| format!("Claude Code couldn't be asked: {error}"))?;
        }
        stdin.flush().ok();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Claude Code gave no answer.".to_string())?;
        let (sender, receiver) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + PROBE_TIMEOUT;
        let mut context = None;
        let mut mcp = None;
        while context.is_none() || mcp.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match receiver.recv_timeout(left) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "Claude Code took longer than {} seconds to answer.",
                        PROBE_TIMEOUT.as_secs()
                    ))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("type").and_then(Value::as_str) != Some("control_response") {
                continue;
            }
            let response = &message["response"];
            let answer = (response["subtype"] == "success").then(|| response["response"].clone());
            match response["request_id"].as_str() {
                Some("ctx") => {
                    context = Some(answer.ok_or_else(|| {
                        response["error"]
                            .as_str()
                            .map(|error| format!("Claude Code couldn't count the context: {error}"))
                            .unwrap_or_else(|| {
                                "Claude Code couldn't count the context.".to_string()
                            })
                    })?)
                }
                Some("mcp") => mcp = Some(answer.unwrap_or(Value::Null)),
                _ => {}
            }
        }
        drop(stdin);
        let Some(context) = context else {
            let said = stderr_last.recv_timeout(STDERR_GRACE).unwrap_or_default();
            return Err(if said.is_empty() {
                "Claude Code stopped before it counted the context.".to_string()
            } else {
                format!("Claude Code stopped before it counted the context: {said}")
            });
        };
        Ok((
            compact_breakdown(&context),
            mcp.map(|mcp| compact_mcp_status(&mcp)),
        ))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn number(value: &Value, key: &str) -> Value {
    value
        .get(key)
        .filter(|value| value.is_number())
        .cloned()
        .unwrap_or(Value::Null)
}

/// The parts of `get_context_usage` the panel shows: the categories, the memory files, the
/// skills, the MCP tools per server, and what the messages are made of.
fn compact_breakdown(context: &Value) -> Value {
    let categories: Vec<Value> = context["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|category| {
            json!({
                "id": category["id"],
                "name": category["name"],
                "tokens": number(category, "tokens"),
                "kind": category["kind"],
            })
        })
        .collect();
    let memory_files: Vec<Value> = context["memoryFiles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| json!({"path": file["path"], "type": file["type"], "tokens": number(file, "tokens")}))
        .collect();
    let mut servers: Map<String, Value> = Map::new();
    for tool in context["mcpTools"].as_array().into_iter().flatten() {
        let Some(server) = tool["serverName"].as_str() else {
            continue;
        };
        let entry = servers
            .entry(server.to_string())
            .or_insert_with(|| json!({"name": server, "tools": 0, "tokens": 0, "loaded": 0}));
        entry["tools"] = json!(entry["tools"].as_u64().unwrap_or(0) + 1);
        entry["tokens"] =
            json!(entry["tokens"].as_u64().unwrap_or(0) + tool["tokens"].as_u64().unwrap_or(0));
        if tool["isLoaded"] == true {
            entry["loaded"] = json!(entry["loaded"].as_u64().unwrap_or(0) + 1);
        }
    }
    let skills: Vec<Value> = context["skills"]["skillFrontmatter"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|skill| json!({"name": skill["name"], "source": skill["source"], "path": skill["path"], "tokens": number(skill, "tokens")}))
        .collect();
    let breakdown = &context["messageBreakdown"];
    let tool_calls: Vec<Value> = breakdown["toolCallsByType"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| {
            json!({
                "name": tool["name"],
                "tokens": tool["callTokens"].as_u64().unwrap_or(0) + tool["resultTokens"].as_u64().unwrap_or(0),
            })
        })
        .collect();
    json!({
        "totalTokens": number(context, "totalTokens"),
        "maxTokens": number(context, "maxTokens"),
        "percentage": number(context, "percentage"),
        "autoCompactThreshold": number(context, "autoCompactThreshold"),
        "autoCompactEnabled": context["isAutoCompactEnabled"],
        "model": context["model"],
        "categories": categories,
        "memoryFiles": memory_files,
        "mcpServers": servers.into_iter().map(|(_, server)| server).collect::<Vec<_>>(),
        "skills": {
            "count": number(&context["skills"], "totalSkills"),
            "tokens": number(&context["skills"], "tokens"),
            "items": skills,
        },
        "messages": {
            "toolCalls": tool_calls,
            "toolResultTokens": number(breakdown, "toolResultTokens"),
            "toolCallTokens": number(breakdown, "toolCallTokens"),
            "assistantTokens": number(breakdown, "assistantMessageTokens"),
            "userTokens": number(breakdown, "userMessageTokens"),
            "attachmentTokens": number(breakdown, "attachmentTokens"),
        },
    })
}

/// `mcp_status` as `{<server name>: {status, scope}}`.
fn compact_mcp_status(mcp: &Value) -> Value {
    let mut servers = Map::new();
    for server in mcp["mcpServers"].as_array().into_iter().flatten() {
        if let Some(name) = server["name"].as_str() {
            servers.insert(
                name.to_string(),
                json!({"status": server["status"], "scope": server["scope"]}),
            );
        }
    }
    Value::Object(servers)
}
