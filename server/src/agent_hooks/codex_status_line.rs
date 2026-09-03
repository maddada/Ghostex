/*
CDXC:AgentHooks 2026-09-03 WHY:
Codex has no statusLine command and no hook that carries the model, so the
TUI footer is the ONLY pre-turn source of the model and effort for the chat
pills (the transcript's `turn_context` row exists only once a turn starts).
The footer is the user-ordered `tui.status_line` list in `config.toml`, and a
user who trimmed `model-with-reasoning` out of it gets empty pills until the
first send. So installing the Codex hooks also guarantees the item is listed,
the same way installing the Claude hooks guarantees a statusline: an absent
key is left alone (Codex's built-in default is
`["model-with-reasoning", "current-dir"]`), and an explicit list that lacks the
item gets it appended, order and formatting otherwise untouched.
*/

use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Value};

use crate::domain::DomainStateError;

use super::probing::{io_error, read_file_text, temp_path_for};

pub(crate) const CODEX_MODEL_STATUS_LINE_ITEM: &str = "model-with-reasoning";

fn status_line_array(document: &DocumentMut) -> Option<&Array> {
    document
        .as_table()
        .get("tui")?
        .as_table_like()?
        .get("status_line")?
        .as_array()
}

fn array_names_model(array: &Array) -> bool {
    array
        .iter()
        .any(|item| item.as_str() == Some(CODEX_MODEL_STATUS_LINE_ITEM))
}

/// Whether Codex's footer will name the model with this config: the key is
/// absent (Codex's default lists it), or an explicit list contains it. A
/// config that does not parse is reported as fine, since Codex itself refuses
/// to start on it and there is nothing Ghostex could repair.
pub(crate) fn codex_status_line_names_model(config_path: &Path) -> bool {
    let text = read_file_text(config_path);
    if text.trim().is_empty() {
        return true;
    }
    let Ok(document) = text.parse::<DocumentMut>() else {
        return true;
    };
    status_line_array(&document).map_or(true, array_names_model)
}

/// Appends `model-with-reasoning` to an explicit `tui.status_line` that lacks
/// it. Returns whether `config.toml` changed.
pub(crate) fn ensure_codex_status_line_names_model(
    config_path: &Path,
) -> Result<bool, DomainStateError> {
    let text = read_file_text(config_path);
    if text.trim().is_empty() {
        return Ok(false);
    }
    let Ok(mut document) = text.parse::<DocumentMut>() else {
        return Ok(false);
    };
    let Some(array) = document
        .as_table_mut()
        .get_mut("tui")
        .and_then(Item::as_table_like_mut)
        .and_then(|tui| tui.get_mut("status_line"))
        .and_then(Item::as_array_mut)
    else {
        return Ok(false);
    };
    if array_names_model(array) {
        return Ok(false);
    }
    array.push(Value::from(CODEX_MODEL_STATUS_LINE_ITEM));
    let temp_path = temp_path_for(config_path);
    std::fs::write(&temp_path, document.to_string()).map_err(io_error)?;
    std::fs::rename(&temp_path, config_path).map_err(io_error)?;
    Ok(true)
}
