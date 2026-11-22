mod prompt;
mod support;
pub use prompt::prompt_editor_command;
use prompt::*;
use support::*;

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::launchers;
use crate::ghostex_cli::rpc::{
    self, call_gxserver_rpc, unsupported_action_error, CliError, CliResult,
};
use crate::logging::read_routine_diagnostic_enabled;

/*
CDXC:Cli 2026-07-13:
Faithful port of the prompt-editor / floating-editor / GhostexEditor daemon
surface of scripts/ghostex-cli.mjs (lines 3231-4360). The standalone
GhostexEditor daemon is an existing separate app; this CLI is a client that
speaks newline-delimited JSON over a unix domain socket, so the message
sequencing (open → opened → closed, warm/status/shutdown requests, retitle
notifications, save-and-close on SIGTERM/SIGINT) must match the Node CLI
byte-for-byte where the daemon or EDITOR callers can observe it.
*/

#[cfg(unix)]
type DaemonStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type DaemonStream = ghostex_editor_client::PipeStream;

// ---------------------------------------------------------------------------
// Error carrier matching the JS Error shape (name / message / code).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct EditorError {
    /// JS error.name: "Error" or "GhostexEditorUnavailableError".
    name: &'static str,
    message: String,
    /// Node system error code ("ECONNREFUSED", "ENOENT", ...).
    code: Option<&'static str>,
}

impl EditorError {
    fn new(message: impl Into<String>) -> Self {
        EditorError {
            name: "Error",
            message: message.into(),
            code: None,
        }
    }

    /// class GhostexEditorUnavailableError extends Error.
    fn unavailable(message: impl Into<String>) -> Self {
        EditorError {
            name: "GhostexEditorUnavailableError",
            message: message.into(),
            code: None,
        }
    }

    fn from_io(error: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        let code = match error.kind() {
            ErrorKind::NotFound => Some("ENOENT"),
            ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => Some("ECONNRESET"),
            ErrorKind::BrokenPipe => Some("EPIPE"),
            _ => None,
        };
        EditorError {
            name: "Error",
            message: error.to_string(),
            code,
        }
    }

    fn is_unavailable(&self) -> bool {
        self.name == "GhostexEditorUnavailableError"
    }
}

/// isGhostexEditorConnectionError.
fn is_ghostex_editor_connection_error(error: &EditorError) -> bool {
    matches!(
        error.code,
        Some("ECONNREFUSED" | "ENOENT" | "ENOTFOUND" | "ECONNRESET" | "EPIPE")
    )
}

// ---------------------------------------------------------------------------
// Small JS-shaped helpers.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Prompt-editor selection (lines 3231-3510).
// ---------------------------------------------------------------------------

pub fn floating_editor_command(args: &[String]) -> CliResult<()> {
    let _ = args;
    /*
    The Node command throws GxserverCliUnsupportedError("floating-editor") on
    its first line; the bridge-based body below it is unreachable. The
    still-referenced helpers are preserved in floating_editor_bridge_parity.
    */
    Err(unsupported_action_error("floating-editor"))
}

// ---------------------------------------------------------------------------

pub fn floating_monaco_editor_command(args: &[String]) -> CliResult<()> {
    floating_monaco_editor_command_with_trace(args, &json!({})).map(|_| ())
}

fn floating_monaco_editor_command_with_trace(
    args: &[String],
    trace: &Value,
) -> CliResult<Option<String>> {
    /*
    CDXC:PromptEditor 2026-07-05 (ported): Monaco prompt editing is served by
    the resident GhostexEditor daemon. Keep the EDITOR-facing status-file
    crash semantics and timeline breadcrumbs, while opening each prompt
    through the pinned JSON-line socket protocol.
    */
    let parsed = parse_args(args);
    let Some(file_path) = parsed.rest.iter().find(|arg| !arg.trim().is_empty()) else {
        return Err(CliError::Other(
            "Usage: ghostex floating-monaco-editor <file>".to_string(),
        ));
    };

    let cwd = js_path_resolve(&parsed.flags.text("cwd").unwrap_or_else(current_dir_string));
    let command_started_at = Instant::now();
    let request_id = format!(
        "floating-monaco-editor-{}-{}",
        to_base36(now_millis()),
        random_base36(6)
    );
    let work_dir = mkdtemp("ghostex-floating-monaco-editor-")?;
    let status_file = work_dir.join("status").to_string_lossy().into_owned();
    let resolved_file_path = js_path_resolve_from(&cwd, file_path)
        .to_string_lossy()
        .into_owned();
    let originating_session_id = prompt_editor_originating_session_id_from_environment();
    let input_byte_count = std::fs::metadata(&resolved_file_path)
        .map(|meta| json!(meta.len()))
        .unwrap_or(Value::Null);

    let mut prepared = trace.as_object().cloned().unwrap_or_default();
    prepared.insert(
        "commandElapsedMs".to_string(),
        json!(elapsed_ms(command_started_at)),
    );
    prepared.insert("hasInputFile".to_string(), Value::Bool(true));
    prepared.insert(
        "hasOriginatingSessionId".to_string(),
        Value::Bool(originating_session_id.is_some()),
    );
    prepared.insert("inputByteCount".to_string(), input_byte_count);
    prepared.insert("requestId".to_string(), json!(request_id));
    append_prompt_editor_timeline_log("cli.monaco.requestPrepared", Value::Object(prepared));

    append_floating_editor_log(json!({
        "cwd": cwd.to_string_lossy(),
        "event": "cli.monaco_request",
        "filePath": resolved_file_path,
        "originatingSessionId": originating_session_id.clone().unwrap_or_default(),
        "requestId": request_id,
        "statusFile": status_file,
    }));

    let outcome = run_monaco_daemon_session(
        &request_id,
        &resolved_file_path,
        &status_file,
        originating_session_id.as_deref(),
        command_started_at,
    );

    let result = match outcome {
        Ok(status) => Ok(Some(status)),
        Err(error) => {
            append_prompt_editor_timeline_log(
                "cli.monaco.failed",
                json!({
                    "errorName": error.name,
                    "requestId": request_id,
                    "totalDurationMs": elapsed_ms(command_started_at),
                }),
            );
            append_floating_editor_log(json!({
                "error": error.message,
                "event": "cli.monaco_machine_editor",
                "filePath": resolved_file_path,
                "requestId": request_id,
            }));
            eprintln!(
                "{}",
                ghostex_editor_unavailable_message(Some(&error.message))
            );
            let editor_command = machine_prompt_editor_command_from_environment();
            run_editor_inline(
                &machine_editor_args(&editor_command, &resolved_file_path),
                &cwd,
            )
            .map(|_| None)
        }
    };

    // finally: keep the temp dir only for --keep-temp true (flags.keepTemp
    // !== true && !== "true" in the JS).
    let keep_temp = match parsed.flags.0.get("keepTemp") {
        Some(FlagValue::Bool(true)) => true,
        Some(FlagValue::Text(text)) => text == "true",
        _ => false,
    };
    if !keep_temp {
        let _ = std::fs::remove_dir_all(&work_dir);
    }
    result
}

