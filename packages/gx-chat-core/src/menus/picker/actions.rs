//! Family e2's picker actions: the model menu's tab, search, stars and picks, and the fork
//! branch switch.
//!
//! Port of the `modelMenu*` and `selectForkBranch` arms of
//! `packages/shared/session-chat-controller/native-host.ts`.

use serde_json::{json, Map, Value};

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::menus::picker::favorites::{
    model_favorites_key, serialize_model_favorites, toggle_model_favorite,
};
use crate::menus::picker::model_menu::ModelMenuTabId;
use crate::menus::picker::model_picker::{model_pick_scope, ModelPickerSelection};
use crate::menus::picker::projection::{model_menu_pick, ModelMenuPick};
use crate::menus::picker::settle::queue_model_selection;
use crate::state::{ChatContext, ChatState};

/// Handles one model menu or fork branch action.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::ModelMenuView => model_menu_view(state, action),
        ActionKind::ModelMenuFavorite => model_menu_favorite(state, action),
        ActionKind::ModelMenuPick => model_menu_pick_action(state, action, context),
        ActionKind::ModelMenuTrait => model_menu_trait(state, action),
        ActionKind::SelectForkBranch => vec![Effect::HostAction {
            action: "selectForkBranch".to_string(),
            params: Box::new(Value::Object(action.params.clone())),
        }],
        _ => Vec::new(),
    }
}

/// `modelMenuView`. A null tab is the picker opening: stars set in other sessions since the last
/// open arrive here, which is the one read of the favorites record.
fn model_menu_view(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let tab = action.param("tab");
    let mut effects = Vec::new();
    match tab {
        Some(Value::Null) => {
            state.pickers.model_menu_view.tab = None;
            state.pickers.model_menu_view.legacy_open = false;
            effects.push(Effect::ReadStorage {
                key: model_favorites_key(),
            });
        }
        Some(Value::String(tab)) => {
            let tab = Some(ModelMenuTabId::from_wire(tab));
            if state.pickers.model_menu_view.tab != tab {
                state.pickers.model_menu_view.legacy_open = false;
            }
            state.pickers.model_menu_view.tab = tab;
        }
        // `command.tab === undefined` leaves the tab alone.
        _ => {}
    }
    if let Some(query) = action.param("query").and_then(Value::as_str) {
        state.pickers.model_menu_view.query = query.to_string();
    }
    if let Some(open) = action.param("legacyOpen").and_then(Value::as_bool) {
        state.pickers.model_menu_view.legacy_open = open;
    }
    effects
}

/// `modelMenuFavorite`: the star is written through the shared list, not a per-session copy.
fn model_menu_favorite(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(key) = action.param("key").and_then(Value::as_str) else {
        return Vec::new();
    };
    let next = toggle_model_favorite(&state.pickers.model_favorites, key);
    state.pickers.model_favorites = next.clone();
    vec![Effect::WriteStorage {
        key: model_favorites_key(),
        value: Some(serialize_model_favorites(&next)),
        durable: false,
    }]
}

