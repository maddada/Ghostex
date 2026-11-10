//! Where gxserver asks or refuses before a route handler runs (GitHub #495, option B since 10 Nov,
//! CDXC:Security 2026-11-10 in security/policy.rs):
//!
//! - The capability a route needs (`security::route_access`): for an agent's session token only
//!   approving requests and secrets are out of reach; peers, guests and extensions keep their own
//!   allow-lists.
//! - A GBot API chat takes input only from you (`api_chat_refused`), checked before everything else.
//! - Secret references are read only for you (`owner_secret_refused`).
//! - Chief turned off, and coordinators managing only their own threads.
//! - What still asks here: `/api/updateTailcatState` turning Easy Connect on or letting a device in,
//!   when an agent's session token does it. Pairing and Collab ask in their own handlers. Steering
//!   other sessions never asks; Chief's is written to the security log as `allowed`.

use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::rpc_error,
    security::{self, ActionRequest, Principal, RouteAccess, SensitiveAction},
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
    // A GBot API chat takes input from the owner's devices only (`api_chat_refused`), whoever asks.
    if SESSION_INPUT_ROUTES.contains(&endpoint_path) {
        if let Ok(params) = read_domain_rpc_params(body) {
            if api_chat_refused(state, caller, endpoint_path, &params) {
                return Some(forbidden(
                    endpoint_path,
                    request_id,
                    "This chat takes messages from an app through the GBot API, so only you, from Ghostex on your computer or phone, answer what it waits on or send it anything.",
                ));
            }
        }
    }
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
                let message = match capability {
                    security::Capability::OwnerSecrets => OWNER_SECRETS_MESSAGE,
                    _ => "Only your own computer or phone can do this.",
                };
                return Some(forbidden(endpoint_path, request_id, message));
            }
        }
    }
    let params = read_domain_rpc_params(body).ok()?;
    if owner_secret_refused(caller, endpoint_path, &params) {
        return Some(forbidden(endpoint_path, request_id, OWNER_SECRETS_MESSAGE));
    }
    if chief_turned_off_for(state, caller, endpoint_path, &params) {
        return Some(routed_json(
            Some(endpoint_path.to_string()),
            StatusCode::CONFLICT,
            rpc_error(
                "turnedOff",
                crate::chief::chief_off_message(),
                Some(request_id.to_string()),
            ),
        ));
    }
    if let Some(message) = coordinator_refusal(caller, endpoint_path, &params) {
        return Some(forbidden(endpoint_path, request_id, message));
    }
    let action = sensitive_action(state, caller, endpoint_path, &params)?;
    // Only what lets someone new in asks (CDXC:Security 2026-11-10 in security/policy.rs).
    if state.security.skips_approval(caller, &action) {
        if caller.is_head() {
            record_allowed(caller, &action);
        }
        return None;
    }
    require_owner_presence(state, caller, action, body, endpoint_path, request_id).err()
}

/// Every route that types, sends, queues, answers or interrupts in a session.
const SESSION_INPUT_ROUTES: &[&str] = &[
    "/api/answerSessionChatPrompt",
    "/api/sendSessionText",
    "/api/sendSessionEnter",
    "/api/sendSessionMessage",
    "/api/sendSessionChatMessage",
    "/api/queueSessionChatPrompt",
    "/api/updateSessionChatQueuedPrompt",
    "/api/interruptSessionChat",
];