fn run_monaco_daemon_session(
    request_id: &str,
    resolved_file_path: &str,
    status_file: &str,
    originating_session_id: Option<&str>,
    command_started_at: Instant,
) -> Result<String, EditorError> {
    let socket_path = resolve_ghostex_editor_socket_path().map_err(EditorError::new)?;
    let daemon_started_at = Instant::now();
    let daemon = connect_or_start_ghostex_editor_daemon(&socket_path)?;
    let mut client = EditorDaemonClient::new(daemon.stream)?;
    append_prompt_editor_timeline_log(
        "cli.monaco.editorResolved",
        json!({
            "daemonAlreadyRunning": !daemon.launched,
            "daemonLaunchDurationMs": elapsed_ms(daemon_started_at),
            "requestId": request_id,
            "resolveDurationMs": elapsed_ms(command_started_at),
        }),
    );

    // process.on("SIGTERM"/"SIGINT", requestSaveAndClose); restored on drop.
    let _signal_guard = SaveCloseSignalGuard::install();

    let open_started_at = Instant::now();
    client.send(&json!({
        "filePath": resolved_file_path,
        "language": "markdown",
        "originatingSessionId": originating_session_id,
        "requestId": request_id,
        "statusFile": status_file,
        "title": "Prompt Editor",
        "type": "open",
        "v": 1,
    }))?;
    client.arm_signal_save_close(request_id);
    schedule_prompt_editor_tab_retitle(&client, request_id, originating_session_id);

    wait_for_ghostex_editor_daemon_message(
        &mut client,
        &|message| message["type"] == "opened" && message["requestId"] == request_id,
        "opened",
        15_000,
    )?;

    let status = wait_for_ghostex_editor_closed_status(&mut client, request_id, status_file)?;
    append_prompt_editor_timeline_log(
        "cli.monaco.statusResolved",
        json!({
            "finalStatus": status,
            "requestId": request_id,
            "totalDurationMs": elapsed_ms(command_started_at),
            "waitDurationMs": elapsed_ms(open_started_at),
        }),
    );
    focus_prompt_editor_originating_session(originating_session_id, request_id);
    if status == "saved" {
        stash_saved_prompt_editor_content(resolved_file_path, originating_session_id, request_id);
    }
    crate::ghostex_cli::set_exit_code(if status == "saved" { 0 } else { 1 });
    Ok(status)
}

fn stash_saved_prompt_editor_content(
    resolved_file_path: &str,
    originating_session_id: Option<&str>,
    request_id: &str,
) {
    /*
    CDXC:SavedPrompts 2026-07-29-00:00:
    Every prompt-editor save-and-close stashes the composed text in gxserver so
    it can be recalled from the Prompts modal later. Best-effort: an
    unreachable gxserver must not change the editor's exit status, and the
    prompt body itself goes only through the authenticated RPC, never into the
    timeline logs.
    */
    let content = match std::fs::read_to_string(resolved_file_path) {
        Ok(content) => content,
        Err(_) => return,
    };
    if content.trim().is_empty() {
        return;
    }
    let content_chars = content.chars().count();
    match stash_prompt_content_via_gxserver(&content, originating_session_id, false) {
        Ok(_) => append_prompt_editor_timeline_log(
            "cli.monaco.promptStashed",
            json!({ "contentChars": content_chars, "ok": true, "requestId": request_id }),
        ),
        Err(error) => append_prompt_editor_timeline_log(
            "cli.monaco.promptStashed",
            json!({
                "contentChars": content_chars,
                "errorName": error_name_for_cli_error(&error),
                "ok": false,
                "requestId": request_id,
            }),
        ),
    }
}

fn stash_prompt_content_via_gxserver(
    content: &str,
    originating_session_id: Option<&str>,
    draft_handoff: bool,
) -> Result<Value, CliError> {
    let mut params = json!({ "content": content, "draftHandoff": draft_handoff });
    let parts: Vec<&str> = originating_session_id.unwrap_or("").split(':').collect();
    if parts.len() == 2
        && matches_session_ref_part(parts[0], b'P', 3)
        && matches_session_ref_part(parts[1], b'G', 3)
    {
        params["projectId"] = json!(parts[0]);
        params["sessionId"] = json!(parts[1]);
    }
    if let Ok(cwd) = std::env::current_dir() {
        params["cwd"] = json!(cwd.to_string_lossy());
    }
    let mut flags = Flags::default();
    flags.insert_text("timeoutMs", "3000");
    call_gxserver_rpc("/api/saveStashedPrompt", &params, &flags)
}