/// `modelMenuPick`.
///
/// A pick on the session's own agent becomes a `selectOption`, which is family e1's; a pick on
/// another agent's model hands the conversation over, or switches a draft's agent.
fn model_menu_pick_action(
    state: &mut ChatState,
    action: &UserAction,
    context: &ChatContext,
) -> Vec<Effect> {
    let Some(menu) = state.pickers.model_menu_context.clone() else {
        return Vec::new();
    };
    let Some(key) = action.param("key").and_then(Value::as_str) else {
        return Vec::new();
    };
    let rows = state.pickers.model_menu_rows(&menu);
    let Some(row) = rows.into_iter().find(|row| row.key == key) else {
        return Vec::new();
    };
    // `modelMenuEffortFor` needs the other agent's option catalog, which is family e1's. Until
    // it is wired the hand-off starts the model on no effort, which is what an agent without one
    // already gets.
    let effort = action
        .param("effort")
        .and_then(Value::as_str)
        .map(str::to_string);
    let pick = model_menu_pick(&row, &menu, effort, |_, _, _| String::new());
    match pick {
        // A pick that carries a reasoning level queues model and level together.
        ModelMenuPick::Select {
            value,
            effort: Some(effort),
        } => {
            let Some(provider) = menu.provider else {
                return Vec::new();
            };
            let secondary = action.param("secondary").and_then(Value::as_bool) == Some(true);
            let scope = model_pick_scope(Some(provider), secondary);
            queue_model_selection(
                state,
                ModelPickerSelection {
                    model: value,
                    effort,
                },
                Some(provider),
                scope,
                context.random_id(0),
            )
        }
        ModelMenuPick::Select {
            value,
            effort: None,
        } => {
            let mut params = Map::new();
            params.insert("type".into(), json!("selectOption"));
            params.insert("descriptorId".into(), json!(menu.model_id));
            params.insert("value".into(), json!(value));
            if let Some(secondary) = action.param("secondary") {
                params.insert("secondary".into(), secondary.clone());
            }
            vec![Effect::HostAction {
                action: "selectOption".to_string(),
                params: Box::new(Value::Object(params)),
            }]
        }
        ModelMenuPick::Handoff {
            provider,
            model,
            effort,
        } => {
            // A draft has no conversation to hand over, so another agent's model switches the
            // draft to that agent.
            //
            // CDXC:SessionChat 2026-09-22 WHY:
            // The agent is looked up HERE rather than handed to the host as a
            // `switchDraftAgentForProvider` action nobody performs: `native-host.ts:1058` did the
            // same `availableAgents.find(...)` before it dispatched `switchDraftAgent`, and the
            // core already holds the list.
            if state.session.available_agents.is_some() {
                state.pickers.model_menu_view = Default::default();
                let agent_id = crate::menus::option_menus::DraftAgent::list(
                    state.session.available_agents.as_ref(),
                )
                .and_then(|agents| {
                    agents
                        .iter()
                        .find(|agent| {
                            crate::menus::picker::model_picker::model_picker_provider(
                                agent.icon.as_deref(),
                            ) == Some(provider)
                        })
                        .map(|agent| agent.agent_id.clone())
                });
                // The launch carries a model only for Claude, Codex and Pi (whose lineup values are
                // its own `provider/id` and thinking levels), and Empryo, which gxserver types into
                // it once it is up; the other CLIs start on their own default.
                let launchable = matches!(
                    provider,
                    crate::menus::picker::model_picker::ModelPickerProvider::Claude
                        | crate::menus::picker::model_picker::ModelPickerProvider::Codex
                        | crate::menus::picker::model_picker::ModelPickerProvider::Pi
                        | crate::menus::picker::model_picker::ModelPickerProvider::Empryo
                );
                let carry =
                    |value: &str| (launchable && !value.is_empty()).then(|| value.to_string());
                let Some(agent_id) = agent_id else {
                    return Vec::new();
                };
                let effects = crate::menus::actions::switch_draft_agent_with(
                    state,
                    &agent_id,
                    carry(&model),
                    carry(&effort),
                );
                // OpenCode takes its model once it runs (`settle::deliver_draft_handoff_model`).
                if provider == crate::menus::picker::model_picker::ModelPickerProvider::OpenCode
                    && !model.is_empty()
                    && state.menus.draft_agent_switch.is_some()
                {
                    state.pickers.draft_handoff_model = Some(crate::state::DraftHandoffModel {
                        provider,
                        selection: ModelPickerSelection { model, effort },
                        until_ms: context.now_millis()
                            + crate::menus::picker::settle::DRAFT_HANDOFF_MODEL_MS,
                    });
                }
                return effects;
            }
            vec![Effect::HostAction {
                action: "handoffToModel".to_string(),
                params: Box::new(json!({
                    "provider": provider.as_str(),
                    "model": model,
                    "effort": effort,
                })),
            }]
        }
    }
}

/// `modelMenuTrait`: a footer button becomes a `selectOption` on the descriptor it names, except
/// the context window, which picks a model variant.
fn model_menu_trait(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(menu) = state.pickers.model_menu_context.as_ref() else {
        return Vec::new();
    };
    let id = action
        .param("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let descriptor_id = if id == "context" {
        json!(menu.model_id)
    } else {
        json!(id)
    };
    let mut params = Map::new();
    params.insert("type".into(), json!("selectOption"));
    params.insert("descriptorId".into(), descriptor_id);
    if let Some(value) = action.param("value") {
        params.insert("value".into(), value.clone());
    }
    if let Some(exit_plan) = action.param("exitPlan") {
        params.insert("exitPlan".into(), exit_plan.clone());
    }
    if let Some(secondary) = action.param("secondary") {
        params.insert("secondary".into(), secondary.clone());
    }
    vec![Effect::HostAction {
        action: "selectOption".to_string(),
        params: Box::new(Value::Object(params)),
    }]
}
