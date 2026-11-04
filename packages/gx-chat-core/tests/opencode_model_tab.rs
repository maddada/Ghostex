//! The model pill's OpenCode tab in a thread of another agent (issue #390): it is there with and
//! without the lineup gxserver serves, its models can be starred, and a pick hands the thread to
//! OpenCode.

use ghostex_gx_chat_core::menus::catalog::{
    bundled_agent_model_catalog, parse_agent_model_catalog,
};
use ghostex_gx_chat_core::menus::picker::inputs::menu_inputs;
use ghostex_gx_chat_core::menus::picker::model_menu::{
    model_menu_entries, model_menu_rows, model_menu_search_enabled, model_menu_tabs,
    ModelMenuCurrent, ModelMenuTabId,
};
use ghostex_gx_chat_core::menus::picker::{ModelPickerProvider, ModelSelectionScope};
use ghostex_gx_chat_core::Event;
use ghostex_gx_chat_core::{ChatContext, ChatState, Effect, Tri, UserAction};
use serde_json::{json, Value};

const OPENCODE: ModelMenuTabId = ModelMenuTabId::Provider(ModelPickerProvider::OpenCode);

/// The bundled catalog with the OpenCode lineup a gxserver serves added, as its push carries it.
fn served_catalog() -> Value {
    let mut catalog: Value =
        serde_json::from_str(include_str!("../../../agent-model-catalog.json")).unwrap();
    catalog["updatedAt"] = json!("2099-01-01T00:00:00Z");
    catalog["agents"]["opencode"] = json!({
        "name": "OpenCode",
        "efforts": ["low", "high"],
        "models": [
            {"value": "opencode/big-pickle", "label": "Big Pickle", "efforts": []},
            {"value": "anthropic/claude-sonnet-5-5", "label": "Claude Sonnet 5.5", "efforts": ["low", "high"]},
            {"value": "anthropic/claude-opus-5-5", "label": "Claude Opus 5.5", "efforts": ["low", "high"]},
            {"value": "openai/gpt-6-luna", "label": "GPT-6 Luna", "efforts": ["low", "high"]},
            {"value": "openai/gpt-6-sol", "label": "GPT-6 Sol", "efforts": ["low", "high"]},
            {"value": "openrouter/google/gemini-4-pro", "label": "Gemini 4 Pro", "efforts": []},
            {"value": "openrouter/x-ai/grok-5", "label": "Grok 5", "efforts": []},
            {"value": "openrouter/z-ai/glm-5", "label": "GLM 5", "efforts": []}
        ]
    });
    catalog
}

/// A Claude draft: nothing sent yet, Claude and OpenCode offered, and a gxserver that can queue a
/// model.
fn claude_draft() -> ChatState {
    let mut state = ChatState::default();
    state.identity.session_key = "project:draft".into();
    state.menus.model_catalog = bundled_agent_model_catalog().unwrap();
    state.session.agent = Some("claude".into());
    state.session.session_agent_id = Some("claude".into());
    state.session.available_agents = Some(json!([
        {"agentId": "claude", "name": "Claude Code", "icon": "claude"},
        {"agentId": "opencode", "name": "OpenCode", "icon": "opencode"}
    ]));
    state.session.pending_model_selection = Tri::Null;
    state
}

fn context() -> ChatContext {
    ChatContext {
        now_ms: 1_000_000.0,
        ..Default::default()
    }
}

/// Runs the menus' per-event settle, which fills the model menu's inputs.
fn settle(state: &mut ChatState, event: Event) -> Vec<Effect> {
    ghostex_gx_chat_core::menus::settle(state, &event, &context())
}

fn opencode_values(state: &ChatState, favorites: &[String]) -> Vec<String> {
    let inputs = menu_inputs(state, &context());
    model_menu_rows(
        &model_menu_entries(&inputs.catalogs),
        OPENCODE,
        "",
        favorites,
        &ModelMenuCurrent::default(),
        false,
    )
    .into_iter()
    .map(|row| row.entry.value)
    .collect()
}

fn pick(state: &mut ChatState, key: &str) -> Vec<Effect> {
    state.pickers.model_menu_view.tab = Some(OPENCODE);
    let action: UserAction =
        serde_json::from_value(json!({"type": "modelMenuPick", "key": key})).unwrap();
    ghostex_gx_chat_core::menus::picker::handle(state, &action, &context())
}

#[test]
fn a_claude_draft_shows_the_opencode_tab_before_any_lineup_arrives() {
    let mut state = claude_draft();
    assert!(!state.menus.model_catalog.agents.contains_key("opencode"));
    settle(&mut state, Event::Tick);
    let inputs = menu_inputs(&state, &context());
    let tabs = model_menu_tabs(&model_menu_entries(&inputs.catalogs), OPENCODE);
    assert!(tabs
        .iter()
        .any(|tab| tab.id == "opencode" && tab.name == "OpenCode"));
    assert_eq!(opencode_values(&state, &[]), ["opencode/big-pickle"]);
}