/// A stash request older than this is an orphan from a Ctrl+G the agent CLI
/// never answered (plain shell pane, editor already open); the next real
/// prompt-editor invocation must not be silently swallowed by it.
const PROMPT_STASH_REQUEST_FRESHNESS: Duration = Duration::from_secs(15);
const PROMPT_HANDOFF_MARKER_PREFIX: &str = "handoff:";
/// A marker line asking for the moved prompt to be handed back as a lone newline instead of an
/// empty file (session_chat_send/draft_capture.rs, OpenCode without its plugin).
const PROMPT_HANDOFF_EMPTY_AS_NEWLINE: &str = "empty:newline";

fn valid_prompt_handoff_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// The folder gxserver keeps this session's handshake marker and answer in: the one the session
/// names (CDXC:PromptEditor 2026-11-03 in server/src/zmx/env.rs), or for a session started before
/// it named one, the state folder this environment resolves, as before.
fn prompt_handshake_dir() -> PathBuf {
    prompt_handshake_dir_with(&|key| std::env::var(key).ok(), rpc::ghostex_home)
}

fn prompt_handshake_dir_with(
    env: &dyn Fn(&str) -> Option<String>,
    own_state_dir: impl FnOnce() -> PathBuf,
) -> PathBuf {
    env(crate::zmx::PROMPT_HANDSHAKE_DIR_ENV)
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(own_state_dir)
}

fn prompt_handoff_response_path(request_id: &str) -> Option<PathBuf> {
    valid_prompt_handoff_request_id(request_id).then(|| {
        prompt_handshake_dir()
            .join("prompt-handoffs")
            .join(format!("{request_id}.json"))
    })
}

fn write_prompt_handoff_response(request_id: &str, response: &Value) {
    let Some(path) = prompt_handoff_response_path(request_id) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_ok() {
        let _ = std::fs::write(path, response.to_string());
    }
}

fn prompt_stash_request_marker_path(originating_session_id: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = originating_session_id.split(':').collect();
    if parts.len() != 2
        || !matches_session_ref_part(parts[0], b'P', 3)
        || !matches_session_ref_part(parts[1], b'G', 3)
    {
        return None;
    }
    Some(
        prompt_handshake_dir()
            .join("prompt-stash-requests")
            .join(format!("{}-{}", parts[0], parts[1])),
    )
}

fn consume_prompt_stash_request(
    originating_session_id: Option<&str>,
    resolved_file_path: &str,
) -> bool {
    let Some(marker_path) = originating_session_id.and_then(prompt_stash_request_marker_path)
    else {
        return false;
    };
    let Ok(metadata) = std::fs::metadata(&marker_path) else {
        return false;
    };
    let fresh = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age <= PROMPT_STASH_REQUEST_FRESHNESS);
    let marker = std::fs::read_to_string(&marker_path).unwrap_or_default();
    let empty_as_newline = marker
        .lines()
        .any(|line| line.trim() == PROMPT_HANDOFF_EMPTY_AS_NEWLINE);
    let handoff_request_id = marker
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .strip_prefix(PROMPT_HANDOFF_MARKER_PREFIX)
        .filter(|value| valid_prompt_handoff_request_id(value))
        .map(str::to_string);
    let _ = std::fs::remove_file(&marker_path);
    if !fresh {
        return false;
    }
    let content = std::fs::read_to_string(resolved_file_path).unwrap_or_default();
    if content.trim().is_empty() {
        if let Some(request_id) = handoff_request_id.as_deref() {
            write_prompt_handoff_response(request_id, &json!({ "empty": true, "ok": true }));
        }
        append_prompt_editor_timeline_log("cli.stashRequest.skippedEmpty", json!({ "ok": true }));
        return true;
    }
    let content_chars = content.chars().count();
    match stash_prompt_content_via_gxserver(
        &content,
        originating_session_id,
        handoff_request_id.is_some(),
    ) {
        Ok(result) => {
            // Clear the composer only after the stash is durable; on failure
            // the file (and therefore the composer) stays untouched.
            let _ = std::fs::write(
                resolved_file_path,
                if empty_as_newline { &b"\n"[..] } else { b"" },
            );
            if let Some(request_id) = handoff_request_id.as_deref() {
                write_prompt_handoff_response(
                    request_id,
                    &json!({
                        "content": content,
                        "draftVersion": result.get("draftVersion"),
                        "created": result.get("created").and_then(Value::as_bool).unwrap_or(false),
                        "ok": true,
                        "promptId": result
                            .get("prompt")
                            .and_then(|prompt| prompt.get("promptId"))
                            .and_then(Value::as_str),
                    }),
                );
            }
            append_prompt_editor_timeline_log(
                "cli.stashRequest.stashed",
                json!({ "contentChars": content_chars, "ok": true }),
            );
        }
        Err(error) => {
            if let Some(request_id) = handoff_request_id.as_deref() {
                write_prompt_handoff_response(request_id, &json!({ "ok": false }));
            }
            append_prompt_editor_timeline_log(
                "cli.stashRequest.stashed",
                json!({
                    "contentChars": content_chars,
                    "errorName": error_name_for_cli_error(&error),
                    "ok": false,
                }),
            );
        }
    }
    true
}

