//! Owner-initiated command runner for Needs you cards and coordinator asks.
//!
//! When the user clicks "Run it" on desktop (or "Run on <computer>" on their paired phone),
//! `/api/coordinatorBoard { op: "runCommand" }` validates that:
//! 1. The caller is an owner device (`Principal::Owner | Principal::Device | Principal::LocalAgent`).
//! 2. The named Needs you item or coordinator work ask is still open and carries a stored command.
//! 3. That stored command passes the strict Ghostex allowlist (`parse_runnable_command`).
//!
//! It then spawns the `ghostex` binary directly (never through a shell, stdin closed so nothing is
//! ever auto-approved), streams sanitized output and user-action hints (Touch ID / password /
//! browser sign-in / approval) via `needsYou.commandRuns` on live chat frames, and on exit code 0
//! automatically settles the card as Done and tells the waiting session.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ghostex_gx_chat_core::extras::needs_you_command::{display_command, parse_runnable_command};
use serde_json::{json, Map, Value};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, oneshot};

use super::store::NeedsYouRecord;
use crate::domain::{DomainRepository, DomainStateError};
use crate::security::Principal;
use crate::server::AppState;

const MAX_OUTPUT_BYTES: usize = 16 * 1024;
const PUBLISH_THROTTLE_MS: u64 = 250;
/// Short beat after exit 0 so the user sees "Done automatically" before the row clears.
const AUTO_DONE_SETTLE_DELAY_MS: u64 = 1_500;

type RunKey = (String, String, String);

struct ActiveRun {
    run_id: u64,
    command: String,
    state: String,
    hint_title: String,
    hint: String,
    output: String,
    error: String,
    exit_code: Option<i32>,
    started_at: String,
    finished_at: Option<String>,
    stopped_by_user: bool,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl ActiveRun {
    fn to_value(&self) -> Value {
        json!({
            "runId": self.run_id,
            "command": self.command,
            "state": self.state,
            "hintTitle": self.hint_title,
            "hint": self.hint,
            "output": self.output,
            "error": self.error,
            "exitCode": self.exit_code,
            "startedAt": self.started_at,
            "finishedAt": self.finished_at,
        })
    }

    fn is_active(&self) -> bool {
        matches!(self.state.as_str(), "running" | "waiting")
    }
}

enum CommandTarget {
    SessionTodo {
        tag: String,
        command: String,
    },
    WorkAsk {
        item_ref: String,
        ask_tag: String,
        command: String,
        offers_done: bool,
    },
}

impl CommandTarget {
    fn row_key(&self) -> String {
        match self {
            Self::SessionTodo { tag, .. } => format!("needs-you:{tag}"),
            Self::WorkAsk {
                item_ref, ask_tag, ..
            } => format!("work-ask:{item_ref}:{ask_tag}"),
        }
    }

    fn command(&self) -> &str {
        match self {
            Self::SessionTodo { command, .. } | Self::WorkAsk { command, .. } => command,
        }
    }
}

fn runs() -> &'static Mutex<HashMap<RunKey, ActiveRun>> {
    static RUNS: OnceLock<Mutex<HashMap<RunKey, ActiveRun>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
}