/*
CDXC:GBots 2026-11-09 WHY:
Plan piece 14: a GBot API chat runs on text from outside Ghostex, "with no head answering for it". So what it waits on (an approval, a question), and anything typed, sent, queued or interrupted in it, is the owner's alone: every input route (security review finding 3: the three first covered missed sending a message, queueing, editing the queue and interrupting) is refused for every caller but your own computer and phone, the chat itself aside, agents (Chief of Staff included), the token file, other computers, extensions and Collab guests alike. It is checked before every other gate. The agents' general freedom to answer and type into other sessions (CDXC:Security at `SecurityState::skips_approval`) stays for every other chat.
*/
fn api_chat_refused(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> bool {
    if !SESSION_INPUT_ROUTES.contains(&endpoint_path) {
        return false;
    }
    let Some(session_id) = text(params, "sessionId") else {
        return false;
    };
    let project_id = text(params, "projectId").unwrap_or_default();
    match caller {
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => false,
        Principal::Session {
            project_id: own_project,
            session_id: own_session,
            ..
        } if own_session == session_id && (project_id.is_empty() || own_project == project_id) => {
            false
        }
        _ => crate::gbots::is_api_chat_session(state, project_id, session_id),
    }
}

/// Whether this call messages Chief's session while Chief is off (chief/feature.rs). The settings
/// file is read only when the target is Chief's session.
fn chief_turned_off_for(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> bool {
    let target = text(params, "projectId").zip(text(params, "sessionId"));
    let chief = crate::chief::chief_key(&state.paths);
    let targets_chief = matches!((target, &chief), (Some((project, session)), Some((chief_project, chief_session)))
        if project == chief_project && session == chief_session);
    targets_chief
        && crate::chief::refuses_message(
            caller.is_owner_device(),
            endpoint_path,
            target,
            chief.as_ref(),
            crate::chief::chief_on(&state.paths),
        )
}

/// The security-log entry for what went through without an approval (Chief steering another
/// session, the agents' bigger moves the user stopped being asked about): kept in the log with the
/// quiet `allowed` outcome, which the bell leaves out (audit/notify.rs).
fn record_allowed(caller: &Principal, request: &ActionRequest) {
    let mut action = crate::audit::Action::new(request.action.as_str(), request.summary.clone());
    if let Some(target) = &request.target {
        action = action.target(target.clone());
    }
    crate::audit::record(
        action,
        match crate::audit::Actor::from(caller) {
            crate::audit::Actor::Session { session_id } if caller.is_head() => {
                crate::audit::Actor::Chief { session_id }
            }
            other => other,
        },
        crate::audit::Outcome::Allowed,
        None,
    );
}

/// Why a caller without `Capability::OwnerSecrets` is refused.
const OWNER_SECRETS_MESSAGE: &str =
    "Only you can give a secret or use your saved keys and 1Password, from Ghostex on your computer or phone.";

/*
CDXC:Secrets 2026-11-08 DECISION:
Secret requests are answered only by the owner (GitHub #495 model): only the owner's devices hold `Capability::OwnerSecrets`, so only they answer a secret request, have gxserver read a secret provider reference, or get stored keys back. `/api/answerSecretRequest` and `/api/checkSecretReference` need it in `route_access`; here, `/api/setVoiceAgentKey` needs it when the key is an `op://` reference, and `/api/cloudBoxes` `cloudKit` (the provider keys and box logins a paired phone keeps) always. Agents, the token file, connected computers, guests and extensions are refused.
SEE-ALSO: server/src/secret_requests/api.rs (`answer` checks the caller again before reading a reference), server/src/cloud_boxes/kit.rs.
*/
/// Whether this call would read a secret provider reference, or return stored keys, for a
/// caller that may not.
fn owner_secret_refused(
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> bool {
    let owner_only = match endpoint_path {
        "/api/setVoiceAgentKey" => {
            text(params, "apiKey").is_some_and(|key| key.starts_with("op://"))
        }
        "/api/cloudBoxes" => text(params, "action") == Some("cloudKit"),
        _ => false,
    };
    owner_only && !caller.has(security::Capability::OwnerSecrets)
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

/// The approval a call needs, or `None`. Under option B only what lets someone new in asks
/// (CDXC:Security 2026-11-10 in security/policy.rs): Easy Connect turning on or letting a device in.
/// The steering actions only name the change for Chief's `allowed` log entry; pairing and Collab ask
/// in their own handlers.
fn sensitive_action(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    match endpoint_path {
        "/api/updateTailcatState" => easy_connect_action(state, params),
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
        // Asked once and remembered (Q2101); a computer that already has paired devices on its
        // allow-list was let in by an earlier approval, which keeps counting.
        TailcatStateUpdate::SetEnabled { enabled: true } => {
            if current
                .as_ref()
                .is_some_and(|state| !state.allowed_client_keys.is_empty())
            {
                state
                    .security
                    .remember(security::EASY_CONNECT_ENABLE, security::now_ms());
                return None;
            }
            Some(
                ActionRequest::new(SensitiveAction::EasyConnectEnable, "Turn on Easy Connect")
                    .params(exact()),
            )
        }
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
            // An empty allow-list lets nobody in (8b3687891), so only a device added asks.
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

fn prompt_action(caller: &Principal, params: &Map<String, Value>) -> Option<ActionRequest> {
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
User (#495 follow-up): raw terminal input (send-text / send-key) into another session, own threads included, and interrupting a session that is neither the caller's own nor one of its threads, were asked about from 11.1.0; since the user's "please let agents manage other sessions like before" (CDXC:Security at `SecurityState::skips_approval`) nobody is asked, and these requests only name the change for Chief's security log entry.
*/
fn input_action(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
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
pub(super) fn is_own_thread(
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
