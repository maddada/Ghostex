// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the Command Pane Action execution-text,
// status-file, and mounted/staged-script helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandActionRunFileStatus {
    Working,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandActionStatusFile {
    pub(crate) exit_code: i32,
    pub(crate) status: GpuiCommandActionRunFileStatus,
    pub(crate) run_id: String,
}

pub(crate) fn gpui_command_action_execution_text_for_current_backend(
    command: &str,
    run_id: &str,
    status_file_path: &Path,
) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
    ) {
        return gpui_powershell_command_action_execution_text(command, run_id, status_file_path);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = status_file_path;
    gpui_command_action_execution_text(command, run_id)
}

pub(crate) fn gpui_command_action_startup_text(execution_text: &str, status_file_path: &Path) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
    ) {
        return format!("{execution_text}\r");
    }
    format!(
        "{}\r",
        gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_powershell_command_action_execution_text(
    command: &str,
    run_id: &str,
    status_file_path: &Path,
) -> String {
    /*
    PowerShell command tabs use the same bounded status-file contract as the
    Unix wrapper, but write it with native PowerShell syntax and ASCII output
    so the existing parser never sees a UTF-8 BOM on its first key. The saved
    command remains visible terminal input and runs in the interactive ConPTY
    shell; this wrapper does not start a hidden process or persist the command.
    */
    let state_file = gpui_powershell_single_quote(status_file_path.to_string_lossy().as_ref());
    let run_id = gpui_powershell_single_quote(run_id);
    format!(
        r#"$__ghostexStateFile = '{state_file}'
$__ghostexStateDir = Split-Path -Parent $__ghostexStateFile
if ($__ghostexStateDir) {{ New-Item -ItemType Directory -Force -Path $__ghostexStateDir | Out-Null }}
$__ghostexUpdatedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
@('status=working', "statusUpdatedAt=$__ghostexUpdatedAt", 'commandRunId={run_id}', 'commandExitCode=0', "lastActivityAt=$__ghostexUpdatedAt") | Set-Content -LiteralPath $__ghostexStateFile -Encoding Ascii
$global:LASTEXITCODE = $null
& {{
{command}
}}
$__ghostexSucceeded = $?
$__ghostexExit = if ($null -ne $LASTEXITCODE) {{ [int]$LASTEXITCODE }} elseif ($__ghostexSucceeded) {{ 0 }} else {{ 1 }}
if ($__ghostexExit -lt 0 -or $__ghostexExit -gt 255) {{ $__ghostexExit = 1 }}
$__ghostexUpdatedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
@('status=idle', "statusUpdatedAt=$__ghostexUpdatedAt", 'commandRunId={run_id}', "commandExitCode=$__ghostexExit", "lastActivityAt=$__ghostexUpdatedAt") | Set-Content -LiteralPath $__ghostexStateFile -Encoding Ascii
"#
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_powershell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

pub(crate) fn gpui_command_action_execution_text(command: &str, run_id: &str) -> String {
    /*
    CDXC:GPUICommandPane 2026-06-24-23:36:
    GPUI command-pane Actions need the same hidden process-command shape as the macOS app: wrap the saved command in a function, preserve the user's output as the visible terminal result, then return to an interactive login shell instead of pasting the wrapper into the prompt or closing the pane.

    CDXC:GPUICommandPane 2026-06-24-23:36:
    The wrapper stamps only safe command lifecycle fields into the env-provided session-state file before and after the action. That file is how GPUI can clear a reused command-pane tab from Working back to Idle without parsing terminal output, titles, command text, cwd, env, or stdout/stderr.
    */
    let working_stamp = gpui_command_action_status_stamp_text("working", run_id, "0");
    let idle_stamp = gpui_command_action_status_stamp_text("idle", run_id, "$__ghostex_exit");
    gpui_with_atuin_ignored_shell_history_prefix(
        vec![
            "__ghostex_command_pane_action() {",
            command,
            "}",
            working_stamp.as_str(),
            "__ghostex_command_pane_action",
            "__ghostex_exit=$?",
            "unset -f __ghostex_command_pane_action",
            idle_stamp.as_str(),
            "exec /bin/zsh -l",
        ]
        .join("\n"),
    )
}

pub(crate) fn gpui_debug_command_action_initial_input(command: &str) -> String {
    gpui_with_atuin_ignored_shell_history_prefix(format!("{command}\r"))
}

pub(crate) fn gpui_with_atuin_ignored_shell_history_prefix(text: String) -> String {
    if text.starts_with(' ') {
        text
    } else {
        format!(" {text}")
    }
}

pub(crate) fn gpui_shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn gpui_command_action_status_stamp_text(status: &str, run_id: &str, exit_code: &str) -> String {
    /*
    CDXC:GPUICommandPaneActions 2026-06-26-06:18:
    Command Action status stamps must keep macOS parity by writing a replacement file beside the session-state file with shell process uniqueness before atomically moving it into place. Do not use a shared fixed temp file because concurrent command/status writers can clobber the idle stamp that drives completion sound.
    */
    let status = gpui_shell_single_quote(status);
    let run_id = gpui_shell_single_quote(run_id);
    vec![
        "__ghostex_session_state_file=\"${GHOSTEX_SESSION_STATE_FILE:-${VSMUX_SESSION_STATE_FILE:-$ghostex_SESSION_STATE_FILE}}\"".to_string(),
        "if [ -n \"$__ghostex_session_state_file\" ]; then".to_string(),
        "  __ghostex_state_dir=\"${__ghostex_session_state_file:h}\"".to_string(),
        "  [ \"$__ghostex_state_dir\" = \"$__ghostex_session_state_file\" ] && __ghostex_state_dir=\".\"".to_string(),
        "  mkdir -p -- \"$__ghostex_state_dir\"".to_string(),
        "  __ghostex_status_updated_at=\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"".to_string(),
        "  __ghostex_state_tmp=\"$__ghostex_session_state_file.$$.$RANDOM.command.tmp\"".to_string(),
        "  {".to_string(),
        format!("    printf 'status=%s\\n' {status}"),
        "    printf 'statusUpdatedAt=%s\\n' \"$__ghostex_status_updated_at\"".to_string(),
        format!("    printf 'commandRunId=%s\\n' {run_id}"),
        format!("    printf 'commandExitCode=%s\\n' {exit_code}"),
        "    printf 'lastActivityAt=%s\\n' \"$__ghostex_status_updated_at\"".to_string(),
        "  } > \"$__ghostex_state_tmp\" && /bin/mv -f -- \"$__ghostex_state_tmp\" \"$__ghostex_session_state_file\"".to_string(),
        "fi".to_string(),
    ]
    .join("\n")
}

pub(crate) fn create_gpui_command_action_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("gpui-{millis}-{}", std::process::id())
}

pub(crate) fn gpui_command_action_status_file_path(session_id: CommandSessionId) -> PathBuf {
    let directory = ghostex_state_root().join("gpui-command-actions");
    prune_stale_gpui_command_action_temp_files(&directory);
    directory.join(format!("command-session-{}.state", session_id.0))
}

pub(crate) fn prune_stale_gpui_command_action_temp_files(directory: &Path) {
    const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

    let Some(cutoff) = SystemTime::now().checked_sub(STALE_AFTER) else {
        return;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("command-session-") || !name.ends_with(".command.tmp") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() && metadata.modified().is_ok_and(|modified| modified <= cutoff) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn gpui_command_action_state_export_text(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        let windows_path = gpui_shell_single_quote(path.to_string_lossy().as_ref());
        return [
            format!("__ghostex_windows_session_state_file={windows_path}"),
            "__ghostex_session_state_file=\"$(wslpath -a -u \"$__ghostex_windows_session_state_file\")\"".to_string(),
            "export GHOSTEX_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "export VSMUX_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "export ghostex_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "unset __ghostex_windows_session_state_file __ghostex_session_state_file".to_string(),
        ]
        .join("\n");
    }
    let path = gpui_shell_single_quote(path.to_string_lossy().as_ref());
    [
        format!("export GHOSTEX_SESSION_STATE_FILE={path}"),
        format!("export VSMUX_SESSION_STATE_FILE={path}"),
        format!("export ghostex_SESSION_STATE_FILE={path}"),
    ]
    .join("\n")
}

pub(crate) fn gpui_command_action_mounted_terminal_script_text(
    execution_text: &str,
    status_file_path: &Path,
) -> String {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Mounted reused default Actions need the same wrapper text as a created Action launch payload, but it belongs in a private staged script so the interactive shell sees only a short source command. Keep the status-file path inside process-local script/env setup and out of sidebar bridges, shell-state JSON, logs, and command titles.
    */
    format!(
        "{}\n{execution_text}\n",
        gpui_command_action_state_export_text(status_file_path)
    )
}

// CDXC:GPUILinuxX11Backend 2026-07-05: staged mounted-Action scripts are
// plain POSIX (temp script + 0600/0700 permissions + `. path; rm path`), and
// the mounted GPUI-engine terminal branch that consumes them is
// cross-platform, so the staging helpers are unix-wide rather than
// macOS-only. main.rs already imports PermissionsExt unconditionally.
#[cfg(unix)]
pub(crate) fn gpui_command_action_staged_mounted_script_source_command(
    execution_text: &str,
    status_file_path: &Path,
) -> Option<String> {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Native `writeTerminalScript` parity for mounted Action reuse stages the full wrapper in a temp script and submits `. script; rm script` through the current interactive shell. The staged command carries only the temp script path, not command text, run ids, status-file paths, cwd/env values, terminal output, or persisted shell metadata.
    */
    let script_path = gpui_command_action_staged_mounted_script_path();
    let directory = script_path.parent()?;
    fs::create_dir_all(directory).ok()?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).ok()?;
    let script_text =
        gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script_path)
        .ok()?;
    if file.write_all(script_text.as_bytes()).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    drop(file);
    if fs::set_permissions(&script_path, fs::Permissions::from_mode(0o600)).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    Some(gpui_command_action_mounted_script_source_command(
        &script_path,
    ))
}

#[cfg(not(unix))]
pub(crate) fn gpui_command_action_staged_mounted_script_source_command(
    execution_text: &str,
    status_file_path: &Path,
) -> Option<String> {
    let _ = (execution_text, status_file_path);
    None
}

#[cfg(unix)]
pub(crate) fn gpui_command_action_staged_mounted_script_path() -> PathBuf {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir()
        .join("ghostex-restore-scripts")
        .join(format!(
            "gpui-command-action-{}-{unique_id}.zsh",
            std::process::id()
        ))
}

pub(crate) fn gpui_command_action_mounted_script_source_command(script_path: &Path) -> String {
    let path = gpui_shell_single_quote(script_path.to_string_lossy().as_ref());
    format!(" . {path}; /bin/rm -f -- {path}")
}

pub(crate) fn gpui_command_action_should_insert_launch_payload(
    selection_kind: CommandPaneActionSessionSelectionKind,
    mounted_reuse_surface_available: bool,
    wrote_to_mounted_reuse: bool,
) -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Native default Action routing has two mutually exclusive execution paths: mounted idle reuse writes to the current command surface and never queues startup data, while created or unmounted Action tabs receive an exact-slot launch payload for their first mount. A failed mounted write must not become a hidden later launch payload for the same live shell.
    */
    match selection_kind {
        CommandPaneActionSessionSelectionKind::Created => true,
        CommandPaneActionSessionSelectionKind::Reused => {
            !mounted_reuse_surface_available && !wrote_to_mounted_reuse
        }
        CommandPaneActionSessionSelectionKind::ReusedActive => false,
    }
}

