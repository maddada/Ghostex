//! Where gxserver asks for the owner's signed approval (GitHub #495), before any route handler
//! runs: route capabilities (`security::route_access`) and the sensitive actions this thread owns.
//!
//! | Route | Asks when |
//! | --- | --- |
//! | `/api/updateTailcatState` | Easy Connect goes on, opens a new port, lets new devices in (or empties its allow-list), changes its relay |
//! | `/api/registerConnectedComputer`, `/api/issueComputerGrant` | a computer not approved in the last 30 days, with Agent access on: one request per computer, whoever asks (registering a computer with Agent access off never asks) |
//! | `/api/autopilot` `op: "set"` | `answerApprovals` goes on, `limits` change |
//! | `/api/gbots` `op: "create"` | from your own computer, phone or terminal (Chief of Staff's adds don't ask; other agents are refused) |
//! | `/api/dispatchRendererCommand` `updateSettingsPatch` | a security-relaxing setting changes, or GBots goes on |
//! | `/api/cloudBoxes` `create`, `setAgentLogin` (allowed), `projectSettings` (write) | anyone but the owner's devices: logins and project secrets go onto a box |
//! | a create forcing full access | see security_full_access.rs |
//!
//! What only the owner's devices may do at all, with no approval to ask for, is refused before
//! these (`security_owner_only.rs`).
//!
//! Answering another session's prompt, typing into it and interrupting it
//! (`/api/answerSessionChatPrompt`, `/api/sendSessionText`, `/api/sendSessionEnter`,
//! `/api/sendSessionMessage`, `/api/interruptSessionChat`) never ask: every caller skips them
//! (`Principal::skips_approval`, security/capability.rs), and only Chief's (the head role, settled
//! per request by `crate::chief::settle_principal`) are still written to the security log.
//!
//! Pairing and Collab routes are gated by their own threads with the same
//! `require_owner_presence`.

use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::rpc_error,
    security::{self, ActionRequest, Principal, RouteAccess, SensitiveAction},
};

