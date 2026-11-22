use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::domain::DomainStateError;

use crate::agent_hooks::config::{HookPaths, NOTIFY_HOOK_MARKER, NOTIFY_HOOK_VERSION};
use crate::agent_hooks::plugin_sources::{build_notify_hook_script, shell_quote};
use crate::agent_hooks::probing::{io_error, path_string, read_file_text, temp_path_for};

use super::*;

pub(crate) fn install_notify_hook(hook_paths: &HookPaths) -> Result<(), DomainStateError> {
    if let Some(parent) = hook_paths.notify_hook_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::create_dir_all(&hook_paths.hook_state_directory).map_err(io_error)?;
    let executable = env::current_exe()
        .ok()
        .map(|path| path_string(&path))
        .unwrap_or_else(|| "gxserver".to_string());
    let script = build_notify_hook_script(&executable, &hook_paths.hook_state_directory);
    write_executable_notify_hook(&hook_paths.notify_hook_path, &script)?;
    Ok(())
}

pub(crate) fn write_executable_notify_hook(
    path: &Path,
    contents: &str,
) -> Result<(), DomainStateError> {
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, contents).map_err(io_error)?;
    set_executable_permissions(&temp_path).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(&temp_path);
    fs::rename(&temp_path, path).map_err(io_error)?;
    set_executable_permissions(path).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(path);
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_executable_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
pub(crate) fn set_executable_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn parent_process_id() -> u32 {
    unsafe { libc::getppid() as u32 }
}

#[cfg(windows)]
pub(crate) fn parent_process_id() -> u32 {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
    };

    let current_pid = std::process::id();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return 0;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Process32FirstW(snapshot, &mut entry) != 0;
        let mut parent_pid = 0;
        while found {
            if entry.th32ProcessID == current_pid {
                parent_pid = entry.th32ParentProcessID;
                break;
            }
            found = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        parent_pid
    }
}

fn remove_macos_notify_hook_execution_attributes(path: &Path) {
    if std::env::consts::OS != "macos" {
        return;
    }
    for attribute in ["com.apple.quarantine", "com.apple.provenance"] {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .arg("-d")
            .arg(attribute)
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub(crate) fn notify_hook_state_directory(contents: &str) -> Option<PathBuf> {
    let value = contents
        .lines()
        .find_map(|line| line.strip_prefix("DEFAULT_HOOK_STATE_DIR="))?;
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    let path = PathBuf::from(inner.replace("'\\''", "'"));
    (path.is_absolute() && path.file_name().and_then(|name| name.to_str()) == Some("agent-hooks"))
        .then_some(path)
}

pub(crate) fn migrate_hook_session_sidecars(
    source_directory: &Path,
    destination_directory: &Path,
) -> Result<Vec<String>, DomainStateError> {
    if source_directory == destination_directory || !source_directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut migrated_paths = Vec::new();
    for entry in fs::read_dir(source_directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if !entry.file_type().map_err(io_error)?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.ends_with("-hook-sessions.json") {
            continue;
        }
        let source_data = read_json_object(&read_file_text(&entry.path()));
        let Some(source_sessions) = source_data.get("sessions").and_then(Value::as_object) else {
            continue;
        };
        let destination_path = destination_directory.join(file_name);
        let mut destination_data = read_json_object(&read_file_text(&destination_path));
        let destination_object = destination_data.as_object_mut().expect("JSON object");
        let destination_sessions = destination_object
            .entry("sessions".to_string())
            .or_insert_with(|| json!({}));
        if !destination_sessions.is_object() {
            *destination_sessions = json!({});
        }
        let destination_sessions = destination_sessions
            .as_object_mut()
            .expect("sessions object");
        let mut changed = false;
        for (session_id, source_session) in source_sessions {
            let source_updated_at = source_session
                .get("updatedAt")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let destination_updated_at = destination_sessions
                .get(session_id)
                .and_then(|session| session.get("updatedAt"))
                .and_then(Value::as_f64)
                .unwrap_or_default();
            if !destination_sessions.contains_key(session_id)
                || source_updated_at > destination_updated_at
            {
                destination_sessions.insert(session_id.clone(), source_session.clone());
                changed = true;
            }
        }
        if changed {
            write_json_file(&destination_path, &destination_data)?;
            migrated_paths.push(path_string(&destination_path));
        }
    }
    Ok(migrated_paths)
}

pub(crate) fn is_notify_hook_current(hook_paths: &HookPaths, contents: &str) -> bool {
    /*
    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 parity requires Rust status/install/uninstall to treat the TypeScript gxserver v6 hook marker as the shared notify-hook currency contract. Do not require Rust-only helper text here; existing gxserver-owned v6 hooks should stay installed instead of forcing a needless updateRequired state.

    The marker alone is not enough when Ghostex's resolved state directory
    changes (for example, after moving from the macOS Application Support
    layout to XDG state). The hook embeds that directory at install time, so a
    marker-current script pointing at a different directory is stale and must
    be rewritten before live Codex identity repair can consume its sidecar.
    */
    let state_directory_assignment = format!(
        "DEFAULT_HOOK_STATE_DIR={}",
        shell_quote(&path_string(&hook_paths.hook_state_directory))
    );
    contents.contains(&format!("{NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}"))
        && contents
            .lines()
            .any(|line| line == state_directory_assignment)
}
