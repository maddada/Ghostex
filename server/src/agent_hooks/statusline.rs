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
  - with no user statusline it rendered Ghostex's own line (`Fable | high |
    Ctx 8% | $0.12`); since 2026-11-18 (Q4161, below) Claude's row stays empty;
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

use serde_json::{Map, Value, json};

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

    /// Whether the helper prints a line of its own when no user command is wrapped: only
    /// Cursor, whose footer a statusLine replaces (CDXC:AgentHooks 2026-11-18).
    fn renders_own_line(self) -> bool {
        matches!(self, Self::Cursor)
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

/*
CDXC:AgentHooks 2026-11-18 DECISION:
The user chose (Q4161) that the helper prints nothing of its own in Claude's
status row, or only their own wrapped line, so Claude looks as it would without
Ghostex: the chat reads the stored payload, never the printed line. Cursor keeps
its line, because a registered statusLine replaces Cursor's own footer and
Ghostex's line is the stand-in that names the session and model (see
`render_cursor_statusline`). They also chose (Q4162) that the script does
nothing outside Ghostex, by the notify hook's test (a session ref and a gxserver
address in the environment): it starts no gxserver there, only runs their own
command, so a Claude in another terminal, over ssh or in another app pays two
shell tests per refresh. Cursor without a command of the user's still starts gxserver
there to draw its footer's stand-in (stored nowhere), or its footer would be empty.
Version 2 of the script, so installed ones are rewritten; version 3 for the Cursor
stand-in outside Ghostex.
*/
pub(crate) fn build_statusline_hook_script(
    executable: &str,
    hook_state_directory: &Path,
    agent: StatuslineAgent,
) -> String {
    let script = format!(
        r#"#!/bin/bash
# {STATUSLINE_HOOK_MARKER} v{STATUSLINE_HOOK_VERSION}
# Claude Code statusLine command installed by Ghostex. Claude pipes its status
# JSON on stdin; inside Ghostex gxserver stores it for the session's chat view.
# A wrapped user statusline command ($1) draws the line; otherwise {silence}.
# Outside Ghostex nothing is stored, and no gxserver starts unless it must draw the line.
if [ -z "${{GHOSTEX_GLOBAL_SESSION_REF:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_BASE_URL:-}}" ]; then
  if [ -n "${{1:-}}" ]; then
    exec /bin/sh -c "$1"
  fi
{outside_without_command}fi
INPUT="$(cat)"
DEFAULT_HOOK_STATE_DIR={hook_state_directory}
HOOK_STATE_DIR="${{GHOSTEX_AGENT_HOOK_STATE_DIR:-$DEFAULT_HOOK_STATE_DIR}}"
if [ -n "${{1:-}}" ]; then
  printf '%s' "$INPUT" | {executable} agent-statusline "$HOOK_STATE_DIR"{arguments} >/dev/null 2>&1 || true
  printf '%s' "$INPUT" | /bin/sh -c "$1"
  exit 0
fi
printf '%s' "$INPUT" | {executable} agent-statusline "$HOOK_STATE_DIR"{arguments}{render} 2>/dev/null
exit 0
"#,
        executable = shell_quote(executable),
        hook_state_directory = shell_quote(&path_string(hook_state_directory)),
        arguments = agent.runtime_arguments(),
        render = if agent.renders_own_line() {
            " --render"
        } else {
            " >/dev/null"
        },
        // Cursor's footer is replaced by its statusLine, so outside Ghostex it still draws the
        // stand-in (gxserver stores nothing there: the payload is keyed by the Ghostex session).
        outside_without_command = if agent.renders_own_line() {
            ""
        } else {
            "  read -r -d '' _ || true\n  exit 0\n"
        },
        silence = if agent.renders_own_line() {
            "gxserver prints Ghostex's stand-in for the footer"
        } else {
            "the row stays empty"
        },
    );
    match agent {
        StatuslineAgent::Claude => script,
        StatuslineAgent::Cursor => script
            .replace("# Claude Code statusLine", "# Cursor CLI statusLine")
            .replace("Claude pipes", "Cursor pipes"),
    }
}

#[cfg(windows)]
/// Whether this process runs in a Ghostex session: the notify hook's test (CDXC:AgentHooks
/// 2026-11-18 above).
fn inside_ghostex_session() -> bool {
    ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_GXSERVER_BASE_URL"]
        .iter()
        .all(|key| std::env::var(key).is_ok_and(|value| !value.trim().is_empty()))
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

/// True for a command in the exact form Ghostex writes (its script, current path or not, alone or
/// wrapping one user command): Ghostex owns it and may rewrite or remove it.
///
/// CDXC:AgentHooks 2026-11-18 WHY: review of c33dcd576: any command containing
/// `agent-statusline.sh` was claimed, so a user's own `my-agent-statusline.sh` was taken for
/// Ghostex's, dropped when the helper was turned off and never wrapped. Only Ghostex's own shapes
/// count now; anything else is a user command and is wrapped like one.
pub(crate) fn is_ghostex_statusline_command(command: &str) -> bool {
    parse_ghostex_statusline_command(command).is_some()
}

/// Ghostex's statusLine command split into the user command it wraps: `None` when the command is
/// not Ghostex's, `Some(None)` when it wraps nothing.
fn parse_ghostex_statusline_command(command: &str) -> Option<Option<String>> {
    #[cfg(windows)]
    if let Some(wrapped) = windows_wrapped_statusline_command(command) {
        return Some(wrapped);
    }
    // `'<hooks>/agent-statusline.sh'` or `'<hooks>/cursor-agent-statusline.sh'`, then nothing or
    // one more single-quoted word (`shell_quote`).
    let (script, rest) = take_shell_quoted(command.trim())?;
    if !is_ghostex_statusline_script(&script) {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(None);
    }
    let (wrapped, tail) = take_shell_quoted(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some(Some(wrapped).filter(|wrapped| !wrapped.trim().is_empty()))
}

/// A path that names one of Ghostex's statusline scripts: `agent-statusline.sh` or
/// `cursor-agent-statusline.sh` in a `hooks` folder.
fn is_ghostex_statusline_script(script: &str) -> bool {
    let mut parts = script.rsplit(['/', '\\']);
    matches!(
        parts.next(),
        Some("agent-statusline.sh" | "cursor-agent-statusline.sh")
    ) && parts.next() == Some("hooks")
}

/// The first `shell_quote`d word of `text` (`'...'`, with `'\\''` for a quote) and what follows it.
fn take_shell_quoted(text: &str) -> Option<(String, &str)> {
    let mut rest = text.strip_prefix('\'')?;
    let mut word = String::new();
    loop {
        let end = rest.find('\'')?;
        word.push_str(&rest[..end]);
        rest = &rest[end + 1..];
        match rest.strip_prefix("\\''") {
            Some(after) => {
                word.push('\'');
                rest = after;
            }
            None => return Some((word, rest)),
        }
    }
}