fn next_run_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn text(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn is_todo_reference(reference: &str) -> bool {
    reference
        .strip_prefix(['Q', 'q'])
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// All live or recent command runs for `(project_id, session_id)` keyed by row key.
pub(crate) fn runs_for_session(project_id: &str, session_id: &str) -> Map<String, Value> {
    let mut map = Map::new();
    let Ok(guard) = runs().lock() else {
        return map;
    };
    for ((proj, sess, row_key), run) in guard.iter() {
        if proj == project_id && sess == session_id {
            map.insert(row_key.clone(), run.to_value());
        }
    }
    map
}

/// Compact identity string for `needs_you_identity` so SSH/long-poll clients wake on status changes.
pub(crate) fn identity_for_session(project_id: &str, session_id: &str) -> String {
    let Ok(guard) = runs().lock() else {
        return String::new();
    };
    let mut parts: Vec<String> = guard
        .iter()
        .filter(|((proj, sess, _), _)| proj == project_id && sess == session_id)
        .map(|((_, _, row_key), run)| {
            format!(
                "{row_key}:{}:{}:{}:{}:{}",
                run.run_id,
                run.state,
                run.hint_title,
                run.output.len(),
                run.error.len()
            )
        })
        .collect();
    parts.sort();
    parts.join("|")
}

fn resolve_target(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    item_ref: &str,
    ask_tag: &str,
) -> Result<CommandTarget, DomainStateError> {
    if ask_tag.is_empty() && is_todo_reference(item_ref) {
        let session = repository
            .get_session(project_id, session_id)?
            .ok_or_else(|| DomainStateError::bad_request("No such session."))?;
        let record = NeedsYouRecord::of(&session);
        let item = record
            .open
            .iter()
            .find(|item| item.tag.eq_ignore_ascii_case(item_ref))
            .ok_or_else(|| {
                DomainStateError::bad_request(format!(
                    "{} is no longer open.",
                    item_ref.to_ascii_uppercase()
                ))
            })?;
        return Ok(CommandTarget::SessionTodo {
            tag: item.tag.clone(),
            command: item.command.clone(),
        });
    }

    let number = crate::coordinators::item_number_of(item_ref)
        .ok_or_else(|| DomainStateError::bad_request("Name the open item and ask to run."))?;
    let key = (project_id.to_string(), session_id.to_string());
    let items = crate::coordinators::list_work_items(db, &key)?;
    let item = items
        .iter()
        .find(|item| item.dropped_at.is_none() && item.number == number)
        .ok_or_else(|| DomainStateError::bad_request(format!("W{number} is not open.")))?;
    let ask = item
        .asks
        .iter()
        .find(|ask| ask.is_open() && ask.tag.eq_ignore_ascii_case(ask_tag))
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "{} is no longer open on W{number}.",
                ask_tag.to_ascii_uppercase()
            ))
        })?;
    let offers_done = ask.answers.trim().is_empty()
        || ask
            .answers
            .split(',')
            .any(|part| part.trim().trim_start_matches('*').trim() == "Done");
    Ok(CommandTarget::WorkAsk {
        item_ref: item.reference(),
        ask_tag: ask.tag.clone(),
        command: ask.command.clone(),
        offers_done,
    })
}

