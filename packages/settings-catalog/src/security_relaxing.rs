//! Which settings let someone in (GitHub #495, narrowed by Q2102): changing one needs the owner's presence,
//! whether the change comes through gxserver's API (an approval request, server/src/server/
//! security_gates.rs) or the desktop's own Settings window (the system prompt before the save,
//! apps/desktop/src/app/owner_security/settings.rs). One rule for both.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::Value;

/// The pages whose settings hand out access: Accounts, Secrets and Connectors. Collab's are gated
/// by Collab's own approvals.
const SECURITY_RELAXING_TABS: &[&str] = &["accounts", "secrets", "connectors"];

/*
CDXC:Security 2026-11-10 DECISION:
User (Q2102, "ask only for settings that let someone in"): Accounts, Secrets and Connectors keep asking, and on the Remote page only its ways in do (SSH access, Easy Connect on, pairing and paired devices, Tailscale); Sync and the everyday Remote rows (names, saved computers, and the Easy Connect relay, ports and allow-list, which Q2101 lets follow without asking) don't. This supersedes Q1936 (11.1.2), which asked for every Remote row but the computer's short name, and every Sync row.
*/
/// The Remote page's settings that let someone in: changing one asks.
const REMOTE_WAYS_IN: &[&str] = &[
    "sshAccess",
    "easyConnectEnabled",
    "pairingCode",
    "pairedDevices",
    "pairedDevicesOnly",
    "tailscaleEnabled",
    "remoteTailscaleEnabled",
];

fn security_relaxing_keys() -> &'static HashSet<String> {
    static KEYS: OnceLock<HashSet<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        crate::help::catalog()
            .settings
            .into_iter()
            .filter(|entry| SECURITY_RELAXING_TABS.contains(&entry.tab.as_str()))
            .map(|entry| entry.key)
            .chain(REMOTE_WAYS_IN.iter().map(|key| key.to_string()))
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
/// does; turning them on asks only from the terminal and agents (Q2105: gxserver's policy lets
/// your own computer and phone through, and the Settings window uses [`own_save_needs_presence`]).
pub fn needs_owner_presence(key: &str, value: &Value) -> bool {
    if key == "gbotsEnabled" {
        return value.as_bool() == Some(true);
    }
    // Shortening the home folder's deny list lets agents read what it kept them out of.
    key == "homeSecretFolders" || holds_secret(key) || security_relaxing_keys().contains(key)
}

/// Whether the owner's own Settings window asks for the system prompt before saving `key`:
/// [`needs_owner_presence`] without GBots, which your own clicks turn on without asking (Q2105,
/// CDXC:Security 2026-11-10 in server/src/security/policy.rs).
pub fn own_save_needs_presence(key: &str, value: &Value) -> bool {
    key != "gbotsEnabled" && needs_owner_presence(key, value)
}