/// The user command a Ghostex statusline command wraps, if any.
pub(crate) fn wrapped_statusline_command(command: &str) -> Option<String> {
    parse_ghostex_statusline_command(command).flatten()
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

/// A `statusLine` that runs Ghostex's script, current or not.
pub(crate) fn ghostex_statusline_registered(data: &Value) -> bool {
    statusline_entry(data)
        .and_then(statusline_entry_command)
        .is_some_and(is_ghostex_statusline_command)
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
    let started = helper_started();
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
    let Some((input, late_since)) = read_statusline_input(started) else {
        return Ok(());
    };
    store_and_render_statusline(&hook_state_dir, agent, render, &input, late_since)
}

/// CDXC:AgentHooks 2026-11-18 WHY:
/// GitHub #666 (Windows, about 40 agents): a statusline helper read its stdin with no time limit, so one whose writer never closed the pipe waited for ever, and they piled up (about 7 an hour). Claude Code and Cursor write the whole payload at once, so after this long the helper uses what arrived when it is a whole JSON object (a writer that sent the payload but kept the pipe open), and otherwise gives up quietly: it stores and prints nothing, as an empty update would.
const STATUSLINE_INPUT_LIMIT: Duration = Duration::from_secs(5);

/// When this helper began: on Windows its process's creation time, elsewhere the moment the hook
/// starts, so a helper slow to start is not taken for a later one by the late-payload guard
/// (`store_and_render_statusline`).
fn helper_started() -> SystemTime {
    #[cfg(windows)]
    if let Some(created) =
        crate::platform::process::process_creation_filetime(i64::from(std::process::id()))
    {
        // FILETIME counts 100 ns ticks from 1601.
        const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
        if let Some(since) = created.checked_sub(UNIX_EPOCH_FILETIME) {
            return SystemTime::UNIX_EPOCH + Duration::from_nanos(since.saturating_mul(100));
        }
    }
    SystemTime::now()
}

/// The payload on stdin, or `None` when it did not end within `STATUSLINE_INPUT_LIMIT` and what arrived is not a whole JSON object. A payload used at the limit comes with when this helper began reading (`store_and_render_statusline` then never stores it over a newer one). The reading thread is left blocked; returning ends the process with it.
fn read_statusline_input(started: SystemTime) -> Option<(String, Option<SystemTime>)> {
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (sender, receiver) = std::sync::mpsc::channel();
    let buffer = received.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut chunk = [0_u8; 8192];
        loop {
            match stdin.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => match buffer.lock() {
                    Ok(mut buffer) => buffer.extend_from_slice(&chunk[..count]),
                    Err(_) => break,
                },
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = sender.send(());
    });
    let ended = receiver.recv_timeout(STATUSLINE_INPUT_LIMIT).is_ok();
    let bytes = received
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default();
    // Not UTF-8: empty, as `read_to_string` left it.
    let text = String::from_utf8(bytes).unwrap_or_default();
    if ended {
        return Some((text, None));
    }
    serde_json::from_str::<Value>(&text)
        .is_ok_and(|value| value.is_object())
        .then_some((text, Some(started)))
}