/// Handles `/api/coordinatorBoard` with `op: "runCommand"` or `op: "stopCommand"`.
pub(crate) fn handle_board_command_op(
    state: &AppState,
    caller: &Principal,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if !caller.is_owner_device() {
        return Err(DomainStateError {
            code: "forbidden",
            message:
                "Only your own computer or paired phone can run commands from a Needs you card."
                    .to_string(),
        });
    }
    let project_id = text(params, "projectId");
    let session_id = text(params, "sessionId");
    let op = text(params, "op");
    let item_ref = text(params, "item");
    let ask_tag = text(params, "ask");
    if project_id.is_empty() || session_id.is_empty() || item_ref.is_empty() {
        return Err(DomainStateError::bad_request(
            "projectId, sessionId, and item are required.",
        ));
    }

    if op == "stopCommand" {
        let row_key = resolve_target(
            db,
            repository,
            &project_id,
            &session_id,
            &item_ref,
            &ask_tag,
        )
        .map(|target| target.row_key())
        .unwrap_or_else(|_| text(params, "key"));
        if row_key.is_empty() {
            return Err(DomainStateError::bad_request("No running command found."));
        }
        let run_key = (project_id.clone(), session_id.clone(), row_key);
        let run_value = {
            let mut guard = runs()
                .lock()
                .map_err(|_| DomainStateError::corrupt_state("Command runner lock poisoned."))?;
            let Some(active) = guard.get_mut(&run_key) else {
                return Ok(json!({ "ok": true, "stopped": false }));
            };
            if active.is_active() {
                active.stopped_by_user = true;
                active.state = "stopped".to_string();
                active.hint_title.clear();
                active.hint.clear();
                active.error = "Stopped before it finished.".to_string();
                active.finished_at = Some(crate::coordinators::now_iso());
                if let Some(stop_tx) = active.stop_tx.take() {
                    let _ = stop_tx.send(());
                }
            }
            active.to_value()
        };
        crate::server::republish_session_chat(state, &project_id, &session_id);
        return Ok(json!({ "ok": true, "stopped": true, "run": run_value }));
    }

    let target = resolve_target(
        db,
        repository,
        &project_id,
        &session_id,
        &item_ref,
        &ask_tag,
    )?;
    let raw_command = target.command().trim().to_string();
    if raw_command.is_empty() {
        return Err(DomainStateError::bad_request(
            "This item does not carry a command to run.",
        ));
    }
    let Some(args) = parse_runnable_command(&raw_command) else {
        return Err(DomainStateError::bad_request(
            "Ghostex cannot run this command automatically. Copy and run it in your terminal instead.",
        ));
    };
    let clean_command = display_command(&raw_command);
    let row_key = target.row_key();
    let run_key = (project_id.clone(), session_id.clone(), row_key.clone());

    let (run_id, stop_rx, initial_value) = {
        let mut guard = runs()
            .lock()
            .map_err(|_| DomainStateError::corrupt_state("Command runner lock poisoned."))?;
        if let Some(existing) = guard.get(&run_key) {
            if existing.is_active() {
                return Ok(json!({
                    "ok": true,
                    "alreadyRunning": true,
                    "run": existing.to_value(),
                }));
            }
        }
        let run_id = next_run_id();
        let (stop_tx, stop_rx) = oneshot::channel();
        let active = ActiveRun {
            run_id,
            command: clean_command.clone(),
            state: "running".to_string(),
            hint_title: String::new(),
            hint: String::new(),
            output: String::new(),
            error: String::new(),
            exit_code: None,
            started_at: crate::coordinators::now_iso(),
            finished_at: None,
            stopped_by_user: false,
            stop_tx: Some(stop_tx),
        };
        let value = active.to_value();
        guard.insert(run_key.clone(), active);
        (run_id, stop_rx, value)
    };

    crate::server::republish_session_chat(state, &project_id, &session_id);

    let task_state = state.clone();
    tokio::spawn(async move {
        run_command_task(
            task_state, project_id, session_id, row_key, run_id, target, args, stop_rx,
        )
        .await;
    });

    Ok(json!({ "ok": true, "run": initial_value }))
}

fn build_ghostex_command(args: &[String]) -> Result<tokio::process::Command, String> {
    let exe = std::env::current_exe().map_err(|err| format!("Could not locate ghostex: {err}"))?;
    #[cfg(unix)]
    let mut std_cmd = {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg0("ghostex");
        cmd
    };
    #[cfg(windows)]
    let mut std_cmd = {
        let sibling = exe.with_file_name("ghostex.exe");
        if sibling.is_file() {
            std::process::Command::new(sibling)
        } else {
            std::process::Command::new("ghostex")
        }
    };

    std_cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb");

    for var in [
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_PROJECT_ID",
        "GHOSTEX_SESSION_TOKEN",
        "GHOSTEX_ACTOR_TOKEN",
        "GHOSTEX_AGENT_ID",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CODEX_THREAD_ID",
        "CODEX_SANDBOX",
        "GEMINI_CLI",
        "OPENCODE",
        "CURSOR_AGENT",
    ] {
        std_cmd.env_remove(var);
    }

    let mut cmd = tokio::process::Command::from(std_cmd);
    cmd.kill_on_drop(true);
    Ok(cmd)
}