fn schedule_prompt_editor_tab_retitle(
    client: &EditorDaemonClient,
    request_id: &str,
    originating_session_id: Option<&str>,
) {
    /*
    CDXC:PromptEditor 2026-07-06 (ported): the gxserver lookup starts after
    `open` is already sent so tab naming never delays window presentation, and
    `retitle` is a no-reply notification the daemon applies silently.
    */
    let Some(originating_session_id) = originating_session_id else {
        return;
    };
    let writer = client.writer.clone();
    let request_id = request_id.to_string();
    let originating_session_id = originating_session_id.to_string();
    std::thread::spawn(move || {
        let mut flags = Flags::default();
        flags.insert_text("timeout", "5000");
        let Ok(response) = call_gxserver_rpc("/api/readPresentationSnapshot", &json!({}), &flags)
        else {
            // gxserver being unreachable only affects tab naming.
            return;
        };
        let sessions = response
            .get("snapshot")
            .and_then(|snapshot| snapshot.get("sessions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let title = sessions
            .iter()
            .find(|session| {
                cli_session_key(session.get("projectId"), session.get("sessionId"))
                    == originating_session_id
            })
            .map(|row| js_string_or_empty(row.get("displayTitle")))
            .unwrap_or_default()
            .trim()
            .to_string();
        if title.is_empty() {
            return;
        }
        let line = format!(
            "{}\n",
            json!({ "requestId": request_id, "title": title, "type": "retitle", "v": 1 })
        );
        if let Ok(mut stream) = writer.lock() {
            let _ = stream.write_all(line.as_bytes());
        }
    });
}

/// cliSessionKey(projectId, sessionId) private copy.
fn cli_session_key(project_id: Option<&Value>, session_id: Option<&Value>) -> String {
    let normalized_project_id = js_string_or_empty(project_id).trim().to_string();
    let normalized_session_id = js_string_or_empty(session_id).trim().to_string();
    if !normalized_project_id.is_empty() && !normalized_session_id.is_empty() {
        format!("{normalized_project_id}:{normalized_session_id}")
    } else {
        String::new()
    }
}

fn focus_prompt_editor_originating_session(originating_session_id: Option<&str>, request_id: &str) {
    /*
    CDXC:PromptEditor 2026-07-05 (ported): best-effort — a missing ref or
    unreachable gxserver must not change the editor's exit status.
    */
    let parts: Vec<&str> = originating_session_id.unwrap_or("").split(':').collect();
    if parts.len() != 2
        || !matches_session_ref_part(parts[0], b'P', 3)
        || !matches_session_ref_part(parts[1], b'G', 3)
    {
        return;
    }
    let mut flags = Flags::default();
    flags.insert_text("timeoutMs", "3000");
    match call_gxserver_rpc(
        "/api/focusSession",
        &json!({ "projectId": parts[0], "sessionId": parts[1] }),
        &flags,
    ) {
        Ok(_) => append_prompt_editor_timeline_log(
            "cli.monaco.returnFocusRequested",
            json!({ "ok": true, "requestId": request_id }),
        ),
        Err(error) => append_prompt_editor_timeline_log(
            "cli.monaco.returnFocusRequested",
            json!({
                "errorName": error_name_for_cli_error(&error),
                "ok": false,
                "requestId": request_id,
            }),
        ),
    }
}

fn error_name_for_cli_error(error: &CliError) -> &'static str {
    match error {
        CliError::Connection(_) => "GxserverCliConnectionError",
        CliError::Rpc { .. } => "GxserverCliRpcError",
        CliError::Other(_) => "Error",
    }
}

// ---------------------------------------------------------------------------
// editor-daemon command and the daemon socket client (lines 3713-4360).
// ---------------------------------------------------------------------------

pub fn editor_daemon_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let action = parsed.rest.first().map(String::as_str).unwrap_or("");
    if !["ensure", "status", "warm", "shutdown"].contains(&action) {
        return Err(CliError::Other(
            "Usage: ghostex editor-daemon <ensure|status|warm|shutdown>".to_string(),
        ));
    }

    let socket_path = resolve_ghostex_editor_socket_path().map_err(CliError::Other)?;
    let outcome: Result<(), EditorError> = (|| {
        if action == "ensure" {
            let daemon = connect_or_start_ghostex_editor_daemon(&socket_path)?;
            let mut client = EditorDaemonClient::new(daemon.stream)?;
            let warmed = send_ghostex_editor_daemon_request(
                &mut client,
                &json!({ "type": "warm", "v": 1 }),
                "warmed",
                10_000,
            )?;
            println!(
                "Ghostex editor daemon warm: {}",
                serde_json::to_string(&warmed).unwrap_or_else(|_| "null".to_string())
            );
            return Ok(());
        }

        let stream = connect_ghostex_editor_daemon(&socket_path, 750)?;
        let mut client = EditorDaemonClient::new(stream)?;
        let expected_type = match action {
            "shutdown" => "ok",
            "warm" => "warmed",
            other => other,
        };
        let reply = send_ghostex_editor_daemon_request(
            &mut client,
            &json!({ "type": action, "v": 1 }),
            expected_type,
            10_000,
        )?;
        println!(
            "{}",
            serde_json::to_string(&reply).unwrap_or_else(|_| "null".to_string())
        );
        Ok(())
    })();

    match outcome {
        Ok(()) => Ok(()),
        Err(error) if error.is_unavailable() || is_ghostex_editor_connection_error(&error) => {
            eprintln!(
                "Ghostex editor daemon unavailable; continuing without prewarm. {}",
                error.message
            );
            Ok(())
        }
        Err(error) => Err(CliError::Other(error.message)),
    }
}

fn resolve_ghostex_editor_socket_path() -> Result<String, String> {
    if let Some(override_path) = normalized_env("GHOSTEX_EDITOR_SOCKET") {
        if cfg!(not(windows)) && !Path::new(&override_path).is_absolute() {
            return Err("GHOSTEX_EDITOR_SOCKET must be an absolute path.".to_string());
        }
        return Ok(override_path);
    }
    #[cfg(windows)]
    return Ok(ghostex_editor_client::default_pipe_path());
    #[cfg(not(windows))]
    Ok(rpc::ghostex_runtime_home()
        .join("ghostex-editor.sock")
        .to_string_lossy()
        .into_owned())
}

