//! Which settings loosen security (GitHub #495): changing one needs the owner's presence,
//! whether the change comes through gxserver's API (an approval request, server/src/server/
//! security_gates.rs) or the desktop's own Settings window (the system prompt before the save,
//! apps/desktop/src/app/owner_security/settings.rs). One rule for both.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::Value;

/// The pages whose settings open a way in or hand out access: Remote, Accounts, Secrets, Sync and
/// Connectors. Collab's are gated by Collab's own approvals.
const SECURITY_RELAXING_TABS: &[&str] = &["remote", "accounts", "secrets", "sync", "connectors"];

fn security_relaxing_keys() -> &'static HashSet<String> {
    static KEYS: OnceLock<HashSet<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        crate::help::catalog()
            .settings
            .into_iter()
            .filter(|entry| SECURITY_RELAXING_TABS.contains(&entry.tab.as_str()))
            .map(|entry| entry.key)
            .collect()
    })
}

/// A key that holds a token, password, secret, API key or credential.
pub fn holds_secret(key: &str) -> bool {
    let lowered = key.to_lowercase();
    ["token", "password", "secret", "apikey", "credential"]
        .iter()
        .any(|word| lowered.contains(word))
}

/// Whether changing setting `key` to `value` needs the owner's presence. Turning GBots off never
/// does.
pub fn needs_owner_presence(key: &str, value: &Value) -> bool {
    if key == "gbotsEnabled" {
        return value.as_bool() == Some(true);
    }
    key == "gbotsComputer" || holds_secret(key) || security_relaxing_keys().contains(key)
}
