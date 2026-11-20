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
        // Named by its computer: the card may show on another computer (the approval cards page).
        Principal::LocalAgent => format!(
            "A terminal on {}",
            crate::remote_access::cached_remote_access_identity()
                .map(|identity| identity.computer_name)
                .unwrap_or_else(|_| crate::connected_computers::this_computer_name())
        ),
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
/// `/api/enrollOwnerKey`'s one-time value from the app for the first key (review round 2, R2-3).
const FIRST_KEY_SECRET_PARAM: &str = "firstKeySecret";
/// How many "act as you" requests (`addApproverKey`) may wait at once before a new one pushes
/// out the oldest that isn't the app's (review rounds 2 and 3, R2-2 and R3-2).
const OWNER_KEY_REQUESTS_WAITING: usize = 3;
/// The answer when the token file made too many "act as you" requests this hour (R3-2).
pub(crate) const OWNER_KEY_FLOODED: &str = "ownerKeyRequestsFlooded";
use crate::security::OWNER_KEY_RESUME;

/*
CDXC:Security 2026-11-17 FIX:
Live, 17 Nov: the user approved "Let m7-arch (Ghostex app) show the pairing code of <the Mac> … (angle easel magnet monkey)" on the Mac at 03:39, and it lapsed at 03:41, "Approved, but the change wasn't made within 2 minutes": the key went in only when the asking app came back with the approval, and m7-arch's app, told of the approval, dropped the approval's id and came back for nothing (apps/desktop/src/app/owner_security/mod.rs, `note_owner_key_approved`). Its own ask on m7-arch ("candle cobalt river pony") meanwhile lapsed unseen after ten minutes. The coordinator: the user's approval must take, and the card must still be there when the user is at the Mac, without the asking app asking again on every reconnect. The key an owner-key request names (bound by the request's own params digest) now goes in the moment the owner approves it, whatever the asking app does, and the request is marked spent. (Such requests waited a day for a few hours on 17 Nov; that was taken back the same day, `OWNER_KEY_ASK_TTL_MS` in security/approvals.rs.)
*/
/// The owner approved `request`: when it asks for an owner key and kept that key with it, the key
/// goes in now (door-only when asked so), checked against the request's own target and params
/// digest, and the request is marked spent. Returns whether the key is in.
fn enroll_owner_key_on_approval(
    state: &AppState,
    request: &security::ApprovalRequest,
    by: &Principal,
    now: u64,
) -> bool {
    let security = &state.security;
    if request.action != ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey {
        return false;
    }
    let Some(saved) = security.approvals.resume_of(&request.id, OWNER_KEY_RESUME) else {
        return false;
    };
    let Ok(key) =
        serde_json::from_value::<ghostex_gx_protocol::approval::ApproverKey>(saved["key"].clone())
    else {
        return false;
    };
    let door_only = saved["doorOnly"].as_bool() == Some(true);
    let Ok(key_id) = security::key_id_of(&key) else {
        return false;
    };
    let digest = security::params_digest(&json!({
        "key": serde_json::to_value(&key).unwrap_or(Value::Null),
        "doorOnly": door_only,
    }));
    if request.target.as_deref() != Some(key_id.as_str()) || digest != request.params_digest {
        return false;
    }
    if door_only {
        security.mark_door_only_key(&key_id);
    }
    enroll_approved_key(state, by, true, key, now);
    if security.owner_keys.get(&key_id).is_none() {
        return false;
    }
    if security.approvals.spend_approved(&request.id).is_some() {
        log_approval_line(
            state,
            "approvalSpent",
            json!({ "approvalId": request.id, "action": request.action.as_str(), "caller": "gxserver" }),
        );
    }
    true
}

/// This computer's name and server id as the "act as you" card names them (review round 3, R3-4).
fn this_computer_label(state: &AppState) -> String {
    let name = crate::remote_access::cached_remote_access_identity()
        .map(|identity| identity.computer_name)
        .unwrap_or_else(|_| crate::connected_computers::this_computer_name());
    format!("{name} (Ghostex server {})", &*state.security.server_id)
}

