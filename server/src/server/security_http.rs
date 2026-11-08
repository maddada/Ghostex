//! The approval API (`crate::security`, GitHub #495) and the helper sensitive routes call.
//!
//! - `/api/listApprovalRequests` → `{ requests }`, pending, oldest first (owner's devices).
//! - `/api/readApprovalRequest` `{ id }` → `{ request }` (its requester or the owner's devices).
//! - `/api/approveApprovalRequest` `{ id, keyId, signature }` → `{ request }` (owner's devices).
//! - `/api/denyApprovalRequest` `{ id }` → `{ request }` (owner's devices).
//! - `/api/listApproverKeys` → `{ keys }`; `/api/removeApproverKey` `{ keyId }` (owner's devices).
//! - `/api/enrollApproverKey`: Phase 2.

use axum::http::StatusCode;
use ghostex_gx_protocol::approval::{
    ApproveApprovalParams, DenyApprovalParams, ReadApprovalParams, APPROVAL_DENIED_ERROR,
    APPROVAL_ID_PARAM, APPROVAL_REQUIRED_ERROR,
};
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    security::{self, ActionRequest, PresenceGrant, PresenceOutcome, Principal},
};

use super::{routed_json, AppState, RoutedResponse};

/// The gate a sensitive route calls before it changes anything:
///
/// ```ignore
/// let action = ActionRequest::new(SensitiveAction::EasyConnectEnable, "Turn on Easy Connect")
///     .params(Value::Object(params.clone()));
/// let grant = match require_owner_presence(&state, &caller, action, &body_json, &path, &request_id) {
///     Ok(grant) => grant,
///     Err(response) => return response,
/// };
/// // … make the change …
/// state.security.record_performed(grant.approval.action, grant.approval.target, &caller, true);
/// ```
///
/// The error response is HTTP 403 with `error: "approvalRequired"` (or `approvalDenied`, or
/// `forbidden` for a caller that may not ask) and the request under `approval`.
pub(crate) fn require_owner_presence(
    state: &AppState,
    caller: &Principal,
    action: ActionRequest,
    body: &Value,
    endpoint_path: &str,
    request_id: &str,
) -> Result<PresenceGrant, RoutedResponse> {
    presence_outcome_response(
        presence_outcome(state, caller, action, body),
        endpoint_path,
        request_id,
    )
}

/// The gate's decision without the HTTP answer, for a route that also needs the pending request
/// itself (Collab finishes an approved action on its own, `collab_shim/http.rs`). Answer with
/// [`presence_outcome_response`].
pub(crate) fn presence_outcome(
    state: &AppState,
    caller: &Principal,
    action: ActionRequest,
    body: &Value,
) -> PresenceOutcome {
    let presented = body
        .get("params")
        .and_then(|params| params.get(APPROVAL_ID_PARAM))
        .and_then(Value::as_str);
    // Phase 2 replaces the session id with its title and a device id with its name.
    let label = match caller {
        Principal::Session { session_id, .. } => session_id.clone(),
        Principal::Owner { device_id } | Principal::Device { device_id } => device_id.clone(),
        Principal::Peer { name, .. } => name.clone(),
        Principal::LocalAgent => "A terminal on this computer".to_string(),
        other => other.kind().to_string(),
    };
    let now = security::now_ms();
    let refused_action = crate::audit::Action::new(action.action.as_str(), action.summary.clone());
    let outcome = state
        .security
        .require_owner_presence(caller, &label, action, presented, now);
    // The security log (server/src/audit/): a new request is logged as pending, which also tells
    // the phone; a spent approval is named by the entry the route's handler records for this
    // request; a denial is logged where the owner says no (`/api/denyApprovalRequest`).
    match &outcome {
        PresenceOutcome::Pending(request) if request.created_at_ms == now => {
            crate::audit::record(
                crate::audit::Action::from_approval(request),
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::Pending,
                Some(crate::audit::ApprovalRef::from(request)),
            );
        }
        PresenceOutcome::Granted(grant) => {
            crate::audit::note_approval(crate::audit::ApprovalRef::from(&grant.approval));
        }
        PresenceOutcome::Refused { message } => {
            crate::audit::record(
                refused_action,
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::Refused(message.clone()),
                None,
            );
        }
        _ => {}
    }
    outcome
}

