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
/// Never constructed on Windows: connect always fails the way the JS named
/// pipe path would when no daemon pipe exists. TcpStream only supplies the
/// Read/Write/try_clone/shutdown surface so the client code compiles.
#[cfg(windows)]
type DaemonStream = std::net::TcpStream;

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

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// normalizedEnvironmentString: String(value ?? "").trim() || undefined.
fn normalized_environment_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalized_env(key: &str) -> Option<String> {
    normalized_environment_string(&env_or_empty(key))
}

/// String(value ?? "") for JSON scalars (used where JS coerces payload fields).
fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

/// String(message.status) including the JS "undefined" spelling.
fn js_string_or_undefined(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}

fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// new Date().toISOString().
fn iso_timestamp() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Date.now().toString(36) / Math.random().toString(36) digits.
fn to_base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        output.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    output.reverse();
    String::from_utf8(output).unwrap_or_default()
}

fn random_base36(length: usize) -> String {
    use rand::Rng;
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| DIGITS[rng.gen_range(0..36)] as char)
        .collect()
}

/// JS shellQuote from ghostex-cli.mjs (always single-quotes, unlike the
/// foundation's word-preserving rpc::shell_quote).
fn cli_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// path.resolve(value) — lexical, against the current working directory.
fn js_path_resolve(value: &str) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    js_path_resolve_from(&base, value)
}

/// path.resolve(base, value).
fn js_path_resolve_from(base: &Path, value: &str) -> PathBuf {
    let raw = Path::new(value);
    let combined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };
    normalize_lexically(&combined)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

fn current_dir_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// await mkdtemp(path.join(tmpdir(), prefix)).
fn mkdtemp(prefix: &str) -> CliResult<PathBuf> {
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let candidate = base.join(format!("{prefix}{}", random_base36(6)));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CliError::Other(error.to_string())),
        }
    }
    Err(CliError::Other(format!(
        "Could not create a temporary directory for {prefix}."
    )))
}