fn ensure_ghostex_editor_socket_directory(socket_path: &str) -> Result<(), EditorError> {
    if cfg!(windows) || socket_path.starts_with("\\\\.\\") {
        return Ok(());
    }
    let Some(parent) = Path::new(socket_path).parent() else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|error| EditorError::from_io(&error))
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(parent).map_err(|error| EditorError::from_io(&error))
    }
}

struct EditorDaemonConnection {
    launched: bool,
    stream: DaemonStream,
}

fn connect_or_start_ghostex_editor_daemon(
    socket_path: &str,
) -> Result<EditorDaemonConnection, EditorError> {
    ensure_ghostex_editor_socket_directory(socket_path)?;
    if let Ok(stream) = connect_ghostex_editor_daemon(socket_path, 500) {
        return Ok(EditorDaemonConnection {
            launched: false,
            stream,
        });
    }

    let Some(editor_executable) = resolve_ghostex_editor_executable() else {
        return Err(EditorError::unavailable(
            "Could not find an executable GhostexEditor daemon. Set GHOSTEX_EDITOR_APP or install GhostexEditor.",
        ));
    };

    spawn_ghostex_editor_daemon(&editor_executable);

    let started_at = Instant::now();
    let mut last_error: Option<EditorError> = None;
    while started_at.elapsed() < Duration::from_millis(5_000) {
        match connect_ghostex_editor_daemon(socket_path, 250) {
            Ok(stream) => {
                return Ok(EditorDaemonConnection {
                    launched: true,
                    stream,
                });
            }
            Err(error) => last_error = Some(error),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(EditorError::new(format!(
        "Timed out connecting to GhostexEditor daemon at {socket_path}: {}",
        last_error
            .map(|error| error.message)
            .unwrap_or_else(|| "unknown error".to_string())
    )))
}

/// spawn(editorExecutable, ["--daemon"], { detached: true, stdio: "ignore" }).
fn spawn_ghostex_editor_daemon(executable: &str) {
    let mut command = Command::new(executable);
    command
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    // Its own hidden console, so it outlives the terminal that ran the CLI without opening a window.
    #[cfg(windows)]
    {
        use crate::platform::process::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    // child.once("error", () => undefined); child.unref();
    let _ = command.spawn();
}

#[cfg(unix)]
fn connect_ghostex_editor_daemon(
    socket_path: &str,
    _timeout_ms: u64,
) -> Result<DaemonStream, EditorError> {
    // Unix socket connects fail immediately when no daemon is listening; the
    // Node-side connect timeout only guards a pathological hang.
    DaemonStream::connect(socket_path).map_err(|error| EditorError::from_io(&error))
}

#[cfg(windows)]
fn connect_ghostex_editor_daemon(
    socket_path: &str,
    timeout_ms: u64,
) -> Result<DaemonStream, EditorError> {
    DaemonStream::connect(socket_path, Duration::from_millis(timeout_ms))
        .map_err(|error| EditorError::from_io(&error))
}

// -- Save-and-close signal forwarding (process.on SIGTERM/SIGINT) -----------

#[cfg(unix)]
static SIGNAL_SAVE_CLOSE_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn handle_prompt_editor_signal(_signal: libc::c_int) {
    SIGNAL_SAVE_CLOSE_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
}

struct SaveCloseSignalGuard;

impl SaveCloseSignalGuard {
    fn install() -> Self {
        #[cfg(unix)]
        unsafe {
            SIGNAL_SAVE_CLOSE_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
            let handler = handle_prompt_editor_signal as extern "C" fn(libc::c_int);
            libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
            libc::signal(libc::SIGINT, handler as libc::sighandler_t);
        }
        SaveCloseSignalGuard
    }
}

impl Drop for SaveCloseSignalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            SIGNAL_SAVE_CLOSE_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

// -- Newline-delimited JSON daemon client (createGhostexEditorDaemonClient) --

#[cfg(unix)]
struct SignalCloseRequest {
    request_id: String,
    sent: bool,
}

struct EditorDaemonClient {
    reader: DaemonStream,
    writer: Arc<Mutex<DaemonStream>>,
    buffer: Vec<u8>,
    pending: Vec<Value>,
    closed: bool,
    close_error: Option<EditorError>,
    #[cfg(unix)]
    signal_close: Option<SignalCloseRequest>,
}

impl EditorDaemonClient {
    fn new(stream: DaemonStream) -> Result<Self, EditorError> {
        let writer = stream
            .try_clone()
            .map_err(|error| EditorError::from_io(&error))?;
        Ok(EditorDaemonClient {
            reader: stream,
            writer: Arc::new(Mutex::new(writer)),
            buffer: Vec::new(),
            pending: Vec::new(),
            closed: false,
            close_error: None,
            #[cfg(unix)]
            signal_close: None,
        })
    }

    /// client.send(message): one JSON message per line.
    fn send(&self, message: &Value) -> Result<(), EditorError> {
        let line = format!("{message}\n");
        let mut stream = self
            .writer
            .lock()
            .map_err(|_| EditorError::new("GhostexEditor daemon socket closed."))?;
        stream
            .write_all(line.as_bytes())
            .map_err(|error| EditorError::from_io(&error))
    }

    /// Arm the SIGTERM/SIGINT save-and-close notification for this request
    /// (requestSaveAndClose becomes reachable once `open` was sent).
    #[cfg(unix)]
    fn arm_signal_save_close(&mut self, request_id: &str) {
        self.signal_close = Some(SignalCloseRequest {
            request_id: request_id.to_string(),
            sent: false,
        });
    }

    #[cfg(not(unix))]
    fn arm_signal_save_close(&mut self, _request_id: &str) {}

    fn poll_signal_save_close(&mut self) {
        #[cfg(unix)]
        {
            if !SIGNAL_SAVE_CLOSE_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let pending_request_id = match self.signal_close.as_mut() {
                Some(request) if !request.sent => {
                    request.sent = true;
                    Some(request.request_id.clone())
                }
                _ => None,
            };
            if let Some(request_id) = pending_request_id {
                let _ = self.send(&json!({
                    "action": "save",
                    "requestId": request_id,
                    "type": "close",
                    "v": 1,
                }));
            }
        }
    }

    /// client.waitForMessage(predicate, timeoutMs): earlier unmatched
    /// messages stay buffered for later waiters; timeoutMs 0 waits forever.
    fn wait_for_message(
        &mut self,
        predicate: &dyn Fn(&Value) -> bool,
        timeout_ms: u64,
    ) -> Result<Value, EditorError> {
        if let Some(index) = self.pending.iter().position(predicate) {
            return Ok(self.pending.remove(index));
        }
        if self.closed {
            return Err(self
                .close_error
                .clone()
                .unwrap_or_else(|| EditorError::new("GhostexEditor daemon socket closed.")));
        }
        let deadline = if timeout_ms > 0 {
            Some(Instant::now() + Duration::from_millis(timeout_ms))
        } else {
            None
        };
        let _ = self
            .reader
            .set_read_timeout(Some(Duration::from_millis(100)));
        let mut chunk = [0u8; 4096];
        loop {
            self.poll_signal_save_close();
            match self.reader.read(&mut chunk) {
                Ok(0) => self.closed = true,
                Ok(read_bytes) => {
                    self.buffer.extend_from_slice(&chunk[..read_bytes]);
                    self.drain_buffered_lines();
                    if let Some(index) = self.pending.iter().position(predicate) {
                        return Ok(self.pending.remove(index));
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => {
                    self.close_error = Some(EditorError::from_io(&error));
                    self.closed = true;
                }
            }
            if self.closed {
                if let Some(index) = self.pending.iter().position(predicate) {
                    return Ok(self.pending.remove(index));
                }
                return Err(self
                    .close_error
                    .clone()
                    .unwrap_or_else(|| EditorError::new("GhostexEditor daemon socket closed.")));
            }
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(EditorError::new(
                        "Timed out waiting for GhostexEditor daemon response.",
                    ));
                }
            }
        }
    }

    fn drain_buffered_lines(&mut self) {
        while let Some(newline_index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=newline_index).collect();
            let line = String::from_utf8_lossy(&line_bytes[..newline_index])
                .trim()
                .to_string();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(message) => self.pending.push(message),
                Err(error) => self.pending.push(json!({
                    "message": error.to_string(),
                    "type": "error",
                    "v": 1,
                })),
            }
        }
    }

    /// closeSocket(socket) — socket.destroy().
    fn close(&self) {
        #[cfg(unix)]
        let _ = self.reader.shutdown(std::net::Shutdown::Both);
    }
}

impl Drop for EditorDaemonClient {
    fn drop(&mut self) {
        self.close();
    }
}

fn send_ghostex_editor_daemon_request(
    client: &mut EditorDaemonClient,
    request: &Value,
    expected_type: &str,
    timeout_ms: u64,
) -> Result<Value, EditorError> {
    client.send(request)?;
    wait_for_ghostex_editor_daemon_message(
        client,
        &|message| message["type"] == expected_type,
        expected_type,
        timeout_ms,
    )
}

fn wait_for_ghostex_editor_daemon_message(
    client: &mut EditorDaemonClient,
    predicate: &dyn Fn(&Value) -> bool,
    description: &str,
    timeout_ms: u64,
) -> Result<Value, EditorError> {
    let message = client.wait_for_message(
        &|candidate| candidate["type"] == "error" || predicate(candidate),
        timeout_ms,
    )?;
    if message["type"] == "error" {
        let text = message
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Err(EditorError::new(if text.is_empty() {
            format!("GhostexEditor daemon returned an error while waiting for {description}.")
        } else {
            text
        }));
    }
    Ok(message)
}

fn wait_for_ghostex_editor_closed_status(
    client: &mut EditorDaemonClient,
    request_id: &str,
    status_file: &str,
) -> Result<String, EditorError> {
    let attempt = (|| -> Result<String, EditorError> {
        let message = wait_for_ghostex_editor_daemon_message(
            client,
            &|candidate| candidate["type"] == "closed" && candidate["requestId"] == request_id,
            "closed",
            0,
        )?;
        match message.get("status").and_then(Value::as_str) {
            Some("saved") => Ok("saved".to_string()),
            Some("cancelled") => Ok("cancelled".to_string()),
            _ => Err(EditorError::new(format!(
                "GhostexEditor daemon returned unknown close status: {}",
                js_string_or_undefined(message.get("status"))
            ))),
        }
    })();
    match attempt {
        Ok(status) => Ok(status),
        Err(error) => {
            let text = std::fs::read_to_string(status_file).unwrap_or_default();
            let status = final_ghostex_editor_status_from_text(&text);
            if status != "unknown" {
                Ok(status.to_string())
            } else {
                Err(error)
            }
        }
    }
}

/// finalGhostexEditorStatusFromText: multiline ^saved$ / ^cancelled$. JS
/// multiline anchors treat both \r and \n as line terminators.
fn final_ghostex_editor_status_from_text(status: &str) -> &'static str {
    if status.split(['\r', '\n']).any(|line| line == "saved") {
        return "saved";
    }
    if status.split(['\r', '\n']).any(|line| line == "cancelled") {
        return "cancelled";
    }
    "unknown"
}

