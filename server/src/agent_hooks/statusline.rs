/*
CDXC:AgentHooks 2026-09-03 WHY:
A default Claude Code install has no `statusLine`, so its screen never names
the model or effort and the chat's option pills sat empty until the first
assistant turn (and missed every idle `/model` or `/effort` change until the
next one). Claude's statusLine command receives a JSON payload with the live
model, effort, fast mode, context usage, cost and rate limits, re-run on every
change we care about with a 300ms debounce. So installing the Claude hooks also
installs a Ghostex-owned statusLine command:

  - it hands the payload to `gxserver agent-statusline`, which stores it under
    `<hook state dir>/claude-statusline/<claude session id>.json` where the
    chat option detector reads it as first-class evidence;
  - with no user statusline it renders Ghostex's own line (`Fable | high |
    Ctx 8% | $0.12`), whose `|` grammar the screen parser already understands;
  - with a user statusline it WRAPS it: the original command travels as the
    script's first argument, so the user's display is untouched, every profile
    keeps its own, and uninstall restores it verbatim.

Permission mode is not in the payload; it stays on the footer scrape and the
transcript's `permission-mode` rows.
*/

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::config::{HookPaths, STATUSLINE_HOOK_MARKER, STATUSLINE_HOOK_VERSION};
use super::install::{read_json_object, write_executable_notify_hook};
use super::plugin_sources::shell_quote;
use super::probing::{
    expand_home_path, io_error, now_iso, parse_global_session_ref, path_string, temp_path_for,
};

/// Directory under the hook state directory holding one payload per Claude
/// session id.
pub const CLAUDE_STATUSLINE_STATE_DIRECTORY: &str = "claude-statusline";

/// Directory under the hook state directory holding one payload per Cursor
/// session id.
pub const CURSOR_STATUSLINE_STATE_DIRECTORY: &str = "cursor-statusline";

