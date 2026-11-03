//! The Run on row above a new thread's composer: where a DRAFT will run, this computer or an
//! agentbox box, picked before its first message.
//!
//! gxserver owns the switch and the facts (`/api/draftRunLocation`: whether the session is still a
//! draft, where it runs now, whether its agent can run in a box, and the box locations agentbox
//! reports ready on the session's machine). This module decides when the row shows, what it
//! offers and which chip is selected, and turns a pick into the call. Both renderers draw the
//! `runLocation` document key as it is.
//!
//! CDXC:AgentBox 2026-10-01 DECISION:
//! User: "also i should be able to pick another place to run when i start a new thread pls (Can be a picker above the current composer chat box". The row shows only on a draft (the agent switcher's `availableAgents` is the draft signal) whose agent a box can run (Claude, Codex, OpenCode, Pi), while agentbox reports at least one ready location on the session's machine; it offers This computer plus those locations with the New Thread picker's labels, starts on the draft's current location, and disappears with the first message, which starts the box.
//! SEE-ALSO: server/src/agents/draft_run_location.rs (the switch and the first send),
//! packages/gx-protocol/src/agentbox.rs (labels, icons and tooltips shared with gx-core),
//! apps/desktop/src/app/native_chat/run_location.rs, apps/mobile/app/src/chat/native/RunLocationRow.tsx.

use ghostex_gx_protocol::agentbox::{
    agentbox_location_icon, agentbox_location_tooltip, THIS_COMPUTER_LABEL,
};
use serde_json::{json, Map, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::event::Event;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The `runLocation` of this computer.
pub const LOCAL_RUN_LOCATION: &str = "local";
const ROW_LABEL: &str = "Run on";
/// The icon id of the This computer chip (the box kinds use [`agentbox_location_icon`]).
const ICON_THIS_COMPUTER: &str = "computer";

/// What the row remembers between events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunLocationState {
    /// The last `/api/draftRunLocation` answer.
    pub answer: Option<Value>,
    /// The session agent the answer was read for: an agent switch reads again, because the new
    /// agent may or may not run in a box.
    pub answered_agent: Option<Option<String>>,
    /// The read or switch in flight.
    pub request: Option<u64>,
    /// The location a switch in flight moves to, drawn selected until it answers.
    pub switching_to: Option<String>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_draft(state: &ChatState) -> bool {
    state.session.available_agents.is_some()
}

/// Reads the draft's location once per draft agent, and drops what a promoted session no longer
/// needs. Runs on every event.
fn observe(state: &mut ChatState) -> Vec<Effect> {
    if !is_draft(state) {
        if state.menus.run_location.answer.is_some() {
            state.menus.run_location = RunLocationState::default();
            state.core.request_render();
        }
        return Vec::new();
    }
    let agent = state.session.session_agent_id.clone();
    let row = &state.menus.run_location;
    if !state.core.controller_started
        || row.request.is_some()
        || row.answered_agent.as_ref() == Some(&agent)
    {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    let row = &mut state.menus.run_location;
    row.request = Some(request_id);
    row.answered_agent = Some(agent);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftRunLocation,
        params: Box::new(json!({})),
    }]
}

/// Folds an answer to the read or the switch.
fn settle_answer(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) {
    let row = &mut state.menus.run_location;
    if row.request != Some(request_id) {
        return;
    }
    row.request = None;
    row.switching_to = None;
    // A refused switch keeps the last answer (the composer's error line says why); an older
    // gxserver without the endpoint leaves no answer, so no row.
    if let RpcOutcome::Ok { result } = outcome {
        row.answer = Some(result.clone());
    }
    state.core.request_render();
}

/// Family e's settle hook for the row.
pub fn settle(state: &mut ChatState, event: &Event, _context: &ChatContext) -> Vec<Effect> {
    if let Event::RpcSettled {
        request_id,
        outcome,
    } = event
    {
        settle_answer(state, *request_id, outcome.as_ref());
    }
    observe(state)
}

/// `{"type": "switchDraftRunLocation", "runLocation": "local" | "agentbox:<provider>"}`.
pub fn switch(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let Some(target) = action
        .params
        .get("runLocation")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Vec::new();
    };
    let Some(row) = row_value(state) else {
        return Vec::new();
    };
    let offered = row
        .get("options")
        .and_then(Value::as_array)
        .is_some_and(|options| {
            options
                .iter()
                .any(|option| option.get("runLocation").and_then(Value::as_str) == Some(&target))
        });
    if !offered
        || row.get("busy").and_then(Value::as_bool) == Some(true)
        || row.get("selected").and_then(Value::as_str) == Some(&target)
    {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.menus.run_location.request = Some(request_id);
    state.menus.run_location.switching_to = Some(target.clone());
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::DraftRunLocation,
        params: Box::new(json!({ "runLocation": target })),
    }]
}