#[test]
fn the_served_lineup_fills_the_tab_with_search_and_stars() {
    let mut state = claude_draft();
    settle(
        &mut state,
        Event::ModelCatalogChanged {
            catalog: served_catalog(),
        },
    );
    let inputs = menu_inputs(&state, &context());
    let entries = model_menu_entries(&inputs.catalogs);
    assert!(model_menu_search_enabled(&entries, OPENCODE, &[]));
    assert_eq!(opencode_values(&state, &[]).len(), 8);
    // A starred OpenCode model sits first on its tab and on the Favorites tab.
    let starred = vec!["opencode:openai/gpt-6-sol".to_string()];
    assert_eq!(opencode_values(&state, &starred)[0], "openai/gpt-6-sol");
    let favorites: Vec<String> = model_menu_rows(
        &entries,
        ModelMenuTabId::Favorites,
        "",
        &starred,
        &ModelMenuCurrent::default(),
        false,
    )
    .into_iter()
    .map(|row| row.key)
    .collect();
    assert_eq!(favorites, starred);
    // The search field filters the OpenCode tab by label or id.
    let matches: Vec<String> = model_menu_rows(
        &entries,
        OPENCODE,
        "grok",
        &[],
        &ModelMenuCurrent::default(),
        false,
    )
    .into_iter()
    .map(|row| row.entry.value)
    .collect();
    assert_eq!(matches, ["openrouter/x-ai/grok-5"]);
}

#[test]
fn a_thread_running_opencode_keeps_its_own_lineup_over_a_push() {
    let mut state = claude_draft();
    state.session.available_agents = None;
    state.session.agent = Some("opencode".into());
    state.session.selected_options = Some(json!({"modelCatalog": {
        "schemaVersion": 1, "updatedAt": "2026-09-25", "effortLabels": {},
        "agents": {"opencode": {"name": "OpenCode", "efforts": [], "models": [
            {"value": "local/folder-model", "label": "Folder model", "efforts": []}
        ]}}
    }}));
    settle(
        &mut state,
        Event::ModelCatalogChanged {
            catalog: served_catalog(),
        },
    );
    assert_eq!(opencode_values(&state, &[]), ["local/folder-model"]);
}

#[test]
fn picking_an_opencode_model_in_a_claude_draft_switches_it_and_then_queues_that_model() {
    let mut state = claude_draft();
    settle(
        &mut state,
        Event::ModelCatalogChanged {
            catalog: served_catalog(),
        },
    );
    // The menu's rows are rebuilt on the event after the push.
    settle(&mut state, Event::Tick);
    let effects = pick(&mut state, "opencode:openai/gpt-6-luna");
    // The draft's agent switch starts with the composer flush, as for every other agent.
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::FlushStorage { .. })));
    let switch = state
        .menus
        .draft_agent_switch
        .clone()
        .expect("a draft switch");
    assert_eq!(switch.agent_id, "opencode");
    // OpenCode 2 has no launch model flag, so the model does not ride the switch.
    assert_eq!(switch.model, None);
    let waiting = state
        .pickers
        .draft_handoff_model
        .clone()
        .expect("the model waits");
    assert_eq!(waiting.selection.model, "openai/gpt-6-luna");

    // Nothing is queued while the draft still runs Claude.
    state.menus.draft_agent_switch = None;
    settle(&mut state, Event::Tick);
    assert!(state.pickers.model_selection.outbox.is_none());

    // The switch landed: the session reads as OpenCode, and the pick goes into the outbox for
    // this session only, then out to gxserver.
    state.session.agent = Some("opencode".into());
    state.session.session_agent_id = Some("opencode".into());
    let effects = settle(&mut state, Event::Tick);
    assert!(state.pickers.draft_handoff_model.is_none());
    let intent = state
        .pickers
        .model_selection
        .outbox
        .clone()
        .expect("an outbox intent");
    assert_eq!(intent.model, "openai/gpt-6-luna");
    assert_eq!(intent.scope, Some(ModelSelectionScope::Session));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SendRpc { params, .. } if params["model"] == "openai/gpt-6-luna"
    )));
}

#[test]
fn picking_an_opencode_model_in_a_started_claude_thread_hands_it_off() {
    let mut state = claude_draft();
    state.session.available_agents = None;
    settle(&mut state, Event::Tick);
    let effects = pick(&mut state, "opencode:opencode/big-pickle");
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::HostAction { action, params }
            if action == "handoffToModel"
                && params["provider"] == "opencode"
                && params["model"] == "opencode/big-pickle"
    )));
    assert!(state.pickers.draft_handoff_model.is_none());
}

#[test]
fn the_served_catalog_parses_with_an_opencode_lineup() {
    let parsed = parse_agent_model_catalog(&served_catalog()).unwrap();
    assert_eq!(parsed.agents["opencode"].models.len(), 8);
}