#[allow(clippy::too_many_arguments)]
async fn run_command_task(
    state: AppState,
    project_id: String,
    session_id: String,
    row_key: String,
    run_id: u64,
    target: CommandTarget,
    args: Vec<String>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let run_key = (project_id.clone(), session_id.clone(), row_key.clone());

    let mut cmd = match build_ghostex_command(&args) {
        Ok(cmd) => cmd,
        Err(message) => {
            finish_run_failed(&state, &run_key, run_id, None, message);
            return;
        }
    };
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            finish_run_failed(
                &state,
                &run_key,
                run_id,
                None,
                format!("Could not start ghostex: {err}"),
            );
            return;
        }
    };

    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        spawn_pipe_reader(stdout, chunk_tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe_reader(stderr, chunk_tx.clone());
    }
    drop(chunk_tx);

    let mut last_publish = Instant::now();
    let mut stopped = false;
    let mut exit_status = None;

    loop {
        tokio::select! {
            _ = &mut stop_rx, if !stopped => {
                stopped = true;
                let _ = child.kill().await;
                let _ = child.wait().await;
                break;
            }
            maybe_chunk = chunk_rx.recv() => {
                match maybe_chunk {
                    Some(chunk) => {
                        let transitioned = append_output(&run_key, run_id, &chunk);
                        if transitioned || last_publish.elapsed() >= Duration::from_millis(PUBLISH_THROTTLE_MS) {
                            last_publish = Instant::now();
                            crate::server::republish_session_chat(&state, &project_id, &session_id);
                        }
                    }
                    None => {
                        exit_status = child.wait().await.ok();
                        break;
                    }
                }
            }
            status = child.wait() => {
                exit_status = status.ok();
                while let Ok(chunk) = chunk_rx.try_recv() {
                    let _ = append_output(&run_key, run_id, &chunk);
                }
                break;
            }
        }
    }

    if stopped || is_stopped_by_user(&run_key, run_id) {
        crate::server::republish_session_chat(&state, &project_id, &session_id);
        return;
    }

    let code = exit_status.and_then(|status| status.code());
    if exit_status.is_some_and(|status| status.success()) {
        if let Ok(mut guard) = runs().lock() {
            if let Some(active) = guard.get_mut(&run_key).filter(|r| r.run_id == run_id) {
                active.state = "succeeded".to_string();
                active.exit_code = Some(0);
                active.hint_title.clear();
                active.hint.clear();
                active.error.clear();
                active.finished_at = Some(crate::coordinators::now_iso());
                active.stop_tx = None;
            }
        }
        crate::server::republish_session_chat(&state, &project_id, &session_id);

        // Give the user a moment to see "Done automatically" before clearing the card and
        // handing Done back to the waiting agent.
        tokio::time::sleep(Duration::from_millis(AUTO_DONE_SETTLE_DELAY_MS)).await;
        auto_answer_done(&state, &project_id, &session_id, &row_key, run_id, &target).await;
    } else {
        let error = latest_error_line(&run_key, run_id).unwrap_or_else(|| match code {
            Some(code) => format!("Command exited with code {code}."),
            None => "Command ended before finishing.".to_string(),
        });
        finish_run_failed(&state, &run_key, run_id, code, error);
    }
}

fn spawn_pipe_reader<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    tx: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn append_output(run_key: &RunKey, run_id: u64, raw_chunk: &str) -> bool {
    let clean = strip_ansi(raw_chunk);
    if clean.is_empty() {
        return false;
    }
    let Ok(mut guard) = runs().lock() else {
        return false;
    };
    let Some(active) = guard.get_mut(run_key).filter(|r| r.run_id == run_id) else {
        return false;
    };
    if !active.is_active() {
        return false;
    }
    active.output.push_str(&clean);
    if active.output.len() > MAX_OUTPUT_BYTES {
        let keep_from = active.output.len() - MAX_OUTPUT_BYTES;
        let boundary = active
            .output
            .char_indices()
            .find(|(idx, _)| *idx >= keep_from)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        active.output.drain(..boundary);
    }
    let prev_state = active.state.clone();
    let prev_title = active.hint_title.clone();
    if let Some((title, hint)) = detect_waiting_hint(&active.output) {
        active.state = "waiting".to_string();
        active.hint_title = title.to_string();
        active.hint = hint.to_string();
    }
    active.state != prev_state || active.hint_title != prev_title
}

fn is_stopped_by_user(run_key: &RunKey, run_id: u64) -> bool {
    runs()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .get(run_key)
                .filter(|r| r.run_id == run_id)
                .map(|r| r.stopped_by_user)
        })
        .unwrap_or(false)
}

fn latest_error_line(run_key: &RunKey, run_id: u64) -> Option<String> {
    let guard = runs().lock().ok()?;
    let active = guard.get(run_key).filter(|r| r.run_id == run_id)?;
    active
        .output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(180).collect())
}