fn is_executable_file(path: &str) -> bool {
    #[cfg(unix)]
    {
        let Ok(cpath) = std::ffi::CString::new(path) else {
            return false;
        };
        unsafe { libc::access(cpath.as_ptr(), libc::X_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(path).is_ok()
    }
}

/// fileExistsSync (realpathSync-based existence probe).
fn file_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

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

pub fn prompt_editor_command(args: &[String]) -> CliResult<()> {
    /*
    CDXC:PromptEditor 2026-06-30-03:11 (ported):
    Ctrl+G fallback is the machine's editor, not gte. Preserve Monaco only for
    macOS app clients or zmx leaders that explicitly advertise Monaco; every
    other context runs the first non-Ghostex editor from the preserved
    provider environment, VISUAL, EDITOR, then vi.
    */
    let selection_started_at = Instant::now();
    let parsed = parse_args(args);
    let Some(file_path) = parsed.rest.iter().find(|arg| !arg.trim().is_empty()) else {
        return Err(CliError::Other(
            "Usage: ghostex prompt-editor <file>".to_string(),
        ));
    };

    let cwd = js_path_resolve(&parsed.flags.text("cwd").unwrap_or_else(current_dir_string));
    let resolved_file_path = js_path_resolve_from(&cwd, file_path)
        .to_string_lossy()
        .into_owned();
    let backend = prompt_editor_backend_from_environment();
    let capability_started_at = Instant::now();
    let client_capability = zmx_prompt_editor_capability();
    let capability_duration_ms = elapsed_ms(capability_started_at);
    let selection =
        select_prompt_editor_command(&backend, client_capability.as_deref(), &resolved_file_path);
    let originating_session_id = prompt_editor_originating_session_id_from_environment();
    let selection_duration_ms = elapsed_ms(selection_started_at);

    append_floating_editor_log(json!({
        "backend": backend,
        "command": selection.command_args.join(" "),
        "cwd": cwd.to_string_lossy(),
        "event": "cli.prompt_editor_select",
        "globalSessionRef": env_or_empty("GHOSTEX_GLOBAL_SESSION_REF"),
        "gxserverBaseUrl": env_or_empty("GHOSTEX_GXSERVER_BASE_URL"),
        "macosAppClient": is_macos_app_prompt_editor_client(client_capability.as_deref()),
        "originatingSessionId": originating_session_id.clone().unwrap_or_default(),
        "promptEditorClientCapability": client_capability.clone().unwrap_or_default(),
    }));

    /*
    CDXC:SavedPrompts 2026-07-29:
    The GPUI "Stash Prompt" agent action writes a one-shot marker for the
    session and sends Ctrl+G. When this invocation finds a fresh marker, the
    file already holds the composer text the agent CLI wrote for $EDITOR, so
    stash it and exit without presenting any editor. Clearing the file on a
    durable stash is the visible result: the agent CLI reads the emptied file
    back and the composer text has moved into the stash.
    */
    if consume_prompt_stash_request(originating_session_id.as_deref(), &resolved_file_path) {
        crate::ghostex_cli::set_exit_code(0);
        return Ok(());
    }

    if selection.kind == "monaco" {
        let trace = json!({
            "backend": backend,
            "capabilityDurationMs": capability_duration_ms,
            "clientCapability": client_capability.unwrap_or_default(),
            "hasGlobalSessionRef": !env_or_empty("GHOSTEX_GLOBAL_SESSION_REF").is_empty(),
            "hasOriginatingSessionId": originating_session_id.is_some(),
            "selectionDurationMs": selection_duration_ms,
            "selectionKind": selection.kind,
        });
        return floating_monaco_editor_command_with_trace(args, &trace);
    }
    if selection.kind == "code-server" {
        return run_code_server_prompt_editor(&selection.command_args, &cwd);
    }
    run_editor_inline(&selection.command_args, &cwd)
}

fn prompt_editor_backend_from_environment() -> String {
    let backend = env_or_empty("GHOSTEX_PROMPT_EDITOR_BACKEND")
        .trim()
        .to_string();
    if backend == "monaco" || backend == "custom" {
        return backend;
    }
    // "gte" and GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE both collapse to inherit.
    "inherit".to_string()
}

fn zmx_prompt_editor_capability() -> Option<String> {
    if env_or_empty("ZMX_SESSION").trim().is_empty() {
        return None;
    }
    /*
    CDXC:PromptEditor 2026-06-07-08:09 (ported): zmx sessions without an
    explicit GHOSTEX_ZMX_BIN stay terminal-native instead of probing PATH.
    */
    let zmx_command = env_or_empty("GHOSTEX_ZMX_BIN").trim().to_string();
    if zmx_command.is_empty() {
        return Some("editor".to_string());
    }
    let capability = run_zmx_capability_probe(&zmx_command);
    match capability.as_deref() {
        Some("monaco") | Some("code-server") | Some("editor") | Some("gte") => capability,
        _ => Some("editor".to_string()),
    }
}

/// execFileAsync(zmx, ["prompt-editor-capability"], { timeout: 750 }).
fn run_zmx_capability_probe(command: &str) -> Option<String> {
    let mut child = Command::new(command)
        .arg("prompt-editor-capability")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_millis(750);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return Some(output.trim().to_string());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
}

fn is_macos_app_prompt_editor_client(client_capability: Option<&str>) -> bool {
    is_macos_app_prompt_editor_client_with(client_capability, &|key| std::env::var(key).ok())
}

fn is_macos_app_prompt_editor_client_with(
    client_capability: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    if let Some(capability) = client_capability.filter(|value| !value.is_empty()) {
        return capability == "monaco";
    }
    env("GHOSTEX_PROMPT_EDITOR_CLIENT").as_deref() == Some("macos-app")
}

struct PromptEditorSelection {
    command_args: Vec<String>,
    kind: &'static str,
}

fn select_prompt_editor_command(
    backend: &str,
    client_capability: Option<&str>,
    file_path: &str,
) -> PromptEditorSelection {
    select_prompt_editor_command_with(
        backend,
        client_capability,
        file_path,
        &|key| std::env::var(key).ok(),
        &|| resolve_ghostex_editor_executable().is_some(),
        &mut |message| eprintln!("{message}"),
    )
}

fn select_prompt_editor_command_with(
    backend: &str,
    client_capability: Option<&str>,
    file_path: &str,
    env: &dyn Fn(&str) -> Option<String>,
    ghostex_editor_available: &dyn Fn() -> bool,
    warn: &mut dyn FnMut(&str),
) -> PromptEditorSelection {
    let capability = client_capability.filter(|value| !value.is_empty());
    if capability == Some("code-server") {
        return PromptEditorSelection {
            command_args: code_server_prompt_editor_command(file_path),
            kind: "code-server",
        };
    }
    if backend == "custom" {
        let custom_command = env("GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "code --wait".to_string());
        return PromptEditorSelection {
            command_args: vec![
                "/bin/zsh".to_string(),
                "-lc".to_string(),
                format!("exec {custom_command} \"$@\""),
                "ghostex-prompt-editor".to_string(),
                file_path.to_string(),
            ],
            kind: "custom",
        };
    }
    if (backend == "monaco" || capability == Some("monaco"))
        && is_macos_app_prompt_editor_client_with(capability, env)
    {
        if ghostex_editor_available() {
            return PromptEditorSelection {
                command_args: vec![
                    "ghostex".to_string(),
                    "floating-monaco-editor".to_string(),
                    file_path.to_string(),
                ],
                kind: "monaco",
            };
        }
        warn(&ghostex_editor_unavailable_message(None));
    }
    let editor_command = machine_prompt_editor_command_with(env);
    PromptEditorSelection {
        command_args: vec![
            "/bin/zsh".to_string(),
            "-lc".to_string(),
            format!("exec {editor_command} \"$@\""),
            "ghostex-prompt-editor".to_string(),
            file_path.to_string(),
        ],
        kind: "editor",
    }
}

fn code_server_prompt_editor_command(file_path: &str) -> Vec<String> {
    let code_root = rpc::ghostex_data_home().join("code-server");
    let package = code_root.join("package");
    let user_data = code_root.join("runtime/user-data");
    vec![
        package.join("lib/node").to_string_lossy().into_owned(),
        package
            .join("out/node/entry.js")
            .to_string_lossy()
            .into_owned(),
        "--user-data-dir".to_string(),
        user_data.to_string_lossy().into_owned(),
        "--session-socket".to_string(),
        user_data
            .join("code-server-ipc.sock")
            .to_string_lossy()
            .into_owned(),
        "--reuse-window".to_string(),
        "--wait".to_string(),
        file_path.to_string(),
    ]
}

fn run_code_server_prompt_editor(command_args: &[String], cwd: &Path) -> CliResult<()> {
    let node = command_args.first().map(Path::new);
    let entrypoint = command_args.get(1).map(Path::new);
    let session_socket = command_args.get(5).map(Path::new);
    if !node.is_some_and(|path| path.to_str().is_some_and(is_executable_file))
        || !entrypoint.is_some_and(|path| path.is_file())
        || !cwd.is_dir()
    {
        return Err(CliError::Other(
            "Remote Ghostex Code editor is unavailable for this session.".to_string(),
        ));
    }
    let Some(session_socket) = session_socket else {
        return Err(CliError::Other(
            "Remote Ghostex Code editor target is invalid.".to_string(),
        ));
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !session_socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if !session_socket.exists() {
        return Err(CliError::Other(
            "Remote Ghostex Code editor did not become ready for this session.".to_string(),
        ));
    }
    run_editor_inline(command_args, cwd)
}

fn machine_prompt_editor_command_from_environment() -> String {
    machine_prompt_editor_command_with(&|key| std::env::var(key).ok())
}

fn machine_prompt_editor_command_with(env: &dyn Fn(&str) -> Option<String>) -> String {
    [
        "GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL",
        "GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR",
        "VISUAL",
        "EDITOR",
    ]
    .iter()
    .filter_map(|key| env(key))
    .filter_map(|value| normalized_environment_string(&value))
    .find(|command| is_usable_machine_editor_command(command))
    .unwrap_or_else(|| "vi".to_string())
}

fn is_usable_machine_editor_command(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    !is_ghostex_prompt_editor_command(command)
}

fn is_ghostex_prompt_editor_command(command: &str) -> bool {
    let trimmed = command.trim();
    let mut executable = trimmed.split_whitespace().next().unwrap_or("");
    // /^['"]|['"]$/gu — strip one leading and one trailing quote character.
    if executable.starts_with('\'') || executable.starts_with('"') {
        executable = &executable[1..];
    }
    if executable.ends_with('\'') || executable.ends_with('"') {
        executable = &executable[..executable.len() - 1];
    }
    let executable_name = executable.rsplit('/').next().unwrap_or("");
    executable_name == "prompt-editor"
        || trimmed.contains("ghostex prompt-editor")
        || (trimmed.contains("ghostex-cli.mjs") && trimmed.contains("prompt-editor"))
        || trimmed.contains("floating-monaco-editor")
        || command.contains("floating-editor -- gte")
}

fn prompt_editor_originating_session_id_from_environment() -> Option<String> {
    /*
    CDXC:PromptEditor 2026-06-09-21:50 (ported): derive the native P:G focus
    id from the per-session gxserver S:P:G ref before falling back to the
    legacy native env key for older direct terminals.
    */
    native_focus_session_id_from_global_session_ref(&env_or_empty("GHOSTEX_GLOBAL_SESSION_REF"))
        .or_else(|| normalized_environment_string(&env_or_empty("GHOSTEX_NATIVE_SESSION_ID")))
}

fn native_focus_session_id_from_global_session_ref(global_session_ref: &str) -> Option<String> {
    let parts: Vec<&str> = global_session_ref.trim().split(':').collect();
    if parts.len() == 3
        && matches_session_ref_part(parts[0], b'S', 1)
        && matches_session_ref_part(parts[1], b'P', 3)
        && matches_session_ref_part(parts[2], b'G', 3)
    {
        return Some(format!("{}:{}", parts[1], parts[2]));
    }
    None
}

/// ^X[0-9][a-z0-9]{tail_len}$ for the S/P/G global session ref parts.
fn matches_session_ref_part(part: &str, prefix: u8, tail_len: usize) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 + tail_len
        && bytes[0] == prefix
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// floating-monaco-editor (lines 3509-3712).
// ---------------------------------------------------------------------------

pub fn floating_monaco_editor_command(args: &[String]) -> CliResult<()> {
    floating_monaco_editor_command_with_trace(args, &json!({}))
}

fn floating_monaco_editor_command_with_trace(args: &[String], trace: &Value) -> CliResult<()> {
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
        Ok(()) => Ok(()),
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
                &[
                    "/bin/zsh".to_string(),
                    "-lc".to_string(),
                    format!("exec {editor_command} \"$@\""),
                    "ghostex-prompt-editor".to_string(),
                    resolved_file_path.clone(),
                ],
                &cwd,
            )
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
) -> Result<(), EditorError> {
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
    Ok(())
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
    match stash_prompt_content_via_gxserver(&content, originating_session_id) {
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
) -> Result<Value, CliError> {
    let mut params = json!({ "content": content });
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

fn valid_prompt_handoff_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn prompt_handoff_response_path(request_id: &str) -> Option<PathBuf> {
    valid_prompt_handoff_request_id(request_id).then(|| {
        rpc::ghostex_home()
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
        rpc::ghostex_home()
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
    let handoff_request_id = marker
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
    match stash_prompt_content_via_gxserver(&content, originating_session_id) {
        Ok(result) => {
            // Clear the composer only after the stash is durable; on failure
            // the file (and therefore the composer) stays untouched.
            let _ = std::fs::write(resolved_file_path, b"");
            if let Some(request_id) = handoff_request_id.as_deref() {
                write_prompt_handoff_response(
                    request_id,
                    &json!({
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
    if cfg!(windows) {
        let raw_user = normalized_env("USERNAME")
            .or_else(|| normalized_env("USER"))
            .unwrap_or_else(|| "user".to_string());
        let mut username: String = raw_user
            .chars()
            .map(|letter| {
                if letter.is_ascii_alphanumeric() || matches!(letter, '.' | '_' | '-') {
                    letter
                } else {
                    '-'
                }
            })
            .collect();
        if username.is_empty() {
            username = "user".to_string();
        }
        return Ok(format!("\\\\.\\pipe\\ghostex-editor-{username}"));
    }
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
                })
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
    _timeout_ms: u64,
) -> Result<DaemonStream, EditorError> {
    Err(EditorError {
        name: "Error",
        message: format!("connect ENOENT {socket_path}"),
        code: Some("ENOENT"),
    })
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
        if let Some(index) = self.pending.iter().position(|message| predicate(message)) {
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
                    if let Some(index) = self.pending.iter().position(|message| predicate(message))
                    {
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
                if let Some(index) = self.pending.iter().position(|message| predicate(message)) {
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
        #[cfg(windows)]
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
                    .join("editor")
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
    format!(
        "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or install /Applications/GhostexEditor.app.{detail}"
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
        if file_exists(&current.join("scripts").join("ghostex-cli.mjs")) {
            return Some(current);
        }
        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            return None;
        };
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
mod floating_editor_bridge_parity {
    use super::*;
    use crate::ghostex_cli::args::js_number;

    const DEFAULT_PORT: f64 = 58743.0;
    const DEV_PORT: f64 = 58742.0;

    /// bridgePortFromFlags.
    pub(super) fn bridge_port_from_flags(flags: &Flags) -> f64 {
        if let Some(port) = flags.number("port") {
            return port;
        }
        if let Ok(port) = std::env::var("GHOSTEX_CLI_PORT") {
            if let Some(port) = js_number(&port) {
                return port;
            }
        }
        if std::env::var("GHOSTEX_APP_VARIANT").as_deref() == Ok("dev") {
            DEV_PORT
        } else {
            DEFAULT_PORT
        }
    }

    /// readBridgeAuthToken.
    pub(super) fn read_bridge_auth_token(flags: &Flags) -> CliResult<String> {
        /*
        CDXC:ServerApi 2026-05-15-18:25 (ported): CLI commands read
        the per-launch token that the app writes under resolved Ghostex state storage.
        */
        let explicit_token = flags
            .text("token")
            .or_else(|| flags.text("bridgeToken"))
            .or_else(|| std::env::var("GHOSTEX_BRIDGE_TOKEN").ok())
            .unwrap_or_default()
            .trim()
            .to_string();
        if !explicit_token.is_empty() {
            return Ok(explicit_token);
        }
        let bridge_token_path = rpc::ghostex_home().join("cli").join("bridge-token");
        let token = std::fs::read_to_string(&bridge_token_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(CliError::Other(format!(
                "Could not read Ghostex bridge token at {}. Is Ghostex running?",
                bridge_token_path.display()
            )));
        }
        Ok(token)
    }

    /// floatingEditorEnvironment.
    pub(super) fn floating_editor_environment() -> Value {
        let env_or =
            |key: &str, fallback: &str| std::env::var(key).unwrap_or_else(|_| fallback.to_string());
        let mut environment = Map::new();
        environment.insert(
            "HOME".to_string(),
            json!(std::env::var("HOME")
                .unwrap_or_else(|_| rpc::home_dir().to_string_lossy().into_owned())),
        );
        environment.insert("LANG".to_string(), json!(env_or("LANG", "en_US.UTF-8")));
        environment.insert(
            "PATH".to_string(),
            json!(env_or(
                "PATH",
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
            )),
        );
        environment.insert("SHELL".to_string(), json!(env_or("SHELL", "/bin/zsh")));
        environment.insert("TERM".to_string(), json!(env_or("TERM", "xterm-256color")));
        environment.insert("USER".to_string(), json!(env_or("USER", "")));
        environment.insert("GHOSTEX_FLOATING_EDITOR".to_string(), json!("1"));
        if let Ok(variant) = std::env::var("GHOSTEX_APP_VARIANT") {
            environment.insert("GHOSTEX_APP_VARIANT".to_string(), json!(variant));
        }
        Value::Object(environment)
    }

    /// floatingEditorWrapperScript.
    pub(super) fn floating_editor_wrapper_script(
        command_args: &[String],
        cwd: &str,
        status_file: &str,
        log_path: &str,
    ) -> String {
        let command = command_args
            .iter()
            .map(|arg| cli_shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let dirname = |path: &str| {
            Path::new(path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".to_string())
        };
        format!(
            "#!/bin/zsh\nset +e\nmkdir -p {status_dir} {log_dir} 2>/dev/null\nprintf 'started\\n' > {status}\n{{\n  printf '[%s] child.start cwd=%s command=%s\\n' \"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\" {cwd_quoted} {command_quoted}\n}} >> {log} 2>/dev/null\ncd {cwd_quoted} || {{\n  _ghostex_status=$?\n  printf 'exit:%s\\n' \"$_ghostex_status\" >> {status}\n  exit \"$_ghostex_status\"\n}}\n{command}\n_ghostex_status=$?\n{{\n  printf '[%s] child.exit status=%s\\n' \"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\" \"$_ghostex_status\"\n}} >> {log} 2>/dev/null\nprintf 'exit:%s\\n' \"$_ghostex_status\" >> {status}\nexit \"$_ghostex_status\"\n",
            status_dir = cli_shell_quote(&dirname(status_file)),
            log_dir = cli_shell_quote(&dirname(log_path)),
            status = cli_shell_quote(status_file),
            cwd_quoted = cli_shell_quote(cwd),
            command_quoted = cli_shell_quote(&command),
            log = cli_shell_quote(log_path),
            command = command,
        )
    }

    /// resolveExecutable (command -v through the interactive shell).
    pub(super) fn resolve_executable(command: &str) -> CliResult<String> {
        if command.contains('/') {
            return Ok(command.to_string());
        }
        let shell = resolve_cli_interactive_shell_launch();
        let output = Command::new(&shell.executable)
            .arg(shell.command_flag)
            .arg(format!("command -v -- {}", cli_shell_quote(command)))
            .output()
            .map_err(|error| CliError::Other(error.to_string()))?;
        if !output.status.success() {
            return Err(CliError::Other(format!(
                "Command failed: command -v -- {}",
                cli_shell_quote(command)
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout
            .trim()
            .split(['\r', '\n'])
            .next()
            .unwrap_or("")
            .to_string();
        Ok(if first_line.is_empty() {
            command.to_string()
        } else {
            first_line
        })
    }

    pub(super) struct ShellLaunch {
        pub executable: String,
        pub command_flag: &'static str,
    }

    /// resolveCliInteractiveShellLaunch (macOS pinned to zsh; POSIX shells
    /// elsewhere).
    pub(super) fn resolve_cli_interactive_shell_launch() -> ShellLaunch {
        if cfg!(target_os = "macos") {
            return ShellLaunch {
                executable: "/bin/zsh".to_string(),
                command_flag: "-lc",
            };
        }
        let mut candidates: Vec<String> = Vec::new();
        let shell = std::env::var("SHELL")
            .unwrap_or_default()
            .trim()
            .to_string();
        if !shell.is_empty() && is_supported_cli_posix_shell(&shell) {
            candidates.push(shell);
        }
        for fallback in ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"] {
            if !candidates.iter().any(|candidate| candidate == fallback) {
                candidates.push(fallback.to_string());
            }
        }
        let executable = candidates
            .iter()
            .find(|candidate| is_executable_file(candidate))
            .cloned()
            .or_else(|| candidates.first().cloned())
            .unwrap_or_else(|| "/bin/sh".to_string());
        let command_flag = if matches!(shell_basename(&executable).as_str(), "bash" | "zsh") {
            "-lc"
        } else {
            "-c"
        };
        ShellLaunch {
            executable,
            command_flag,
        }
    }

    fn is_supported_cli_posix_shell(shell_path: &str) -> bool {
        matches!(
            shell_basename(shell_path).as_str(),
            "ash" | "bash" | "dash" | "ksh" | "mksh" | "sh" | "zsh"
        )
    }

    fn shell_basename(path: &str) -> String {
        path.rsplit('/').next().unwrap_or(path).to_lowercase()
    }

    /// waitForStatus (status-file polling loop).
    pub(super) fn wait_for_status(
        status_file: &Path,
        predicate: &dyn Fn(&str) -> bool,
        timeout_ms: u64,
    ) -> CliResult<String> {
        let started_at = Instant::now();
        loop {
            let status = std::fs::read_to_string(status_file).unwrap_or_default();
            if predicate(&status) {
                return Ok(status);
            }
            if timeout_ms > 0 && started_at.elapsed().as_millis() as u64 > timeout_ms {
                return Err(CliError::Other(format!(
                    "Timed out waiting for floating editor status at {}.",
                    status_file.display()
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// runGhostexEditorProcess (pre-daemon direct spawn; unused since the
    /// resident daemon landed).
    pub(super) fn run_ghostex_editor_process(
        command: &str,
        args: &[String],
        cwd: &Path,
    ) -> CliResult<i32> {
        let status = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| CliError::Other(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if status.signal().is_some() {
                return Ok(1);
            }
        }
        Ok(status.code().unwrap_or(0))
    }
}

// ---------------------------------------------------------------------------
// Tests (hermetic: no network, no live daemon, no $HOME dependence).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn select(
        backend: &str,
        client_capability: Option<&str>,
        env: &HashMap<String, String>,
        available: bool,
    ) -> (PromptEditorSelection, Vec<String>) {
        let mut warnings = Vec::new();
        let selection = select_prompt_editor_command_with(
            backend,
            client_capability,
            "/tmp/prompt.md",
            &|key| env.get(key).cloned(),
            &move || available,
            &mut |message| warnings.push(message.to_string()),
        );
        (selection, warnings)
    }

    #[test]
    fn select_custom_backend_uses_custom_command_or_default() {
        let env = env_map(&[("GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND", "  subl -w  ")]);
        let (selection, warnings) = select("custom", None, &env, true);
        assert_eq!(selection.kind, "custom");
        assert_eq!(
            selection.command_args,
            vec![
                "/bin/zsh",
                "-lc",
                "exec subl -w \"$@\"",
                "ghostex-prompt-editor",
                "/tmp/prompt.md"
            ]
        );
        assert!(warnings.is_empty());

        let (selection, _) = select("custom", None, &env_map(&[]), true);
        assert_eq!(selection.command_args[2], "exec code --wait \"$@\"");
    }

    #[test]
    fn select_monaco_requires_macos_app_client_and_executable() {
        // Capability monaco → macOS app client → monaco.
        let (selection, _) = select("inherit", Some("monaco"), &env_map(&[]), true);
        assert_eq!(selection.kind, "monaco");
        assert_eq!(
            selection.command_args,
            vec!["ghostex", "floating-monaco-editor", "/tmp/prompt.md"]
        );

        // Backend monaco with macos-app env client (no zmx capability).
        let env = env_map(&[("GHOSTEX_PROMPT_EDITOR_CLIENT", "macos-app")]);
        let (selection, _) = select("monaco", None, &env, true);
        assert_eq!(selection.kind, "monaco");

        // Backend monaco but the zmx leader only advertises the machine
        // editor → not a macOS app client → machine editor.
        let (selection, _) = select("monaco", Some("editor"), &env, true);
        assert_eq!(selection.kind, "editor");

        // Backend monaco without any macOS app marker → machine editor.
        let (selection, _) = select("monaco", None, &env_map(&[]), true);
        assert_eq!(selection.kind, "editor");

        // Monaco requested but executable missing → warn + machine editor.
        let (selection, warnings) = select("inherit", Some("monaco"), &env_map(&[]), false);
        assert_eq!(selection.kind, "editor");
        assert_eq!(warnings, vec![ghostex_editor_unavailable_message(None)]);
    }

    #[test]
    fn select_remote_code_server_never_falls_through_to_monaco_or_machine_editor() {
        let env = env_map(&[("EDITOR", "nvim")]);
        let (selection, warnings) = select("monaco", Some("code-server"), &env, true);
        assert_eq!(selection.kind, "code-server");
        assert!(selection
            .command_args
            .iter()
            .any(|arg| arg == "--reuse-window"));
        assert!(selection.command_args.iter().any(|arg| arg == "--wait"));
        assert_eq!(
            selection.command_args.last().map(String::as_str),
            Some("/tmp/prompt.md")
        );
        assert!(!selection
            .command_args
            .iter()
            .any(|arg| arg.contains("GhostexEditor")));
        assert!(!selection.command_args.iter().any(|arg| arg == "nvim"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn select_machine_editor_falls_through_environment_chain() {
        let env = env_map(&[
            ("GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL", "   "),
            (
                "GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR",
                "ghostex prompt-editor",
            ),
            ("VISUAL", "nvim"),
            ("EDITOR", "nano"),
        ]);
        let (selection, _) = select("inherit", Some("editor"), &env, true);
        assert_eq!(selection.kind, "editor");
        assert_eq!(selection.command_args[2], "exec nvim \"$@\"");

        // Everything unusable → vi.
        let env = env_map(&[("EDITOR", "gte --floating-editor -- gte")]);
        assert_eq!(
            machine_prompt_editor_command_with(&|key| env.get(key).cloned()),
            "vi"
        );
        assert_eq!(machine_prompt_editor_command_with(&|_| None), "vi");
    }

    #[test]
    fn ghostex_prompt_editor_command_detection() {
        assert!(is_ghostex_prompt_editor_command("prompt-editor"));
        assert!(is_ghostex_prompt_editor_command(
            "/usr/local/bin/prompt-editor --flag"
        ));
        assert!(is_ghostex_prompt_editor_command("\"prompt-editor\""));
        assert!(is_ghostex_prompt_editor_command("ghostex prompt-editor"));
        assert!(is_ghostex_prompt_editor_command(
            "node /x/scripts/ghostex-cli.mjs prompt-editor"
        ));
        assert!(is_ghostex_prompt_editor_command(
            "ghostex floating-monaco-editor"
        ));
        assert!(is_ghostex_prompt_editor_command(
            "ghostex floating-editor -- gte"
        ));
        assert!(!is_ghostex_prompt_editor_command("vim"));
        assert!(!is_ghostex_prompt_editor_command("code --wait"));
        assert!(!is_ghostex_prompt_editor_command(
            "node /x/scripts/ghostex-cli.mjs open"
        ));
    }

    #[test]
    fn native_focus_session_id_parses_global_refs() {
        assert_eq!(
            native_focus_session_id_from_global_session_ref("S1a:P2cde:G3fgh"),
            Some("P2cde:G3fgh".to_string())
        );
        assert_eq!(
            native_focus_session_id_from_global_session_ref("  S0a:P0abc:G0abc  "),
            Some("P0abc:G0abc".to_string())
        );
        assert_eq!(native_focus_session_id_from_global_session_ref(""), None);
        assert_eq!(
            native_focus_session_id_from_global_session_ref("P2cde:G3fgh"),
            None
        );
        assert_eq!(
            native_focus_session_id_from_global_session_ref("S1a:P2cde:G3fgh:extra"),
            None
        );
        // ^S[0-9][a-z0-9]$ is exactly three characters.
        assert_eq!(
            native_focus_session_id_from_global_session_ref("S1ab:P2cde:G3fgh"),
            None
        );
        // Uppercase tail rejected by [a-z0-9].
        assert_eq!(
            native_focus_session_id_from_global_session_ref("S1a:P2CDE:G3fgh"),
            None
        );
    }

    #[test]
    fn final_status_matches_multiline_regex() {
        assert_eq!(final_ghostex_editor_status_from_text("saved"), "saved");
        assert_eq!(
            final_ghostex_editor_status_from_text("started\nsaved\n"),
            "saved"
        );
        // JS multiline anchors also break on \r (verified against Node).
        assert_eq!(final_ghostex_editor_status_from_text("saved\r\n"), "saved");
        assert_eq!(
            final_ghostex_editor_status_from_text("started\r\nsaved"),
            "saved"
        );
        assert_eq!(
            final_ghostex_editor_status_from_text("started\ncancelled"),
            "cancelled"
        );
        assert_eq!(final_ghostex_editor_status_from_text("exit:0"), "unknown");
        assert_eq!(final_ghostex_editor_status_from_text(" saved"), "unknown");
        assert_eq!(final_ghostex_editor_status_from_text(""), "unknown");
    }

    #[test]
    fn sanitize_redacts_by_key_and_shape() {
        let payload = json!({
            "authToken": "abc",
            "backend": "monaco",
            "commandElapsedMs": 12,
            "command": "vim file",
            "cwd": "/Users/madda",
            "errorName": "Error\r\nBad",
            "event": "cli.monaco.failed",
            "hasInputFile": true,
            "inputByteCount": Value::Null,
            "items": [1, 2, 3],
            "nested": { "filePath": "/tmp/x", "ok": false },
            "note": "/Users/madda/secret.md",
            "randomKey": "hello",
            "serverUrl": "wat",
            "site": "https://example.com",
        });
        let sanitized = Value::Object(sanitize_prompt_editor_timeline_payload(
            payload.as_object().unwrap(),
        ));
        assert_eq!(sanitized["authToken"], "[redacted:secret]");
        assert_eq!(sanitized["backend"], "monaco");
        assert_eq!(sanitized["commandElapsedMs"], 12);
        assert_eq!(sanitized["command"], "[redacted]");
        assert_eq!(sanitized["cwd"], "[redacted:path]");
        assert_eq!(sanitized["errorName"], "Error\\n\\nBad");
        assert_eq!(sanitized["event"], "cli.monaco.failed");
        assert_eq!(sanitized["hasInputFile"], true);
        assert_eq!(sanitized["inputByteCount"], Value::Null);
        assert_eq!(sanitized["items"], json!({ "count": 3, "redacted": true }));
        assert_eq!(sanitized["nested"]["filePath"], "[redacted:path]");
        assert_eq!(sanitized["nested"]["ok"], false);
        assert_eq!(sanitized["note"], "[redacted:path]");
        assert_eq!(sanitized["randomKey"], "[redacted]");
        assert_eq!(sanitized["serverUrl"], "[redacted:url]");
        assert_eq!(sanitized["site"], "[redacted:url]");
    }

    #[test]
    fn cli_session_key_requires_both_parts() {
        assert_eq!(
            cli_session_key(Some(&json!(" P1 ")), Some(&json!("G2"))),
            "P1:G2"
        );
        assert_eq!(cli_session_key(Some(&json!("P1")), None), "");
        assert_eq!(cli_session_key(Some(&json!("")), Some(&json!("G2"))), "");
        assert_eq!(cli_session_key(None, None), "");
        assert_eq!(cli_session_key(Some(&Value::Null), Some(&json!("G2"))), "");
    }

    #[test]
    fn unavailable_message_appends_error_detail() {
        assert_eq!(
            ghostex_editor_unavailable_message(None),
            "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or install /Applications/GhostexEditor.app."
        );
        assert_eq!(
            ghostex_editor_unavailable_message(Some("boom")),
            "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or install /Applications/GhostexEditor.app. boom"
        );
    }

    #[test]
    fn connection_error_codes_match_js_list() {
        for code in ["ECONNREFUSED", "ENOENT", "ENOTFOUND", "ECONNRESET", "EPIPE"] {
            let error = EditorError {
                name: "Error",
                message: "x".to_string(),
                code: Some(match code {
                    "ECONNREFUSED" => "ECONNREFUSED",
                    "ENOENT" => "ENOENT",
                    "ENOTFOUND" => "ENOTFOUND",
                    "ECONNRESET" => "ECONNRESET",
                    _ => "EPIPE",
                }),
            };
            assert!(is_ghostex_editor_connection_error(&error), "{code}");
        }
        assert!(!is_ghostex_editor_connection_error(&EditorError::new(
            "plain"
        )));
        assert!(EditorError::unavailable("x").is_unavailable());
        assert!(!is_ghostex_editor_connection_error(
            &EditorError::unavailable("x")
        ));
    }

    #[test]
    fn js_path_resolution_is_lexical() {
        let base = Path::new("/base/dir");
        assert_eq!(
            js_path_resolve_from(base, "file.md"),
            PathBuf::from("/base/dir/file.md")
        );
        assert_eq!(
            js_path_resolve_from(base, "../other/./x.md"),
            PathBuf::from("/base/other/x.md")
        );
        assert_eq!(
            js_path_resolve_from(base, "/abs/y.md"),
            PathBuf::from("/abs/y.md")
        );
        assert_eq!(
            js_path_resolve_from(Path::new("/"), ".."),
            PathBuf::from("/")
        );
    }

    #[test]
    fn editor_candidates_per_platform_match_node_lists() {
        let home = Path::new("/home/u");
        let repo = Path::new("/repo");
        assert_eq!(
            ghostex_editor_executable_candidates_for_platform("darwin", Some(repo), home, None),
            vec![
                "/home/u/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor",
                "/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor",
                "/repo/editor/dist/GhostexEditor.app/Contents/MacOS/GhostexEditor",
            ]
        );
        assert_eq!(
            ghostex_editor_executable_candidates_for_platform("darwin", None, home, None).len(),
            2
        );
        assert_eq!(
            ghostex_editor_executable_candidates_for_platform("linux", Some(repo), home, None),
            vec![
                "/home/u/.local/bin/ghostex-editor",
                "/usr/local/bin/ghostex-editor",
                "/repo/editor/dist/desktop/ghostex-editor",
            ]
        );
    }

    #[test]
    fn app_bundle_candidate_appends_binary_path() {
        assert_eq!(
            ghostex_editor_executable_candidate("/Applications/GhostexEditor.app"),
            Some("/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor".to_string())
        );
        assert_eq!(
            ghostex_editor_executable_candidate("/opt/bin/ghostex-editor"),
            Some("/opt/bin/ghostex-editor".to_string())
        );
        assert_eq!(ghostex_editor_executable_candidate("   "), None);
        assert_eq!(ghostex_editor_executable_candidate(""), None);
    }

    #[test]
    fn wrapper_script_matches_node_template() {
        let script = floating_editor_bridge_parity::floating_editor_wrapper_script(
            &["vim".to_string(), "a b".to_string()],
            "/work dir",
            "/w/status",
            "/dev/null",
        );
        assert!(script.starts_with("#!/bin/zsh\nset +e\n"));
        assert!(script.contains("mkdir -p '/w' '/dev' 2>/dev/null\n"));
        assert!(script.contains("printf 'started\\n' > '/w/status'\n"));
        assert!(script.contains("cd '/work dir' || {\n"));
        assert!(script.contains("\n'vim' 'a b'\n_ghostex_status=$?\n"));
        assert!(script.contains("printf 'exit:%s\\n' \"$_ghostex_status\" >> '/w/status'\n"));
        assert!(script.ends_with("exit \"$_ghostex_status\"\n"));
    }

    #[test]
    fn base36_and_random_ids() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        let id = random_base36(6);
        assert_eq!(id.len(), 6);
        assert!(id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn js_string_coercions() {
        assert_eq!(js_string_or_undefined(None), "undefined");
        assert_eq!(js_string_or_undefined(Some(&Value::Null)), "null");
        assert_eq!(js_string_or_undefined(Some(&json!("ok"))), "ok");
        assert_eq!(js_string_or_undefined(Some(&json!(3))), "3");
        assert_eq!(js_string_or_empty(None), "");
        assert_eq!(js_string_or_empty(Some(&Value::Null)), "");
        assert_eq!(js_string_or_empty(Some(&json!(true))), "true");
    }
}
