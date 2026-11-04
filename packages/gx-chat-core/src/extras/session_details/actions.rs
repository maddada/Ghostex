//! The Session details panel's clicks.
//!
//! - `sessionDetailsVisible {visible}`: the renderer put the panel on screen or took it away.
//! - `sessionDetailsRefresh`: the Context section's refresh button.
//! - `sessionDetailsSection {section}`: a section's header folds or unfolds it.
//! - `sessionDetailsList {list}`: a list inside a section unfolds (`finished`, `skills`, `mcp`,
//!   `messages`); `{list: "tasks", step}` sets how far the Tasks card's done list is shown.
//! - `sessionDetailsStop {id}`: a background command's stop button.
//! - `sessionDetailsOpen {target, …}`: a row that opens something elsewhere in the app.

use serde_json::{json, Value};

use crate::action::{ActionKind, UserAction};
use crate::effect::{Effect, OpenTarget};
use crate::extras::session_details::settle;
use crate::extras::session_details::state::collapsed_key;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

fn text<'a>(action: &'a UserAction, name: &str) -> &'a str {
    action
        .param(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn toggle(list: &mut Vec<String>, id: &str) {
    if let Some(at) = list.iter().position(|known| known == id) {
        list.remove(at);
    } else {
        list.push(id.to_string());
    }
}

/// Handles one of the panel's actions.
pub fn handle(
    state: &mut ChatState,
    action: &UserAction,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<Effect> {
    match action.kind {
        ActionKind::SessionDetailsVisible => settle::set_visible(
            state,
            action.param("visible") == Some(&Value::Bool(true)),
            next_request_id,
        ),
        ActionKind::SessionDetailsRefresh => settle::refresh(state, next_request_id),
        ActionKind::SessionDetailsSection => {
            let section = text(action, "section");
            if section.is_empty() {
                return Vec::new();
            }
            let collapsed = &mut state.extras.session_details.collapsed;
            toggle(collapsed, section);
            let value = (!collapsed.is_empty()).then(|| collapsed.join(","));
            state.core.request_publish();
            vec![Effect::WriteStorage {
                key: collapsed_key(),
                value,
                durable: false,
            }]
        }
        ActionKind::SessionDetailsList => {
            let list = text(action, "list");
            if list == "tasks" {
                // The Tasks card's "Show more": `step` presses so far (0 is the short form).
                let step = action.param("step").and_then(Value::as_u64).unwrap_or(0);
                state.extras.session_details.tasks_step = step.min(u32::MAX as u64) as u32;
                state.core.request_publish();
            } else if !list.is_empty() {
                toggle(&mut state.extras.session_details.open_lists, list);
                state.core.request_publish();
            }
            Vec::new()
        }
        ActionKind::SessionDetailsStop => {
            let id = text(action, "id").to_string();
            let details = &mut state.extras.session_details;
            if id.is_empty() || details.stopping.contains(&id) {
                return Vec::new();
            }
            details.stopping.push(id.clone());
            let request_id = next_request_id();
            details.stops.push((request_id, id.clone()));
            state.core.request_publish();
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::StopSessionBackgroundTask,
                params: Box::new(json!({ "taskId": id })),
            }]
        }
        ActionKind::SessionDetailsOpen => open(action),
        _ => Vec::new(),
    }
}

/// A row that opens something: a skill in Agents Hub, a memory file, Settings > Connectors, the
/// changes in the Code view, the Git menu's Commit or Create PR, or the branch's pull request.
fn open(action: &UserAction) -> Vec<Effect> {
    let host = |name: &str, params: Value| {
        vec![Effect::HostAction {
            action: name.to_string(),
            params: Box::new(params),
        }]
    };
    match text(action, "target") {
        "skill" => {
            let name = text(action, "name");
            if name.is_empty() {
                return Vec::new();
            }
            host(
                "openAgentsHubSkill",
                json!({ "name": name, "path": text(action, "path") }),
            )
        }
        "file" => {
            let path = text(action, "path");
            if path.is_empty() {
                return Vec::new();
            }
            vec![Effect::Open(OpenTarget::File {
                path: path.to_string(),
                line: None,
                column: None,
            })]
        }
        "url" => {
            let url = text(action, "url");
            if !url.starts_with("https://") {
                return Vec::new();
            }
            vec![Effect::Open(OpenTarget::Url {
                url: url.to_string(),
                external: false,
            })]
        }
        "connectors" => host("openSettingsPage", json!({ "page": "connectors" })),
        "changes" => host("reviewChanges", json!({ "paths": [] })),
        "commit" => host("gitAction", json!({ "action": "commit" })),
        "createPullRequest" => host("gitAction", json!({ "action": "createPullRequest" })),
        _ => Vec::new(),
    }
}