// ---------------------------------------------------------------------------
// GhostexEditor executable resolution.
// ---------------------------------------------------------------------------

fn current_platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    }
}

fn resolve_ghostex_editor_executable() -> Option<String> {
    let cli_dir = launchers::cli_dir();
    let repo_root = ghostex_cli_repo_root_from_cli_dir(&cli_dir);
    if let Some(app_override) = normalized_env("GHOSTEX_EDITOR_APP") {
        let executable = ghostex_editor_executable_candidate(&app_override)?;
        return if is_executable_file(&executable) {
            Some(executable)
        } else {
            None
        };
    }
    #[cfg(windows)]
    if let Some(executable) = std::env::current_exe()
        .ok()
        .and_then(|executable| ghostex_editor_client::bundled_executable(&executable))
        .or_else(ghostex_editor_client::installed_executable)
    {
        return Some(executable.to_string_lossy().into_owned());
    }
    let candidates = ghostex_editor_executable_candidates_for_platform(
        current_platform_name(),
        repo_root.as_deref(),
        &rpc::home_dir(),
        Some(cli_dir.as_path()),
    );
    for candidate in candidates {
        if let Some(executable) = ghostex_editor_executable_candidate(&candidate) {
            if is_executable_file(&executable) {
                return Some(executable);
            }
        }
    }
    None
}