/*
CDXC:AgentHooks 2026-09-24 DECISION:
User: Cursor gets the same statusline wrapper as Claude, so its chat status line
and More details can show what Cursor hands its statusline command (context,
output tokens, version, Max Mode, auto-run, worktree, folder) instead of only
what its footer prints. Cursor's `statusLine` lives in `~/.cursor/cli-config.json`
with Claude's `{type, command, padding}` shape and pipes a Claude-like JSON
payload, so one mechanism serves both: each agent has its own script (the Cursor
one passes `--agent cursor`), payload directory and fallback line, and a user's
own command is wrapped and still renders the footer.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatuslineAgent {
    Claude,
    Cursor,
}

impl StatuslineAgent {
    fn from_arg(value: Option<&str>) -> Self {
        match value {
            Some("cursor") => Self::Cursor,
            _ => Self::Claude,
        }
    }

    fn state_directory(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_STATUSLINE_STATE_DIRECTORY,
            Self::Cursor => CURSOR_STATUSLINE_STATE_DIRECTORY,
        }
    }

    pub(crate) fn script_path(self, hook_paths: &HookPaths) -> &Path {
        match self {
            Self::Claude => &hook_paths.statusline_hook_path,
            Self::Cursor => &hook_paths.cursor_statusline_hook_path,
        }
    }

    /// Extra arguments the script passes to `gxserver agent-statusline`. Claude's
    /// stays empty so its installed script is unchanged.
    fn runtime_arguments(self) -> &'static str {
        match self {
            Self::Claude => "",
            Self::Cursor => " --agent cursor",
        }
    }
}

/// Payloads older than this are removed when a new session's file is created.
const CLAUDE_STATUSLINE_PAYLOAD_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);

// ---------------------------------------------------------------------------
// Script + settings registration
// ---------------------------------------------------------------------------

pub(crate) fn build_statusline_hook_script(
    executable: &str,
    hook_state_directory: &Path,
    agent: StatuslineAgent,
) -> String {
    let script = format!(
        r#"#!/bin/bash
# {STATUSLINE_HOOK_MARKER} v{STATUSLINE_HOOK_VERSION}
# Claude Code statusLine command installed by Ghostex. Claude pipes its status
# JSON on stdin; gxserver stores it for the session's chat view. With a wrapped
# user statusline command as $1 that command renders the line; otherwise
# gxserver renders Ghostex's own.
INPUT="$(cat)"
DEFAULT_HOOK_STATE_DIR={hook_state_directory}
HOOK_STATE_DIR="${{GHOSTEX_AGENT_HOOK_STATE_DIR:-$DEFAULT_HOOK_STATE_DIR}}"
if [ -n "${{1:-}}" ]; then
  printf '%s' "$INPUT" | {executable} agent-statusline "$HOOK_STATE_DIR"{arguments} >/dev/null 2>&1 || true
  printf '%s' "$INPUT" | /bin/sh -c "$1"
  exit 0
fi
printf '%s' "$INPUT" | {executable} agent-statusline "$HOOK_STATE_DIR"{arguments} --render 2>/dev/null
exit 0
"#,
        executable = shell_quote(executable),
        hook_state_directory = shell_quote(&path_string(hook_state_directory)),
        arguments = agent.runtime_arguments(),
    );
    match agent {
        StatuslineAgent::Claude => script,
        StatuslineAgent::Cursor => script
            .replace("# Claude Code statusLine", "# Cursor CLI statusLine")
            .replace("Claude pipes", "Cursor pipes"),
    }
}

/// Writes both agents' statusline scripts.
pub(crate) fn install_statusline_hook(hook_paths: &HookPaths) -> Result<(), DomainStateError> {
    fs::create_dir_all(&hook_paths.hook_state_directory).map_err(io_error)?;
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path_string(&path))
        .unwrap_or_else(|| "gxserver".to_string());
    for agent in [StatuslineAgent::Claude, StatuslineAgent::Cursor] {
        let path = agent.script_path(hook_paths);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let script =
            build_statusline_hook_script(&executable, &hook_paths.hook_state_directory, agent);
        write_executable_notify_hook(path, &script)?;
    }
    Ok(())
}

/// Both agents' statusline scripts are current.
pub(crate) fn are_statusline_hooks_current(hook_paths: &HookPaths) -> bool {
    [StatuslineAgent::Claude, StatuslineAgent::Cursor]
        .into_iter()
        .all(|agent| {
            is_statusline_hook_current(
                hook_paths,
                &super::probing::read_file_text(agent.script_path(hook_paths)),
            )
        })
}

pub(crate) fn is_statusline_hook_current(hook_paths: &HookPaths, contents: &str) -> bool {
    let state_directory_assignment = format!(
        "DEFAULT_HOOK_STATE_DIR={}",
        shell_quote(&path_string(&hook_paths.hook_state_directory))
    );
    contents.contains(&format!(
        "{STATUSLINE_HOOK_MARKER} v{STATUSLINE_HOOK_VERSION}"
    )) && contents
        .lines()
        .any(|line| line == state_directory_assignment)
}

#[cfg(not(windows))]
fn statusline_script_token(script: &Path) -> String {
    shell_quote(&path_string(script))
}

/// The Claude `statusLine.command` Ghostex writes.
#[cfg(test)]
pub(crate) fn statusline_command(hook_paths: &HookPaths, wrapped: Option<&str>) -> String {
    agent_statusline_command(&hook_paths.statusline_hook_path, wrapped)
}

/// The `statusLine.command` Ghostex writes: the script alone, or the script
/// with the user's original command as its single argument.
#[cfg(not(windows))]
fn agent_statusline_command(script: &Path, wrapped: Option<&str>) -> String {
    let script = statusline_script_token(script);
    match wrapped {
        Some(wrapped) => format!("{script} {}", shell_quote(wrapped)),
        None => script,
    }
}

/// CDXC:AgentHooks 2026-10-04 WHY:
/// Claude Code runs the statusLine command through Git Bash when Git for Windows is installed and through PowerShell when it is not, so the command must parse in both: a quoted path is a string to PowerShell, which on 2026-09-30 left computers without Git printing the script path and sending no payload. That fix started gxserver through powershell.exe, but PowerShell's own startup cost about 0.5 s on every statusline update, and the first one is what fills a new chat's model pill and status line. The command now names gxserver by its 8.3 short path with forward slashes, a bare word both shells run, with every argument single-quoted (literal in both); the powershell.exe form remains wherever no space-free short path exists or an argument holds a quote. Cursor's statusline has its own form (`cursor_statusline_command`). The script path stays in the command: it carries the hook state directory and marks the command as Ghostex's. A wrapped user command travels as base64 so its quotes survive both shells.
/// SEE-ALSO: `windows::bare_command_path`, `windows::command` (the hook form), `run_native_statusline_hook` (the runtime), `windows_wrapped_statusline_command` (the parser).
#[cfg(windows)]
fn agent_statusline_command(script: &Path, wrapped: Option<&str>) -> String {
    use base64::Engine as _;
    let executable = std::env::current_exe().unwrap_or_default();
    let claude = !script
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("cursor-"));
    let script_path = script;
    let script = path_string(script);
    let wrapped = wrapped.map(|wrapped| base64::engine::general_purpose::STANDARD.encode(wrapped));
    if !claude {
        return cursor_statusline_command(&executable, script_path, wrapped.as_deref());
    }
    if let Some(executable) =
        super::windows::bare_command_path(&executable).filter(|_| claude && !script.contains('\''))
    {
        let wrapped = wrapped
            .map(|wrapped| format!(" '{wrapped}'"))
            .unwrap_or_default();
        return format!("{executable} {WINDOWS_STATUSLINE_VERB} '{script}'{wrapped}");
    }
    let quote = crate::platform::shell::powershell_single_quote;
    let wrapped = wrapped
        .map(|wrapped| format!(" {}", quote(&wrapped)))
        .unwrap_or_default();
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -Command \"& {} {WINDOWS_STATUSLINE_VERB} {}{wrapped}\"",
        quote(&executable.to_string_lossy()),
        quote(&script),
    )
}

/// CDXC:AgentHooks 2026-10-07 WHY:
/// Cursor's CLI (2026.10.01) splits its statusLine command with `string-argv`, which drops one layer of quotes, then joins the words with spaces and runs them with `shell: true`, which on Windows is `cmd.exe /d /s /c`. The powershell.exe form lost the double quotes around its `-Command`, so cmd.exe read its `&` as a command separator: PowerShell printed its usage, and Cursor showed that as the status line under its composer. Each path is now double-quoted inside single quotes: `string-argv` removes the single quotes, and cmd.exe gets the double-quoted paths, spaces included. A path that holds a single quote uses its 8.3 short form instead. gxserver starts directly, without PowerShell's startup on every update.
/// SEE-ALSO: `windows_wrapped_statusline_command`, which reads the wrapped user command back from this form too.
#[cfg(windows)]
fn cursor_statusline_command(executable: &Path, script: &Path, wrapped: Option<&str>) -> String {
    let word = |path: &Path| {
        let text = path_string(path);
        let text = if text.contains('\'') {
            super::windows::bare_command_path(path).unwrap_or(text)
        } else {
            text
        };
        format!("'\"{text}\"'")
    };
    let wrapped = wrapped
        .map(|wrapped| format!(" '{wrapped}'"))
        .unwrap_or_default();
    format!(
        "{} {WINDOWS_STATUSLINE_VERB} {}{wrapped}",
        word(executable),
        word(script)
    )
}

/// The gxserver verb the Windows statusLine command runs.
#[cfg(windows)]
pub(crate) const WINDOWS_STATUSLINE_VERB: &str = "agent-statusline-native";

/// The user command wrapped by a Windows statusLine command in the `agent-statusline-native` form: `None` when the command is not in that form, `Some(None)` when it wraps nothing.
#[cfg(windows)]
fn windows_wrapped_statusline_command(command: &str) -> Option<Option<String>> {
    use base64::Engine as _;
    let (_, arguments) = command.split_once(&format!(" {WINDOWS_STATUSLINE_VERB} "))?;
    let arguments = arguments.trim().trim_end_matches('"').trim();
    // `'<script>'` or `'<script>' '<base64>'`; a PowerShell literal doubles its quotes.
    let after_script = arguments.strip_prefix('\'')?;
    let mut script_end = None;
    let bytes = after_script.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            if bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
                continue;
            }
            script_end = Some(index);
            break;
        }
        index += 1;
    }
    let rest = after_script[script_end? + 1..].trim();
    if rest.is_empty() {
        return Some(None);
    }
    let encoded = rest.strip_prefix('\'')?.strip_suffix('\'')?;
    let wrapped = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|wrapped| !wrapped.trim().is_empty());
    Some(wrapped)
}

/// True for any command that runs Ghostex's statusline script, current path or
/// not: Ghostex owns it and may rewrite or remove it.
pub(crate) fn is_ghostex_statusline_command(command: &str) -> bool {
    command.contains("agent-statusline.sh")
}

/// Inverse of `shell_quote`.
fn shell_unquote(value: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('\'')?.strip_suffix('\'')?;
    Some(inner.replace("'\\''", "'"))
}

/// The user command a Ghostex statusline command wraps, if any.
pub(crate) fn wrapped_statusline_command(command: &str) -> Option<String> {
    if !is_ghostex_statusline_command(command) {
        return None;
    }
    #[cfg(windows)]
    if let Some(wrapped) = windows_wrapped_statusline_command(command) {
        return wrapped;
    }
    // `'<script>' '<wrapped>'` — the script token is single-quoted, so the
    // argument starts at the first quote after it.
    let after_script = command.trim().strip_prefix('\'')?;
    let script_end = after_script.find("' ")?;
    let rest = after_script[script_end + 1..].trim();
    if rest.is_empty() {
        return None;
    }
    shell_unquote(rest).filter(|wrapped| !wrapped.trim().is_empty())
}

fn statusline_entry(data: &Value) -> Option<&Map<String, Value>> {
    data.get("statusLine").and_then(Value::as_object)
}

fn statusline_entry_command(entry: &Map<String, Value>) -> Option<&str> {
    if entry.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    entry
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
}

/// Ghostex's statusline is registered with the current script path.
pub(crate) fn claude_statusline_is_current(data: &Value, hook_paths: &HookPaths) -> bool {
    statusline_is_current(data, &hook_paths.statusline_hook_path)
}

/// Point Claude's `statusLine` at the Ghostex script, wrapping any user
/// command already there. Returns whether the settings changed.
pub(crate) fn register_claude_statusline(data: &mut Value, hook_paths: &HookPaths) -> bool {
    register_statusline(data, &hook_paths.statusline_hook_path)
}

/// `statusLine` runs `script` through this gxserver, bare or wrapping a user command. The bash-script form an earlier build wrote is not current, so the next install or repair rewrites it.
#[cfg(windows)]
pub(crate) fn statusline_is_current(data: &Value, script: &Path) -> bool {
    statusline_entry(data)
        .and_then(statusline_entry_command)
        .is_some_and(|command| {
            windows_wrapped_statusline_command(command).is_some_and(|wrapped| {
                command == agent_statusline_command(script, wrapped.as_deref())
            })
        })
}

/// `statusLine` runs `script`, bare or wrapping a user command.
#[cfg(not(windows))]
pub(crate) fn statusline_is_current(data: &Value, script: &Path) -> bool {
    statusline_entry(data)
        .and_then(statusline_entry_command)
        .is_some_and(|command| {
            command == agent_statusline_command(script, None)
                || command.starts_with(&format!("{} ", statusline_script_token(script)))
        })
}

/// Point `statusLine` at `script`, wrapping any user command already there.
/// Returns whether the settings changed.
pub(crate) fn register_statusline(data: &mut Value, script: &Path) -> bool {
    let Some(object) = data.as_object_mut() else {
        return false;
    };
    let mut entry = object
        .get("statusLine")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let existing = statusline_entry_command(&entry).map(str::to_string);
    let wrapped = match existing.as_deref() {
        Some(command) if is_ghostex_statusline_command(command) => {
            wrapped_statusline_command(command)
        }
        Some(command) => Some(command.to_string()),
        None => None,
    };
    let next_command = agent_statusline_command(script, wrapped.as_deref());
    if existing.as_deref() == Some(next_command.as_str()) {
        return false;
    }
    entry.insert("type".to_string(), json!("command"));
    entry.insert("command".to_string(), json!(next_command));
    if wrapped.is_none() {
        entry.entry("padding".to_string()).or_insert(json!(0));
    }
    object.insert("statusLine".to_string(), Value::Object(entry));
    true
}

/// Restore the wrapped user command, or drop the entry Ghostex created.
/// Returns whether the settings changed.
pub(crate) fn unregister_statusline(data: &mut Value) -> bool {
    let Some(object) = data.as_object_mut() else {
        return false;
    };
    let Some(command) = object
        .get("statusLine")
        .and_then(Value::as_object)
        .and_then(statusline_entry_command)
        .map(str::to_string)
    else {
        return false;
    };
    if !is_ghostex_statusline_command(&command) {
        return false;
    }
    match wrapped_statusline_command(&command) {
        Some(wrapped) => {
            if let Some(entry) = object.get_mut("statusLine").and_then(Value::as_object_mut) {
                entry.insert("command".to_string(), json!(wrapped));
            }
        }
        None => {
            object.remove("statusLine");
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Runtime: `gxserver agent-statusline <hook state dir> [--render]`
// ---------------------------------------------------------------------------

/// Path of the stored payload for one Claude session id, when the id is safe
/// to use as a file name.
pub fn claude_statusline_payload_path(
    hook_state_directory: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    statusline_payload_path(hook_state_directory, StatuslineAgent::Claude, session_id)
}

/// Path of the stored payload for one agent session id, when the id is safe to
/// use as a file name.
pub fn statusline_payload_path(
    hook_state_directory: &Path,
    agent: StatuslineAgent,
    session_id: &str,
) -> Option<PathBuf> {
    let session_id = session_id.trim();
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        || session_id.starts_with('.')
    {
        return None;
    }
    Some(
        hook_state_directory
            .join(agent.state_directory())
            .join(format!("{session_id}.json")),
    )
}

/*
CDXC:AgentHooks 2026-09-26 WHY:
Cursor's payloads are keyed by the Ghostex session the script runs in
(`GHOSTEX_GLOBAL_SESSION_REF`, which every Ghostex terminal exports), not by
Cursor's own session id: a new Cursor chat has no agent session id on its row
until its first prompt, so a payload keyed by Cursor's id could not be found (or
watched) for the whole draft, and the chat's model pill and status line waited
for the 30-second draft probe. A Cursor run outside Ghostex stores nothing.
*/
/// Path of the stored Cursor payload for one Ghostex session.
pub fn cursor_statusline_payload_path(
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
) -> Option<PathBuf> {
    statusline_payload_path(
        hook_state_directory,
        StatuslineAgent::Cursor,
        &format!("ghostex-{project_id}-{session_id}"),
    )
}

