/*
CDXC:CodexHookTrust 2026-09-02:
Codex CLI does not run a command hook merely because it sits in `hooks.json`.
Every non-managed hook must be TRUSTED first: Codex hashes the hook's
normalized identity (event + matcher group + handler config, canonical JSON,
SHA-256) and only runs it while `[hooks.state."<key>"].trusted_hash` in the
CODEX_HOME `config.toml` equals that hash. An untrusted or modified hook is
silently skipped, and the TUI opens a startup review that a user can dismiss
with "Continue without trusting". Ghostex used to write only `hooks.json`, so
Settings reported the Codex hooks as Installed while Codex never ran them and
no SessionStart ever reached gxserver.

Installing and trusting are one act of the same consent: the user pressed
Install Hooks in Ghostex for a hook that runs nothing but Ghostex's own notify
script. This module therefore writes the trust record for GHOSTEX-OWNED hooks
only, and only on that explicit install/update. The daemon-startup repair pass
re-trusts a slot solely when Codex already held a trust record for that exact
slot (a command-path upgrade of an already-approved hook); a slot Codex never
saw approved stays untrusted and surfaces as "needs update" in Settings until
the user presses Update Hooks themselves. User-authored hooks in the same file
are never touched.

Hash and key formats mirror `codex-rs/hooks/src/engine/discovery.rs`
(`hook_hash`, `hook_key`) and `codex-rs/config/src/fingerprint.rs`
(`version_for_toml`), verified byte-for-byte against a live trusted entry.
The config file is edited with `toml_edit` so every other line, comment and
table the user wrote survives untouched.
*/

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item, Table};

use crate::domain::DomainStateError;

use super::config::HookPaths;
use super::install::read_json_object;
use super::probing::{io_error, path_string, read_file_text, temp_path_for};
use super::resolution::is_ghostex_owned_hook_command;

/// One Ghostex-owned Codex hook as Codex identifies it: the `[hooks.state]`
/// key and the trust hash Codex computes for its current definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodexHookTrustEntry {
    pub(crate) key: String,
    pub(crate) hash: String,
}

/// What `config.toml` says about the Ghostex-owned hooks in one `hooks.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexHookTrustStatus {
    /// Every Ghostex-owned hook carries a matching `trusted_hash` and is enabled.
    Trusted,
    /// At least one Ghostex-owned hook has no trust record, or a stale one.
    Untrusted,
    /// Every hook is trusted but the user disabled at least one in `/hooks`.
    Disabled,
}

/// How much consent a trust write may assume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexTrustWriteMode {
    /// The user pressed Install/Update Hooks: trust every Ghostex-owned hook
    /// and clear a user-side `enabled = false` on it.
    Explicit,
    /// Daemon-startup repair: only refresh slots Codex already had a trust
    /// record for, never add trust to a slot that was never approved.
    RefreshExisting,
}

/// Codex's `hook_event_key_label`: the snake_case event name used in state keys.
fn codex_hook_event_key_label(event_name: &str) -> Option<&'static str> {
    Some(match event_name {
        "PreToolUse" => "pre_tool_use",
        "PermissionRequest" => "permission_request",
        "PostToolUse" => "post_tool_use",
        "PreCompact" => "pre_compact",
        "PostCompact" => "post_compact",
        "SessionStart" => "session_start",
        "SessionEnd" => "session_end",
        "UserPromptSubmit" => "user_prompt_submit",
        "SubagentStart" => "subagent_start",
        "SubagentStop" => "subagent_stop",
        "Stop" => "stop",
        "Interrupt" => "interrupt",
        _ => return None,
    })
}

/// Codex's `normalize_command_hook`: SessionEnd and Interrupt default to one
/// second and are clamped to three; everything else defaults to ten minutes.
fn codex_normalized_timeout(event_name: &str, timeout: Option<u64>) -> u64 {
    match event_name {
        "SessionEnd" | "Interrupt" => timeout.unwrap_or(1).clamp(1, 3),
        _ => timeout.unwrap_or(600).max(1),
    }
}

