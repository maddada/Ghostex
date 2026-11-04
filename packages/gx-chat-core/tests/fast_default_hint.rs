use std::collections::BTreeMap;

use ghostex_gx_chat_core::menus::{
    catalog::parse_agent_model_catalog, picker::model_menu::ModelMenuEntries,
    picker::traits::model_menu_traits,
};
use serde_json::json;

fn catalog(fast_scope: &str) -> ghostex_gx_chat_core::menus::catalog::AgentModelCatalog {
    parse_agent_model_catalog(&json!({"schemaVersion":1,"updatedAt":"2026-10-28","effortLabels":{},"agents":{
        "codex":{"name":"Codex CLI","efforts":["low","high"],"fastMode":{"available":true,"command":"/fast","scope":fast_scope},"models":[{"value":"gpt-6.1-sol","label":"GPT-6.1-Sol","efforts":["low","high"]}]},
        "claude":{"name":"Claude Code","efforts":["low","high"],"fastMode":{"available":true,"command":"/fast","scope":"model"},"models":[{"value":"opus","label":"Opus","efforts":["low","high"]}]}
    }}))
    .unwrap()
}

fn fast_hint(provider: &str, model: &str, fast_scope: &str) -> serde_json::Value {
    let entries: ModelMenuEntries = BTreeMap::new();
    let traits = model_menu_traits(
        &entries,
        &[],
        Some(provider),
        Some(model),
        None,
        &catalog(fast_scope),
    );
    let fast = traits
        .iter()
        .find(|row| row.id == "fastMode")
        .expect("the Fast button is always drawn");
    fast.to_json()["hint"].clone()
}

/// Codex saves Fast to its config, so the picker's Fast button carries an ⓘ note saying so; the
/// row itself gets no extra text, and agents whose Fast stays with the session get no note.
#[test]
fn codex_fast_button_notes_that_codex_keeps_it_for_new_sessions() {
    assert_eq!(
        fast_hint("codex", "gpt-6.1-sol", "default"),
        json!("Codex keeps Fast for new Codex sessions too.")
    );
    assert_eq!(fast_hint("codex", "gpt-6.1-sol", "session"), json!(null));
    assert_eq!(fast_hint("claude", "opus", "default"), json!(null));
}