/// The stored statusLine payload for a Claude session: the raw object Claude
/// piped, plus when it was stored.
pub struct ClaudeStatuslinePayload {
    pub payload: Map<String, Value>,
    pub updated_at: String,
    pub modified: Option<SystemTime>,
}

pub fn read_claude_statusline_payload(
    hook_state_directory: &Path,
    session_id: &str,
) -> Option<ClaudeStatuslinePayload> {
    read_statusline_payload(hook_state_directory, StatuslineAgent::Claude, session_id)
}

/// The stored statusLine payload for one agent session.
pub fn read_statusline_payload(
    hook_state_directory: &Path,
    agent: StatuslineAgent,
    session_id: &str,
) -> Option<ClaudeStatuslinePayload> {
    read_statusline_payload_file(&statusline_payload_path(
        hook_state_directory,
        agent,
        session_id,
    )?)
}

/// A stored statusLine payload file.
pub fn read_statusline_payload_file(path: &Path) -> Option<ClaudeStatuslinePayload> {
    let text = fs::read_to_string(path).ok()?;
    let data = read_json_object(&text);
    let payload = data.get("payload")?.as_object()?.clone();
    let updated_at = data
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let modified = fs::metadata(path).and_then(|meta| meta.modified()).ok();
    Some(ClaudeStatuslinePayload {
        payload,
        updated_at,
        modified,
    })
}