/// Packaged macOS builds ship the standalone editor daemon inside the app at
/// `Contents/Resources/GhostexEditor.app`. The bundled `ghostex` binary lives
/// under the same `Resources` tree (`Resources/CLI/ghostex` and
/// `Resources/Web/gxserver/bin/ghostex`), so walk up to the enclosing
/// `Contents/Resources` directory instead of hardcoding an app name or an
/// install location. Returns nothing outside a macOS app bundle.
fn ghostex_editor_bundled_app_candidates(cli_dir: Option<&Path>) -> Vec<PathBuf> {
    let file_name = |path: Option<&Path>| {
        path.and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_string)
    };
    let mut candidates = Vec::new();
    let mut current = cli_dir;
    while let Some(dir) = current {
        if file_name(Some(dir)).as_deref() == Some("Resources")
            && file_name(dir.parent()).as_deref() == Some("Contents")
        {
            candidates.push(dir.join("GhostexEditor.app"));
        }
        current = dir.parent();
    }
    candidates
}

fn ghostex_editor_executable_candidates_for_platform(
    platform_name: &str,
    repo_root: Option<&Path>,
    home: &Path,
    cli_dir: Option<&Path>,
) -> Vec<String> {
    let path_string = |path: PathBuf| path.to_string_lossy().into_owned();
    if platform_name == "darwin" {
        let mut candidates: Vec<String> = ghostex_editor_bundled_app_candidates(cli_dir)
            .into_iter()
            .map(|app| path_string(app.join("Contents").join("MacOS").join("GhostexEditor")))
            .collect();
        candidates.extend([
            path_string(
                home.join("Applications")
                    .join("GhostexEditor.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("GhostexEditor"),
            ),
            "/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor".to_string(),
        ]);
        if let Some(repo_root) = repo_root {
            candidates.push(path_string(
                repo_root
                    .join("editor")
                    .join("dist")
                    .join("GhostexEditor.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("GhostexEditor"),
            ));
        }
        return candidates;
    }
    if platform_name == "win32" {
        let mut candidates = Vec::new();
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            if !local_app_data.is_empty() {
                candidates.push(path_string(
                    Path::new(&local_app_data)
                        .join("Ghostex")
                        .join("GhostexEditor")
                        .join("GhostexEditor.exe"),
                ));
            }
        }
        if let Some(repo_root) = repo_root {
            candidates.push(path_string(
                repo_root
                    .join("apps/editor")
                    .join("dist")
                    .join("desktop")
                    .join("GhostexEditor.exe"),
            ));
        }
        return candidates;
    }
    let mut candidates = vec![
        path_string(home.join(".local").join("bin").join("ghostex-editor")),
        "/usr/local/bin/ghostex-editor".to_string(),
    ];
    if let Some(repo_root) = repo_root {
        candidates.push(path_string(
            repo_root
                .join("editor")
                .join("dist")
                .join("desktop")
                .join("ghostex-editor"),
        ));
    }
    candidates
}

fn ghostex_editor_unavailable_message(detail: Option<&str>) -> String {
    let detail = detail
        .map(|message| format!(" {message}"))
        .unwrap_or_default();
    let installation = if cfg!(target_os = "macos") {
        "install /Applications/GhostexEditor.app"
    } else {
        "reinstall Ghostex with its bundled editor"
    };
    format!(
        "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or {installation}.{detail}"
    )
}

fn ghostex_editor_executable_candidate(candidate: &str) -> Option<String> {
    let value = candidate.trim();
    if value.is_empty() {
        return None;
    }
    let expanded = if value == "~" {
        rpc::home_dir().to_string_lossy().into_owned()
    } else if let Some(rest) = value.strip_prefix("~/") {
        rpc::home_dir().join(rest).to_string_lossy().into_owned()
    } else {
        value.to_string()
    };
    let resolved = js_path_resolve(&expanded);
    let resolved_string = resolved.to_string_lossy().into_owned();
    if resolved_string.ends_with(".app") {
        return Some(
            resolved
                .join("Contents")
                .join("MacOS")
                .join("GhostexEditor")
                .to_string_lossy()
                .into_owned(),
        );
    }
    Some(resolved_string)
}

fn ghostex_cli_repo_root_from_cli_dir(cli_dir: &Path) -> Option<PathBuf> {
    let mut current = js_path_resolve(&cli_dir.to_string_lossy());
    loop {
        let marker = if cfg!(windows) {
            "apps/editor/desktop/Cargo.toml"
        } else {
            "scripts/ghostex-cli.mjs"
        };
        if file_exists(&current.join(marker)) {
            return Some(current);
        }
        let parent = current.parent().map(Path::to_path_buf)?;
        if parent == current {
            return None;
        }
        current = parent;
    }
}

// ---------------------------------------------------------------------------
// Inline editor execution.
// ---------------------------------------------------------------------------

fn run_editor_inline(command_args: &[String], cwd: &Path) -> CliResult<()> {
    let Some((program, program_args)) = command_args.split_first() else {
        return Err(CliError::Other("spawn undefined ENOENT".to_string()));
    };
    let status = Command::new(program)
        .args(program_args)
        .current_dir(cwd)
        .status()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CliError::Other(format!("spawn {program} ENOENT"))
            } else {
                CliError::Other(error.to_string())
            }
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            // JS: process.kill(process.pid, signal)
            unsafe {
                libc::raise(signal);
            }
            return Ok(());
        }
    }
    crate::ghostex_cli::set_exit_code(status.code().unwrap_or(0));
    Ok(())
}