/// A new "act as you" request beyond `OWNER_KEY_REQUESTS_WAITING` pushes out the oldest waiting
/// one that isn't the app's own (review round 3, R3-2 in security/policy.rs); its card closes.
fn push_out_oldest_owner_key_request(state: &AppState, newest: &str, now: u64) {
    let security = &state.security;
    let mut waiting: Vec<_> = security
        .approvals
        .list_pending(now)
        .into_iter()
        .filter(|request| {
            request.action == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
                && request.id != newest
        })
        .collect();
    if waiting.len() < OWNER_KEY_REQUESTS_WAITING {
        return;
    }
    waiting.sort_by_key(|request| request.created_at_ms);
    if let Some(oldest) = waiting
        .iter()
        .find(|request| !security.is_app_owner_key_request(&request.id))
    {
        if let Some(pushed) = security.approvals.push_out(&oldest.id, newest, now) {
            security.publish(security::SecurityEvent::ApprovalExpired { request: pushed });
        }
    }
}

/// The security log notes, once an hour, that the token file asked to act as you too often.
fn note_owner_key_flood(state: &AppState, caller: &Principal, now: u64) {
    static LAST: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if *last + 60 * 60 * 1000 > now {
        return;
    }
    *last = now;
    let _ = state;
    crate::audit::record(
        crate::audit::Action::new(
            "ownerKey.flood",
            "Too many apps asked to act as you on this computer within an hour",
        ),
        crate::audit::Actor::from(caller),
        crate::audit::Outcome::Refused("More asks wait until the hour is over".to_string()),
        None,
    );
}
/// `/api/enrollOwnerKey`'s and `/api/openOwnerSession`'s scope (`approval::OWNER_SCOPE_DOOR`).
const OWNER_SCOPE_PARAM: &str = "scope";
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
    /// The owner's Approve is needed first (`approvalRequired`, `approvalDenied`).
    Response(RoutedResponse),
}

