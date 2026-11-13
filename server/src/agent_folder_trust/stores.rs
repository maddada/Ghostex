//! Each agent's own record of trusted folders, written the way the agent writes it when the user
//! answers its folder-trust prompt with yes.
//!
//! CDXC:AgentProviders 2026-10-17 WHY:
//! Measured on macOS against Claude Code 2.1.296, Codex 0.163.0, Cursor CLI 2026.10.16, Copilot CLI 1.0.98, Grok Build 1.0.62, Antigravity CLI 1.4.0, Pi 1.3.0 and Factory Droid 0.205.1. OMP 18.11.1 trusts every project in code, and Hermes 0.21.4 and OpenCode 2.0.24 have no folder-trust step (Hermes asks about each shell hook, which is not folder trust), so they have no writer here. Codex's `-c projects."<folder>".trust_level=…` override is ignored inside a git worktree (the prompt still showed), while the same entry in `config.toml` is honoured, so every agent is trusted through its file, never a launch flag. Copilot keeps its list in `config.json` (what its own "remember this folder" writes); a `trustedFolders` list in `settings.json`, where its `/settings` help lists it, did not stop the prompt. A folder the user explicitly marked untrusted in an agent's own record is left as it is.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};
use toml_edit::{DocumentMut, Item, Table};

use super::files::{
    dedupe_files, env_folder_or, existing_files_in_subfolders, read_json_object, read_text,
    write_atomically, write_json_object, JsonFile,
};

/// The agents whose own folder-trust record Ghostex writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrustingAgent {
    Claude,
    Codex,
    Cursor,
    Copilot,
    Grok,
    Antigravity,
    Pi,
    Droid,
}

impl TrustingAgent {
    pub(super) fn from_agent_id(agent_id: &str) -> Option<Self> {
        Some(match agent_id.trim().to_ascii_lowercase().as_str() {
            "claude" | "openclaude" | "claude-code" => Self::Claude,
            "codex" => Self::Codex,
            "cursor" | "cursor-cli" | "cursor-agent" => Self::Cursor,
            "copilot" | "github-copilot" => Self::Copilot,
            "grok" => Self::Grok,
            "antigravity" | "antigravity-cli" | "agy" => Self::Antigravity,
            "pi" => Self::Pi,
            "droid" | "factory-droid" => Self::Droid,
            _ => return None,
        })
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor CLI",
            Self::Copilot => "Copilot CLI",
            Self::Grok => "Grok Build",
            Self::Antigravity => "Antigravity CLI",
            Self::Pi => "Pi",
            Self::Droid => "Factory Droid",
        }
    }
}

/// One file that could not be updated, as a sentence for the log.
pub(super) type TrustWarning = String;

/// Marks `folder` trusted in every record `agent` reads it from. Records of an agent that never ran
/// on this computer (no configuration folder yet) are skipped: it has nothing to trust with.
pub(super) fn trust_folder(agent: TrustingAgent, home: &Path, folder: &Path) -> Vec<TrustWarning> {
    let key = folder_key(folder);
    let files: Vec<PathBuf> = match agent {
        TrustingAgent::Claude => claude_config_files(home),
        TrustingAgent::Codex => codex_config_files(home),
        TrustingAgent::Cursor => {
            let data = env_folder_or("CURSOR_DATA_DIR", home.join(".cursor"));
            existing_folder(&data)
                .map(|data| {
                    data.join("projects")
                        .join(cursor_slug(&key))
                        .join(".workspace-trusted")
                })
                .into_iter()
                .collect()
        }
        TrustingAgent::Copilot => {
            existing_folder(&env_folder_or("COPILOT_HOME", home.join(".copilot")))
                .map(|copilot| copilot.join("config.json"))
                .into_iter()
                .collect()
        }
        TrustingAgent::Grok => existing_folder(&env_folder_or("GROK_HOME", home.join(".grok")))
            .map(|grok| grok.join("trusted_folders.toml"))
            .into_iter()
            .collect(),
        TrustingAgent::Antigravity => {
            existing_folder(&home.join(".gemini").join("antigravity-cli"))
                .map(|cli| cli.join("settings.json"))
                .into_iter()
                .collect()
        }
        TrustingAgent::Pi => existing_folder(&env_folder_or(
            "PI_CODING_AGENT_DIR",
            home.join(".pi").join("agent"),
        ))
        .map(|pi| pi.join("trust.json"))
        .into_iter()
        .collect(),
        TrustingAgent::Droid => existing_folder(&home.join(".factory"))
            .map(|factory| factory.join("settings.json"))
            .into_iter()
            .collect(),
    };
    files
        .iter()
        .filter_map(|file| {
            write_record(agent, file, &key)
                .err()
                .map(|error| format!("{} ({}): {error}", agent.name(), file.display()))
        })
        .collect()
}

