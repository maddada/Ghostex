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
        PresenceOutcome::Granted(grant) if !grant.by_policy => {
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
                "forbidden" | OWNER_APPROVAL_NEEDED => StatusCode::FORBIDDEN,
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

/*
CDXC:Security 2026-11-08 WHY:
GitHub #508 / Windows security log: "Let Mohamads-MacBook-Pro-2 (Ghostex app) act as the owner of
Ghostex on this computer" was asked 8 times in one day, never answered, because the Mac's app
enrolled its owner key on every connect (its owner session opener runs on the first call to a
connected computer), and each restart of either side made a new request. That owner session only
lets the owner's Stop, prompt answers and typing on this computer's sessions go without asking, so
it was asked for when one of those first needs it, never at connect. Since 2026-11-10 the user
chose to be asked once at connect (Q2099, desktop `owner_security::cached_token`), which needs only
that one request to be reused while it waits. Callers that only check (the app's quiet opener for
listing requests, `owner_security::quiet_owner_session`) send
`askOwner: false`, which enrolls a key that needs no approval and otherwise answers
`ownerApprovalNeeded` without making a request. A call without the field asks, as before.
*/
/// `/api/enrollOwnerKey`'s flag: `false` enrolls only when no approval is needed.
const ASK_OWNER_PARAM: &str = "askOwner";
/// The answer to `askOwner: false` when the key needs the owner's approval.
const OWNER_APPROVAL_NEEDED: &str = "ownerApprovalNeeded";

/// `/api/enrollOwnerKey`'s optional second key: the same app's approver key. Enrolled with the
/// first owner key while no approver key is, and with a later owner key on the one approval that
/// lets it in, so the app that may act as the owner here can also answer requests here without a
/// second approval nobody would see (CDXC:Security 2026-11-10 at `security::ApprovalEffect`).
/// Older apps don't send it and keep today's two steps.
const APPROVER_KEY_PARAM: &str = "approverKey";

/// How long another computer's request to act as the owner here waits for an answer.
const OWNER_KEY_REQUEST_TTL_MS: u64 = 24 * 60 * 60 * 1000;

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
    _request_id: &str,
    _body: &Value,
    params: Map<String, Value>,
) -> Result<Value, EnrollRefusal> {
    use ghostex_gx_protocol::approval::EnrollKeyParams;
    let security = &state.security;
    let now = security::now_ms();
    // Older apps send `askOwner` and `approvalId`; nothing asks any more, so both are ignored.
    let mut params = params;
    params.remove(ASK_OWNER_PARAM);
    params.remove(ghostex_gx_protocol::approval::APPROVAL_ID_PARAM);
    let owner = endpoint_path == security::OWNER_KEY_ENROLL_ENDPOINT;
    // The same app's approver key, sent with its owner key (see `APPROVER_KEY_PARAM`).
    let approver_key = match params.get(APPROVER_KEY_PARAM).filter(|_| owner) {
        Some(value) => Some(
            serde_json::from_value::<ghostex_gx_protocol::approval::ApproverKey>(value.clone())
                .map_err(|error| {
                    EnrollRefusal::Reply("badRequest", format!("Invalid approverKey: {error}"))
                })?,
        ),
        None => None,
    };
    let EnrollKeyParams { key } = parse(
        params
            .iter()
            .filter(|(name, _)| name.as_str() != APPROVER_KEY_PARAM)
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    )
    .map_err(|(code, message)| EnrollRefusal::Reply(code, message))?;
    /*
    CDXC:Security 2026-11-10 WHY:
    An owner key opens an owner session, which is one of your devices (it approves pairings and answers secret requests). Only the token file or an owner session may add one, as since 8 Nov: an agent's session token must not make itself yours. With approvals plain (security/policy.rs) nothing else is asked: a key from your own app goes in at once, on this computer and on another computer through its SSH tunnel, so the "act as the owner" card (#532) is gone; apps from before 11.2.3 still enroll and open their owner sessions as they always did.
    */
    if owner && !matches!(caller, Principal::LocalAgent | Principal::Owner { .. }) {
        return Err(EnrollRefusal::Reply(
            "forbidden",
            "An owner key is added by the Ghostex app, on this computer or from your own computer over SSH."
                .to_string(),
        ));
    }
    let registry = if owner {
        &security.owner_keys
    } else {
        &security.approver_keys
    };
    let key_id = security::key_id_of(&key)
        .map_err(|error| EnrollRefusal::Reply("badRequest", error.to_string()))?;
    // A known key changes nothing, and its `approverKey` is not taken: a public key proves nothing
    // about who sends it (gregg-thread-1110). Approvals are plain now, so approver keys only keep
    // older apps' lists whole.
    if let Some(known) = registry.get(&key_id) {
        return Ok(json!({ "key": known }));
    }
    if let Some(approver) = approver_key {
        enroll_approved_key(state, caller, false, approver, now);
    }
    enroll_approved_key(state, caller, owner, key, now);
    registry
        .get(&key_id)
        .map(|known| json!({ "key": known }))
        .ok_or_else(|| EnrollRefusal::Reply("badRequest", "The key could not be kept.".to_string()))
}