fn finish_run_failed(
    state: &AppState,
    run_key: &RunKey,
    run_id: u64,
    exit_code: Option<i32>,
    error: String,
) {
    if let Ok(mut guard) = runs().lock() {
        if let Some(active) = guard.get_mut(run_key).filter(|r| r.run_id == run_id) {
            if active.stopped_by_user {
                return;
            }
            active.state = "failed".to_string();
            active.exit_code = exit_code;
            active.hint_title.clear();
            active.hint.clear();
            active.error = error;
            active.finished_at = Some(crate::coordinators::now_iso());
            active.stop_tx = None;
        }
    }
    crate::server::republish_session_chat(state, &run_key.0, &run_key.1);
}

async fn auto_answer_done(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    row_key: &str,
    run_id: u64,
    target: &CommandTarget,
) {
    let (item_param, ask_param, op, chat_message) = match target {
        CommandTarget::SessionTodo { tag, .. } => {
            (tag.clone(), String::new(), "answer", format!("{tag}: Done"))
        }
        CommandTarget::WorkAsk {
            item_ref,
            ask_tag,
            offers_done,
            ..
        } => (
            item_ref.clone(),
            ask_tag.clone(),
            if *offers_done { "answer" } else { "reply" },
            format!("{item_ref} {ask_tag}: Done"),
        ),
    };

    let settled = {
        let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("op".to_string(), json!(op));
        params.insert("item".to_string(), json!(item_param));
        params.insert("answer".to_string(), json!("Done"));
        if !ask_param.is_empty() {
            params.insert("ask".to_string(), json!(ask_param));
        }
        match crate::server::handle_coordinator_http(
            state,
            "/api/coordinatorBoard",
            &db,
            &repository,
            &params,
        ) {
            Ok(_) => true,
            Err(_) => {
                // Already answered or dismissed by the user while the command was finishing.
                false
            }
        }
    };

    if !settled {
        return;
    }

    // Clear the tracked run once the card itself has settled.
    if let Ok(mut guard) = runs().lock() {
        let run_key = (
            project_id.to_string(),
            session_id.to_string(),
            row_key.to_string(),
        );
        if guard.get(&run_key).is_some_and(|r| r.run_id == run_id) {
            guard.remove(&run_key);
        }
    }

    // Notify the waiting session the same way clicking Done on the card does.
    crate::session_chat_send::sender_mark::note_user_send(
        &state.paths,
        project_id,
        session_id,
        &chat_message,
    );
    let send_request_id = format!("needs-you-run:{session_id}:{row_key}:{run_id}");
    let body = json!({
        "params": {
            "projectId": project_id,
            "sessionId": session_id,
            "text": chat_message,
            "sendRequestId": send_request_id,
        }
    });
    let _ = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        send_request_id,
        &body,
    )
    .await;
}

/// Detects common user-action pauses from command output (Touch ID / password, browser sign-in,
/// pairing / approval prompts) so the card tells the user what is waiting.
fn detect_waiting_hint(output: &str) -> Option<(&'static str, &'static str)> {
    let lower = output.to_ascii_lowercase();
    if lower.contains("touch id")
        || lower.contains("fingerprint")
        || lower.contains("windows hello")
        || lower.contains("keychain")
        || lower.contains("sudo")
        || lower.contains("password:")
        || lower.contains("enter password")
    {
        return Some((
            "Waiting for Touch ID or your password",
            "Use Touch ID or enter your password on this computer to approve. Ghostex will finish automatically once you approve.",
        ));
    }
    if lower.contains("approve")
        || lower.contains("pairing code")
        || lower.contains("confirmation words")
        || lower.contains("computer code")
        || lower.contains("scan the qr")
        || lower.contains("type repair")
    {
        return Some((
            "Waiting for your approval",
            "Approve the prompt or confirm the pairing code on this computer. Ghostex will finish automatically once you approve.",
        ));
    }
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("open your browser")
        || lower.contains("browser window")
        || lower.contains("sign in")
        || lower.contains("device code")
        || lower.contains("authorization code")
        || lower.contains("oauth")
    {
        return Some((
            "Waiting for sign-in in your browser",
            "Finish signing in on the browser window that opened on this computer. Ghostex will finish automatically once you're signed in.",
        ));
    }
    None
}

/// Strips ANSI CSI/OSC escape sequences so command output reads cleanly in the card.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\x07' {
                            break;
                        }
                        if next == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        if ch == '\r' {
            continue;
        }
        out.push(ch);
    }
    out
}