/// One chip.
fn option(run_location: &str, label: &str, icon: &str, tooltip: Option<String>) -> Value {
    json!({
        "runLocation": run_location,
        "label": label,
        "icon": icon,
        "tooltip": tooltip,
    })
}

/// Moves the draft to `target` (the Cloud Box picker's More providers rows and the tray's other
/// segments use it while the Run on row itself is hidden).
pub fn switch_to(state: &mut ChatState, target: &str) -> Vec<Effect> {
    let mut action = UserAction::new(crate::action::ActionKind::SwitchDraftRunLocation);
    action
        .params
        .insert("runLocation".to_string(), json!(target));
    switch(state, &action)
}

/// The row as the renderers draw it, or `None` when it is hidden: wherever the new chat's tray
/// offers Cloud Box, whose picker lists these places under More providers.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION:
/// User (D3 "into cloud box menu"): the Run on row goes away where the tray's Cloud Box segment is
/// offered; its agentbox places are rows of that segment's picker. A folder that is not a git
/// checkout has no Cloud Box segment, so it keeps the row. This narrows the AgentBox 2026-10-01
/// decision above.
pub fn row(state: &ChatState) -> Option<Value> {
    let tray_offers_cloud = state
        .menus
        .draft_worktree
        .answer
        .as_ref()
        .is_some_and(crate::menus::cloud_box::offered);
    if tray_offers_cloud {
        return None;
    }
    row_value(state)
}

/// The row's content whether or not it is shown.
///
/// `{label, selected, busy, options: [{runLocation, label, icon, tooltip}]}`: This computer first,
/// then every ready box location in gxserver's order, then the draft's current box when agentbox
/// no longer reports it ready (so the selected chip is always on the row).
fn row_value(state: &ChatState) -> Option<Value> {
    if !is_draft(state) {
        return None;
    }
    let answer = state.menus.run_location.answer.as_ref()?;
    if answer.get("draft").and_then(Value::as_bool) != Some(true)
        || answer.get("boxStarted").and_then(Value::as_bool) == Some(true)
        || text(answer, "boxAgent").is_none()
    {
        return None;
    }
    let locations = answer
        .pointer("/agentbox/locations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let current = text(answer, "runLocation").unwrap_or_else(|| LOCAL_RUN_LOCATION.to_string());
    if locations.is_empty() && current == LOCAL_RUN_LOCATION {
        return None;
    }
    let mut options = vec![option(
        LOCAL_RUN_LOCATION,
        THIS_COMPUTER_LABEL,
        ICON_THIS_COMPUTER,
        None,
    )];
    for location in &locations {
        let (Some(run_location), Some(provider)) =
            (text(location, "runLocation"), text(location, "provider"))
        else {
            continue;
        };
        let label = text(location, "label").unwrap_or_else(|| {
            provider
                .strip_prefix("docker:")
                .unwrap_or(&provider)
                .to_string()
        });
        let kind = text(location, "kind").unwrap_or_default();
        options.push(option(
            &run_location,
            &label,
            agentbox_location_icon(&kind),
            agentbox_location_tooltip(&provider),
        ));
    }
    let listed = |run_location: &str| {
        options
            .iter()
            .any(|option| option.get("runLocation").and_then(Value::as_str) == Some(run_location))
    };
    if !listed(&current) {
        if let Some(current_box) = answer.get("current").filter(|value| value.is_object()) {
            let provider = text(current_box, "provider").unwrap_or_default();
            let kind = if provider.starts_with("docker:") {
                "remoteDocker"
            } else if provider == "docker" {
                "local"
            } else {
                "cloud"
            };
            options.push(option(
                &current,
                &text(current_box, "label").unwrap_or_else(|| provider.clone()),
                agentbox_location_icon(kind),
                agentbox_location_tooltip(&provider),
            ));
        }
    }
    let row = &state.menus.run_location;
    let mut value = Map::new();
    value.insert("label".to_string(), json!(ROW_LABEL));
    value.insert(
        "selected".to_string(),
        json!(row.switching_to.clone().unwrap_or(current)),
    );
    value.insert("busy".to_string(), json!(row.switching_to.is_some()));
    value.insert("options".to_string(), Value::Array(options));
    Some(Value::Object(value))
}
