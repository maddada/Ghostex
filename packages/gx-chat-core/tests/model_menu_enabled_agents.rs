//! The model menu shows tabs only for the agents turned on in Settings › Agents (issue #486): the
//! list rides every chat read, and the session's own agent keeps its tab even when it was turned
//! off after the session started.

use ghostex_gx_chat_core::menus::catalog::bundled_agent_model_catalog;
use ghostex_gx_chat_core::menus::picker::inputs::menu_inputs;
use ghostex_gx_chat_core::menus::picker::model_menu::{
    model_menu_entries, model_menu_rows, model_menu_tabs, ModelMenuCurrent, ModelMenuTabId,
};
use ghostex_gx_chat_core::session::apply::apply_draft_agent_carriage;
use ghostex_gx_chat_core::Event;
use ghostex_gx_chat_core::{ChatContext, ChatState, ReadSessionChatResult};
use serde_json::{json, Value};

fn context() -> ChatContext {
    ChatContext {
        now_ms: 1_000_000.0,
        ..Default::default()
    }
}

/// A started chat running `agent`, with the bundled lineup of every agent.
fn chat(agent: &str) -> ChatState {
    let mut state = ChatState::default();
    state.identity.session_key = "project:session".into();
    state.menus.model_catalog = bundled_agent_model_catalog().unwrap();
    state.session.agent = Some(agent.into());
    state.session.session_agent_id = Some(agent.into());
    state
}

/// A chat read as gxserver answers it, reduced to the fields this test is about.
fn read(extra: Value) -> ReadSessionChatResult {
    let mut read = json!({"status": "ready"});
    read.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(read).unwrap()
}

/// Runs the menus' settle (which builds the menu's inputs) and lists the tabs, favorites first.
fn tabs(state: &mut ChatState) -> Vec<String> {
    ghostex_gx_chat_core::menus::settle(state, &Event::Tick, &context());
    let inputs = menu_inputs(state, &context());
    model_menu_tabs(
        &model_menu_entries(&inputs.catalogs),
        ModelMenuTabId::Favorites,
    )
    .into_iter()
    .map(|tab| tab.id)
    .collect()
}

#[test]
fn with_only_opencode_on_the_menu_shows_only_its_tab() {
    let mut state = chat("opencode");
    let all = tabs(&mut state);
    assert!(all.contains(&"claude".to_string()) && all.contains(&"codex".to_string()));

    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode"]})),
    );
    assert_eq!(tabs(&mut state), ["favorites", "opencode"]);
}

#[test]
fn a_custom_agent_counts_for_its_base_agents_icon() {
    let mut state = chat("opencode");
    // A custom agent built on Claude carries Claude's icon; a Cursor agent carries `cursor-cli`.
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode", "claude", "cursor-cli"]})),
    );
    assert_eq!(
        tabs(&mut state),
        ["favorites", "claude", "cursor", "opencode"]
    );
}

#[test]
fn the_sessions_own_agent_keeps_its_tab_after_it_was_turned_off() {
    let mut state = chat("claude");
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode"]})),
    );
    assert_eq!(tabs(&mut state), ["favorites", "claude", "opencode"]);
}

#[test]
fn turning_an_agent_back_on_brings_its_tab_back() {
    let mut state = chat("opencode");
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode"]})),
    );
    assert_eq!(tabs(&mut state), ["favorites", "opencode"]);
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode", "codex"]})),
    );
    assert_eq!(tabs(&mut state), ["favorites", "codex", "opencode"]);
}

#[test]
fn a_read_without_the_list_changes_nothing() {
    // An older gxserver never sends the list: every tab stays, as before.
    let mut state = chat("opencode");
    let all = tabs(&mut state);
    apply_draft_agent_carriage(&mut state, &read(json!({})));
    assert_eq!(tabs(&mut state), all);
    // And once a read has said, a read without it keeps what the last one said.
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode"]})),
    );
    apply_draft_agent_carriage(&mut state, &read(json!({})));
    assert_eq!(tabs(&mut state), ["favorites", "opencode"]);
}

#[test]
fn a_starred_model_of_an_agent_that_is_off_leaves_the_favorites_tab() {
    let mut state = chat("opencode");
    apply_draft_agent_carriage(
        &mut state,
        &read(json!({"enabledAgentIcons": ["opencode"]})),
    );
    tabs(&mut state);
    let inputs = menu_inputs(&state, &context());
    let entries = model_menu_entries(&inputs.catalogs);
    let codex_first = bundled_agent_model_catalog().unwrap().agents["codex"].models[0]
        .value
        .clone();
    let starred = vec![format!("codex:{codex_first}")];
    let rows = model_menu_rows(
        &entries,
        ModelMenuTabId::Favorites,
        "",
        &starred,
        &ModelMenuCurrent::default(),
        false,
    );
    assert!(rows.is_empty());
}
