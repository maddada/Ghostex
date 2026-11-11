use super::error;
use crate::domain::DomainStateError;
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use serde_json::Value;
use std::{fs, io::Write, path::Path};

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(v) => CstInputValue::Bool(*v),
        Value::Number(v) => CstInputValue::Number(v.to_string()),
        Value::String(v) => CstInputValue::String(v.clone()),
        Value::Array(v) => CstInputValue::Array(v.iter().map(input).collect()),
        Value::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), input(v))).collect())
        }
    }
}

pub(crate) fn read_config(text: &str) -> Result<Value, DomainStateError> {
    jsonc_parser::parse_to_serde_value(
        if text.trim().is_empty() || text.bytes().all(|byte| byte == 0) {
            "{}"
        } else {
            text
        },
        &Default::default(),
    )
    .map_err(|e| error(format!("Invalid OpenCode configuration: {e}")))
}

/// OpenCode accepts JSONC. Edit only the owned property so comments and other settings survive.
///
/// CDXC:AgentHooks 2026-11-02 WHY: A Windows OpenCode cli.json was 81 NUL bytes (the size of the cli.json Install hooks writes) after the computer lost power about 90 seconds after Install hooks wrote it: the temp file was renamed over cli.json before its data reached the disk, so NTFS kept the new length with zeroed contents, and every later install failed with "Invalid OpenCode configuration". The new contents are synced before the rename so a crash leaves the old or the new file, never NULs. A file of only NULs has no settings to keep, so it is replaced with an empty object after a backup copy is written next to it; any other malformed configuration is still refused.
pub(crate) fn patch_config(path: &Path, key: &str, value: &Value) -> Result<(), DomainStateError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}\n".into(),
        Err(e) => return Err(error(format!("Could not read OpenCode configuration: {e}"))),
    };
    let text = if text.bytes().all(|byte| byte == 0) {
        if !text.is_empty() {
            let backup = path.with_file_name(format!(
                "{}.corrupt-{}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("config"),
                chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
            ));
            fs::copy(path, &backup).map_err(|e| {
                error(format!(
                    "Could not back up the unreadable OpenCode configuration: {e}"
                ))
            })?;
        }
        "{}\n"
    } else {
        &text
    };
    let root = CstRootNode::parse(text, &Default::default()).map_err(|e| error(e.to_string()))?;
    let object = root
        .object_value()
        .ok_or_else(|| error("OpenCode configuration must contain an object."))?;
    match object.get(key) {
        Some(property) => property.set_value(input(value)),
        None => {
            object.append(key, input(value));
        }
    }
    write_config_file(path, &root.to_string())
}

/// Replaces an OpenCode file through a synced temp file, so a crash leaves the old or the new
/// contents (see `patch_config`).
pub(crate) fn write_config_file(path: &Path, text: &str) -> Result<(), DomainStateError> {
    let directory = path
        .parent()
        .ok_or_else(|| error("Invalid OpenCode configuration path."))?;
    fs::create_dir_all(directory).map_err(|e| error(e.to_string()))?;
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|e| error(e.to_string()))?;
    temp.write_all(text.as_bytes())
        .map_err(|e| error(e.to_string()))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| error(e.to_string()))?;
    temp.persist(path).map_err(|e| error(e.error.to_string()))?;
    Ok(())
}

pub(crate) fn save_default_model(model: Value) -> Result<(), DomainStateError> {
    let directory = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::resume_lookup::home_dir().join(".config"))
        .join("opencode");
    let path = if directory.join("opencode.jsonc").is_file() {
        directory.join("opencode.jsonc")
    } else {
        directory.join("opencode.json")
    };
    patch_config(&path, "model", &model)
}

/// The bytes that open the TUI's prompt editor, read from OpenCode 2's own keybinds in `cli.json`
/// (`keybinds."prompt.editor"`, default `<leader>e`, and `keybinds.leader`, default `ctrl+x`).
///
/// CDXC:Drafts 2026-11-11 WHY:
/// Without Ghostex's plugin, the chat opens OpenCode's prompt editor by typing the TUI's own key, so a user who rebinds it in cli.json would get a key that does something else (OpenCode 1's `editor_open: ctrl+g` meant "first message" in OpenCode 2). The first alternative that is a terminal byte sequence (`ctrl+<letter>`, `alt+<key>`, a single key, `<leader>` before any of those) is typed; an editor bound to nothing is refused instead of typing the default.
pub(crate) fn prompt_editor_keys(config_dir: &Path) -> Result<String, DomainStateError> {
    let config = fs::read_to_string(config_dir.join("cli.json"))
        .ok()
        .and_then(|text| read_config(&text).ok())
        .unwrap_or(Value::Null);
    let keybinds = &config["keybinds"];
    let leader = binding_keys(&keybinds["leader"], None).or_else(|| key_bytes("ctrl+x"));
    let editor = match &keybinds["prompt.editor"] {
        Value::Null => binding_keys(&Value::String("<leader>e".into()), leader.as_deref()),
        value => binding_keys(value, leader.as_deref()),
    };
    editor.ok_or_else(|| {
        error("OpenCode's prompt editor has no key Ghostex can type (keybinds.\"prompt.editor\" in OpenCode's cli.json). Open it in the terminal.")
    })
}

/// The first alternative of a keybind value (`"a,b"`, `["a", "b"]`, `"none"`, `false`) that is
/// a byte sequence.
fn binding_keys(value: &Value, leader: Option<&str>) -> Option<String> {
    let alternatives: Vec<&str> = match value {
        Value::String(text) => text.split(',').collect(),
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    alternatives.into_iter().find_map(|alternative| {
        let alternative = alternative.trim();
        if alternative.is_empty() || alternative == "none" {
            return None;
        }
        let (prefix, rest) = match alternative.strip_prefix("<leader>") {
            Some(rest) => (Some(leader?), rest.trim()),
            None => (None, alternative),
        };
        let mut keys = prefix.unwrap_or_default().to_string();
        for stroke in rest.split_whitespace() {
            keys.push_str(&key_bytes(stroke)?);
        }
        (!keys.is_empty()).then_some(keys)
    })
}

/// One key as the bytes a terminal sends for it.
fn key_bytes(stroke: &str) -> Option<String> {
    let lower = stroke.to_ascii_lowercase();
    if let Some(key) = lower.strip_prefix("ctrl+") {
        let [letter] = key.as_bytes() else {
            return None;
        };
        return letter
            .is_ascii_lowercase()
            .then(|| char::from(letter & 0x1f).to_string());
    }
    if let Some(key) = lower
        .strip_prefix("alt+")
        .or_else(|| lower.strip_prefix("meta+"))
    {
        return (key.chars().count() == 1).then(|| format!("\u{1b}{key}"));
    }
    let mut chars = stroke.chars();
    match (chars.next(), chars.next()) {
        (Some(key), None) if !key.is_control() => Some(key.to_string()),
        _ => None,
    }
}