pub fn run_statusline_hook(args: Vec<String>) -> Result<(), DomainStateError> {
    let hook_state_dir = expand_home_path(
        args.first()
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("~/.ghostexterm"),
    );
    let render = args.iter().skip(1).any(|arg| arg == "--render");
    let agent = StatuslineAgent::from_arg(
        args.iter()
            .position(|arg| arg == "--agent")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str),
    );
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    store_and_render_statusline(&hook_state_dir, agent, render, &input)
}

/// Runtime of the Windows statusLine command: `gxserver agent-statusline-native <script> [<base64 user command>]`. It does what the bash script does on the other platforms: store the payload, then print Ghostex's line or run the wrapped user command with the same payload on its stdin.
#[cfg(windows)]
pub fn run_native_statusline_hook(args: Vec<String>) -> Result<(), DomainStateError> {
    use base64::Engine as _;
    use std::io::Write as _;
    let script = PathBuf::from(args.first().map(String::as_str).unwrap_or_default());
    let agent = if script
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("cursor-"))
    {
        StatuslineAgent::Cursor
    } else {
        StatuslineAgent::Claude
    };
    let hook_state_dir = std::env::var("GHOSTEX_AGENT_HOOK_STATE_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            super::install::notify_hook_state_directory(&super::probing::read_file_text(&script))
        })
        .ok_or_else(|| {
            DomainStateError::corrupt_state(
                "statusline script does not name a hook state directory".to_string(),
            )
        })?;
    let wrapped = args
        .get(1)
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|wrapped| !wrapped.trim().is_empty());
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Some(wrapped) = wrapped else {
        return store_and_render_statusline(&hook_state_dir, agent, true, &input);
    };
    let _ = store_and_render_statusline(&hook_state_dir, agent, false, &input);
    kill_children_with_this_process();
    // The user's command was written for the shell Claude Code runs it in: Git Bash when Git for Windows is installed, PowerShell otherwise.
    let mut command = super::statusline_launch_windows::wrapped_command(&wrapped);
    let mut child = crate::platform::process::NoConsoleWindow::no_console_window(&mut command)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let _ = child.wait();
    Ok(())
}

