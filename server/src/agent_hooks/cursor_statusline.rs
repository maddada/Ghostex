/*
CDXC:AgentHooks 2026-09-24 WHY:
Cursor keeps its `statusLine` in `cli-config.json`, not in the `hooks.json` the
Ghostex hooks live in, so the Cursor install also points that file's statusLine
at the Ghostex script (see `statusline.rs`). Cursor creates `cli-config.json` on
its first run and rewrites it itself, so a missing file is left alone (the next
repair pass registers the statusLine once Cursor has written it) and a file that
does not parse as a JSON object is never overwritten.
*/

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::DomainStateError;

use super::config::HookPaths;
use super::probing::{io_error, read_config_text, write_config_file};
use super::statusline::{register_statusline, unregister_statusline, StatuslineAgent};

/// `cli-config.json` beside the `hooks.json` a Cursor install writes.
pub(crate) fn cursor_cli_config_path(hooks_path: &Path) -> Option<PathBuf> {
    Some(hooks_path.parent()?.join("cli-config.json"))
}

/// Cursor's config, or `None` when it is missing, unreadable or not a JSON object: nothing to
/// change then, and nothing is written over it.
fn read_config(path: &Path) -> Option<Value> {
    let text = read_config_text(path).ok()??;
    serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)
}

fn write_config(path: &Path, data: &Value) -> Result<(), DomainStateError> {
    let text = serde_json::to_string_pretty(data).map_err(|error| {
        DomainStateError::corrupt_state(format!("Cursor config is not serializable: {error}"))
    })?;
    write_config_file(path, format!("{text}\n").as_bytes()).map_err(io_error)
}

/// Points Cursor's statusLine at the Ghostex script, wrapping the user's own
/// command. Returns the config path when it changed.
pub(crate) fn ensure_cursor_statusline(
    hooks_path: &Path,
    hook_paths: &HookPaths,
) -> Result<Option<PathBuf>, DomainStateError> {
    let Some(path) = cursor_cli_config_path(hooks_path) else {
        return Ok(None);
    };
    let Some(mut data) = read_config(&path) else {
        return Ok(None);
    };
    if !register_statusline(&mut data, StatuslineAgent::Cursor.script_path(hook_paths)) {
        return Ok(None);
    }
    write_config(&path, &data)?;
    Ok(Some(path))
}

/// Restores the user's own statusLine command, or drops the entry Ghostex
/// created. Returns the config path when it changed.
pub(crate) fn remove_cursor_statusline(
    hooks_path: &Path,
) -> Result<Option<PathBuf>, DomainStateError> {
    let Some(path) = cursor_cli_config_path(hooks_path) else {
        return Ok(None);
    };
    let Some(mut data) = read_config(&path) else {
        return Ok(None);
    };
    if !unregister_statusline(&mut data) {
        return Ok(None);
    }
    write_config(&path, &data)?;
    Ok(Some(path))
}
