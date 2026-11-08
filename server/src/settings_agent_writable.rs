//! Which settings a caller of gxserver's API may change: the catalog's `agentWritable` rule,
//! applied to every `updateSettingsPatch` that reaches the app through `/api/dispatchRendererCommand`.
//!
//! CDXC:Security 2026-11-08 WHY: the design review (GitHub #495, section 5) found that `ghostex settings set` refused Remote, Accounts and secret-like keys while `ghostex actions dispatch updateSettingsPatch` (or any raw call to `/api/dispatchRendererCommand`) sent them to the app anyway. The rule now lives here, where every API write passes; the app's own Settings window saves without this route and is not limited by it. Settings an agent may write but that loosen security still need the owner's approval, which is the approvals gate's part.
//! SEE-ALSO: packages/settings-catalog/src/help/mod.rs (`agent_writable`), server/src/ghostex_cli/settings.rs (the same rule in the CLI's own words).

use std::collections::HashSet;
use std::sync::OnceLock;

use serde_json::{Map, Value};

/// Keys the API may write although the catalog lists them as JSON: the hotkey maps `ghostex
/// settings hotkeys` saves whole (ghostex_cli/settings_hotkeys.rs).
fn api_writable_json_keys() -> [&'static str; 2] {
    [
        "hotkeys",
        ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY,
    ]
}

fn agent_writable_keys() -> &'static HashSet<String> {
    static KEYS: OnceLock<HashSet<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        ghostex_settings_catalog::help::catalog()
            .settings
            .into_iter()
            .filter(|entry| entry.agent_writable)
            .map(|entry| entry.key)
            .chain(api_writable_json_keys().into_iter().map(str::to_string))
            .collect()
    })
}

/// The keys of `patch` the API may not change, in the patch's order.
pub(crate) fn refused_keys(patch: &Map<String, Value>) -> Vec<String> {
    let allowed = agent_writable_keys();
    patch
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect()
}

/// The refusal's words: what can't be changed and where the user changes it.
pub(crate) fn refusal_message(keys: &[String]) -> String {
    let catalog = ghostex_settings_catalog::help::catalog();
    let named = keys
        .iter()
        .map(|key| {
            catalog
                .settings
                .iter()
                .find(|entry| &entry.key == key)
                .map(|entry| format!("\"{}\" (Settings > {})", entry.title, entry.tab_title))
                .unwrap_or_else(|| format!("\"{key}\""))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Only you can change this in Ghostex's Settings window, not an agent or the ghostex CLI: {named}."
    )
}
