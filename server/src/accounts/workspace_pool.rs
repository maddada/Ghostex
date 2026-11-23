//! Shared workspace account-pool helper for every automatic account switcher and limit-wait manager
//! (`recovery.rs`, `autopilot.rs`, `login_expired.rs`, `disabled.rs`, `model_fallback.rs`,
//! `wake_account.rs`, `limit_wait.rs`).
//!
//! Returns `None` when Workspaces is off, the provider is not Claude, the session skips workspace
//! context (GBot API chats, Help chats), or the project's workspace does not pin any Claude
//! accounts. In all those cases, callers behave identically to unscoped provider-wide pools.

use super::model::Provider;
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Returns the Claude account IDs pinned by `project`'s workspace for `session`, or `None` when
/// automatic switches should stay unscoped (non-Claude provider, Workspaces off, workspace-skipped
/// chat, or no accounts pinned on that workspace).
pub(super) fn session_pool(
    db: &Connection,
    project: &Value,
    session: &Value,
    provider: Provider,
) -> Option<Vec<String>> {
    if provider != Provider::Claude || !crate::workspaces::workspaces_feature_enabled() {
        return None;
    }
    if crate::workspaces::session_skips_workspace_context(
        project,
        session.get("runtimeSettings").and_then(Value::as_object),
    ) {
        return None;
    }
    crate::workspaces::project_claude_account_ids(db, project)
        .ok()
        .filter(|ids| !ids.is_empty())
}

/// Whether `account_id` belongs to the optional workspace pool (`None` or empty allows all).
pub(super) fn in_pool(account_id: &str, allowed_ids: Option<&[String]>) -> bool {
    match allowed_ids {
        Some(ids) if !ids.is_empty() => ids.iter().any(|id| id == account_id),
        _ => true,
    }
}

/// Stable episode key for `limit_wait`: returns the plain provider ID (`"claude"`, `"codex"`) when
/// unscoped so existing episodes and notification IDs stay unchanged, or `"<provider>:pool:<hash>"`
/// for a workspace-pinned account set so each pool tracks its own single exhaustion episode.
pub(super) fn pool_key(provider: Provider, allowed_ids: Option<&[String]>) -> String {
    let Some(ids) = allowed_ids.filter(|ids| !ids.is_empty()) else {
        return provider.id().to_string();
    };
    let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut hasher = Sha256::new();
    for id in sorted {
        hasher.update(id.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hex::encode(hasher.finalize());
    format!("{}:pool:{}", provider.id(), &digest[..12])
}

/// Recovers the provider from a [`pool_key`] (`"claude"`, `"codex"`, or `"claude:pool:..."`).
pub(super) fn provider_from_pool_key(key: &str) -> Option<Provider> {
    match key.split(':').next() {
        Some("claude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        _ => None,
    }
}