/// Codex's `version_for_toml`: canonical (key-sorted, compact) JSON, SHA-256.
fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(item) = map.get(&key) {
                    sorted.insert(key, canonical_json(item));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

fn sha256_identity(identity: &Value) -> String {
    let serialized = serde_json::to_vec(&canonical_json(identity)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized);
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

/// Codex's `hook_hash` for one command handler: the matcher group reduced to
/// this single normalized handler, plus the event label. `None` values are
/// absent from the identity exactly as TOML serialization drops them.
fn codex_hook_trust_hash(event_name: &str, matcher: Option<&str>, hook: &Value) -> Option<String> {
    if hook.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    let mut command = hook.get("command").and_then(Value::as_str)?.to_string();
    let command_windows = hook
        .get("commandWindows")
        .or_else(|| hook.get("command_windows"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if cfg!(windows) {
        if let Some(command_windows) = command_windows.clone() {
            command = command_windows;
        }
    }
    if command.trim().is_empty() {
        return None;
    }
    let timeout = codex_normalized_timeout(event_name, hook.get("timeout").and_then(Value::as_u64));
    let runs_async =
        hook.get("async").and_then(Value::as_bool).unwrap_or(false) && event_name != "SessionEnd";
    let mut handler = Map::new();
    handler.insert("type".to_string(), json!("command"));
    handler.insert("command".to_string(), json!(command));
    if let Some(command_windows) = command_windows {
        handler.insert("commandWindows".to_string(), json!(command_windows));
    }
    handler.insert("timeout".to_string(), json!(timeout));
    handler.insert("async".to_string(), json!(runs_async));
    if let Some(status_message) = hook.get("statusMessage").and_then(Value::as_str) {
        handler.insert("statusMessage".to_string(), json!(status_message));
    }
    let context_limit_event = matches!(
        event_name,
        "PreToolUse" | "PostToolUse" | "SessionStart" | "UserPromptSubmit" | "SubagentStart"
    );
    if let Some(limit) = hook
        .get("additionalContextLimit")
        .and_then(Value::as_u64)
        .filter(|_| context_limit_event)
        .filter(|limit| *limit != 2_500)
    {
        handler.insert("additionalContextLimit".to_string(), json!(limit));
    }
    let mut group = Map::new();
    group.insert(
        "event_name".to_string(),
        json!(codex_hook_event_key_label(event_name)?),
    );
    if let Some(matcher) = matcher {
        group.insert("matcher".to_string(), json!(matcher));
    }
    group.insert(
        "hooks".to_string(),
        Value::Array(vec![Value::Object(handler)]),
    );
    Some(sha256_identity(&Value::Object(group)))
}

/// The `key_source` Codex uses for this `hooks.json`: its path as Codex
/// resolves CODEX_HOME. The default `~/.codex` is joined verbatim, while an
/// explicit CODEX_HOME (which is how a profile directory is selected) is
/// canonicalized first.
fn codex_hook_key_source(hooks_path: &Path, hook_paths: &HookPaths) -> String {
    let Some(codex_home) = hooks_path.parent() else {
        return path_string(hooks_path);
    };
    let default_codex_home = hook_paths.home_dir.join(".codex");
    let resolved_home = if codex_home == default_codex_home {
        codex_home.to_path_buf()
    } else {
        fs::canonicalize(codex_home).unwrap_or_else(|_| codex_home.to_path_buf())
    };
    let file_name = hooks_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "hooks.json".to_string());
    path_string(&resolved_home.join(file_name))
}

/// The `config.toml` that holds `[hooks.state]` for this `hooks.json`: the
/// sibling file in the same CODEX_HOME.
pub(crate) fn codex_config_path_for_hooks(hooks_path: &Path) -> PathBuf {
    hooks_path.with_file_name("config.toml")
}

/// Every Ghostex-owned command hook in `hooks_path`, keyed and hashed the way
/// Codex will see it. Group and handler indexes are positions in the file,
/// which is why Ghostex always appends its group and never reorders a user's.
pub(crate) fn ghostex_codex_hook_trust_entries(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    command: &str,
) -> Vec<CodexHookTrustEntry> {
    let data = read_json_object(&read_file_text(hooks_path));
    let Some(events) = data.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };
    let key_source = codex_hook_key_source(hooks_path, hook_paths);
    let mut entries = Vec::new();
    for (event_name, groups) in events {
        let Some(label) = codex_hook_event_key_label(event_name) else {
            continue;
        };
        let Some(groups) = groups.as_array() else {
            continue;
        };
        for (group_index, group) in groups.iter().enumerate() {
            let matcher = group.get("matcher").and_then(Value::as_str);
            let Some(hooks) = group.get("hooks").and_then(Value::as_array) else {
                continue;
            };
            for (handler_index, hook) in hooks.iter().enumerate() {
                if !is_ghostex_owned_hook_command(hook, command) {
                    continue;
                }
                let Some(hash) = codex_hook_trust_hash(event_name, matcher, hook) else {
                    continue;
                };
                entries.push(CodexHookTrustEntry {
                    key: format!("{key_source}:{label}:{group_index}:{handler_index}"),
                    hash,
                });
            }
        }
    }
    entries
}

fn parse_config_document(config_path: &Path) -> Result<DocumentMut, DomainStateError> {
    let text = read_file_text(config_path);
    if text.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    text.parse::<DocumentMut>()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!(
                "Codex config {} could not be parsed, so its hook trust was left untouched: {error}",
                path_string(config_path)
            ),
        })
}

