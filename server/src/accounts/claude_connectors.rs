//! Read-only detection of `claude.ai` MCP connectors per Claude account.
//!
//! Claude Code records the `claude.ai` connectors it has seen connected on an account in that
//! account's local `.claude.json` under `claudeAiMcpEverConnected` (for example `"claude.ai Slack"`,
//! `"claude.ai Linear"`, `"claude.ai Gmail"`). We read only those local profile JSON files (never
//! Keychain, tokens, or network endpoints), memoized by modification time and size.
//!
//! Because `claudeAiMcpEverConnected` is updated by Claude Code itself when a session runs on that
//! account, an account whose local profile file does not exist yet (or has no
//! `claudeAiMcpEverConnected` entry) reports `detected: false` rather than guessing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::Value;

use super::claude_resets::ClaudeSlot;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeAiConnectorsSummary {
    /// True when at least one matching local `.claude.json` contained `claudeAiMcpEverConnected`.
    pub(crate) detected: bool,
    /// Display names of detected `claude.ai` connectors (for example `"Slack"`, `"Linear"`).
    pub(crate) apps: Vec<String>,
    /// Whether Slack was detected among `apps`.
    pub(crate) slack: bool,
    /// Whether Linear was detected among `apps`.
    pub(crate) linear: bool,
}

type FileStamp = (Option<SystemTime>, u64);
type CachedFileEntry = (FileStamp, Option<String>, Option<Vec<String>>);

static CONNECTOR_FILE_MEMO: OnceLock<Mutex<HashMap<PathBuf, CachedFileEntry>>> = OnceLock::new();

/// Reads the `claude.ai` connector summary for one Claude account from its local `.claude.json`
/// profile file(s): the slot's cswap session profile and, when the default login matches this
/// account's identity, `~/.claude.json`.
pub(crate) fn read_account_claude_ai_connectors(
    home: &Path,
    slot: &str,
    identity: &str,
) -> ClaudeAiConnectorsSummary {
    let mut candidate_paths: Vec<(PathBuf, bool)> = Vec::new();
    let expected_identity = identity.trim().to_ascii_lowercase();
    let trimmed_slot = slot.trim();
    if !trimmed_slot.is_empty() {
        if let Ok(claude_slot) = ClaudeSlot::read(home, trimmed_slot) {
            let session_file = PathBuf::from(claude_slot.session_dir()).join(".claude.json");
            let slot_identity_matches = expected_identity.is_empty()
                || claude_slot
                    .identity()
                    .trim()
                    .eq_ignore_ascii_case(&expected_identity);
            if slot_identity_matches {
                candidate_paths.push((session_file, false));
            } else {
                candidate_paths.push((session_file, true));
            }
            if claude_slot.names_default_login(home) {
                candidate_paths.push((ghostex_paths::agent_homes::claude_json_path(home), true));
            }
        }
    }
    // Also check the default `~/.claude.json` if its `oauthAccount` identity matches this account.
    let default_json = ghostex_paths::agent_homes::claude_json_path(home);
    if !candidate_paths.iter().any(|(p, _)| p == &default_json) {
        candidate_paths.push((default_json, true));
    }

    let mut detected = false;
    let mut apps: Vec<String> = Vec::new();

    for (path, require_identity_match) in candidate_paths {
        let Some((file_identity, entries)) = read_file_connectors_memoized(&path) else {
            continue;
        };
        if require_identity_match {
            let matches = !expected_identity.is_empty()
                && file_identity
                    .as_deref()
                    .is_some_and(|found| found.eq_ignore_ascii_case(&expected_identity));
            if !matches {
                continue;
            }
        } else if let Some(found) = file_identity.as_deref() {
            if !expected_identity.is_empty() && !found.eq_ignore_ascii_case(&expected_identity) {
                continue;
            }
        }
        if let Some(entries) = entries {
            detected = true;
            for entry in entries {
                if !apps
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&entry))
                {
                    apps.push(entry);
                }
            }
        }
    }

    let slack = apps.iter().any(|app| is_slack_connector(app));
    let linear = apps.iter().any(|app| is_linear_connector(app));
    ClaudeAiConnectorsSummary {
        detected,
        apps,
        slack,
        linear,
    }
}

fn read_file_connectors_memoized(path: &Path) -> Option<(Option<String>, Option<Vec<String>>)> {
    let meta = std::fs::metadata(path).ok()?;
    let stamp = (meta.modified().ok(), meta.len());
    let memo = CONNECTOR_FILE_MEMO.get_or_init(Default::default);
    if let Ok(guard) = memo.lock() {
        if let Some((known_stamp, identity, entries)) = guard.get(path) {
            if *known_stamp == stamp {
                return Some((identity.clone(), entries.clone()));
            }
        }
    }
    let parsed = parse_claude_json_connectors(path);
    if let Some((ref identity, ref entries)) = parsed {
        if let Ok(mut guard) = memo.lock() {
            guard.insert(
                path.to_path_buf(),
                (stamp, identity.clone(), entries.clone()),
            );
        }
    }
    parsed
}

fn parse_claude_json_connectors(path: &Path) -> Option<(Option<String>, Option<Vec<String>>)> {
    let raw = std::fs::read(path).ok()?;
    let data: Value = serde_json::from_slice(&raw).ok()?;
    let identity = data.get("oauthAccount").and_then(|account| {
        let email = account.get("emailAddress")?.as_str()?.trim();
        let org = account.get("organizationUuid")?.as_str()?.trim();
        if email.is_empty() || org.is_empty() {
            None
        } else {
            Some(format!("{}:{org}", email.to_ascii_lowercase()))
        }
    });
    let entries = data
        .get("claudeAiMcpEverConnected")
        .and_then(extract_connector_names);
    Some((identity, entries))
}

fn extract_connector_names(value: &Value) -> Option<Vec<String>> {
    let mut names = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(raw) = item.as_str() {
                    if let Some(label) = normalize_connector_label(raw) {
                        if !names
                            .iter()
                            .any(|existing: &String| existing.eq_ignore_ascii_case(&label))
                        {
                            names.push(label);
                        }
                    }
                }
            }
            Some(names)
        }
        Value::Object(map) => {
            for (key, val) in map {
                if matches!(val, Value::Bool(false) | Value::Null) {
                    continue;
                }
                if let Some(label) = normalize_connector_label(key) {
                    if !names
                        .iter()
                        .any(|existing: &String| existing.eq_ignore_ascii_case(&label))
                    {
                        names.push(label);
                    }
                }
            }
            Some(names)
        }
        _ => None,
    }
}

fn normalize_connector_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let stripped = if lower.starts_with("claude.ai ") {
        &trimmed["claude.ai ".len()..]
    } else if lower.starts_with("claude_ai_") {
        &trimmed["claude_ai_".len()..]
    } else if lower.starts_with("mcp__claude_ai_") {
        &trimmed["mcp__claude_ai_".len()..]
    } else {
        trimmed
    };
    let cleaned = stripped.replace('_', " ").trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn is_slack_connector(label: &str) -> bool {
    let lower = label.trim().to_ascii_lowercase();
    lower == "slack" || lower.split_whitespace().any(|word| word == "slack")
}

fn is_linear_connector(label: &str) -> bool {
    let lower = label.trim().to_ascii_lowercase();
    lower == "linear" || lower.split_whitespace().any(|word| word == "linear")
}