/// Adds an owner or approver key the owner approved (or that comes with the first owner key),
/// unless it is already there, and writes it to the security log.
fn enroll_approved_key(
    state: &AppState,
    caller: &Principal,
    owner: bool,
    mut key: ghostex_gx_protocol::approval::ApproverKey,
    now: u64,
) {
    let security = &state.security;
    let registry = if owner {
        &security.owner_keys
    } else {
        &security.approver_keys
    };
    if security::key_id_of(&key).is_ok_and(|key_id| registry.get(&key_id).is_some()) {
        return;
    }
    let label = key.label.clone();
    key.added_at_ms = now;
    let added = if owner {
        security.add_owner_key(key)
    } else {
        security.add_approver_key(key, now)
    };
    match added {
        Ok(added) => {
            let _ = crate::audit::record(
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
        }
        Err(error) => eprintln!("[gxserver] an approved key was not enrolled: {error}"),
    }
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
            // Plain: the click of one of your devices (the route needs `DecideApprovals`). An
            // older app's `keyId` and signature are accepted and not checked (CDXC:Security
            // 2026-11-10 in security/policy.rs).
            let ApproveApprovalParams { id, key_id, .. } = parse(params)?;
            let request = security
                .approvals
                .approve_plain(&id, caller, now)
                .map_err(|error| approval_error(&error))?;
            let key_id = if key_id.is_empty() {
                caller.kind().to_string()
            } else {
                key_id
            };
            /*
            CDXC:Security 2026-11-08 WHY:
            Windows repro of GitHub #508: the user entered the PIN for "Let the agents on SSH repro
            test and this computer reach each other" and the request stayed pending in the
            security log, because a computer counted as approved only once the call that waited
            on the request came back with it, and that call never ran again. Approving a
            computer's request now lets it in for the 30 days at once; the waiting call, when it
            runs, finds it approved.
            */
            if matches!(
                request.action,
                ghostex_gx_protocol::approval::SensitiveAction::ComputerGrant
                    | ghostex_gx_protocol::approval::SensitiveAction::ComputerRegister
            ) {
                if let Some(server_id) = request.target.as_deref() {
                    security.approve_computer(server_id, now);
                }
            }
            // Asked once, kept for good (CDXC:Security 2026-11-10 in security/policy.rs).
            security.remember_approved(&request, now);
            // A key the owner approved goes in now (CDXC:Security 2026-11-10 at
            // `security::ApprovalEffect`).
            if let Some(security::ApprovalEffect::EnrollKeys { owner, approver }) =
                security.approvals.effect_of(&id)
            {
                if let Some(key) = owner {
                    enroll_approved_key(state, caller, true, key, now);
                }
                if let Some(key) = approver {
                    enroll_approved_key(state, caller, false, key, now);
                }
            }
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
            // Approvals need no signature here; newer apps stop signing (older ones still may).
            "plainApprovals": true,
            // Settings > Security: the approving device checks Touch ID, Windows Hello, its
            // password or the phone's Face ID first (`approvalsRequirePresence`, off by default).
            "requirePresence": approvals_require_presence(state),
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

/// The Settings > Security switch, read from this computer's settings file.
fn approvals_require_presence(state: &AppState) -> bool {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| settings.get(APPROVALS_REQUIRE_PRESENCE)?.as_bool())
        .unwrap_or(false)
}

/// The settings key of "Require Touch ID, Windows Hello or your password to approve".
pub(crate) const APPROVALS_REQUIRE_PRESENCE: &str = "approvalsRequirePresence";

fn approval_error(error: &security::ApprovalError) -> (&'static str, String) {
    let code = match error {
        security::ApprovalError::NotFound => "notFound",
        security::ApprovalError::OwnRequest => "forbidden",
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
