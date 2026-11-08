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
// No route calls it until Phase 2 of #495 wires the sensitive routes (and the pairing and
// Collab threads wire theirs).
#[allow(dead_code)]
pub(crate) fn require_owner_presence(
    state: &AppState,
    caller: &Principal,
    action: ActionRequest,
    body: &Value,
    endpoint_path: &str,
    request_id: &str,
) -> Result<PresenceGrant, RoutedResponse> {
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
    match state.security.require_owner_presence(
        caller,
        &label,
        action,
        presented,
        security::now_ms(),
    ) {
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

#[allow(dead_code)]
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
    let result = handle(state, caller, endpoint_path, params);
    Some(reply(StatusCode::OK, result))
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
            security.publish(security::SecurityEvent::ApproverKeyRemoved {
                key: key.clone(),
                by: caller.clone(),
            });
            Ok(json!({ "key": key }))
        }
        security::APPROVER_KEYS_ENROLL_ENDPOINT => Err((
            "notImplemented",
            "Adding approver keys arrives with the desktop's Touch ID / Windows Hello / polkit approvals."
                .to_string(),
        )),
        _ => Err(("notFound", format!("{endpoint_path} is not an approval endpoint."))),
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
