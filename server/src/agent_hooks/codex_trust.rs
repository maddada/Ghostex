/*
CDXC:AgentHooks 2026-09-02:
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
re-trusts a Ghostex hook solely when Codex already held a trust record for
that hook (a command-path upgrade of an already-approved hook); a slot Codex never
saw approved stays untrusted and surfaces as "needs update" in Settings until
the user presses Update Hooks themselves. Hooks other writers put in the same
file are never trusted by Ghostex; their existing records only follow them
when a Ghostex edit moves them (`follow_codex_hook_trust`).

Hash and key formats mirror `codex-rs/hooks/src/engine/discovery.rs`
(`hook_hash`, `hook_key`) and `codex-rs/config/src/fingerprint.rs`
(`version_for_toml`), verified byte-for-byte against a live trusted entry.
The config file is edited with `toml_edit` so every other line, comment and
table the user wrote survives untouched.
*/

use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    // CDXC:AgentHooks 2026-11-07 WHY: Codex hashes the handler it normalized: the command this platform runs, with `command_windows: None` (so no `commandWindows` key at all), and `async` as written even where SessionEnd runs it synchronously. Ghostex hashed `commandWindows` beside the command and SessionEnd's `async` as false, so such a hook was never trusted (checked against a live Codex 0.167, codex_trust_tests.rs).
    let written_async = hook.get("async").and_then(Value::as_bool).unwrap_or(false);
    let mut handler = Map::new();
    handler.insert("type".to_string(), json!("command"));
    handler.insert("command".to_string(), json!(command));
    handler.insert("timeout".to_string(), json!(timeout));
    handler.insert("async".to_string(), json!(written_async));
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
/// resolves its home. A Codex given CODEX_HOME (which is how a profile
/// directory is selected) canonicalizes it; without one it joins `.codex` to
/// the home folder as it is spelled, through any symlink or junction.
fn codex_hook_key_source(hooks_path: &Path, hook_paths: &HookPaths) -> String {
    let codex_home_set = hook_paths.respect_config_environment
        && std::env::var_os("CODEX_HOME").is_some_and(|value| !value.is_empty());
    codex_hook_key_source_with(
        hooks_path,
        hook_paths,
        codex_home_set,
        cfg!(windows),
        |path| fs::canonicalize(path).ok().map(|path| path_string(&path)),
    )
}

/// `codex_hook_key_source` for a given platform and canonicalizer, so Windows homes are testable
/// anywhere.
fn codex_hook_key_source_with(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    codex_home_set: bool,
    windows: bool,
    canonicalize: impl Fn(&Path) -> Option<String>,
) -> String {
    let Some(codex_home) = hooks_path.parent() else {
        return path_string(hooks_path);
    };
    let explicit = codex_home
        != ghostex_paths::agent_homes::codex_default_home(&hook_paths.home_dir)
        || codex_home_set;
    let canonical = explicit.then(|| canonicalize(codex_home)).flatten();
    let file_name = hooks_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "hooks.json".to_string());
    codex_key_source_for(&path_string(codex_home), canonical, &file_name, windows)
}

/*
CDXC:AgentHooks 2026-11-07 WHY:
Codex 0.167 keys a hook's trust by its `hooks.json` path as it found the file (codex-rs/utils/home-dir `find_codex_home`, AbsolutePathBuf): CODEX_HOME is canonicalized (symlinks resolved, the case on disk), while the default home is `.codex` joined to the home folder and only tidied, never canonicalized. Both were confirmed on a live Codex 0.167: through a symlinked `~/.codex` the key kept the symlink's path, and a CODEX_HOME spelled `…/Ghostex/…` was keyed `…/ghostex/…`, the folder's case on disk. Ghostex canonicalized the default home too on Windows, so a profile folder behind a junction, a symlinked `.codex`, or a case or short-name difference gave every Ghostex hook a key Codex never reads, and Codex asked to review hooks Ghostex had trusted (GitHub #458).
SEE-ALSO: codex_trust_tests.rs (the live records, and Windows-shaped homes).
*/
fn codex_key_source_for(
    codex_home: &str,
    canonical: Option<String>,
    file_name: &str,
    windows: bool,
) -> String {
    let home = canonical.unwrap_or_else(|| codex_home.to_string());
    if windows {
        format!("{}\\{file_name}", windows_absolute_spelling(&home))
    } else {
        path_string(&Path::new(&home).join(file_name))
    }
}