/// CDXC:AgentHooks 2026-11-18 WHY:
/// GitHub #666: on Windows, killing the shell that ran the statusline command leaves gxserver running, and one waiting on a wrapped user command whose parent was killed waited for ever. This watches the process that started it and exits when that one does; the kill-on-close job (`kill_children_with_this_process`) then takes the wrapped command down too. A parent that is already gone (OpenProcess answers ERROR_INVALID_PARAMETER for a pid no process has, and a pid that now names a process started after this one was reused) exits at once; one that cannot be opened for another reason (access denied), or a wait that fails, is not watched. The parent pid comes from this process's own basic information, not a snapshot of every process, since this runs on every refresh.
#[cfg(windows)]
fn exit_when_parent_exits() {
    use std::mem::size_of;
    use windows_sys::{
        Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation},
        Win32::{
            Foundation::{
                CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, WAIT_OBJECT_0,
            },
            System::Threading::{
                GetCurrentProcess, GetProcessTimes, INFINITE, OpenProcess,
                PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
                WaitForSingleObject,
            },
        },
    };
    unsafe {
        let mut information: PROCESS_BASIC_INFORMATION = std::mem::zeroed();
        let mut length = 0_u32;
        let status = NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessBasicInformation,
            (&mut information as *mut PROCESS_BASIC_INFORMATION).cast(),
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut length,
        );
        let Some(parent) = u32::try_from(information.InheritedFromUniqueProcessId)
            .ok()
            .filter(|parent| status >= 0 && *parent != 0)
        else {
            return;
        };
        let own_created =
            crate::platform::process::process_creation_filetime(i64::from(std::process::id()));
        let process = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            parent,
        );
        if process.is_null() {
            if GetLastError() == ERROR_INVALID_PARAMETER {
                std::process::exit(0);
            }
            return;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [creation, exit, kernel, user] = &mut times;
        let parent_created = (GetProcessTimes(process, creation, exit, kernel, user) != 0)
            .then(|| ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64);
        if let (Some(own), Some(parent)) = (own_created, parent_created) {
            if parent > own {
                CloseHandle(process);
                std::process::exit(0);
            }
        }
        // A raw handle is not `Send`; the thread owns it until the process ends.
        let process = process as usize;
        std::thread::spawn(move || {
            if WaitForSingleObject(process as _, INFINITE) == WAIT_OBJECT_0 {
                std::process::exit(0);
            }
        });
    }
}

