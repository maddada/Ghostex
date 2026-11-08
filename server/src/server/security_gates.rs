//! Where gxserver asks for the owner's signed approval (GitHub #495), before any route handler
//! runs: route capabilities (`security::route_access`) and the sensitive actions this thread owns.
//!
//! | Route | Asks when |
//! | --- | --- |
//! | `/api/updateTailcatState` | Easy Connect goes on, opens a new port, lets new devices in (or empties its allow-list), changes its relay |
//! | `/api/registerConnectedComputer`, `/api/issueComputerGrant` | a computer not approved in the last 30 days (Agent access off never asks) |
//! | `/api/autopilot` `op: "set"` | `answerApprovals` goes on, `limits` change |
//! | `/api/gbots` `action: "create"` | always (a new bot) |
//! | `/api/dispatchRendererCommand` `updateSettingsPatch` | a security-relaxing setting changes, or GBots goes on |
//! | `/api/answerSessionChatPrompt` | anyone but the owner's devices answers another session's prompt |
//! | `/api/interruptSessionChat` | anyone but the owner's devices stops a session that is neither their own nor one of their threads |
//! | `/api/sendSessionText`, `/api/sendSessionEnter`, `/api/sendSessionMessage` | anyone but the owner's devices types into a session that isn't their own, its threads included (a coordinator's Escape or Ctrl-C to its own thread doesn't ask) |
//!
//! Pairing and Collab routes are gated by their own threads with the same
//! `require_owner_presence`.

use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::rpc_error,
    security::{self, ActionRequest, Capability, Principal, RouteAccess, SensitiveAction},
};

use super::{require_owner_presence, routed_json, AppState, RoutedResponse};

/// `Some(response)` when the call must stop here: refused, or waiting for an approval.
pub(crate) fn gate(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
) -> Option<RoutedResponse> {
    // Guests and extensions keep their own endpoint allow-lists, checked at the auth gate; agents
    // on a connected computer keep theirs too, but answering a prompt or typing into a session
    // here asks like it does for an agent here.
    let peer = matches!(caller, Principal::Peer { .. });
    if !peer
        && !matches!(
            caller,
            Principal::Owner { .. }
                | Principal::Device { .. }
                | Principal::Session { .. }
                | Principal::LocalAgent
        )
    {
        return None;
    }
    if !peer {
        if let RouteAccess::Needs(capability) = security::route_access(endpoint_path) {
            if !caller.has(capability) {
                return Some(forbidden(
                    endpoint_path,
                    request_id,
                    "Only your own computer or phone can do this.",
                ));
            }
        }
    }
    let params = read_domain_rpc_params(body).ok()?;
    if let Some(message) = coordinator_refusal(caller, endpoint_path, &params) {
        return Some(forbidden(endpoint_path, request_id, message));
    }
    let computer = computer_gate(state, endpoint_path, &params);
    let action = match &computer {
        Some((_, action)) => Some(action.clone()),
        None => sensitive_action(state, caller, endpoint_path, &params),
    }?;
    match require_owner_presence(state, caller, action, body, endpoint_path, request_id) {
        Ok(grant) => {
            if let Some((server_id, _)) = computer {
                state
                    .security
                    .approve_computer(&server_id, security::now_ms());
            }
            // These routes' handlers keep no security log entry of their own.
            if matches!(
                grant.approval.action,
                SensitiveAction::AnswerOtherSessionPrompt
                    | SensitiveAction::TypeIntoOtherSession
                    | SensitiveAction::InterruptOtherSession
                    | SensitiveAction::SettingsWeakenSecurity
                    | SensitiveAction::GbotsEnable
            ) {
                crate::audit::record(
                    crate::audit::Action::from_approval(&grant.approval),
                    crate::audit::Actor::from(caller),
                    crate::audit::Outcome::Approved,
                    Some(crate::audit::ApprovalRef::from(&grant.approval)),
                );
            }
            None
        }
        Err(response) => Some(response),
    }
}