/// What `/api/approveApprovalRequest` answers the token file (or anyone else but the app's owner
/// session and paired phones) for a new device, a terminal's pairing code or another owner, once
/// this computer has an owner device (CDXC:Security 2026-11-16 in security/policy.rs).
const DOOR_REFUSAL: &str = "Approve this in the Ghostex app on this computer or on your phone. A terminal or an agent can't approve a new device here.";

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
    use ghostex_gx_protocol::approval::EnrollKeyParams;
    let security = &state.security;
    let now = security::now_ms();
    // `askOwner: false` (the app's quiet opener): never make a request, answer
    // `ownerApprovalNeeded` instead. `approvalId` is read from the body by the gate below.
    let ask_owner = params.get(ASK_OWNER_PARAM).and_then(Value::as_bool) != Some(false);
    let mut params = params;
    // The app's one-time value for the first key (review round 2, R2-3 in security/policy.rs).
    let first_key_secret = params
        .remove(FIRST_KEY_SECRET_PARAM)
        .and_then(|value| value.as_str().map(str::to_string));
    // This computer's own app (it carries this start's one-time value): its owner key is never
    // retired here (CDXC:Security 2026-11-17 at `SecurityState::retire_owner_key`). Read before
    // the first key spends the value.
    let from_this_app = endpoint_path == security::OWNER_KEY_ENROLL_ENDPOINT
        && matches!(caller, Principal::LocalAgent)
        && security.is_first_key_secret(first_key_secret.as_deref());
    params.remove(ASK_OWNER_PARAM);
    params.remove(ghostex_gx_protocol::approval::APPROVAL_ID_PARAM);
    let owner = endpoint_path == security::OWNER_KEY_ENROLL_ENDPOINT;
    // `scope: "door"`: the key opens door sessions only (review M2 in security/policy.rs).
    let door_only = params
        .remove(OWNER_SCOPE_PARAM)
        .and_then(|scope| scope.as_str().map(str::to_string))
        .as_deref()
        == Some(ghostex_gx_protocol::approval::OWNER_SCOPE_DOOR);
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
    An owner key opens an owner session, which is one of your devices (it approves pairings and answers secret requests). Only the token file or an owner session may add one, as since 8 Nov: an agent's session token must not make itself yours. With approvals plain (security/policy.rs) nothing else is asked: a key from your own app goes in at once, on this computer and on another computer through its SSH tunnel, so the "act as the owner" card (#532) is gone; apps from before 11.2.3 still enroll and open their owner sessions as they always did. Narrowed on 2026-11-16 for the token file once this computer has an owner device (below).
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
        if from_this_app {
            security.note_app_owner_key(&key_id);
        }
        return Ok(json!({ "key": known }));
    }
    /*
    CDXC:Security 2026-11-16 DECISION:
    User (W524, "Owner only"): an owner key opens the owner session, which makes pairing codes and approves new devices, so once this computer has an owner device the token file (every program of the user, agents included) adds one only with an Approve on the app's owner session or a paired phone: one `addApproverKey` card, reused while it waits, that the desktop already waits on and repeats its call with (`owner_security::enroll_owner_key`). Before that (no owner key and no paired phone: the first key) it goes in at once only with the one-time value the app handed gxserver at start (review round 2, R2-3 in security/policy.rs). An owner session adding a key is the owner. Another computer's app reaching this one through its SSH tunnel (the same token file) asks the same way, once per computer, so it can approve this computer's devices from there (CDXC:Security 2026-11-16 at `is_door_call` in apps/desktop/src/app/owner_security/mod.rs).
    */
    if owner
        && matches!(caller, Principal::LocalAgent)
        && !security.has_owner_device()
        && !security.take_first_key(first_key_secret.as_deref())
    {
        // No owner device yet, so nobody could approve a card: the first key comes only from the
        // app that started gxserver; until then the token file keeps the door.
        return Err(EnrollRefusal::Reply(
            OWNER_APPROVAL_NEEDED,
            "The first owner key here is taken only from the Ghostex app as it starts Ghostex's server. Until then this computer's pairing works as it does without the app."
                .to_string(),
        ));
    }
    if owner && matches!(caller, Principal::LocalAgent) && security.has_owner_device() {
        if !ask_owner {
            return Err(EnrollRefusal::Reply(
                OWNER_APPROVAL_NEEDED,
                format!(
                    "{} already has an owner: approve this app in Ghostex there or on your phone first.",
                    crate::connected_computers::this_computer_name()
                ),
            ));
        }
        /*
        CDXC:Security 2026-11-16 FIX:
        Review M1 and M2 of the owner-only door: the card's words came from the key's own label, which any caller writes, so an agent's request looked like the app's, and it said "show codes and approve devices" while the key opened a full owner session. The card now ends with key words (four since round 3), which the app that asks shows too (from the request's own nonce since round 2, below), and says what the key may really do (door sessions only with `scope: "door"`, which this app sends; an older app's key acts as you for everything here).
        */
        /*
        CDXC:Security 2026-11-16 FIX:
        Review round 2 (R2-2): words from the key id alone were 24 bits a caller chooses (grind keys offline until the words match the app's), and `replaces` let the token file's newer request replace the app's waiting card. The words now come from a nonce gxserver makes for each request, hashed with the key id (`approval::owner_key_words_input`); the request carries that nonce back to the app that asked, which works the words out and shows them. Requests no longer replace each other; instead at most `OWNER_KEY_REQUESTS_WAITING` such requests wait at once, and another key gets a plain refusal until those are answered.
        */
        // Review round 3 (R3-2 in security/policy.rs): this computer's app (it carries this
        // start's one-time value) is never capped or pushed out; the same key asking again, or
        // coming back with its approval, makes no new request.
        let app_request = security.is_first_key_secret(first_key_secret.as_deref());
        let pending = security.approvals.list_pending(now);
        let returning = body
            .pointer("/params/approvalId")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.trim().is_empty())
            || pending.iter().any(|request| {
                request.action == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
                    && request.target.as_deref() == Some(key_id.as_str())
            });
        if !app_request && !returning && !security.note_owner_key_request(now) {
            note_owner_key_flood(state, caller, now);
            return Err(EnrollRefusal::Reply(
                OWNER_KEY_FLOODED,
                "Too many apps asked to manage this computer in the last hour, so this one can't ask yet. Deny the asks you don't know (in Ghostex or on your phone) and try again in an hour.".to_string(),
            ));
        }
        let nonce = security::random_b64(32);
        let words = {
            use sha2::{Digest, Sha256};
            ghostex_gx_protocol::approval::owner_key_words(&Sha256::digest(
                ghostex_gx_protocol::approval::owner_key_words_input(&nonce, &key_id),
            ))
        };
        // Review round 3 (R3-4): the card names this computer and its server id, so a request
        // that reached it through another computer's connection shows a name the app didn't ask
        // for (the asking app shows the computer it means).
        let here = this_computer_label(state);
        let (summary, detail) = if door_only {
            (
                format!(
                    "Let {} show the pairing code of {here} and approve new phones and computers there (key words: {words})",
                    key.label
                ),
                format!("Approve only if it is your own Ghostex app, on this computer or on one of your computers connected to it, and it shows the same four words and names {here}."),
            )
        } else {
            (
                format!(
                    "Let {} act as you on {here} (key words: {words})",
                    key.label
                ),
                format!("It can then do everything you can do there: show the pairing code, approve new phones and computers, answer secret requests. Approve only if it is your own Ghostex app and it shows the same four words and names {here}."),
            )
        };
        let action = ActionRequest::new(
            ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey,
            summary,
        )
        .target(key_id.clone())
        .detail(detail)
        .nonce(nonce)
        .params(json!({
            "key": serde_json::to_value(&key).unwrap_or(Value::Null),
            "doorOnly": door_only,
        }));
        let outcome = presence_outcome(state, caller, action, body);
        if let security::PresenceOutcome::Pending(request) = &outcome {
            // New, not one that already waited (`pending` was read before asking).
            if !pending.iter().any(|waiting| waiting.id == request.id) {
                // The key goes in the moment the owner approves (CDXC:Security 2026-11-17 at
                // `enroll_owner_key_on_approval`); kept once, by the call that made the request.
                security.approvals.attach(
                    &request.id,
                    OWNER_KEY_RESUME,
                    json!({
                        "key": serde_json::to_value(&key).unwrap_or(Value::Null),
                        "doorOnly": door_only,
                    }),
                );
                if app_request {
                    security.note_app_owner_key_request(&request.id);
                } else {
                    push_out_oldest_owner_key_request(state, &request.id, now);
                }
            }
        }
        if let Err(response) = presence_outcome_response(outcome, endpoint_path, request_id) {
            return Err(EnrollRefusal::Response(response));
        }
    }
    if let Some(approver) = approver_key {
        enroll_approved_key(state, caller, false, approver, now);
    }
    if owner && door_only {
        security.mark_door_only_key(&key_id);
    }
    enroll_approved_key(state, caller, owner, key, now);
    if from_this_app && registry.get(&key_id).is_some() {
        security.note_app_owner_key(&key_id);
    }
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
            let ApproveApprovalParams {
                id,
                key_id,
                understands,
                ..
            } = parse(params)?;
            // A new device, a terminal's pairing code or another owner: only the door's keepers
            // (CDXC:Security 2026-11-16 in security/policy.rs).
            // The terminal, on a computer whose own app is its only owner, for the first computer
            // that joins it and then that computer's ask to manage it (CDXC:Security 2026-11-17
            // DECISION at `terminal_may_approve` in security/policy.rs).
            let by_terminal = security
                .approvals
                .read(&id, caller, now)
                .is_some_and(|asked| security.terminal_may_approve(caller, &asked, now));
            if let Some(asked) = security.approvals.read(&id, caller, now) {
                if security::opens_door(&asked.action)
                    && !security.keeps_door(caller)
                    && !by_terminal
                {
                    crate::audit::record(
                        crate::audit::Action::from_approval(&asked),
                        crate::audit::Actor::from(caller),
                        crate::audit::Outcome::Refused(DOOR_REFUSAL.to_string()),
                        Some(crate::audit::ApprovalRef::from(&asked)),
                    );
                    return Err(("forbidden", DOOR_REFUSAL.to_string()));
                }
            }
            // The proven ask to manage this computer reached gxserver with the token file the
            // terminal approves with, so it is the one request its own asker may approve.
            let own_ok = by_terminal
                && security
                    .approvals
                    .read(&id, caller, now)
                    .is_some_and(|asked| {
                        asked.action
                            == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
                    });
            let request = security
                .approvals
                .approve_plain_understanding(&id, caller, now, own_ok, &understands)
                .map_err(|error| approval_error(&error))?;
            let key_id = if key_id.is_empty() {
                caller.kind().to_string()
            } else {
                key_id
            };
            // Asked once, kept for good (CDXC:Security 2026-11-10 in security/policy.rs).
            security.remember_approved(&request, now);
            // An app's owner key goes in now, whether or not that app comes back for it.
            let enrolled = enroll_owner_key_on_approval(state, &request, caller, now);
            if by_terminal {
                note_terminal_handover(state, &request, enrolled, now);
            }
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
            // `scope` is read and changes nothing: every owner session is a door session
            // (CDXC:Security 2026-11-17 in security/owner.rs).
            let ghostex_gx_protocol::approval::OpenOwnerSessionParams {
                key_id,
                challenge,
                signature,
                label,
                ..
            } = parse(params)?;
            security
                .open_owner_session(&key_id, &challenge, &signature, label.as_deref(), now)
                .map(|opened| json!(opened))
                .map_err(|error| ("forbidden", error.to_string()))
        }
        security::PROVE_OWNER_HANDOVER_ENDPOINT => {
            // CDXC:Security 2026-11-17 DECISION at `terminal_may_approve` in security/policy.rs:
            // the joined computer's own SSH login, carrying its one-time secret, names its ask.
            if !matches!(caller, Principal::LocalAgent) {
                return Err((
                    "forbidden",
                    "Only a computer signed in here over SSH can prove its ask.".to_string(),
                ));
            }
            let text = |name: &str| {
                params
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            let (Some(id), Some(secret)) = (text("id"), text("secret")) else {
                return Err(("badRequest", "id and secret are required.".to_string()));
            };
            let refuse = |reason: String| {
                crate::audit::record(
                    crate::audit::Action::new(
                        "ownership.handoverRefused",
                        "A computer's ask to manage this one wasn't proven",
                    )
                    .target(id.clone()),
                    crate::audit::Actor::from(caller),
                    crate::audit::Outcome::Refused(reason.clone()),
                    None,
                );
                ("forbidden", reason)
            };
            // The trace first: a caller that isn't an SSH login learns nothing about the asks.
            if let Err(reason) = crate::remote_access::ssh_login_trace::check(
                crate::server::request_client_address(),
                state.config.listeners.local.port,
            ) {
                return Err(refuse(format!(
                    "This didn't come through an SSH login here ({reason}), so it proves nothing."
                )));
            }
            let request = security
                .approvals
                .read(&id, caller, now)
                .ok_or(("notFound", "That ask expired. Ask again.".to_string()))?;
            let name = security
                .prove_terminal_handover(&secret, &request, now)
                .map_err(|reason| refuse(reason.to_string()))?;
            crate::audit::record(
                crate::audit::Action::new(
                    "ownership.handoverProven",
                    format!("{name} asked to manage this computer through its SSH login"),
                )
                .target(name.clone()),
                crate::audit::Actor::from(caller),
                crate::audit::Outcome::Pending,
                Some(crate::audit::ApprovalRef::from(&request)),
            );
            Ok(json!({ "proven": true, "name": name, "id": request.id }))
        }
        security::APPROVE_WITH_OWNER_KEY_ENDPOINT => {
            // One waiting request for a new owner key, approved by an enrolled owner key's
            // signature over this server id, the request's id and nonce and the new key
            // (CDXC:Security 2026-11-17 in packages/gx-protocol/src/approval.rs). The signature
            // is the approval, so it stands in for the door's keepers here.
            let ghostex_gx_protocol::approval::ApproveWithOwnerKeyParams {
                id,
                key_id,
                signature,
                key: new_key,
            } = parse(params)?;
            let signer_principal = |device_id: String| Principal::Owner { device_id };
            let reader = security
                .owner_keys
                .get(&key_id)
                .map(|key| signer_principal(key.device_id))
                .ok_or((
                    "forbidden",
                    security::OwnerError::Key(security::ApproverKeyError::UnknownKey).to_string(),
                ))?;
            let asked = security
                .approvals
                .read(&id, &reader, now)
                .ok_or(("notFound", security::GONE_MESSAGE.to_string()))?;
            let signer =
                match security.check_owner_key_approval(&asked, &key_id, &signature, &new_key) {
                    Ok(signer) => signer,
                    Err(error) => {
                        crate::audit::record(
                            crate::audit::Action::from_approval(&asked),
                            crate::audit::Actor::from(caller),
                            crate::audit::Outcome::Refused(error.to_string()),
                            Some(crate::audit::ApprovalRef::from(&asked)),
                        );
                        return Err(("forbidden", error.to_string()));
                    }
                };
            let by = signer_principal(signer.device_id.clone());
            let request = security
                .approvals
                .approve_plain(&id, &by, now)
                .map_err(|error| approval_error(&error))?;
            /*
            CDXC:Security 2026-11-17 FIX:
            Third review of the m7-arch unlock, finding 1, user: "approveWithOwnerKey enrolls the new key itself (from the request's own params: key, doorOnly) and only then retires the old key, in one step, so a dropped answer or a quit app can't lock the computer out. A retried click must still finish." The old key was retired on approval while the new key went in only on the app's next call within the approval's two minutes, so a lost answer or a quit app left a computer with no key that could sign. The new key (checked against the request's own parameters) is now enrolled here, door-only, before the old key is retired; if it couldn't be kept, the old key isn't retired. The app's later `enrollOwnerKey` finds the key already there, whenever it comes.
            */
            enroll_approved_key(state, &by, true, new_key, now);
            let Some(enrolled) = asked
                .target
                .as_deref()
                .and_then(|target| security.owner_keys.get(target))
            else {
                return Err((
                    "internalError",
                    "Ghostex could not keep the new owner key here, so the old one still works. Try again."
                        .to_string(),
                ));
            };
            // The change is made: the request is spent, not left to read "approved, but the
            // change wasn't made within 2 minutes" (m7-arch, 17 Nov).
            if security.approvals.spend_approved(&id).is_some() {
                log_approval_line(
                    state,
                    "approvalSpent",
                    json!({ "approvalId": id, "action": request.action.as_str(), "caller": "gxserver" }),
                );
            }
            // It signs nothing here again, unless this computer's own app enrolled it
            // (CDXC:Security 2026-11-17 at `retire_owner_key`).
            let retired = security.retire_owner_key(&signer);
            let mut logged =
                crate::audit::Action::new("approval.approved", request.summary.clone());
            if let Some(target) = &request.target {
                logged = logged.target(target.clone());
            }
            crate::audit::record(
                logged,
                crate::audit::Actor::from(&by),
                crate::audit::Outcome::ApprovedBy(format!(
                    "Approved with the owner key {} ({}), signed for this one request; {}",
                    signer.label,
                    signer.key_id,
                    if retired {
                        "that key signs nothing here any more"
                    } else {
                        "that key is this computer's own app's, so it stays"
                    }
                )),
                Some(crate::audit::ApprovalRef::from(&request)),
            );
            security.publish(security::SecurityEvent::ApprovalApproved {
                request: request.clone(),
                key_id: signer.key_id,
            });
            Ok(json!({ "request": request, "key": enrolled, "retired": retired }))
        }
        security::SECURITY_STATUS_ENDPOINT => Ok(json!({
            "serverId": &*security.server_id,
            // Approvals need no signature here; newer apps stop signing (older ones still may).
            "plainApprovals": true,
            // An owner key here may approve a new owner key's waiting request by signature
            // (`/api/approveWithOwnerKey`, CDXC:Security 2026-11-17); older gxservers don't say.
            "approveWithOwnerKey": true,
            // Settings > Security: the approving device checks Touch ID, Windows Hello, its
            // password or the phone's Face ID first (`approvalsRequirePresence`, off by default).
            "requirePresence": approvals_require_presence(state),
            "caller": caller,
            // Whether this caller may make pairing codes and approve new devices here (the token
            // file only until this computer has an owner device): `ghostex computer-code` asks
            // y/n only when it may (CDXC:Security 2026-11-16 in security/policy.rs). Older
            // clients don't read it.
            "mayApproveDevices": security.keeps_door(caller),
            // The terminal may approve the first computer that joins with the code it was just
            // shown, because this computer's own app is its only owner (CDXC:Security 2026-11-17
            // DECISION at `terminal_may_approve`); `ghostex computer-code` then asks y/n.
            "terminalMayApproveJoin": matches!(caller, Principal::LocalAgent)
                && security.only_own_app_owns(),
            // The asks to manage this computer that a computer the terminal let in proved through
            // its SSH login: the only ones `ghostex computer-code` offers to approve.
            "terminalHandovers": if matches!(caller, Principal::LocalAgent) {
                security
                    .proven_handovers(now)
                    .into_iter()
                    .map(|(id, name)| json!({ "id": id, "name": name }))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            },
            // Retired keys (`retire_owner_key`) aren't listed: they sign nothing here.
            "ownerKeys": security.listed_owner_keys(),
            "approverKeys": security.approver_keys.list(),
            // Who can approve here, in plain names, for the apps' "approve it in Ghostex on …"
            // and `ghostex computer-code` (CDXC:Security 2026-11-17 at `approvers_here`).
            "approvesHere": approvers_here(state, caller, &params),
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

/*
CDXC:Security 2026-11-17 FIX:
The user ran `ghostex computer-code --copy` in a terminal on their Windows PC, which they use headless, to connect their Linux computer: "i dont want to look for approval screen on the windows machine cuz that machine is supposed to be connected to headlessly", and then "it's asking M7-Desktop to let this computer in even tho i'm using headless command to get the code from there man. this doesn't make sense." Both asks (show the code, let the new computer in) can only be approved by this computer's door keepers (its owner apps, on any computer, and its paired phones and computers; never the terminal once there is one, the user's Q-3A), yet every message said "the Ghostex app on this computer or your phone", or "Approve on <this computer>". gxserver now names them: `approvesHere` in `/api/securityStatus` lists, in plain names, each app that owns this computer (on whichever computer it runs; this computer's own app says so) and each paired phone or computer, for `ghostex computer-code`, the CLI's approval wait and the apps' "approve it in Ghostex on …". Older gxservers don't send it; their readers keep the old words.
*/
/*
CDXC:Security 2026-11-20 FIX:
The user pasted M7-Desktop's computer code on m7-arch and the Mac's notice said "Approve it in Ghostex on Mohamads-MacBook-Pro-2 or Mohamad's MacBook Pro (2) or your phone (samsung SM-F976B) or M7-DESKTOP": the Mac reading it was named twice, by old names (its key's label from enrollment and its paired-computer record), though it could approve nothing there, and the Windows PC was named in capitals while every other place says M7-Desktop. `approvesHere` now lists one entry per computer: a paired computer is never an entry of its own (it gets no device token, only phones do, so it approves nothing; its app's owner key does), and only names that key's computer when the two names are the same letters and digits (macOS's host name "Mohamads-MacBook-Pro-2" is its computer name "Mohamad's MacBook Pro (2)" that way); this computer's own app is named as this computer is now; and the computer asking is left out: an owner session's own key, and the keys and name a caller sends (`exceptKeyIds`, `exceptName`; an app reaching here with this computer's token file is otherwise nobody in particular). Keys keep current names as they open sessions (CDXC:Security 2026-11-20 at `current_key_label` in security/owner.rs). Supersedes the "each paired phone or computer" of CDXC:Security 2026-11-17 above.
*/
/// Who can approve this computer's asks (its door keepers), in plain names, one per computer:
/// `{name, kind}` with kind `app` (an owner app on another computer), `thisApp` (this computer's
/// own app) or `phone`. Never keys or ids. Leaves out the computer asking (`caller`'s owner key,
/// and `exceptKeyIds` / `exceptName` in `params`).
fn approvers_here(state: &AppState, caller: &Principal, params: &Map<String, Value>) -> Vec<Value> {
    let security = &state.security;
    let this_name = crate::remote_access::cached_remote_access_identity()
        .map(|identity| identity.computer_name)
        .ok();
    let devices = crate::storage::open_gxserver_database(&state.paths)
        .ok()
        .and_then(|db| crate::remote_access::paired_devices::list_paired_devices_json(&db).ok());
    let paired: Vec<(String, bool)> = devices
        .as_ref()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|device| device["status"] == "paired")
        .filter_map(|device| {
            let name = device["name"].as_str()?.trim().to_string();
            let phone = matches!(device["platform"].as_str(), Some("ios" | "android"));
            (!name.is_empty()).then_some((name, phone))
        })
        .collect();
    let except_ids: Vec<&str> = params
        .get("exceptKeyIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let caller_device = match caller {
        Principal::Owner { device_id } => Some(device_id.as_str()),
        _ => None,
    };
    let mut asking: Vec<String> = params
        .get("exceptName")
        .and_then(Value::as_str)
        .map(same_computer_name)
        .filter(|name| !name.is_empty())
        .into_iter()
        .collect();
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    for key in security.listed_owner_keys() {
        let label = key
            .label
            .trim_end_matches("(Ghostex app)")
            .trim()
            .to_string();
        if label.is_empty() {
            continue;
        }
        let here = security.is_app_owner_key(&key.key_id)
            || this_name
                .as_deref()
                .is_some_and(|name| same_computer_name(name) == same_computer_name(&label));
        if except_ids.contains(&key.key_id.as_str())
            || caller_device == Some(key.device_id.as_str())
        {
            asking.push(same_computer_name(&label));
            continue;
        }
        let entry = match key.platform.as_str() {
            "ios" | "android" => (label, "phone"),
            _ if here => (this_name.clone().unwrap_or(label), "thisApp"),
            // Its paired-computer record names it as that computer names itself.
            _ => (
                paired
                    .iter()
                    .find(|(name, phone)| {
                        !phone && same_computer_name(name) == same_computer_name(&label)
                    })
                    .map_or(label, |(name, _)| name.clone()),
                "app",
            ),
        };
        entries.push(entry);
    }
    entries.extend(
        paired
            .iter()
            .filter(|(_, phone)| *phone)
            .map(|(name, _)| (name.clone(), "phone")),
    );
    let mut out: Vec<Value> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (name, kind) in entries {
        let same = same_computer_name(&name);
        if same.is_empty() || asking.contains(&same) || seen.contains(&same) {
            continue;
        }
        seen.push(same);
        out.push(json!({ "name": name, "kind": kind }));
    }
    out
}

/// A computer's name as its letters and digits, lower-case: "M7-DESKTOP" and "M7-Desktop", or
/// "Mohamads-MacBook-Pro-2" and "Mohamad's MacBook Pro (2)", are one computer.
fn same_computer_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The terminal approved `request` under CDXC:Security 2026-11-17 DECISION at
/// `terminal_may_approve` (security/policy.rs): the join is noted for the ask that may follow, the
/// owner key ends the handover, and each says so in the security log in plain words.
fn note_terminal_handover(
    state: &AppState,
    request: &security::ApprovalRequest,
    enrolled: bool,
    now: u64,
) {
    let security = &state.security;
    let here = crate::remote_access::cached_remote_access_identity()
        .map(|identity| identity.computer_name)
        .unwrap_or_else(|_| crate::connected_computers::this_computer_name());
    match request.action {
        ghostex_gx_protocol::approval::SensitiveAction::PairingApproveDevice => {
            let name = request
                .summary
                .split_once("calls itself \"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name.trim().to_string())
                .unwrap_or_default();
            security.note_terminal_join(&request.id, &name, now);
            crate::audit::record(
                crate::audit::Action::new(
                    "ownership.terminalJoin",
                    format!("Let {name} connect to {here} from {here}'s terminal"),
                )
                .target(name.clone()),
                crate::audit::Actor::SharedToken { claimed_session: None },
                crate::audit::Outcome::ApprovedBy(format!(
                    "Approved in {here}'s terminal (ghostex computer-code) while {here}'s own Ghostex app was its only owner: {name} gets the Easy Connect tunnel only and signs in with your SSH login; once signed in it may ask to manage {here}"
                )),
                Some(crate::audit::ApprovalRef::from(request)),
            );
        }
        ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey if enrolled => {
            let name = security.handover_name(&request.id).unwrap_or_default();
            security.clear_terminal_joins();
            crate::audit::record(
                crate::audit::Action::new(
                    "ownership.handover",
                    format!("{name} now manages {here}"),
                )
                .target(name.clone()),
                crate::audit::Actor::SharedToken { claimed_session: None },
                crate::audit::Outcome::ApprovedBy(format!(
                    "Approved in {here}'s terminal: {name} joined {here} from that terminal while only {here}'s own Ghostex app owned it, and asked through its SSH login. {name} now approves new phones and computers for {here}; the terminal approves nothing more"
                )),
                Some(crate::audit::ApprovalRef::from(request)),
            );
        }
        _ => {}
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