pub(crate) fn gpui_command_action_title_key(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(crate) fn gpui_command_action_status_from_file(path: &Path) -> Option<GpuiCommandActionStatusFile> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > 8 * 1024 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let mut exit_code = None;
    let mut status = None;
    let mut run_id = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "status" => {
                status = match value {
                    "working" => Some(GpuiCommandActionRunFileStatus::Working),
                    "idle" => Some(GpuiCommandActionRunFileStatus::Idle),
                    _ => None,
                };
            }
            "commandRunId" => {
                if !value.is_empty()
                    && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                    && !value.chars().any(char::is_control)
                {
                    run_id = Some(value.to_string());
                }
            }
            "commandExitCode" => {
                if !value.is_empty()
                    && value.len() <= 3
                    && value.chars().all(|character| character.is_ascii_digit())
                {
                    exit_code = value
                        .parse::<i32>()
                        .ok()
                        .filter(|code| (0..=255).contains(code));
                }
            }
            _ => {}
        }
    }
    Some(GpuiCommandActionStatusFile {
        exit_code: exit_code?,
        status: status?,
        run_id: run_id?,
    })
}

/*
CDXC:SidebarBrowserTabReveal 2026-08-18:
The reveal payload names the tab in Rust's own vocabulary (project id + tab id).
Turning that into the sidebar's session id belongs to the sidebar runtime, which
is the code that builds those rows in the first place.
*/
pub(crate) fn gpui_remote_attach_session_reference_from_project_id(
    project_id: &str,
) -> Option<GpuiRemoteAttachSessionReference> {
    let rest = project_id.strip_prefix("remote:")?;
    let (remote_machine_id, rest) = rest.split_once(":session:")?;
    let (project_id, session_id) = rest.split_once(':')?;
    let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
    if !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some(GpuiRemoteAttachSessionReference {
        remote_machine_id,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