fn write_record(agent: TrustingAgent, file: &Path, key: &str) -> io::Result<()> {
    match agent {
        TrustingAgent::Claude => trust_in_claude_config(file, key),
        TrustingAgent::Codex => trust_in_codex_config(file, key),
        TrustingAgent::Cursor => trust_in_cursor_marker(file, key),
        TrustingAgent::Copilot => add_to_json_list(file, "trustedFolders", key),
        TrustingAgent::Grok => trust_in_grok_store(file, key),
        TrustingAgent::Antigravity => add_to_json_list(file, "trustedWorkspaces", key),
        TrustingAgent::Pi => edit_json_object(file, |root| {
            if root.contains_key(key) {
                return false;
            }
            root.insert(key.to_string(), json!(true));
            true
        }),
        TrustingAgent::Droid => edit_json_object(file, |root| {
            let folders = object_entry(root, "trustedFolders");
            if folders.contains_key(key) {
                return false;
            }
            folders.insert(key.to_string(), json!({ "trustedAt": now_iso() }));
            true
        }),
    }
}

/// The folder as the agents spell it: the real path (agents read their working directory with
/// `getcwd`, so `/tmp/x` is `/private/tmp/x` on macOS), and on Windows without the `\\?\` prefix.
fn folder_key(folder: &Path) -> String {
    let text = folder.to_string_lossy().to_string();
    if cfg!(windows) {
        return text
            .strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .unwrap_or_else(|| text.strip_prefix(r"\\?\").unwrap_or(&text).to_string());
    }
    text
}

fn existing_folder(path: &Path) -> Option<PathBuf> {
    path.is_dir().then(|| path.to_path_buf())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn edit_json_object(
    file: &Path,
    edit: impl FnOnce(&mut Map<String, Value>) -> bool,
) -> io::Result<()> {
    let mut document = read_json_object(file)?.unwrap_or_default();
    if edit(&mut document.object) {
        write_json_object(file, &document)?;
    }
    Ok(())
}

fn object_entry<'a>(root: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry
        .as_object_mut()
        .expect("entry was just made an object")
}

fn add_to_json_list(file: &Path, list_key: &str, folder: &str) -> io::Result<()> {
    edit_json_object(file, |root| {
        let list = root
            .entry(list_key.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !list.is_array() {
            *list = Value::Array(Vec::new());
        }
        let list = list.as_array_mut().expect("list was just made an array");
        if list.iter().any(|entry| entry.as_str() == Some(folder)) {
            return false;
        }
        list.push(json!(folder));
        true
    })
}

/// Every Claude Code profile on this computer: the default one (`~/.claude.json`, or
/// `$CLAUDE_CONFIG_DIR/.claude.json`), each cswap account profile, and each `~/.claude-profiles`
/// profile. An account switch restarts Claude under another profile, so the folder is trusted in
/// all of them.
fn claude_config_files(home: &Path) -> Vec<PathBuf> {
    let default_root = env_folder_or(
        "CLAUDE_CONFIG_DIR",
        ghostex_paths::agent_homes::claude_default_home(&home),
    );
    let mut files = vec![
        if default_root == ghostex_paths::agent_homes::claude_default_home(&home) {
            ghostex_paths::agent_homes::claude_json_path(&home)
        } else {
            default_root.join(".claude.json")
        },
    ];
    files.extend(existing_files_in_subfolders(
        &crate::accounts::claude_resets::swap_root(home).join("sessions"),
        ".claude.json",
    ));
    files.extend(existing_files_in_subfolders(
        &home.join(".claude-profiles"),
        ".claude.json",
    ));
    dedupe_files(files.into_iter().filter(|file| file.is_file()).collect())
}

/// Claude Code keys its projects by the folder path, with forward slashes on Windows.
fn trust_in_claude_config(file: &Path, key: &str) -> io::Result<()> {
    let key = if cfg!(windows) {
        key.replace('\\', "/")
    } else {
        key.to_string()
    };
    edit_json_object(file, |root| {
        let project = object_entry(object_entry(root, "projects"), &key);
        if project.get("hasTrustDialogAccepted") == Some(&Value::Bool(true)) {
            return false;
        }
        project.insert("hasTrustDialogAccepted".to_string(), Value::Bool(true));
        true
    })
}

/// Every Codex home on this computer: `CODEX_HOME` (else `~/.codex`), each xswap account and each
/// `~/.codex-profiles` profile. xswap links its accounts' `config.toml` to the default one, so the
/// shared file is written once.
fn codex_config_files(home: &Path) -> Vec<PathBuf> {
    let default_home = env_folder_or(
        "CODEX_HOME",
        ghostex_paths::agent_homes::codex_default_home(home),
    );
    let mut files = Vec::new();
    if default_home.is_dir() {
        files.push(default_home.join("config.toml"));
    }
    files.extend(existing_files_in_subfolders(
        &crate::accounts::launch::xswap_root(home).join("accounts"),
        "config.toml",
    ));
    files.extend(existing_files_in_subfolders(
        &home.join(".codex-profiles"),
        "config.toml",
    ));
    dedupe_files(files)
}

/// `[projects."<folder>"] trust_level = "trusted"`, edited with `toml_edit` so every other line
/// and comment stays as it was.
fn trust_in_codex_config(file: &Path, key: &str) -> io::Result<()> {
    let text = read_text(file)?.unwrap_or_default();
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| io::Error::other(format!("it is not valid TOML ({error})")))?;
    let projects = document
        .entry("projects")
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        })
        .as_table_like_mut()
        .ok_or_else(|| io::Error::other("its `projects` entry is not a table"))?;
    if let Some(existing) = projects.get(key).and_then(|entry| entry.get("trust_level")) {
        if existing.as_str().is_some() {
            return Ok(());
        }
    }
    let project = projects
        .entry(key)
        .or_insert(Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| io::Error::other("its project entry is not a table"))?;
    project.insert("trust_level", toml_edit::value("trusted"));
    write_atomically(file, &document.to_string())
}