/// The grant, or the 403 answer (`approvalRequired`, `approvalDenied`, `forbidden`) for the rest.
pub(crate) fn presence_outcome_response(
    outcome: PresenceOutcome,
    endpoint_path: &str,
    request_id: &str,
) -> Result<PresenceGrant, RoutedResponse> {
    match outcome {
        PresenceOutcome::Granted(grant) => Ok(grant),
        PresenceOutcome::Pending(request) => Err(presence_response(
            endpoint_path,
            request_id,
            APPROVAL_REQUIRED_ERROR,
            format!(
                "Waiting for your approval on your computer or phone: {}. Approve it there, then repeat this call with {APPROVAL_ID_PARAM} \"{}\".",
                request.summary, request.id
            ),
            Some(&request),
        )),
        PresenceOutcome::Denied(request) => Err(presence_response(
            endpoint_path,
            request_id,
            APPROVAL_DENIED_ERROR,
            format!("You denied this: {}.", request.summary),
            Some(&request),
        )),
        PresenceOutcome::Refused { message } => Err(presence_response(
            endpoint_path,
            request_id,
            "forbidden",
            message,
            None,
        )),
    }
}

fn presence_response(
    endpoint_path: &str,
    request_id: &str,
    code: &str,
    message: String,
    request: Option<&security::ApprovalRequest>,
) -> RoutedResponse {
    let mut body = serde_json::to_value(rpc_error(code, message, Some(request_id.to_string())))
        .unwrap_or_else(|_| json!({}));
    if let (Some(request), Some(object)) = (request, body.as_object_mut()) {
        object.insert(
            "approval".to_string(),
            serde_json::to_value(request).unwrap_or(Value::Null),
        );
    }
    routed_json(Some(endpoint_path.to_string()), StatusCode::FORBIDDEN, body)
}

/// Answers the approval API's endpoints; `None` for any other path.
pub(crate) fn route_security_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
) -> Option<RoutedResponse> {
    if !security::SECURITY_ENDPOINTS.contains(&endpoint_path) {
        return None;
    }
    let reply = |status: StatusCode, result: Result<Value, (&str, String)>| match result {
        Ok(value) => routed_json(
            Some(endpoint_path.to_string()),
            status,
            rpc_success(request_id.to_string(), value),
        ),
        Err((code, message)) => routed_json(
            Some(endpoint_path.to_string()),
            match code {
                "forbidden" => StatusCode::FORBIDDEN,
                "notFound" => StatusCode::NOT_FOUND,
                "notImplemented" => StatusCode::NOT_IMPLEMENTED,
                _ => StatusCode::BAD_REQUEST,
            },
            rpc_error(code, message, Some(request_id.to_string())),
        ),
    };
    let needed = match security::route_access(endpoint_path) {
        security::RouteAccess::Needs(capability) => Some(capability),
        _ => None,
    };
    if let Some(capability) = needed {
        if !caller.has(capability) {
            return Some(reply(
                StatusCode::FORBIDDEN,
                Err((
                    "forbidden",
                    "Only your own computer or phone can do this.".to_string(),
                )),
            ));
        }
    }
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return Some(reply(
                StatusCode::BAD_REQUEST,
                Err(("badRequest", error.message)),
            ))
        }
    };
    if endpoint_path == security::APPROVER_KEYS_ENROLL_ENDPOINT
        || endpoint_path == security::OWNER_KEY_ENROLL_ENDPOINT
    {
        return Some(
            match enroll_key(state, caller, endpoint_path, request_id, body, params) {
                Ok(result) => reply(StatusCode::OK, Ok(result)),
                Err(EnrollRefusal::Reply(code, message)) => {
                    reply(StatusCode::OK, Err((code, message)))
                }
                Err(EnrollRefusal::Response(response)) => response,
            },
        );
    }
    let result = handle(state, caller, endpoint_path, params);
    Some(reply(StatusCode::OK, result))
}

