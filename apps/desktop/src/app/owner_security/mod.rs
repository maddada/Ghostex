//! The desktop's side of GitHub #495: the owner session with this computer's gxserver, and
//! approving or denying approval requests with Touch ID / Windows Hello / polkit.
//!
//! - The owner key (`ghostex-owner-keys`, in the OS key store) opens a `gxo_` owner session at
//!   first use; every local gxserver call then sends it instead of the token file
//!   (`read_gpui_gxserver_auth_token`). Until it is open, or when it can't be, the app runs on the
//!   token file's agent level.
//! - A call of the app's own that gxserver answers with `approvalRequired` (the user just clicked
//!   something sensitive) shows the system prompt at once, approves, and is sent again with
//!   `approvalId` (`typed_operations.rs`).
//! - A request an agent made shows as a card with Approve and Deny (`ui.rs`).
//!
//! Contract: `ai/security-tokens-and-approvals.md`.

pub(crate) mod ui;

use std::{
    cell::Cell,
    sync::Mutex,
    time::{Duration, Instant},
};

use ghostex_gx_protocol::approval::{
    APPROVAL_REQUIRED_ERROR, ApprovalRequest, approval_signing_payload,
    owner_session_signing_payload,
};
use serde_json::{Value, json};

use crate::app::helpers::gpui_gxserver_rpc_result_with_code;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after a failed attempt the next call tries to open the owner session again.
const RETRY_AFTER: Duration = Duration::from_secs(60);

struct OwnerSession {
    token: Option<String>,
    last_attempt: Option<Instant>,
    opening: bool,
}

static OWNER_SESSION: Mutex<OwnerSession> = Mutex::new(OwnerSession {
    token: None,
    last_attempt: None,
    opening: false,
});

thread_local! {
    /// Set while this thread runs an approval of its own, so a call inside it that needs an
    /// approval (a second approver key) is not answered by prompting again.
    static IN_APPROVAL: Cell<bool> = const { Cell::new(false) };
}

fn session() -> std::sync::MutexGuard<'static, OwnerSession> {
    OWNER_SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The owner session's token when it is open. When it isn't, a background thread tries to open
/// it (at most once a minute) and this call returns `None`, so the caller uses the token file.
pub(crate) fn cached_owner_token() -> Option<String> {
    let mut state = session();
    if let Some(token) = &state.token {
        return Some(token.clone());
    }
    let due = state
        .last_attempt
        .is_none_or(|last| last.elapsed() >= RETRY_AFTER);
    if due && !state.opening {
        state.opening = true;
        state.last_attempt = Some(Instant::now());
        drop(state);
        let _ = std::thread::Builder::new()
            .name("ghostex-owner-session".into())
            .spawn(|| {
                let opened = open_owner_session();
                let mut state = session();
                state.opening = false;
                match opened {
                    Ok(token) => state.token = Some(token),
                    Err(error) => log::warn!("owner session not opened: {error}"),
                }
            });
    }
    None
}

/// gxserver refused the owner session token (its state was reset): forget it, so the next call
/// uses the token file and opens a new one.
pub(crate) fn forget_owner_token(token: &str) {
    let mut state = session();
    if state.token.as_deref() == Some(token) {
        state.token = None;
        state.last_attempt = None;
    }
}

/// The owner session, opened now on this thread if it isn't yet.
fn require_owner_session() -> Result<String, String> {
    if let Some(token) = session().token.clone() {
        return Ok(token);
    }
    let token = open_owner_session()?;
    session().token = Some(token.clone());
    Ok(token)
}

