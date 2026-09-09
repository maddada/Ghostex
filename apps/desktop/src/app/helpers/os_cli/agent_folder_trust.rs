//! Marks a folder as trusted for the Claude Code and Codex CLIs so agent
//! sessions Ghostex starts there never stop on the "do you trust this folder"
//! prompt.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

fn trust_home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/Shared"))
}

/// `~/.claude.json`, or `$CLAUDE_CONFIG_DIR/.claude.json` for a relocated
/// profile (the same rule gxserver's account identity code uses).
fn claude_config_file() -> PathBuf {
    let home = trust_home_dir();
    match env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from) {
        Some(root) if root != home.join(".claude") => root.join(".claude.json"),
        _ => home.join(".claude.json"),
    }
}

fn codex_home_dir() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| trust_home_dir().join(".codex"))
}

fn write_atomically(path: &Path, contents: &str) -> Result<(), String> {
    let temp_path = path.with_extension(format!("ghostex-trust.{}.tmp", std::process::id()));
    fs::write(&temp_path, contents).map_err(|error| error.to_string())?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        error.to_string()
    })
}

/// Sets `projects["<folder>"].hasTrustDialogAccepted = true` in Claude Code's
/// config, keeping every other key. Does nothing when Claude Code has never
/// written its config (nothing to trust yet).
fn trust_folder_for_claude(folder: &str) -> Result<(), String> {
    let path = claude_config_file();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?;
    let Some(root_object) = root.as_object_mut() else {
        return Err(format!("{} is not a JSON object.", path.display()));
    };
    let projects = root_object
        .entry("projects")
        .or_insert_with(|| Value::Object(Map::new()));
    if !projects.is_object() {
        *projects = Value::Object(Map::new());
    }
    let entry = projects
        .as_object_mut()
        .expect("projects is an object")
        .entry(folder)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    let entry_object = entry.as_object_mut().expect("project entry is an object");
    if entry_object.get("hasTrustDialogAccepted") == Some(&Value::Bool(true)) {
        return Ok(());
    }
    entry_object.insert("hasTrustDialogAccepted".to_string(), Value::Bool(true));
    let serialized = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    write_atomically(&path, &serialized).map_err(|error| format!("{}: {error}", path.display()))
}

fn toml_basic_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

/// Appends `[projects."<folder>"] trust_level = "trusted"` to Codex's
/// `config.toml` unless the folder already has a projects table. Does nothing
/// when the Codex home does not exist (Codex is not installed).
fn trust_folder_for_codex(folder: &str) -> Result<(), String> {
    let home = codex_home_dir();
    if !home.is_dir() {
        return Ok(());
    }
    let path = home.join("config.toml");
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let header = format!("[projects.{}]", toml_basic_string(folder));
    if existing.lines().any(|line| line.trim() == header) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&header);
    updated.push_str("\ntrust_level = \"trusted\"\n");
    write_atomically(&path, &updated).map_err(|error| format!("{}: {error}", path.display()))
}

/// CDXC:Onboarding 2026-09-10 DECISION:
/// User: the Ghostex config folder that hosts Help chats is known to be safe, so mark it trusted in Claude Code and Codex up front instead of letting each new Help session stop on the trust prompt.
/// Returns one message per agent config that could not be updated; an agent that is not installed is skipped silently.
pub(crate) fn gpui_trust_folder_for_agent_clis(folder: &Path) -> Vec<String> {
    let folder = folder.to_string_lossy().to_string();
    let mut warnings = Vec::new();
    if let Err(message) = trust_folder_for_claude(&folder) {
        warnings.push(format!("Claude Code: {message}"));
    }
    if let Err(message) = trust_folder_for_codex(&folder) {
        warnings.push(format!("Codex: {message}"));
    }
    warnings
}