/// Claude Code kills a statusLine command that overruns or is superseded, but on Windows that kills only gxserver: the wrapped command's bash and everything under it (`npx`, `node`) were orphaned and piled up by the hundreds. A kill-on-close job holding this process takes them down with it, since children inherit the job. Best effort: a failure leaves the old behaviour.
#[cfg(windows)]
fn kill_children_with_this_process() {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::GetCurrentProcess,
        },
    };

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let limited = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0;
        if !limited || AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            CloseHandle(job);
        }
        // On success the handle stays open for the life of this process; the
        // OS closes it on exit or kill, which kills the job's processes.
    }
}

fn store_and_render_statusline(
    hook_state_dir: &Path,
    agent: StatuslineAgent,
    render: bool,
    input: &str,
) -> Result<(), DomainStateError> {
    let payload = serde_json::from_str::<Value>(input)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let path = match agent {
        StatuslineAgent::Claude => payload
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|session_id| statusline_payload_path(hook_state_dir, agent, session_id)),
        StatuslineAgent::Cursor => {
            std::env::var("GHOSTEX_GLOBAL_SESSION_REF")
                .ok()
                .and_then(|reference| {
                    let (project_id, session_id) = parse_global_session_ref(&reference);
                    cursor_statusline_payload_path(hook_state_dir, &project_id?, &session_id?)
                })
        }
    };
    if let Some(path) = path {
        store_statusline_payload(&path, &payload)?;
    }
    if render {
        let line = match agent {
            StatuslineAgent::Claude => render_statusline(&payload),
            StatuslineAgent::Cursor => render_cursor_statusline(&payload),
        };
        println!("{line}");
    }
    Ok(())
}