fn forbidden(endpoint_path: &str, request_id: &str, message: &str) -> RoutedResponse {
    routed_json(
        Some(endpoint_path.to_string()),
        StatusCode::FORBIDDEN,
        rpc_error("forbidden", message, Some(request_id.to_string())),
    )
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A session may link threads to, edit, or promote only its own coordinator session
/// ("coordinators can manage their own threads"); the token file and the owner's devices may
/// manage any.
fn coordinator_refusal(
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<&'static str> {
    let Principal::Session {
        project_id,
        session_id,
        ..
    } = caller
    else {
        return None;
    };
    let (project_key, session_key) = match endpoint_path {
        "/api/linkCoordinatorThread" => ("coordinatorProjectId", "coordinatorSessionId"),
        "/api/updateCoordinator" | "/api/setCoordinatorPlaybook" | "/api/promoteCoordinator" => {
            ("projectId", "sessionId")
        }
        _ => return None,
    };
    let own = text(params, project_key) == Some(project_id.as_str())
        && text(params, session_key) == Some(session_id.as_str());
    (!own).then_some("A session can only manage its own threads and its own coordinator.")
}

/// The approval a connected-computer route needs, with the computer's server id; `None` when the
/// computer was approved in the last 30 days or the call only closes a door.
fn computer_gate(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<(String, ActionRequest)> {
    let action = match endpoint_path {
        "/api/registerConnectedComputer" => SensitiveAction::ComputerRegister,
        "/api/issueComputerGrant" => {
            if params.get("agentAccess").and_then(Value::as_bool) == Some(false) {
                return None;
            }
            SensitiveAction::ComputerGrant
        }
        _ => return None,
    };
    let server_id = text(params, "serverId")?.to_string();
    if state
        .security
        .computer_approved(&server_id, security::now_ms())
    {
        return None;
    }
    let name = text(params, "name").unwrap_or(&server_id).to_string();
    let request = ActionRequest::new(
        action,
        format!("Let the agents on {name} and this computer reach each other"),
    )
    .target(server_id.clone())
    .params(json!({ "serverId": server_id }))
    .detail("Approved computers stay connected for 30 days.");
    Some((server_id, request))
}

fn sensitive_action(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    match endpoint_path {
        "/api/updateTailcatState" => easy_connect_action(state, params),
        "/api/autopilot" => autopilot_action(params),
        "/api/gbots" if text(params, "action") == Some("create") => {
            let name = text(params, "name").unwrap_or("a new bot");
            Some(
                ActionRequest::new(SensitiveAction::GbotsAddBot, format!("Add the bot {name}"))
                    .params(Value::Object(params.clone())),
            )
        }
        "/api/dispatchRendererCommand" if text(params, "action") == Some("updateSettingsPatch") => {
            settings_action(params)
        }
        "/api/answerSessionChatPrompt" => prompt_action(caller, params),
        "/api/sendSessionText" | "/api/sendSessionEnter" | "/api/sendSessionMessage" => {
            input_action(state, caller, endpoint_path, params)
        }
        "/api/interruptSessionChat" => interrupt_action(state, caller, params),
        _ => match security::route_access(endpoint_path) {
            RouteAccess::Presence(action) => Some(
                ActionRequest::new(
                    action,
                    endpoint_path.trim_start_matches("/api/").to_string(),
                )
                .params(Value::Object(params.clone())),
            ),
            _ => None,
        },
    }
}

fn easy_connect_action(state: &AppState, params: &Map<String, Value>) -> Option<ActionRequest> {
    use crate::tailcat::{read_tailcat_state_or_default, TailcatRepository, TailcatStateUpdate};
    let update: TailcatStateUpdate = serde_json::from_value(Value::Object(params.clone())).ok()?;
    let current = crate::storage::open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| read_tailcat_state_or_default(&TailcatRepository::new(&db)).ok());
    let exact = || Value::Object(params.clone());
    match &update {
        TailcatStateUpdate::SetEnabled { enabled: false } => None,
        TailcatStateUpdate::SetEnabled { enabled: true } => Some(
            ActionRequest::new(SensitiveAction::EasyConnectEnable, "Turn on Easy Connect")
                .params(exact()),
        ),
        TailcatStateUpdate::SetPorts { ports } => {
            let open: HashSet<u16> = current
                .as_ref()
                .map(|state| state.ports.iter().copied().collect())
                .unwrap_or_default();
            let mut added: Vec<u16> = ports
                .iter()
                .copied()
                .filter(|port| !open.contains(port))
                .collect();
            added.sort_unstable();
            added.dedup();
            (!added.is_empty()).then(|| {
                let list = added
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                ActionRequest::new(
                    SensitiveAction::EasyConnectPorts,
                    format!("Open port {list} to Easy Connect"),
                )
                .params(exact())
            })
        }
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        } => {
            let known: HashSet<&str> = current
                .as_ref()
                .map(|state| {
                    state
                        .allowed_client_keys
                        .iter()
                        .map(String::as_str)
                        .collect()
                })
                .unwrap_or_default();
            let added = allowed_client_keys
                .iter()
                .filter(|key| !known.contains(key.trim()))
                .count();
            // An empty allow-list lets any device in, so emptying one widens it.
            let opened = allowed_client_keys.is_empty() && !known.is_empty();
            if opened {
                return Some(
                    ActionRequest::new(
                        SensitiveAction::EasyConnectAllowList,
                        "Let any device connect through Easy Connect",
                    )
                    .params(exact()),
                );
            }
            (added > 0).then(|| {
                ActionRequest::new(
                    SensitiveAction::EasyConnectAllowList,
                    if added == 1 {
                        "Let one more device connect through Easy Connect".to_string()
                    } else {
                        format!("Let {added} more devices connect through Easy Connect")
                    },
                )
                .params(exact())
            })
        }
        TailcatStateUpdate::SetRelay { mode, custom_host } => {
            let where_to = if custom_host.trim().is_empty() {
                format!("{mode:?}").to_lowercase()
            } else {
                custom_host.trim().to_string()
            };
            Some(
                ActionRequest::new(
                    SensitiveAction::EasyConnectRelay,
                    format!("Send Easy Connect through the relay {where_to}"),
                )
                .params(exact()),
            )
        }
    }
}