// ---------------------------------------------------------------------------
// Diagnostics logs (Debugging Mode gated).
// ---------------------------------------------------------------------------

fn floating_editor_log_path() -> PathBuf {
    rpc::ghostex_logs_home().join("floating-editor.log")
}

fn prompt_editor_timeline_log_path() -> PathBuf {
    rpc::ghostex_logs_home().join("native-prompt-editor-debug.log")
}

fn shared_settings_path() -> PathBuf {
    rpc::ghostex_config_home().join("native-sidebar-settings.json")
}

fn prompt_editor_diagnostic_logging_enabled() -> bool {
    read_routine_diagnostic_enabled(&shared_settings_path(), "native.prompt.editor")
}

fn append_log_line(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

fn append_floating_editor_log(details: Value) {
    /*
    CDXC:Diagnostics 2026-05-16-07:23 (ported): honor the shared Settings
    Debugging Mode and native.prompt.editor scenario before creating or appending
    ~/Library/Logs/ghostex/floating-editor.log.
    */
    if !prompt_editor_diagnostic_logging_enabled() {
        return;
    }
    let mut payload = details.as_object().cloned().unwrap_or_default();
    payload.insert("source".to_string(), json!("ghostex-cli"));
    payload.insert("timestamp".to_string(), json!(iso_timestamp()));
    append_log_line(
        &floating_editor_log_path(),
        &format!("{}\n", Value::Object(payload)),
    );
}

fn append_prompt_editor_timeline_log(event: &str, details: Value) {
    if !prompt_editor_diagnostic_logging_enabled() {
        return;
    }
    let mut payload = details.as_object().cloned().unwrap_or_default();
    payload.insert("event".to_string(), json!(event));
    payload.insert("source".to_string(), json!("ghostex-cli"));
    let sanitized = sanitize_prompt_editor_timeline_payload(&payload);
    append_log_line(
        &prompt_editor_timeline_log_path(),
        &format!("[{}] {}\n", iso_timestamp(), Value::Object(sanitized)),
    );
}

fn sanitize_prompt_editor_timeline_payload(payload: &Map<String, Value>) -> Map<String, Value> {
    payload
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                sanitize_prompt_editor_timeline_value(key, value),
            )
        })
        .collect()
}

fn sanitize_prompt_editor_timeline_value(key: &str, value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::Array(items) => json!({ "count": items.len(), "redacted": true }),
        Value::Object(map) => Value::Object(sanitize_prompt_editor_timeline_payload(map)),
        Value::String(text) => {
            let normalized_key = key.to_lowercase();
            if normalized_key.contains("token")
                || normalized_key.contains("secret")
                || normalized_key.contains("auth")
            {
                return json!("[redacted:secret]");
            }
            if normalized_key.contains("path")
                || normalized_key.contains("cwd")
                || normalized_key.contains("dir")
                || normalized_key.contains("file")
                || text_looks_like_local_path(text)
            {
                return json!("[redacted:path]");
            }
            if normalized_key.contains("url") || text_looks_like_url(text) {
                return json!("[redacted:url]");
            }
            if normalized_key.contains("command")
                || normalized_key.contains("text")
                || normalized_key.contains("message")
            {
                return json!("[redacted]");
            }
            if matches!(
                normalized_key.as_str(),
                "event"
                    | "source"
                    | "requestid"
                    | "backend"
                    | "clientcapability"
                    | "selectionkind"
                    | "finalstatus"
                    | "errorname"
            ) {
                return Value::String(text.replace(['\r', '\n'], "\\n"));
            }
            json!("[redacted]")
        }
    }
}

/// /^(~\/|\/Users\/|\/Volumes\/|\/private\/|\/tmp\/|\/var\/folders\/)/u
fn text_looks_like_local_path(text: &str) -> bool {
    [
        "~/",
        "/Users/",
        "/Volumes/",
        "/private/",
        "/tmp/",
        "/var/folders/",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

/// /^https?:\/\//iu
fn text_looks_like_url(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

// ---------------------------------------------------------------------------
// Parity helpers for the unreachable bridge-based floatingEditorCommand body.
// The Node command throws GxserverCliUnsupportedError before reaching them,
// but they are preserved so the port stays reviewable against the source.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[path = "editors/bridge_parity.rs"]
mod floating_editor_bridge_parity;

// ---------------------------------------------------------------------------
// Tests (hermetic: no network, no live daemon, no $HOME dependence).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