fn hook_state_table(document: &DocumentMut) -> Option<&dyn toml_edit::TableLike> {
    document
        .as_table()
        .get("hooks")?
        .as_table_like()?
        .get("state")?
        .as_table_like()
}

/// Whether Codex would run every Ghostex-owned hook in `hooks_path` today.
pub(crate) fn codex_hook_trust_status(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    command: &str,
) -> CodexHookTrustStatus {
    let entries = ghostex_codex_hook_trust_entries(hooks_path, hook_paths, command);
    if entries.is_empty() {
        return CodexHookTrustStatus::Trusted;
    }
    let Ok(document) = parse_config_document(&codex_config_path_for_hooks(hooks_path)) else {
        return CodexHookTrustStatus::Untrusted;
    };
    let Some(state) = hook_state_table(&document) else {
        return CodexHookTrustStatus::Untrusted;
    };
    let mut disabled = false;
    for entry in &entries {
        let Some(record) = state.get(&entry.key).and_then(Item::as_table_like) else {
            return CodexHookTrustStatus::Untrusted;
        };
        let trusted_hash = record
            .get("trusted_hash")
            .and_then(Item::as_str)
            .map(str::trim);
        if trusted_hash != Some(entry.hash.as_str()) {
            return CodexHookTrustStatus::Untrusted;
        }
        if record.get("enabled").and_then(Item::as_bool) == Some(false) {
            disabled = true;
        }
    }
    if disabled {
        CodexHookTrustStatus::Disabled
    } else {
        CodexHookTrustStatus::Trusted
    }
}

/// The worst trust status across every `hooks.json` that holds Ghostex hooks.
pub(crate) fn codex_hook_trust_status_for_paths(
    hooks_paths: &[PathBuf],
    hook_paths: &HookPaths,
    command: &str,
) -> CodexHookTrustStatus {
    let mut status = CodexHookTrustStatus::Trusted;
    for hooks_path in hooks_paths {
        match codex_hook_trust_status(hooks_path, hook_paths, command) {
            CodexHookTrustStatus::Untrusted => return CodexHookTrustStatus::Untrusted,
            CodexHookTrustStatus::Disabled => status = CodexHookTrustStatus::Disabled,
            CodexHookTrustStatus::Trusted => {}
        }
    }
    status
}