fn autopilot_action(params: &Map<String, Value>) -> Option<ActionRequest> {
    if text(params, "op") != Some("set") {
        return None;
    }
    let patch = params.get("patch")?.as_object()?;
    if patch.get("answerApprovals").and_then(Value::as_bool) == Some(true) {
        return Some(
            ActionRequest::new(
                SensitiveAction::AutopilotAnswerApprovals,
                "Let Autopilot answer agents' permission prompts",
            )
            .params(Value::Object(params.clone())),
        );
    }
    patch.contains_key("limits").then(|| {
        ActionRequest::new(
            SensitiveAction::AutopilotLiftLimits,
            "Change Autopilot's hard limits",
        )
        .params(Value::Object(params.clone()))
    })
}

/// Whether changing setting `key` to `value` needs the owner's approval (the rule the desktop's
/// Settings window shares, `ghostex_settings_catalog::security_relaxing`).
pub(crate) fn setting_needs_approval(key: &str, value: &Value) -> bool {
    ghostex_settings_catalog::security_relaxing::needs_owner_presence(key, value)
}

fn holds_secret(key: &str) -> bool {
    ghostex_settings_catalog::security_relaxing::holds_secret(key)
}

fn settings_action(params: &Map<String, Value>) -> Option<ActionRequest> {
    let patch = params.get("payload")?.get("patch")?.as_object()?;
    // A setting the API may not write at all is refused by the route (settings_agent_writable.rs):
    // asking the owner first would only spend an approval on a refusal.
    if !crate::settings_agent_writable::refused_keys(patch).is_empty() {
        return None;
    }
    let gated: Map<String, Value> = patch
        .iter()
        .filter(|(key, value)| setting_needs_approval(key, value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if gated.is_empty() {
        return None;
    }
    let mut keys: Vec<&String> = gated.keys().collect();
    keys.sort();
    let listed = keys
        .iter()
        .map(|key| key.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let detail = keys
        .iter()
        .map(|key| {
            if holds_secret(key) {
                format!("{key}: (hidden)")
            } else {
                format!("{key}: {}", gated[key.as_str()])
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    let gbots = gated.contains_key("gbotsEnabled") || gated.contains_key("gbotsComputer");
    let (action, summary) = if gbots && gated.len() <= 2 {
        (SensitiveAction::GbotsEnable, "Turn on GBots".to_string())
    } else {
        (
            SensitiveAction::SettingsWeakenSecurity,
            format!("Change {listed} in Settings"),
        )
    };
    Some(
        ActionRequest::new(action, summary)
            .target(listed)
            .params(json!({ "patch": gated }))
            .detail(detail),
    )
}

/// The owner's devices answer any prompt; a session answers its own; everyone else asks
/// (user decision 2 of #495, CDXC:Security in security/capability.rs).
fn prompt_action(caller: &Principal, params: &Map<String, Value>) -> Option<ActionRequest> {
    if caller.has(Capability::AnswerAnyPrompt) {
        return None;
    }
    let project_id = text(params, "projectId")?;
    let session_id = text(params, "sessionId")?;
    if let Principal::Session {
        project_id: own_project,
        session_id: own_session,
        ..
    } = caller
    {
        if own_project == project_id && own_session == session_id {
            return None;
        }
    }
    Some(
        ActionRequest::new(
            SensitiveAction::AnswerOtherSessionPrompt,
            format!("Answer a permission prompt in session {session_id}"),
        )
        .target(format!("{project_id}:{session_id}"))
        .params(Value::Object(params.clone())),
    )
}

/// Escape and Ctrl-C: what `interrupt-session-chat` and `ghostex send-key escape|ctrl-c` send.
fn is_interrupt(text: &str) -> bool {
    matches!(text, "\u{1b}" | "\u{3}")
}

/*
CDXC:Security 2026-11-08 DECISION:
User (#495 follow-up): raw terminal input (send-text / send-key) into ANY other session needs approval, own threads included, since otherwise a coordinator could answer its threads' prompts by typing (decision 2). Interrupting a session (the Escape / Ctrl-C that interrupt-session-chat sends) stays allowed for a coordinator's own threads, because coordinators heal stuck threads with it; chat messages through the chat queue (`ghostex agents send`) are not raw input and stay as they are.
*/
fn input_action(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    if caller.has(Capability::AnswerAnyPrompt) {
        return None;
    }
    let session_id = text(params, "sessionId")?;
    let project_id = text(params, "projectId").unwrap_or_default();
    if let Principal::Session {
        project_id: own_project,
        session_id: own_session,
        role,
    } = caller
    {
        if own_session == session_id && (project_id.is_empty() || own_project == project_id) {
            return None;
        }
        let typed = params
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if *role == security::SessionRole::Coordinator
            && endpoint_path == "/api/sendSessionText"
            && is_interrupt(typed)
            && is_own_thread(state, own_project, own_session, project_id, session_id)
        {
            return None;
        }
    }
    let shown = match endpoint_path {
        "/api/sendSessionEnter" => "Enter".to_string(),
        _ => {
            let typed = params
                .get("text")
                .or_else(|| params.get("message"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut preview: String = typed.chars().take(120).collect();
            if typed.chars().count() > 120 {
                preview.push('…');
            }
            format!("\"{}\"", preview.escape_debug())
        }
    };
    Some(
        ActionRequest::new(
            SensitiveAction::TypeIntoOtherSession,
            format!("Type into the terminal of session {session_id}"),
        )
        .target(format!("{project_id}:{session_id}"))
        .params(Value::Object(params.clone()))
        .detail(format!("Typed: {shown}")),
    )
}

/*
CDXC:Security 2026-11-08 DECISION:
User (#495 follow-up): limit interrupt-session-chat to a session's own threads (and the owner), consistent with typing into a session. A session stops itself and a coordinator stops its own threads without asking (coordinators heal stuck threads that way); anyone else asks.
*/
fn interrupt_action(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    if caller.has(Capability::AnswerAnyPrompt) {
        return None;
    }
    let session_id = text(params, "sessionId")?;
    let project_id = text(params, "projectId").unwrap_or_default();
    if let Principal::Session {
        project_id: own_project,
        session_id: own_session,
        role,
    } = caller
    {
        if own_session == session_id && (project_id.is_empty() || own_project == project_id) {
            return None;
        }
        if *role == security::SessionRole::Coordinator
            && is_own_thread(state, own_project, own_session, project_id, session_id)
        {
            return None;
        }
    }
    Some(
        ActionRequest::new(
            SensitiveAction::InterruptOtherSession,
            format!("Stop what session {session_id} is doing"),
        )
        .target(format!("{project_id}:{session_id}"))
        .params(Value::Object(params.clone())),
    )
}

/// Whether session `session_id` of `project_id` is a thread of coordinator `coordinator`.
fn is_own_thread(
    state: &AppState,
    coordinator_project: &str,
    coordinator_session: &str,
    project_id: &str,
    session_id: &str,
) -> bool {
    if project_id.is_empty() {
        return false;
    }
    crate::storage::open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| crate::coordinators::read_thread(&db, project_id, session_id).ok())
        .flatten()
        .is_some_and(|thread| {
            thread.coordinator_key()
                == (
                    coordinator_project.to_string(),
                    coordinator_session.to_string(),
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_escape_and_ctrl_c_count_as_an_interrupt() {
        assert!(is_interrupt("\u{1b}"));
        assert!(is_interrupt("\u{3}"));
        assert!(!is_interrupt("1"));
        assert!(!is_interrupt("\u{1b}1"));
    }

    #[test]
    fn security_relaxing_settings_are_found_in_the_catalog() {
        assert!(setting_needs_approval("easyConnectEnabled", &json!(true)));
        assert!(setting_needs_approval("syncCoordinator", &json!(true)));
        assert!(setting_needs_approval("projectLinearApiKey", &json!("x")));
        assert!(setting_needs_approval("gbotsEnabled", &json!(true)));
        assert!(!setting_needs_approval("gbotsEnabled", &json!(false)));
        assert!(!setting_needs_approval("terminalFontSize", &json!(14)));
    }

    #[test]
    fn a_session_manages_only_its_own_coordinator() {
        let coordinator = Principal::Session {
            project_id: "P1".into(),
            session_id: "G1".into(),
            role: security::SessionRole::Coordinator,
        };
        let own = json!({ "coordinatorProjectId": "P1", "coordinatorSessionId": "G1" });
        let other = json!({ "coordinatorProjectId": "P1", "coordinatorSessionId": "G2" });
        assert!(coordinator_refusal(
            &coordinator,
            "/api/linkCoordinatorThread",
            own.as_object().unwrap()
        )
        .is_none());
        assert!(coordinator_refusal(
            &coordinator,
            "/api/linkCoordinatorThread",
            other.as_object().unwrap()
        )
        .is_some());
        assert!(coordinator_refusal(
            &Principal::LocalAgent,
            "/api/linkCoordinatorThread",
            other.as_object().unwrap()
        )
        .is_none());
    }

    #[test]
    fn answering_another_sessions_prompt_asks_unless_the_owner_does() {
        let params = json!({ "projectId": "P1", "sessionId": "G2" });
        let params = params.as_object().unwrap();
        let own = Principal::Session {
            project_id: "P1".into(),
            session_id: "G2".into(),
            role: security::SessionRole::Agent,
        };
        let coordinator = Principal::Session {
            project_id: "P1".into(),
            session_id: "G1".into(),
            role: security::SessionRole::Coordinator,
        };
        assert!(prompt_action(&own, params).is_none());
        assert!(prompt_action(&coordinator, params).is_some());
        assert!(prompt_action(&Principal::LocalAgent, params).is_some());
        assert!(prompt_action(
            &Principal::Device {
                device_id: "phone".into()
            },
            params
        )
        .is_none());
    }
}