fn store_statusline_payload(
    path: &Path,
    payload: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(io_error)?;
    let is_new_session = !path.exists();
    let data = json!({
        "version": 1,
        "updatedAt": now_iso(),
        "payload": payload,
    });
    let text = serde_json::to_string(&data).map_err(|error| {
        DomainStateError::corrupt_state(format!("statusline payload is not serializable: {error}"))
    })?;
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, text).map_err(io_error)?;
    fs::rename(&temp_path, path).map_err(io_error)?;
    if is_new_session {
        prune_stale_statusline_payloads(parent, path);
    }
    Ok(())
}

/// Every session Claude ever ran leaves a file; a new session is the natural,
/// rare moment to drop the ones nobody can follow any more.
fn prune_stale_statusline_payloads(directory: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > CLAUDE_STATUSLINE_PAYLOAD_RETENTION);
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering (the line Claude shows when no user statusline is wrapped)
// ---------------------------------------------------------------------------

fn payload_str<'a>(payload: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    let mut current: &Value = payload.get(*keys.first()?)?;
    for key in &keys[1..] {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_f64(payload: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    let mut current: &Value = payload.get(*keys.first()?)?;
    for key in &keys[1..] {
        current = current.get(*key)?;
    }
    current.as_f64()
}

/*
CDXC:AgentHooks 2026-09-03 DECISION:
User: the default Claude statusline uses their own two-row layout (model, effort, context, cost, rate limits; then session id, project, branch), without the second context percentage and without the account email.
Both rows are `|` delimited so the screen grammar in session_chat_options/agent_matchers.rs
reads the model and effort segments exactly as it reads a user's custom line:

    Fable 5.1 | medium | Ctx 25% | $11.14 | 5h 23% · 7d 41%
    70b2ea81-… | …/Ghostex | main

Every segment is optional. The model carries its version, derived from
`model.id` the way the transcript parser does, because `display_name` is the
bare family. Claude draws its own permission-mode footer beneath these.
*/
pub fn render_statusline(payload: &Map<String, Value>) -> String {
    let mut first: Vec<String> = Vec::new();
    if let Some(model) = payload_str(payload, &["model", "id"])
        .and_then(crate::session_chat_options::claude_transcript_model_choice)
        .map(|choice| choice.label)
        .or_else(|| payload_str(payload, &["model", "display_name"]).map(str::to_string))
    {
        first.push(model);
    }
    if let Some(effort) = payload_str(payload, &["effort", "level"]) {
        first.push(effort.to_string());
    }
    if payload.get("fast_mode").and_then(Value::as_bool) == Some(true) {
        first.push("fast".to_string());
    }
    if let Some(used) = payload_f64(payload, &["context_window", "used_percentage"]) {
        first.push(format!("Ctx {}%", used.round() as i64));
    }
    if let Some(cost) = payload_f64(payload, &["cost", "total_cost_usd"]) {
        first.push(format!("${cost:.2}"));
    }
    let limits: Vec<String> = [("five_hour", "5h"), ("seven_day", "7d")]
        .iter()
        .filter_map(|(key, label)| {
            payload_f64(payload, &["rate_limits", key, "used_percentage"])
                .map(|used| format!("{label} {}%", used.round() as i64))
        })
        .collect();
    if !limits.is_empty() {
        first.push(limits.join(" \u{b7} "));
    }

    let mut second: Vec<String> = Vec::new();
    if let Some(session_id) = payload_str(payload, &["session_id"]) {
        second.push(session_id.to_string());
    }
    if let Some(project) = payload_str(payload, &["workspace", "project_dir"])
        .or_else(|| payload_str(payload, &["cwd"]))
        .and_then(|dir| Path::new(dir).file_name())
        .and_then(|name| name.to_str())
    {
        second.push(format!("\u{2026}/{project}"));
    }
    if let Some(branch) = payload_str(payload, &["worktree", "branch"])
        .or_else(|| payload_str(payload, &["workspace", "git_worktree"]))
    {
        second.push(branch.to_string());
    }

    [first, second]
        .into_iter()
        .filter(|row| !row.is_empty())
        .map(|row| row.join(" | "))
        .collect::<Vec<_>>()
        .join("\n")
}

/*
CDXC:AgentHooks 2026-09-24 WHY:
Registering a statusLine replaces Cursor's built-in footer, so with no user
command wrapped Ghostex prints a line in the grammar `match_cursor_statusline`
reads (`<title> · <model> <params> · <tokens> used`); the chat pills keep their
terminal source and the footer still names the session and model.
*/
pub fn render_cursor_statusline(payload: &Map<String, Value>) -> String {
    let title = payload_str(payload, &["session_name"]).unwrap_or("New Agent");
    let model = payload_str(payload, &["model", "display_name"]).unwrap_or("Cursor");
    let params = payload_str(payload, &["model", "param_summary"])
        .map(|params| params.trim_matches(|ch| ch == '(' || ch == ')').trim())
        .filter(|params| !params.is_empty() && !model.contains(*params));
    let model = match params {
        Some(params) => format!("{model} {params}"),
        None => model.to_string(),
    };
    let tokens = payload_f64(payload, &["context_window", "total_input_tokens"]).unwrap_or(0.0);
    let used = if tokens >= 1_000_000.0 {
        format!("{}M", (tokens / 1_000_000.0).floor() as i64)
    } else if tokens >= 1_000.0 {
        format!("{}K", (tokens / 1_000.0).floor() as i64)
    } else {
        format!("{}", tokens.max(0.0) as i64)
    };
    format!("{title} \u{b7} {model} \u{b7} {used} used")
}