use super::{
    presence_outcome, presence_outcome_response, require_owner_presence, routed_json, AppState,
    RoutedResponse,
};

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
                let message = match capability {
                    security::Capability::OwnerSecrets => OWNER_SECRETS_MESSAGE,
                    security::Capability::OwnerSetup => {
                        super::security_owner_only::owner_setup_message(endpoint_path)
                    }
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
    if let Some(message) =
        super::security_owner_only::owner_only_refusal(state, caller, endpoint_path, &params)
    {
        return Some(forbidden(endpoint_path, request_id, message));
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
    let computer = computer_gate(state, endpoint_path, &params);
    let action = match &computer {
        Some((_, action)) => Some(action.clone()),
        None => sensitive_action(state, caller, endpoint_path, &params),
    }?;
    if computer.is_none() && caller.skips_approval(&action.action) {
        if caller.is_head() {
            record_head_steering(caller, &action);
        }
        return None;
    }
    let decided = match &computer {
        Some(_) => computer_presence(state, caller, action, body, endpoint_path, request_id),
        None => require_owner_presence(state, caller, action, body, endpoint_path, request_id),
    };
    match decided {
        Ok(grant) => {
            if let Some((server_id, _)) = computer {
                state
                    .security
                    .approve_computer(&server_id, security::now_ms());
            }
            // These routes' handlers keep no security log entry of their own.
            if matches!(
                grant.approval.action,
                SensitiveAction::SettingsWeakenSecurity
                    | SensitiveAction::GbotsEnable
                    | SensitiveAction::ForceFullAccess
                    | SensitiveAction::CloudBoxShareLogins
                    | SensitiveAction::TeamSyncConnect
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

/// The answer when a call meets a connected computer's request that was already waiting:
/// `approvalPending` with the request, so the caller waits on it.
pub(crate) const APPROVAL_PENDING_ERROR: &str = "approvalPending";

/*
CDXC:Security 2026-11-08 WHY:
The 11.1.0 app showed Windows Hello for every `approvalRequired` answer of its own calls, and its connected-computer renewal runs every minute, so a computer nobody had approved yet opened a new PIN prompt each minute (about 50 open by the time the user saw them). Only the call that makes a computer's request is told `approvalRequired`; a call that meets the request already waiting is told `approvalPending`, with the same request, which no app prompts for. That keeps an app still on 11.1.0, renewing against a computer already updated, to one prompt per request.
*/
/// [`require_owner_presence`] for the connected-computer routes, with a repeat answered
/// `approvalPending` (see the CDXC note above).
fn computer_presence(
    state: &AppState,
    caller: &Principal,
    action: ActionRequest,
    body: &Value,
    endpoint_path: &str,
    request_id: &str,
) -> Result<security::PresenceGrant, RoutedResponse> {
    let now = security::now_ms();
    match presence_outcome(state, caller, action, body) {
        security::PresenceOutcome::Pending(request) if request.created_at_ms < now => {
            let message = format!(
                "Still waiting for your approval on your computer or phone: {}.",
                request.summary
            );
            let mut answer = serde_json::to_value(rpc_error(
                APPROVAL_PENDING_ERROR,
                message,
                Some(request_id.to_string()),
            ))
            .unwrap_or_else(|_| json!({}));
            if let Some(object) = answer.as_object_mut() {
                object.insert(
                    "approval".to_string(),
                    serde_json::to_value(&request).unwrap_or(Value::Null),
                );
            }
            Err(routed_json(
                Some(endpoint_path.to_string()),
                StatusCode::FORBIDDEN,
                answer,
            ))
        }
        outcome => presence_outcome_response(outcome, endpoint_path, request_id),
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

/// The security-log entry for what Chief's head role let through without an approval: kept in the
/// log with the quiet `allowed` outcome, which the bell leaves out (audit/notify.rs).
fn record_head_steering(caller: &Principal, request: &ActionRequest) {
    let mut action = crate::audit::Action::new(request.action.as_str(), request.summary.clone());
    if let Some(target) = &request.target {
        action = action.target(target.clone());
    }
    crate::audit::record(
        action,
        match crate::audit::Actor::from(caller) {
            crate::audit::Actor::Session { session_id } => {
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

/// How long a connected computer's request waits for the owner.
const COMPUTER_REQUEST_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// The approval a connected-computer route needs, with the computer's server id; `None` when the
/// computer was approved in the last 30 days or the call only closes a door.
fn computer_gate(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<(String, ActionRequest)> {
    if !matches!(
        endpoint_path,
        "/api/registerConnectedComputer" | "/api/issueComputerGrant"
    ) {
        return None;
    }
    // Agent access off only closes a door: registering a computer with it off (so refusals can
    // name it) and its renewals never ask.
    if params.get("agentAccess").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let server_id = text(params, "serverId")?.to_string();
    // A computer is never asked about itself (a saved machine that points back at this one).
    if server_id.as_str() == &*state.security.server_id
        || state
            .security
            .computer_approved(&server_id, security::now_ms())
    {
        return None;
    }
    let name = text(params, "name").unwrap_or(&server_id).to_string();
    /*
    CDXC:Security 2026-11-08 WHY:
    User, on 11.1.0: "Man i see 1 million pin requests on my windows machine saying let the agents on x and x reach each other. Please don't do this!" Registering a computer and minting its grant are one question to the owner ("let these two computers' agents reach each other"), so both routes ask the same shared request: one per computer, reused by the owner session and the token file, by this app's renewals and the other computer's, and spent by whichever call comes first (approving it lets the computer in for 30 days, which passes the other route too). It waits a day for an answer instead of ten minutes, so a renewal that keeps meeting it never makes a new one while the owner is away.
    */
    let request = ActionRequest::new(
        SensitiveAction::ComputerGrant,
        format!("Let the agents on {name} and this computer reach each other"),
    )
    .target(server_id.clone())
    .params(json!({ "serverId": server_id }))
    .detail("Approved computers stay connected for 30 days.")
    .shared()
    .pending_ttl(COMPUTER_REQUEST_TTL_MS);
    Some((server_id, request))
}

/*
CDXC:GBots 2026-11-08 DECISION:
User (Q2103 A): Chief of Staff adds a GBot on its own and tells you in one card, so its adds ask nobody, unless you turned off "Chief of Staff may add GBots on its own" (`gbotsChiefAddsOnItsOwn`, Settings > Agents), when each of its adds asks you first. Your own computer, phone and terminal still ask before a bot is added (the previous GBots' rule, kept by M0). Other agents may not add bots at all (Q-D A): `/api/gbots` refuses them with one sentence, so they are not asked for here; neither is anyone while GBots are off or for a name `/api/gbots` refuses, which it answers instead.
*/
fn gbots_add_action(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    let user = matches!(
        caller,
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
    ) || (caller.is_head() && !crate::gbots::chief_adds_on_its_own(&state.paths));
    let name = text(params, "name").unwrap_or_default();
    // A name `/api/gbots` refuses anyway is never worth the user's approval.
    if !user || !crate::gbots::gbots_on(&state.paths) || crate::gbots::name_problem(name).is_some()
    {
        return None;
    }
    Some(
        ActionRequest::new(SensitiveAction::GbotsAddBot, format!("Add the GBot {name}"))
            .params(Value::Object(params.clone())),
    )
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
        "/api/gbots" if text(params, "op") == Some("create") => {
            gbots_add_action(state, caller, params)
        }
        "/api/dispatchRendererCommand" if text(params, "action") == Some("updateSettingsPatch") => {
            settings_action(params)
        }
        "/api/answerSessionChatPrompt" => prompt_action(caller, params),
        "/api/sendSessionText" | "/api/sendSessionEnter" | "/api/sendSessionMessage" => {
            input_action(state, caller, endpoint_path, params)
        }
        "/api/interruptSessionChat" => interrupt_action(state, caller, params),
        "/api/cloudBoxes" => cloud_box_action(state, caller, params),
        "/api/connectTeamSync" => team_connect_action(caller, params),
        path if super::security_full_access::is_full_access_route(path) => {
            super::security_full_access::full_access_action(state, caller, params)
        }
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

/*
CDXC:CloudBoxes 2026-11-08 DECISION:
#495 follow-up (the audit's leftovers, "under the same rule"): `/api/cloudBoxes` `create`, `setAgentLogin` with `allowed: true`, and `projectSettings` with `settings` put the owner's agent logins and a project's secret files onto a box, so an agent's call asks the owner with Touch ID. The owner's own devices don't ask (the Cloud Boxes page and the phone are the owner choosing). Turning a login off and reading settings never ask; `connectInfo` is left as it is (the user is asked about it separately).
*/
fn cloud_box_action(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    if caller.is_owner_device() {
        return None;
    }
    let project_name = |project_id: &str| {
        crate::storage::open_gxserver_database(&state.paths)
            .ok()
            .and_then(|db| {
                crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str())
                    .get_project(project_id)
                    .ok()
                    .flatten()
            })
            .and_then(|project| {
                project
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| project_id.to_string())
    };
    let (summary, target, detail) = match text(params, "action")? {
        "create" => {
            let project_id = text(params, "projectId").unwrap_or_default();
            (
                format!("Start a Cloud Box for {}", project_name(project_id)),
                project_id.to_string(),
                "The box gets the agent logins and project files you set up for Cloud Boxes.",
            )
        }
        "setAgentLogin" if params.get("allowed").and_then(Value::as_bool) == Some(true) => {
            let account = text(params, "accountId").unwrap_or_default();
            (
                format!("Let Cloud Boxes use the agent login {account}"),
                account.to_string(),
                "Boxes you start get a copy of this login.",
            )
        }
        "projectSettings" if params.get("settings").is_some_and(|value| !value.is_null()) => {
            let project_id = text(params, "projectId").unwrap_or_default();
            (
                format!(
                    "Change what Cloud Boxes get from {}",
                    project_name(project_id)
                ),
                project_id.to_string(),
                "Boxes for this project may get its secret files and setup.",
            )
        }
        _ => return None,
    };
    Some(
        ActionRequest::new(SensitiveAction::CloudBoxShareLogins, summary)
            .target(target)
            .params(Value::Object(params.clone()))
            .detail(detail),
    )
}

/*
CDXC:TeamSync 2026-11-08 DECISION:
#495 follow-up (Q1865, "settings only"): joining a team, invites, the team's Linear keys and its Slack flow are the owner's devices' alone (security_owner_only.rs); linking a Slack user stays open, since it needs the one-time code Slack sends only that user. Connecting with a token made elsewhere is `ghostex team deploy`'s last step, and Settings > Workspaces sends the owner to a terminal for it (deploying needs their Convex CLI login), so it asks with Touch ID instead of being refused; the card names the backend's address, so a connection to a backend the owner didn't deploy is visible before it happens.
*/
fn team_connect_action(caller: &Principal, params: &Map<String, Value>) -> Option<ActionRequest> {
    if caller.is_owner_device() {
        return None;
    }
    let url = text(params, "deploymentUrl").unwrap_or_default();
    let workspace = text(params, "workspaceId").unwrap_or_default();
    Some(
        ActionRequest::new(
            SensitiveAction::TeamSyncConnect,
            format!("Connect this computer to the team backend at {url}"),
        )
        .target(url.to_string())
        .params(Value::Object(params.clone()))
        .detail(format!(
            "Workspace {workspace}. Its team's Slack requests can then start sessions on this computer."
        )),
    )
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
    let (action, summary) = if gated.contains_key("gbotsEnabled") && gated.len() == 1 {
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

/// Answering another session's prompt: never asks (`Principal::skips_approval`); the request
/// only names it for Chief's security log entry.
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
User (#495 follow-up): raw terminal input (send-text / send-key) into another session, own threads included, and interrupting a session that is neither the caller's own nor one of its threads, were asked about from 11.1.0; since the user's "please let agents manage other sessions like before" (CDXC:Security at `Principal::skips_approval`) nobody is asked, and these requests only name the change for Chief's security log entry.
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