/// A Windows path the way Codex's `AbsolutePathBuf` spells it: backslashes, no `\\?\` device
/// prefix, `.` and `..` resolved and repeated separators dropped, the drive letter left as written.
fn windows_absolute_spelling(path: &str) -> String {
    let path = path.replace('/', "\\");
    let path = if let Some(unc) = path
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| path.strip_prefix(r"\\.\UNC\"))
    {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\")
            .or_else(|| path.strip_prefix(r"\\.\"))
            .unwrap_or(&path)
            .to_string()
    };
    let (prefix, rest) = if let Some(unc) = path.strip_prefix(r"\\") {
        let mut parts = unc.splitn(3, '\\');
        let server = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        (
            format!(r"\\{server}\{share}"),
            parts.next().unwrap_or_default().to_string(),
        )
    } else if path.as_bytes().get(1) == Some(&b':') {
        (path[..2].to_string(), path[2..].to_string())
    } else {
        (String::new(), path)
    };
    let mut components: Vec<&str> = Vec::new();
    for component in rest.split('\\') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    format!("{prefix}\\{}", components.join("\\"))
}

/// The `config.toml` that holds `[hooks.state]` for this `hooks.json`: the
/// sibling file in the same CODEX_HOME.
pub(crate) fn codex_config_path_for_hooks(hooks_path: &Path) -> PathBuf {
    hooks_path.with_file_name("config.toml")
}

/// One command handler in a Codex `hooks.json`, keyed and hashed the way Codex sees it, whoever
/// wrote it.
struct CodexHookSlot {
    label: &'static str,
    key: String,
    /// Codex's `trusted_hash` for the handler; `None` for a handler type Ghostex cannot hash.
    hash: Option<String>,
    /// What makes two slots the same hook: the hash, or for a handler Ghostex cannot hash, its
    /// event, matcher and whole definition as written.
    identity: String,
    /// A Ghostex-owned command hook (always hashable).
    ghostex: bool,
}

/// Every handler in `data`. Group and handler indexes are positions in the file, so a
/// hook's key changes whenever a group before it in the same event comes or goes.
fn codex_hook_slots(data: &Value, key_source: &str, command: &str) -> Vec<CodexHookSlot> {
    let Some(events) = data.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut slots = Vec::new();
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
                let hash = codex_hook_trust_hash(event_name, matcher, hook);
                let identity = hash.clone().unwrap_or_else(|| {
                    let written = json!({ "event": label, "matcher": matcher, "handler": hook });
                    format!("raw:{}", canonical_json(&written))
                });
                slots.push(CodexHookSlot {
                    label,
                    key: format!("{key_source}:{label}:{group_index}:{handler_index}"),
                    ghostex: hash.is_some() && is_ghostex_owned_hook_command(hook, command),
                    hash,
                    identity,
                });
            }
        }
    }
    slots
}