/// Cursor's projects folder name for a path: every run of characters other than ASCII letters
/// and digits becomes one `-`, trimmed at both ends (`r_` in Cursor's `workspace-paths.js`).
fn cursor_slug(path: &str) -> String {
    let mut slug = String::with_capacity(path.len());
    for character in path.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn trust_in_cursor_marker(file: &Path, key: &str) -> io::Result<()> {
    if file.is_file() {
        return Ok(());
    }
    let mut marker = JsonFile::default();
    marker
        .object
        .insert("trustedAt".to_string(), json!(now_iso()));
    marker
        .object
        .insert("workspacePath".to_string(), json!(key));
    write_json_object(file, &marker)
}

/// `[folders."<folder>"] trusted = true` with `decided_at` in Unix seconds, Grok's own layout.
fn trust_in_grok_store(file: &Path, key: &str) -> io::Result<()> {
    let text = read_text(file)?.unwrap_or_default();
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| io::Error::other(format!("it is not valid TOML ({error})")))?;
    let folders = document
        .entry("folders")
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        })
        .as_table_like_mut()
        .ok_or_else(|| io::Error::other("its `folders` entry is not a table"))?;
    if folders
        .get(key)
        .and_then(|entry| entry.get("trusted"))
        .is_some()
    {
        return Ok(());
    }
    let mut entry = Table::new();
    entry.insert("trusted", toml_edit::value(true));
    entry.insert(
        "decided_at",
        toml_edit::value(chrono::Utc::now().timestamp()),
    );
    folders.insert(key, Item::Table(entry));
    write_atomically(file, &document.to_string())
}
