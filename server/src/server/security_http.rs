//! The approval API (`crate::security`, GitHub #495) and the helper sensitive routes call.
//!
//! - `/api/listApprovalRequests` → `{ requests }`, pending, oldest first (owner's devices).
//! - `/api/readApprovalRequest` `{ id }` → `{ request, current? }` (its requester or the owner's
//!   devices); `current` is the newest request that replaced this one.
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
/// second approval nobody would see (CDXC:Security 2026-11-10 in security/policy.rs).
/// Older apps don't send it and keep today's two steps.
const APPROVER_KEY_PARAM: &str = "approverKey";

enum EnrollRefusal {
    Reply(&'static str, String),
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
            // A replaced request points at the newest one (`current`), so a card holding the
            // old id moves there (CDXC:Security 2026-11-10 in security/approvals.rs).
            let ReadApprovalParams { id } = parse(params)?;
            let Some(request) = security.approvals.read(&id, caller, now) else {
                log_read_when_changed(state, caller, &id, "notFound");
                return Err(("notFound", security::GONE_MESSAGE.to_string()));
            };
            let mut answer = json!({ "request": request });
            let current = security.approvals.replacement(&request, caller, now);
            log_read_when_changed(
                state,
                caller,
                &id,
                &match &current {
                    Some(current) => {
                        format!("{} (current {})", request.status.as_str(), current.id)
                    }
                    None => request.status.as_str().to_string(),
                },
            );
            if let Some(current) = current {
                answer["current"] = json!(current);
            }
            Ok(answer)
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
            // Asked once, kept for good (CDXC:Security 2026-11-10 in security/policy.rs).
            security.remember_approved(&request, now);
            // The request's outcome against its id, by the device that approved it.
            let mut logged =
                crate::audit::Action::new("approval.approved", request.summary.clone());
            if let Some(target) = &request.target {
                logged = logged.target(target.clone());
            }
            crate::audit::record(
                logged,
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::ApprovedBy(approved_on(caller)),
                Some(crate::audit::ApprovalRef::from(&request)),
            );
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
        security::ApprovalError::Expired(_) => "expired",
        security::ApprovalError::Replaced(_) => "replaced",
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
                    note_request_event(&state, &event);
                    let (change, request) = match &event {
                        security::SecurityEvent::ApprovalRequested { request } => ("requested", request),
                        security::SecurityEvent::ApprovalApproved { request, .. } => ("approved", request),
                        security::SecurityEvent::ApprovalDenied { request, .. } => ("denied", request),
                        security::SecurityEvent::ApprovalExpired { request }
                        | security::SecurityEvent::ApprovalWithdrawn { request, .. } => ("expired", request),
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

/*
CDXC:Security 2026-11-10 WHY:
Gregg's report (gregg-approval-bug, Analysis point 5): nothing recorded how an approval request ended, and gxserver.jsonl logged only failed calls, so a path that kept failing could not be read from either log. The user (task 3): the security log records each request's outcome against its id: approved (and on what kind of device), denied, expired, replaced by a newer request (named), and withdrawn with what asked; gxserver.jsonl keeps an info line for each request made, approved, denied, ended and spent, and for a read whose answer changed, with the request id, so the next report like his reads in minutes. Neither log carries a secret or key material: ids, actions, outcomes and device kinds only. A connected computer's once-a-minute renewal makes no request and so writes nothing here.
The request's own outcomes are logged as `approval.*` entries, which never ring the bell: the change an approval allowed notifies as before, and a request lapsing is not news. A request that lapses is announced whichever call found it lapsed (`ApprovalStore::expire`), so its card closes and its line is written.
*/
/// The security log line and the gxserver.jsonl line for how a request ended, and the
/// gxserver.jsonl line for a request made, approved, denied or spent.
fn note_request_event(state: &AppState, event: &security::SecurityEvent) {
    use security::SecurityEvent as E;
    let (name, request, extra) = match event {
        E::ApprovalRequested { request } => (
            "approvalRequested",
            request,
            json!({ "askedBy": request.requested_by.kind }),
        ),
        E::ApprovalApproved { request, .. } => ("approvalApproved", request, json!({})),
        E::ApprovalDenied { request, by } => {
            ("approvalDenied", request, json!({ "by": by.kind() }))
        }
        E::ApprovalExpired { request } => {
            let (kind, outcome) = match &request.replaced_by {
                Some(newer) => (
                    "approval.replaced",
                    crate::audit::Outcome::Replaced(format!(
                        "Replaced by the newer request {newer}"
                    )),
                ),
                None if request.decided_by.is_some() => (
                    "approval.expired",
                    crate::audit::Outcome::Expired(
                        "Approved, but the change wasn't made within 2 minutes".to_string(),
                    ),
                ),
                None => {
                    let minutes = request
                        .expires_at_ms
                        .saturating_sub(request.created_at_ms)
                        .div_ceil(60_000);
                    (
                        "approval.expired",
                        crate::audit::Outcome::Expired(format!(
                            "Nobody answered within {minutes} minute{}",
                            if minutes == 1 { "" } else { "s" }
                        )),
                    )
                }
            };
            record_request_outcome(request, kind, outcome);
            (
                "approvalEnded",
                request,
                json!({
                    "outcome": if request.replaced_by.is_some() { "replaced" } else { "expired" },
                    "replacedBy": request.replaced_by,
                }),
            )
        }
        E::ApprovalWithdrawn { request, why } => {
            record_request_outcome(
                request,
                "approval.withdrawn",
                crate::audit::Outcome::Withdrawn(format!("Withdrawn: {why}")),
            );
            (
                "approvalEnded",
                request,
                json!({ "outcome": "withdrawn", "why": why }),
            )
        }
        E::SensitiveActionAuthorized {
            action,
            approval_id,
            caller,
            ..
        } => {
            log_approval_line(
                state,
                "approvalSpent",
                json!({ "approvalId": approval_id, "action": action.as_str(), "caller": caller.kind() }),
            );
            return;
        }
        _ => return,
    };
    let mut details = json!({
        "approvalId": request.id,
        "action": request.action.as_str(),
        "status": request.status.as_str(),
    });
    if let (Some(details), Some(extra)) = (details.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            if !value.is_null() {
                details.insert(key.clone(), value.clone());
            }
        }
    }
    log_approval_line(state, name, details);
}

/// One security log line for how `request` ended, by gxserver itself, against its id.
fn record_request_outcome(
    request: &security::ApprovalRequest,
    kind: &str,
    outcome: crate::audit::Outcome,
) {
    let mut action = crate::audit::Action::new(kind, request.summary.clone());
    if let Some(target) = &request.target {
        action = action.target(target.clone());
    }
    crate::audit::record(
        action,
        crate::audit::Actor::Ghostex,
        outcome,
        Some(crate::audit::ApprovalRef::from(request)),
    );
}

/// An info line in gxserver.jsonl about an approval request: ids, actions, outcomes and kinds only.
fn log_approval_line(state: &AppState, event: &str, details: Value) {
    let _ = state.logger.log_record(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Info,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// A read of request `id` is logged when its answer changed since the last read logged (a
/// waiting client reads every two seconds; the first read and each change are what tell).
fn log_read_when_changed(state: &AppState, caller: &Principal, id: &str, answer: &str) {
    static LAST: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
        std::sync::Mutex::new(None);
    let changed = {
        let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let last = last.get_or_insert_with(Default::default);
        if last.len() > 2_000 {
            last.clear();
        }
        last.insert(id.to_string(), answer.to_string()).as_deref() != Some(answer)
    };
    if changed {
        log_approval_line(
            state,
            "approvalRead",
            json!({ "approvalId": id, "answer": answer, "caller": caller.kind() }),
        );
    }
}

/// How a click that approved is named in the security log: what kind of device it came from.
fn approved_on(caller: &Principal) -> String {
    match caller {
        Principal::Owner { .. } => "Approved in the Ghostex app (its owner session)".to_string(),
        Principal::Device { .. } => "Approved on a paired phone".to_string(),
        Principal::LocalAgent => {
            "Approved with this computer's token file (the Ghostex app, the web build or the CLI)"
                .to_string()
        }
        other => format!("Approved by a {}", other.kind()),
    }
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