fn rpc(endpoint: &str, params: Value) -> Result<Value, (Option<String>, String)> {
    gpui_gxserver_rpc_result_with_code(endpoint, &params, CALL_TIMEOUT)
        .map_err(|error| (error.code, error.message))
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn this_computer_name() -> String {
    #[cfg(unix)]
    {
        let mut buffer = [0_u8; 256];
        if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0 {
            let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(0);
            if let Ok(name) = std::str::from_utf8(&buffer[..end]) {
                let name = name.trim_end_matches(".local");
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This computer".to_string())
}

/// The wire form of one of this computer's keys; `device_public_key` (the owner key's) names the
/// computer, so its owner and approver keys share one device id.
fn key_json(public_key: &str, device_public_key: &str, role: &str) -> Value {
    let digest = ring::digest::digest(&ring::digest::SHA256, device_public_key.as_bytes());
    let short: String = digest.as_ref()[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    json!({
        "keyId": "",
        "deviceId": format!("desktop-{short}"),
        "label": format!("{} ({role})", this_computer_name()),
        "platform": ghostex_owner_keys::platform_name(),
        "algorithm": "ecdsaP256Sha256",
        "publicKey": public_key,
        "presence": ghostex_owner_keys::default_presence().wire_name(),
        "addedAtMs": 0,
    })
}

/// Enrolls the owner key (trusted on first use) and opens an owner session with it.
fn open_owner_session() -> Result<String, String> {
    let key = ghostex_owner_keys::load_or_create_owner_key().map_err(|error| error.to_string())?;
    let enrolled = rpc(
        "/api/enrollOwnerKey",
        json!({ "key": key_json(&key.public_key_b64(), &key.public_key_b64(), "Ghostex app") }),
    )
    .map_err(|(code, message)| match code.as_deref() {
        Some(APPROVAL_REQUIRED_ERROR) => "Another owner key is enrolled on this computer's Ghostex. Approve this app from a device you trust.".to_string(),
        _ => message,
    })?;
    let key_id = enrolled
        .get("key")
        .and_then(|key| text(key, "keyId"))
        .ok_or("gxserver didn't return the owner key")?;
    let status = rpc("/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
    let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
    let challenge = rpc("/api/ownerChallenge", json!({})).map_err(|(_, message)| message)?;
    let challenge = text(&challenge, "challenge").ok_or("gxserver gave no challenge")?;
    let payload = owner_session_signing_payload(&server_id, &challenge, &key_id)
        .map_err(|error| error.to_string())?;
    let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
    let opened = rpc(
        "/api/openOwnerSession",
        json!({ "keyId": key_id, "challenge": challenge, "signature": signature }),
    )
    .map_err(|(_, message)| message)?;
    text(&opened, "token").ok_or_else(|| "gxserver opened no owner session".to_string())
}

/// Shows the system prompt with the request's summary, signs the request with this computer's
/// approver key (enrolling it the first time) and approves it.
pub(crate) fn approve_request(request: &ApprovalRequest) -> Result<(), String> {
    IN_APPROVAL.with(|flag| flag.set(true));
    let result = approve_request_inner(request);
    IN_APPROVAL.with(|flag| flag.set(false));
    result
}

fn approve_request_inner(request: &ApprovalRequest) -> Result<(), String> {
    require_owner_session()?;
    let owner_key =
        ghostex_owner_keys::load_or_create_owner_key().map_err(|error| error.to_string())?;
    let (key, presence) = ghostex_owner_keys::unlock_approver_key(&request.summary)
        .map_err(|error| error.to_string())?;
    let mut wire_key = key_json(
        &key.public_key_b64(),
        &owner_key.public_key_b64(),
        "approvals",
    );
    wire_key["presence"] = json!(presence.wire_name());
    let enrolled =
        rpc("/api/enrollApproverKey", json!({ "key": wire_key })).map_err(|(code, message)| {
            match code.as_deref() {
            Some(APPROVAL_REQUIRED_ERROR) => {
                "Approve adding this computer as an approver on your phone first, then try again."
                    .to_string()
            }
            _ => message,
        }
        })?;
    let key_id = enrolled
        .get("key")
        .and_then(|key| text(key, "keyId"))
        .ok_or("gxserver didn't return the approver key")?;
    let status = rpc("/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
    let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
    let payload =
        approval_signing_payload(&server_id, request).map_err(|error| error.to_string())?;
    let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
    rpc(
        "/api/approveApprovalRequest",
        json!({ "id": request.id, "keyId": key_id, "signature": signature }),
    )
    .map(|_| ())
    .map_err(|(_, message)| message)
}

/// Denies a request; no prompt, saying no only closes a door.
pub(crate) fn deny_request(id: &str) -> Result<(), String> {
    require_owner_session()?;
    rpc("/api/denyApprovalRequest", json!({ "id": id }))
        .map(|_| ())
        .map_err(|(_, message)| message)
}

/*
CDXC:Security 2026-11-08 WHY:
A sensitive change the user starts in the app (turning on Easy Connect, a security-relaxing setting) reaches gxserver as an owner call that still needs a signed approval. Showing the system prompt right then, with gxserver's summary, and sending the call again is what the user expects from their own click; a card would make them approve their own click twice. Never on the main thread (the prompt would freeze the window) and never inside an approval already running.
*/
/// For a call of the app's own that gxserver answered with `approvalRequired` (`body`): prompts,
/// approves, and returns the approval id to send the call again with. `None` when the answer
/// isn't one, the user didn't approve, or this thread can't prompt.
pub(crate) fn approve_own_call(status: u16, body: &str) -> Option<String> {
    if status != 403 || IN_APPROVAL.with(Cell::get) {
        return None;
    }
    if std::thread::current().name() == Some("main") {
        return None;
    }
    let envelope: Value = serde_json::from_str(body).ok()?;
    if envelope.get("error").and_then(Value::as_str) != Some(APPROVAL_REQUIRED_ERROR) {
        return None;
    }
    let request: ApprovalRequest =
        serde_json::from_value(envelope.get("approval")?.clone()).ok()?;
    match approve_request(&request) {
        Ok(()) => Some(request.id),
        Err(error) => {
            log::info!("own call not approved: {error}");
            None
        }
    }
}