/// Every Ghostex-owned command hook in `hooks_path`, keyed and hashed the way
/// Codex will see it.
pub(crate) fn ghostex_codex_hook_trust_entries(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    command: &str,
) -> Vec<CodexHookTrustEntry> {
    let data = read_json_object(&read_file_text(hooks_path));
    let key_source = codex_hook_key_source(hooks_path, hook_paths);
    codex_hook_slots(&data, &key_source, command)
        .into_iter()
        .filter(|slot| slot.ghostex)
        .filter_map(|slot| {
            Some(CodexHookTrustEntry {
                key: slot.key,
                hash: slot.hash?,
            })
        })
        .collect()
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

/*
CDXC:AgentHooks 2026-11-13 WHY:
Codex keys trust by position (`<file>:<event>:<group>:<handler>`), and Ghostex shares `hooks.json` with other writers (bd's hooks, the user's own). Every Ghostex edit removes its groups and appends them again (install, startup repair), and the hook-free cleanup and Uninstall remove them outright, so a hook written after Ghostex's moved to an earlier slot while its trust record stayed behind: after the 11.6.0 update the hook-free cleanup dropped Ghostex's SessionStart, UserPromptSubmit and Stop groups from slot 0, the bd hooks behind them became slot 0 with no record, and every new Codex session stopped on "Hooks need review: 3 hooks are new or changed" (which also starved the hook-free tracking, whose fallback put Ghostex's hooks back at slot 1, writing Ghostex's hash over bd's old slot-1 record). So every edit is followed by this pass: a hook another writer added keeps the record Codex already holds for it, moved to its new slot, but only when that record's hash is exactly the hook's own (the user approved that hook; Ghostex adds trust for nothing it does not own). Ghostex's own records move with its hooks under the same exact-hash rule, and a repair refreshes only a Ghostex hook whose own record matched its definition before the edit, never a slot whose record was left by an earlier occupant. A handler type Ghostex cannot hash (Codex's MCP-tool hooks) keeps its record by moving it to the same definition's new slot unchanged.
SEE-ALSO: install/install_hook.rs and install/uninstall.rs (each edit reads the file before it changes it).
*/
/// Brings `[hooks.state]` in line with an edit Ghostex just made to `hooks_path`, given the file's
/// contents before the edit. Records follow their hooks to the slots they now hold; with
/// `ghostex_trust`, the Ghostex-owned hooks are trusted under that mode's consent rule. Returns
/// whether `config.toml` changed.
pub(crate) fn follow_codex_hook_trust(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    command: &str,
    before: &Value,
    ghostex_trust: Option<CodexTrustWriteMode>,
) -> Result<bool, DomainStateError> {
    let key_source = codex_hook_key_source(hooks_path, hook_paths);
    let after = read_json_object(&read_file_text(hooks_path));
    let before_slots = codex_hook_slots(before, &key_source, command);
    let after_slots = codex_hook_slots(&after, &key_source, command);
    let config_path = codex_config_path_for_hooks(hooks_path);
    if before_slots.is_empty() && !after_slots.iter().any(|slot| slot.ghostex) {
        return Ok(false);
    }
    let mut document = parse_config_document(&config_path)?;
    let records: HashMap<String, Item> = hook_state_table(&document)
        .map(|state| {
            state
                .iter()
                .map(|(key, item)| (key.to_string(), item.clone()))
                .collect()
        })
        .unwrap_or_default();
    let recorded_hash = |key: &str| {
        records
            .get(key)
            .and_then(Item::as_table_like)
            .and_then(|record| record.get("trusted_hash"))
            .and_then(Item::as_str)
            .map(|hash| hash.trim().to_string())
    };

    let disabled = |key: &str| {
        records
            .get(key)
            .and_then(Item::as_table_like)
            .and_then(|record| record.get("enabled"))
            .and_then(Item::as_bool)
            == Some(false)
    };
    // A record speaks for the hook in its slot before the edit only when its hash is exactly that
    // hook's: a record left at a slot by an earlier occupant proves nothing, so it is never moved,
    // removed or refreshed.
    let approved = |slot: &CodexHookSlot| match &slot.hash {
        Some(hash) => recorded_hash(&slot.key).as_deref() == Some(hash.as_str()),
        // A handler type Ghostex cannot hash: its record is carried to the same definition's new
        // slot as it is, which can never make it match anything it did not match before.
        None => records.contains_key(&slot.key),
    };

    let mut moves: Vec<(String, String)> = Vec::new();
    let mut removals: Vec<String> = Vec::new();
    let mut ghostex_recorded: HashSet<String> = HashSet::new();

    // Another writer's hook is the same hook after the edit when its identity matches (the hash
    // holds the matcher and the whole handler); repeats pair up in file order.
    let mut foreign_after: HashMap<(&str, &str), VecDeque<&str>> = HashMap::new();
    for slot in after_slots.iter().filter(|slot| !slot.ghostex) {
        foreign_after
            .entry((slot.label, slot.identity.as_str()))
            .or_default()
            .push_back(&slot.key);
    }
    for slot in before_slots.iter().filter(|slot| !slot.ghostex) {
        let destination = foreign_after
            .get_mut(&(slot.label, slot.identity.as_str()))
            .and_then(VecDeque::pop_front);
        if let Some(destination) = destination {
            if destination != slot.key && approved(slot) {
                moves.push((slot.key.clone(), destination.to_string()));
            }
        }
    }

    // Ghostex's own hooks: only an approved record follows, so an unapproved duplicate never takes
    // the place an approved one needs. Within an event a destination first takes an approved hook
    // with the same hash (an unchanged hook), then one whose command a repair changed. Where
    // duplicates collapse into one hook, a record the user disabled is taken first, so
    // collapsing never turns a disabled hook back on; the records left over go with their hooks.
    let mut ghostex_sources: HashMap<&str, Vec<&CodexHookSlot>> = HashMap::new();
    for slot in before_slots
        .iter()
        .filter(|slot| slot.ghostex && approved(slot))
    {
        ghostex_sources.entry(slot.label).or_default().push(slot);
    }
    for sources in ghostex_sources.values_mut() {
        sources.sort_by_key(|slot| !disabled(&slot.key));
    }
    let mut ghostex_pairs: Vec<(&CodexHookSlot, &CodexHookSlot)> = Vec::new();
    let mut unpaired: Vec<&CodexHookSlot> = Vec::new();
    for destination in after_slots.iter().filter(|slot| slot.ghostex) {
        let sources = ghostex_sources.entry(destination.label).or_default();
        match sources
            .iter()
            .position(|source| source.hash == destination.hash)
        {
            Some(index) => ghostex_pairs.push((sources.remove(index), destination)),
            None => unpaired.push(destination),
        }
    }
    for destination in unpaired {
        let sources = ghostex_sources.entry(destination.label).or_default();
        if !sources.is_empty() {
            ghostex_pairs.push((sources.remove(0), destination));
        }
    }
    for (source, destination) in ghostex_pairs {
        ghostex_recorded.insert(destination.key.clone());
        if destination.key != source.key {
            moves.push((source.key.clone(), destination.key.clone()));
        }
    }
    removals.extend(
        ghostex_sources
            .values()
            .flatten()
            .map(|source| source.key.clone()),
    );

    let targets = after_slots
        .iter()
        .filter(|slot| slot.ghostex)
        .filter_map(|slot| Some((slot, slot.hash.as_deref()?)))
        .filter(|(slot, _)| match ghostex_trust {
            Some(CodexTrustWriteMode::Explicit) => true,
            Some(CodexTrustWriteMode::RefreshExisting) => ghostex_recorded.contains(&slot.key),
            None => false,
        })
        .collect::<Vec<_>>();
    if moves.is_empty() && removals.is_empty() && targets.is_empty() {
        return Ok(false);
    }

    let state = state_table_mut(&mut document, &config_path)?;
    let mut changed = false;
    for key in moves.iter().map(|(source, _)| source).chain(&removals) {
        changed |= state.remove(key).is_some();
    }
    for (source, destination) in &moves {
        if let Some(record) = records.get(source) {
            state.insert(destination, record.clone());
            changed = true;
        }
    }
    for (slot, hash) in targets {
        let record_item = state
            .entry(&slot.key)
            .or_insert_with(|| Item::Table(Table::new()));
        let Some(record) = record_item.as_table_like_mut() else {
            return Err(unsupported_hooks_shape(&config_path));
        };
        let current_hash = record
            .get("trusted_hash")
            .and_then(Item::as_str)
            .map(str::to_string);
        if current_hash.as_deref() != Some(hash) {
            record.insert("trusted_hash", toml_edit::value(hash));
            changed = true;
        }
        if ghostex_trust == Some(CodexTrustWriteMode::Explicit)
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

/// `[hooks.state]`, created (as implicit tables) when the file has none.
fn state_table_mut<'a>(
    document: &'a mut DocumentMut,
    config_path: &Path,
) -> Result<&'a mut dyn toml_edit::TableLike, DomainStateError> {
    let hooks_item = document.as_table_mut().entry("hooks").or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    let Some(hooks) = hooks_item.as_table_like_mut() else {
        return Err(unsupported_hooks_shape(config_path));
    };
    let state_item = hooks.entry("state").or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    state_item
        .as_table_like_mut()
        .ok_or_else(|| unsupported_hooks_shape(config_path))
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

#[cfg(test)]
#[path = "codex_trust_tests.rs"]
mod tests;