fn write_config_document(
    config_path: &Path,
    document: &DocumentMut,
) -> Result<(), DomainStateError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temp_path = temp_path_for(config_path);
    fs::write(&temp_path, document.to_string()).map_err(io_error)?;
    fs::rename(&temp_path, config_path).map_err(io_error)?;
    Ok(())
}

/// Records Codex trust for the Ghostex-owned hooks in `hooks_path`. Returns
/// whether `config.toml` changed. See the module comment for the consent rule
/// behind `mode`.
pub(crate) fn trust_ghostex_codex_hooks(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    command: &str,
    mode: CodexTrustWriteMode,
) -> Result<bool, DomainStateError> {
    let entries = ghostex_codex_hook_trust_entries(hooks_path, hook_paths, command);
    if entries.is_empty() {
        return Ok(false);
    }
    let config_path = codex_config_path_for_hooks(hooks_path);
    let mut document = parse_config_document(&config_path)?;
    let previously_recorded = hook_state_table(&document)
        .map(|state| {
            entries
                .iter()
                .filter(|entry| state.get(&entry.key).is_some())
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let targets = match mode {
        CodexTrustWriteMode::Explicit => entries,
        CodexTrustWriteMode::RefreshExisting => entries
            .into_iter()
            .filter(|entry| previously_recorded.contains(&entry.key))
            .collect(),
    };
    if targets.is_empty() {
        return Ok(false);
    }

    let root = document.as_table_mut();
    let hooks_item = root.entry("hooks").or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    let Some(hooks) = hooks_item.as_table_mut() else {
        return Err(unsupported_hooks_shape(&config_path));
    };
    let state_item = hooks.entry("state").or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    let Some(state) = state_item.as_table_mut() else {
        return Err(unsupported_hooks_shape(&config_path));
    };

    let mut changed = false;
    for entry in targets {
        let record_item = state
            .entry(&entry.key)
            .or_insert_with(|| Item::Table(Table::new()));
        let Some(record) = record_item.as_table_like_mut() else {
            return Err(unsupported_hooks_shape(&config_path));
        };
        let current_hash = record
            .get("trusted_hash")
            .and_then(Item::as_str)
            .map(str::to_string);
        if current_hash.as_deref() != Some(entry.hash.as_str()) {
            record.insert("trusted_hash", toml_edit::value(entry.hash.clone()));
            changed = true;
        }
        if mode == CodexTrustWriteMode::Explicit
            && record.get("enabled").and_then(Item::as_bool) == Some(false)
        {
            record.remove("enabled");
            changed = true;
        }
    }
    if changed {
        write_config_document(&config_path, &document)?;
    }
    Ok(changed)
}

/// Drops the `[hooks.state]` records for the given keys (computed BEFORE the
/// hooks were removed from `hooks.json`, since the keys are positional).
pub(crate) fn remove_codex_hook_trust_entries(
    hooks_path: &Path,
    keys: &[String],
) -> Result<bool, DomainStateError> {
    if keys.is_empty() {
        return Ok(false);
    }
    let config_path = codex_config_path_for_hooks(hooks_path);
    if read_file_text(&config_path).trim().is_empty() {
        return Ok(false);
    }
    let mut document = parse_config_document(&config_path)?;
    let Some(state) = document
        .as_table_mut()
        .get_mut("hooks")
        .and_then(Item::as_table_like_mut)
        .and_then(|hooks| hooks.get_mut("state"))
        .and_then(Item::as_table_like_mut)
    else {
        return Ok(false);
    };
    let mut changed = false;
    for key in keys {
        if state.remove(key).is_some() {
            changed = true;
        }
    }
    if changed {
        write_config_document(&config_path, &document)?;
    }
    Ok(changed)
}

fn unsupported_hooks_shape(config_path: &Path) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!(
            "Codex config {} keeps `hooks` in a shape Ghostex cannot edit; trust the Ghostex hooks from Codex's /hooks instead.",
            path_string(config_path)
        ),
    }
}
