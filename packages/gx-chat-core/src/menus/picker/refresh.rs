//! The model picker's Fetch latest models button, at the right end of its search row.
//!
//! `modelMenuRefresh {tab}` asks the session's computer to read that agent's model list from its
//! CLI again now (`/api/refreshAgentModelLineup`). The new list arrives the usual way, as gxserver's
//! `agentModelCatalogChanged`, so this module only keeps the button busy until the request ends; the
//! open tab, the search text and the highlighted row are view state it never touches.
//!
//! CDXC:AgentProviders 2026-11-20 DECISION:
//! User, after Gemini 4 Neon was missing from the Antigravity CLI tab: a refresh button at the right end of the picker's search row, "Fetch latest models", that re-fetches the current agent's model list, shows a short spinner and keeps the search text and the selection. Agents whose list is not read from their CLI keep the button, disabled, with a tooltip saying why.
//! SEE-ALSO: server/src/agent_model_catalog.rs (`refresh_lineup_now`), server/src/agent_model_lineups.rs (`RUNTIME_AGENTS`, which [`FETCHED_PROVIDERS`] mirrors), apps/desktop/src/app/native_chat/option_menu/model_menu/render.rs (the button).

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::menus::picker::model_menu::ModelMenuTabId;
use crate::menus::picker::model_picker::ModelPickerProvider;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

/// The agents whose model list gxserver reads from their own CLI, which it can read again on
/// request (gxserver's `RUNTIME_AGENTS`).
pub const FETCHED_PROVIDERS: [ModelPickerProvider; 6] = [
    ModelPickerProvider::Antigravity,
    ModelPickerProvider::Claude,
    ModelPickerProvider::Codex,
    ModelPickerProvider::Cursor,
    ModelPickerProvider::Grok,
    ModelPickerProvider::OpenCode,
];

pub const REFRESH_TOOLTIP: &str = "Fetch latest models";
pub const REFRESH_BUSY_TOOLTIP: &str = "Fetching the latest models…";
pub const REFRESH_FIXED_TOOLTIP: &str =
    "This agent's models can't be fetched from here: Ghostex keeps its list up to date on its own";

fn fetched(provider: ModelPickerProvider) -> bool {
    FETCHED_PROVIDERS.contains(&provider)
}

/// `modelMenuRefresh {tab}`: reads the tab's agent's list again, unless a read is already running.
pub fn refresh(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let tab = action
        .param("tab")
        .and_then(Value::as_str)
        .map(ModelMenuTabId::from_wire)
        .or(state.pickers.model_menu_view.tab);
    let Some(ModelMenuTabId::Provider(provider)) = tab else {
        return Vec::new();
    };
    if !fetched(provider) || state.pickers.model_menu_refresh.is_some() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.pickers.model_menu_refresh = Some(request_id);
    state.pickers.model_menu_view.refreshing = Some(provider);
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::RefreshAgentModelLineup,
        params: Box::new(json!({ "agent": provider.as_str() })),
    }]
}

/// The read ended, whatever its answer: a changed list has already arrived as a catalog push, and
/// an older gxserver without the endpoint simply keeps the list it has.
pub fn settle_rpc(state: &mut ChatState, request_id: u64) -> bool {
    if state.pickers.model_menu_refresh != Some(request_id) {
        return false;
    }
    state.pickers.model_menu_refresh = None;
    state.pickers.model_menu_view.refreshing = None;
    state.core.request_publish();
    true
}

/// The button for the open tab: `null` on the favorites tab, which spans every agent.
pub fn projection(tab: ModelMenuTabId, refreshing: Option<ModelPickerProvider>) -> Value {
    let ModelMenuTabId::Provider(provider) = tab else {
        return Value::Null;
    };
    let available = fetched(provider);
    let busy = available && refreshing == Some(provider);
    json!({
        "available": available,
        "busy": busy,
        "tooltip": if !available {
            REFRESH_FIXED_TOOLTIP
        } else if busy {
            REFRESH_BUSY_TOOLTIP
        } else {
            REFRESH_TOOLTIP
        },
    })
}