/// Runtime of the Windows statusLine command: `gxserver agent-statusline-native <script> [<base64 user command>]`. It does what the bash script does on the other platforms: store the payload, then print Ghostex's line or run the wrapped user command with the same payload on its stdin.
#[cfg(windows)]
pub fn run_native_statusline_hook(args: Vec<String>) -> Result<(), DomainStateError> {
    use base64::Engine as _;
    use std::io::Write as _;
    let started = helper_started();
    exit_when_parent_exits();
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
    let Some((input, late_since)) = read_statusline_input(started) else {
        return Ok(());
    };
    let inside = inside_ghostex_session();
    let Some(wrapped) = wrapped else {
        if inside {
            return store_and_render_statusline(
                &hook_state_dir,
                agent,
                agent.renders_own_line(),
                &input,
                late_since,
            );
        }
        // Outside Ghostex nothing is stored; Cursor still gets its footer's stand-in.
        if agent.renders_own_line() {
            println!("{}", render_agent_line(agent, &input));
        }
        return Ok(());
    };
    if inside {
        let _ = store_and_render_statusline(&hook_state_dir, agent, false, &input, late_since);
    }
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
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
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
    late_since: Option<SystemTime>,
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
    // CDXC:AgentHooks 2026-11-18 WHY: a payload used at the input limit is up to 5 s old, and a
    // later helper for the same session may already have stored a newer one (seen in review: the
    // file read NEW, then OLD 4 s later), which left the chat's pills and the shared 5h/7d limits
    // stale. Such a payload is stored only when the file has not changed since this helper began
    // reading; its line is still printed. A late store dates the file to when its helper began
    // reading, so two late helpers are ordered by that too (the one that began later wins, as
    // the payload it read is the newer one); the change watch compares times with `!=`, so it
    // still fires.
    let newer_stored = |path: &Path| {
        late_since.is_some_and(|started| {
            fs::metadata(path)
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified > started)
        })
    };
    if let Some(path) = path.filter(|path| !newer_stored(path)) {
        store_statusline_payload(&path, &payload, late_since)?;
    }
    if render {
        println!("{}", render_agent_payload_line(agent, &payload));
    }
    Ok(())
}

fn render_agent_payload_line(agent: StatuslineAgent, payload: &Map<String, Value>) -> String {
    match agent {
        StatuslineAgent::Claude => render_statusline(payload),
        StatuslineAgent::Cursor => render_cursor_statusline(payload),
    }
}

/// The line for a raw payload, without storing it.
#[cfg(windows)]
fn render_agent_line(agent: StatuslineAgent, input: &str) -> String {
    let payload = serde_json::from_str::<Value>(input)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    render_agent_payload_line(agent, &payload)
}

/// `dated`: the modified time the stored file gets, instead of now.
fn store_statusline_payload(
    path: &Path,
    payload: &Map<String, Value>,
    dated: Option<SystemTime>,
) -> Result<(), DomainStateError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(io_error)?;
    let is_new_session = !path.exists();
    // A dated store's `updatedAt` is that same time (agent_model_pins.rs reads it).
    let updated_at = dated.map_or_else(now_iso, |dated| {
        chrono::DateTime::<chrono::Utc>::from(dated)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    });
    let data = json!({
        "version": 1,
        "updatedAt": updated_at,
        "payload": payload,
    });
    let text = serde_json::to_string(&data).map_err(|error| {
        DomainStateError::corrupt_state(format!("statusline payload is not serializable: {error}"))
    })?;
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, text).map_err(io_error)?;
    if let Some(dated) = dated {
        let set = fs::File::options()
            .write(true)
            .open(&temp_path)
            .and_then(|file| file.set_modified(dated));
        // Undated, it would read as newer than it is; this late payload is dropped instead and the
        // line is still printed.
        if set.is_err() {
            let _ = fs::remove_file(&temp_path);
            return Ok(());
        }
    }
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