enum EnrollRefusal {
    Reply(&'static str, String),
    Response(RoutedResponse),
}

/// `/api/enrollOwnerKey` and `/api/enrollApproverKey`. The first key of each kind is taken on
/// the owner's word (first owner key: the token file only; first approver key: the desktop's owner
/// session); every later key needs an approved `addApproverKey` signed by a key already there.
fn enroll_key(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
    params: Map<String, Value>,
) -> Result<Value, EnrollRefusal> {
    use ghostex_gx_protocol::approval::{EnrollKeyParams, SensitiveAction};
    let security = &state.security;
    let now = security::now_ms();
    let EnrollKeyParams { key } =
        parse(params.clone()).map_err(|(code, message)| EnrollRefusal::Reply(code, message))?;
    let key_id = security::key_id_of(&key)
        .map_err(|error| EnrollRefusal::Reply("badRequest", error.to_string()))?;
    let owner = endpoint_path == security::OWNER_KEY_ENROLL_ENDPOINT;
    let registry = if owner {
        &security.owner_keys
    } else {
        &security.approver_keys
    };
    if let Some(known) = registry.get(&key_id) {
        return Ok(json!({ "key": known }));
    }
    let first = registry.is_empty();
    /*
    CDXC:Security 2026-11-08 WHY:
    The first owner key is trusted on first use, so whoever enrolls it first becomes the owner of this gxserver: every later key, approval and owner session hangs off it. On a fresh gxserver (a headless box, a new install) a session's own token (an agent, a coordinator, a thread, Chief) could enroll one and take that role before the owner's app ever connected. Only the token file may enroll it: the path the owner's Ghostex app takes, on this computer (`owner_security::open_owner_session` with the local transport) or from another computer through its SSH tunnel, which carries this computer's token file. Same-user limit: the token file is `gxserver/auth/token`, mode 0600, so any program running as this OS user can read it and enroll first, an agent that reads the file itself included; this closes the session-token door, not that one.
    */
    if first && owner && !matches!(caller, Principal::LocalAgent) {
        return Err(EnrollRefusal::Reply(
            "forbidden",
            "The first owner key is added by the Ghostex app, on this computer or from your own computer over SSH."
                .to_string(),
        ));
    }
    if first && owner {
        let key = security
            .enroll_first_owner_key(key, now)
            .map_err(|error| EnrollRefusal::Reply("badRequest", error.to_string()))?;
        // Trusted on first use (user decision 1A), so the log is where a surprise shows.
        crate::audit::record(
            crate::audit::Action::new(
                "ownerKey.add",
                format!("Made {} the owner of Ghostex on this computer", key.label),
            )
            .target(key.key_id.clone()),
            crate::audit::Actor::from(caller),
            crate::audit::Outcome::Done,
            None,
        );
        return Ok(json!({ "key": key }));
    }
    if first && !matches!(caller, Principal::Owner { .. }) {
        return Err(EnrollRefusal::Reply(
            "forbidden",
            "The first approver key is added by Ghostex on your computer.".to_string(),
        ));
    }
    if !first {
        let summary = format!(
            "Let {} ({}) {}",
            key.label,
            key.platform,
            if owner {
                "act as the owner of Ghostex on this computer"
            } else {
                "approve requests for this computer"
            }
        );
        let action = ActionRequest::new(SensitiveAction::AddApproverKey, summary)
            .target(key_id.clone())
            .params(Value::Object(params));
        require_owner_presence(state, caller, action, body, endpoint_path, request_id)
            .map_err(EnrollRefusal::Response)?;
    }
    let label = key.label.clone();
    let added = if owner {
        security.add_owner_key(key)
    } else {
        security.add_approver_key(key, now)
    }
    .map_err(|error| EnrollRefusal::Reply("badRequest", error.to_string()))?;
    crate::audit::record(
        crate::audit::Action::new(
            if owner {
                "ownerKey.add"
            } else {
                "approverKey.add"
            },
            if owner {
                format!("Made {label} an owner of Ghostex on this computer")
            } else {
                format!("Let {label} approve requests for this computer")
            },
        )
        .target(added.key_id.clone()),
        crate::audit::Actor::from(caller),
        crate::audit::Outcome::Done,
        None,
    );
    Ok(json!({ "key": added }))
}

fn parse<T: serde::de::DeserializeOwned>(
    params: Map<String, Value>,
) -> Result<T, (&'static str, String)> {
    serde_json::from_value(Value::Object(params))
        .map_err(|error| ("badRequest", format!("Invalid parameters: {error}")))
}

fn handle(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: Map<String, Value>,
) -> Result<Value, (&'static str, String)> {
    let security = &state.security;
    let now = security::now_ms();
    security.expire_lapsed(now);
    match endpoint_path {
        security::APPROVALS_LIST_ENDPOINT => {
            Ok(json!({ "requests": security.approvals.list_pending(now) }))
        }
        security::APPROVALS_READ_ENDPOINT => {
            let ReadApprovalParams { id } = parse(params)?;
            security
                .approvals
                .read(&id, caller, now)
                .map(|request| json!({ "request": request }))
                .ok_or(("notFound", "No such approval request.".to_string()))
        }
        security::APPROVALS_APPROVE_ENDPOINT => {
            let ApproveApprovalParams {
                id,
                key_id,
                signature,
            } = parse(params)?;
            let request = security
                .approvals
                .approve(
                    &id,
                    &key_id,
                    &signature,
                    &security.approver_keys,
                    &security.server_id,
                    now,
                )
                .map_err(|error| approval_error(&error))?;
            security.publish(security::SecurityEvent::ApprovalApproved {
                request: request.clone(),
                key_id,
            });
            Ok(json!({ "request": request }))
        }
        security::APPROVALS_DENY_ENDPOINT => {
            let DenyApprovalParams { id } = parse(params)?;
            let request = security
                .approvals
                .deny(&id, caller, now)
                .map_err(|error| approval_error(&error))?;
            security.publish(security::SecurityEvent::ApprovalDenied {
                request: request.clone(),
                by: caller.clone(),
            });
            crate::audit::record(
                crate::audit::Action::from_approval(&request),
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::Denied,
                Some(crate::audit::ApprovalRef::from(&request)),
            );
            Ok(json!({ "request": request }))
        }
        security::APPROVER_KEYS_LIST_ENDPOINT => {
            Ok(json!({ "keys": security.approver_keys.list() }))
        }
        security::APPROVER_KEYS_REMOVE_ENDPOINT => {
            let key_id = params
                .get("keyId")
                .and_then(Value::as_str)
                .ok_or(("badRequest", "keyId is required.".to_string()))?;
            let key = security
                .approver_keys
                .remove(key_id)
                .ok_or(("notFound", "No such approver key.".to_string()))?;
            security.save().map_err(|error| {
                (
                    "internalError",
                    format!("Ghostex could not save its keys: {error}"),
                )
            })?;
            security.publish(security::SecurityEvent::ApproverKeyRemoved {
                key: key.clone(),
                by: caller.clone(),
            });
            crate::audit::record(
                crate::audit::Action::new(
                    "approverKey.remove",
                    format!("Removed {} as an approver", key.label),
                )
                .target(key.key_id.clone()),
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::Done,
                None,
            );
            Ok(json!({ "key": key }))
        }
        security::OWNER_CHALLENGE_ENDPOINT => Ok(json!(security.owner_challenge(now))),
        security::OWNER_SESSION_ENDPOINT => {
            let ghostex_gx_protocol::approval::OpenOwnerSessionParams {
                key_id,
                challenge,
                signature,
            } = parse(params)?;
            security
                .open_owner_session(&key_id, &challenge, &signature, now)
                .map(|opened| json!(opened))
                .map_err(|error| ("forbidden", error.to_string()))
        }
        security::SECURITY_STATUS_ENDPOINT => Ok(json!({
            "serverId": &*security.server_id,
            "caller": caller,
            "ownerKeys": security.owner_keys.list(),
            "approverKeys": security.approver_keys.list(),
            "pendingApprovals": if caller.has(security::Capability::DecideApprovals) {
                security.approvals.list_pending(now).len()
            } else {
                0
            },
        })),
        _ => Err((
            "notFound",
            format!("{endpoint_path} is not an approval endpoint."),
        )),
    }
}

fn approval_error(error: &security::ApprovalError) -> (&'static str, String) {
    let code = match error {
        security::ApprovalError::NotFound => "notFound",
        security::ApprovalError::Key(security::ApproverKeyError::SignatureMismatch)
        | security::ApprovalError::Key(security::ApproverKeyError::UnknownKey) => "forbidden",
        _ => "badRequest",
    };
    (code, error.to_string())
}

/// The event `/api/events` carries whenever the approval requests change; the desktop and the
/// phone refresh their approval cards from it (`request` is the request that changed).
pub(crate) const APPROVALS_CHANGED_EVENT_TYPE: &str = "approvalRequestsChanged";

/// For gxserver's lifetime: forwards approval changes to `/api/events` and expires lapsed
/// requests every 15 seconds.
pub(crate) fn start_security_events(state: std::sync::Arc<AppState>) {
    let mut events = state.security.subscribe();
    let mut shutdown = state.shutdown_tx.subscribe();
    let weak = std::sync::Arc::downgrade(&state);
    drop(state);
    tokio::spawn(async move {
        let mut sweep = tokio::time::interval(std::time::Duration::from_secs(15));
        let mut ticks: u64 = 0;
        loop {
            tokio::select! {
                _ = shutdown.recv() => return,
                _ = sweep.tick() => {
                    let Some(state) = weak.upgrade() else { return };
                    state.security.expire_lapsed(security::now_ms());
                    // Every 5 minutes, and once at start: tokens of deleted sessions go.
                    if ticks % 20 == 0 {
                        let paths = state.paths.clone();
                        let security = state.security.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            if let Some(live) = live_sessions(&paths) {
                                security::revoke_tokens_of_deleted_sessions(&security, &live);
                            }
                            if let Some(open) = open_sessions(&paths) {
                                security::forget_reach_of_closed_sessions(&security, &open);
                            }
                        })
                        .await;
                    }
                    ticks += 1;
                }
                event = events.recv() => {
                    let event = match event {
                        Ok(event) => event,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => return,
                    };
                    let Some(state) = weak.upgrade() else { return };
                    let (change, request) = match &event {
                        security::SecurityEvent::ApprovalRequested { request } => ("requested", request),
                        security::SecurityEvent::ApprovalApproved { request, .. } => ("approved", request),
                        security::SecurityEvent::ApprovalDenied { request, .. } => ("denied", request),
                        security::SecurityEvent::ApprovalExpired { request } => ("expired", request),
                        _ => continue,
                    };
                    state.event_hub.broadcast(json!({
                        "type": APPROVALS_CHANGED_EVENT_TYPE,
                        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
                        "serverId": state.metadata.server_id.clone(),
                        "change": change,
                        "request": request,
                    }));
                }
            }
        }
    });
}

/// Every session's `(project id, session id)`, or `None` when the database can't be read (then
/// no token is revoked).
/// Every session that exists and isn't closed (stopped); a sleeping one counts as open.
fn open_sessions(
    paths: &crate::paths::GxserverPaths,
) -> Option<std::collections::HashSet<(String, String)>> {
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let mut statement = db
        .prepare("SELECT projectId, sessionId FROM sessions WHERE lifecycleState != 'stopped'")
        .ok()?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;
    rows.collect::<Result<_, _>>().ok()
}

fn live_sessions(
    paths: &crate::paths::GxserverPaths,
) -> Option<std::collections::HashSet<(String, String)>> {
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let mut statement = db
        .prepare("SELECT projectId, sessionId FROM sessions")
        .ok()?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;
    rows.collect::<Result<_, _>>().ok()
}
