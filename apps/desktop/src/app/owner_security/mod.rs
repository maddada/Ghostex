//! The desktop's side of GitHub #495: the owner session with each gxserver it talks to (this
//! computer's, and every connected remote computer's through its tunnel), and approving or
//! denying approval requests with Touch ID / Windows Hello / polkit.
//!
//! - The owner key (`ghostex-owner-keys`, in the OS key store) opens a `gxo_` owner session at
//!   first use; every call to that gxserver then sends it instead of that computer's token file
//!   (`read_gpui_gxserver_auth_token` here, the remote transport for a remote computer). Until it
//!   is open, or when it can't be, the app runs on the token file's agent level.
//! - A call of the app's own that gxserver answers with `approvalRequired` (the user just clicked
//!   something sensitive) shows the system prompt at once, approves, and is sent again with
//!   `approvalId` (`typed_operations.rs`, `remote/ide_open.rs`).
//! - A request an agent made shows as a card with Approve and Deny (`ui.rs`).
//!
//! Contract: `ai/security-tokens-and-approvals.md`.
//!
//! CDXC:Security 2026-11-10 WHY: a gxserver that says `plainApprovals` (option B, `plain.rs`) needs
//! none of the owner session, owner key, approver key and signature below: this app approves there
//! with one click on its token file and opens no owner session. They stay only for computers still
//! on a gxserver from before plain approvals (11.2.2 and earlier); delete them once those no longer
//! matter.

pub(crate) mod plain;
pub(crate) mod ui;

use std::{
    cell::Cell,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use ghostex_gx_protocol::approval::{
    APPROVAL_REQUIRED_ERROR, ApprovalRequest, approval_signing_payload,
    owner_session_signing_payload,
};
use serde_json::{Value, json};

use crate::app::helpers::{
    gpui_gxserver_rpc_result_with_code, gpui_remote_gxserver_post_typed_operation,
};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after a failed attempt the next call tries to open an owner session again.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// The key of this computer's own gxserver in [`SESSIONS`].
const LOCAL: &str = "local";

/// Which gxserver: this computer's, or a connected computer's through its tunnel.
#[derive(Clone)]
pub(crate) enum Transport {
    Local,
    Remote(GpuiRemoteGxserverRequestTarget),
}

impl Transport {
    /// The owner session's key: `local`, or the remote computer's token file token, which names
    /// that gxserver and never leaves memory.
    fn key(&self) -> String {
        match self {
            Self::Local => LOCAL.to_string(),
            Self::Remote(target) => format!("remote:{}", target.token),
        }
    }
}

#[derive(Default)]
struct OwnerSession {
    token: Option<String>,
    last_attempt: Option<Instant>,
    opening: bool,
    /// The `addApproverKey` request enrolling the owner key there waits on (another owner key is
    /// enrolled on that computer): later attempts read it instead of asking again.
    waiting_approval: Option<String>,
    /// That request was denied or lapsed: no more asking there until the app restarts.
    gave_up: bool,
    /// That gxserver takes plain approvals: no owner session is opened there (`plain.rs`).
    plain: bool,
}

static SESSIONS: Mutex<Option<HashMap<String, OwnerSession>>> = Mutex::new(None);

thread_local! {
    /// Set while this thread runs an approval or opens an owner session, so a call inside it
    /// neither prompts again nor waits on an owner session that is being opened.
    static IN_SECURITY_FLOW: Cell<bool> = const { Cell::new(false) };
    /// Set while this thread runs work the user didn't just start (a renewal, a retry loop), so
    /// an `approvalRequired` answer never shows the system prompt ([`without_prompts`]).
    static NO_PROMPTS: Cell<bool> = const { Cell::new(false) };
}

/// Whether a system prompt for one of the app's own calls is open now; a second call waits for
/// nothing and doesn't prompt (see [`approve_own_call`]).
static PROMPT_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/*
CDXC:Security 2026-11-08 WHY:
User, on 11.1.0: "Man i see 1 million pin requests on my windows machine saying let the agents on x and x reach each other. Please don't do this! You spammed my machine with those, have like 50 open." The connected-computer renewal runs every minute on a background thread, and every `approvalRequired` answer of the app's own calls showed Windows Hello there and then, so a computer not approved yet opened a new PIN prompt each minute, each blocking its thread until answered. The system prompt is for the user's own click only: work the app does on its own runs inside `without_prompts`, waits on the one request it made, and shows it as an approval card instead (`remote_conn/connected_computers.rs`); and only one prompt is ever open at a time, whatever asks.
*/
/// Runs `work` with the system prompt off on this thread: a call answered `approvalRequired`
/// returns that answer instead of prompting.
pub(crate) fn without_prompts<R>(work: impl FnOnce() -> R) -> R {
    let outer = NO_PROMPTS.with(|flag| flag.replace(true));
    let result = work();
    NO_PROMPTS.with(|flag| flag.set(outer));
    result
}

/// The request in an answer that asks for the owner's approval: `approvalRequired` (a new
/// request) or `approvalPending` (one already waiting, server/security_gates.rs).
pub(crate) fn waiting_request_in(status: u16, body: &str) -> Option<ApprovalRequest> {
    if status != 403 {
        return None;
    }
    let envelope: Value = serde_json::from_str(body).ok()?;
    let code = envelope.get("error").and_then(Value::as_str)?;
    if code != APPROVAL_REQUIRED_ERROR && code != APPROVAL_PENDING_ERROR {
        return None;
    }
    serde_json::from_value(envelope.get("approval")?.clone()).ok()
}

/// gxserver's answer for a call that met a request already waiting (server/security_gates.rs).
pub(crate) const APPROVAL_PENDING_ERROR: &str = "approvalPending";

/// Where a request a background call waits on stands.
pub(crate) enum RequestState {
    Pending(ApprovalRequest),
    /// Approved: the call may be sent again with its id.
    Approved,
    /// Denied, or lapsed unanswered: don't ask again on your own.
    Declined,
    /// Gone (gxserver restarted, or the approval was spent by another call): asking again makes
    /// at most one new request.
    Gone,
    /// gxserver didn't answer; try again later.
    Unknown,
}

/// Reads request `id` on `transport`'s gxserver, without prompting.
pub(crate) fn request_state(transport: &Transport, id: &str) -> RequestState {
    use ghostex_gx_protocol::approval::ApprovalStatus;
    let answer =
        in_security_flow(|| rpc(transport, "/api/readApprovalRequest", json!({ "id": id })));
    match answer {
        Ok(answer) => match serde_json::from_value::<ApprovalRequest>(
            answer.get("request").cloned().unwrap_or_default(),
        ) {
            Ok(request) => match request.status {
                ApprovalStatus::Pending => RequestState::Pending(request),
                ApprovalStatus::Approved => RequestState::Approved,
                ApprovalStatus::Denied | ApprovalStatus::Expired => RequestState::Declined,
                _ => RequestState::Gone,
            },
            Err(_) => RequestState::Gone,
        },
        Err((Some(code), _)) if code == "notFound" => RequestState::Gone,
        Err(_) => RequestState::Unknown,
    }
}

fn with_session<R>(key: &str, run: impl FnOnce(&mut OwnerSession) -> R) -> R {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    run(sessions
        .get_or_insert_with(HashMap::new)
        .entry(key.to_string())
        .or_default())
}

fn in_security_flow<R>(run: impl FnOnce() -> R) -> R {
    let outer = IN_SECURITY_FLOW.with(|flag| flag.replace(true));
    let result = run();
    IN_SECURITY_FLOW.with(|flag| flag.set(outer));
    result
}

/// The owner session's token for `transport` when it is open. When it isn't, a background
/// thread tries to open it (at most once a minute) and this returns `None`, so the caller uses
/// the token file.
fn cached_token(transport: &Transport) -> Option<String> {
    if IN_SECURITY_FLOW.with(Cell::get) {
        return with_session(&transport.key(), |session| session.token.clone());
    }
    let key = transport.key();
    let start = with_session(&key, |session| {
        if session.token.is_some() || session.plain {
            return None;
        }
        let due = session
            .last_attempt
            .is_none_or(|last| last.elapsed() >= RETRY_AFTER);
        (due && !session.opening).then(|| {
            session.opening = true;
            session.last_attempt = Some(Instant::now());
        })
    });
    if start.is_some() {
        let transport = transport.clone();
        let _ = std::thread::Builder::new()
            .name("ghostex-owner-session".into())
            .spawn(move || {
                /*
                CDXC:Security 2026-11-10 DECISION:
                User (Q2099, "ask once per computer, earlier: ask at connect instead of when you first act there, so the Mac can answer from day one"): a connected computer that has its own owner is asked once, at connect, to let this app act as the owner there and answer its requests. The one request waits there (a day) and is read, never made again, while it waits (`enroll_owner_key`); once approved the keys are kept for good; denied or lapsed, it isn't asked again until the app restarts. This supersedes asking only when the owner first acts there (#508, CDXC:Security 2026-11-08 in server/src/server/security_http.rs, `ASK_OWNER_PARAM`).
                */
                // Option B: a gxserver that approves plainly needs no owner session.
                if plain::is_plain(&transport) {
                    with_session(&transport.key(), |session| {
                        session.opening = false;
                        session.plain = true;
                    });
                    return;
                }
                let opened = in_security_flow(|| open_owner_session(&transport, true));
                with_session(&transport.key(), |session| {
                    session.opening = false;
                    match opened {
                        Ok(token) => session.token = Some(token),
                        Err(error) => log::warn!("owner session not opened: {error}"),
                    }
                });
            });
    }
    with_session(&key, |session| session.token.clone())
}

/// The owner session's token for this computer's gxserver, when it is open.
pub(crate) fn cached_owner_token() -> Option<String> {
    cached_token(&Transport::Local)
}

/// The owner session's token for the remote computer `target` reaches, when it is open.
pub(crate) fn cached_remote_owner_token(
    target: &GpuiRemoteGxserverRequestTarget,
) -> Option<String> {
    cached_token(&Transport::Remote(target.clone()))
}

/// A gxserver refused owner session `token` (its state was reset): forget it, so the next call
/// uses the token file and opens a new one.
pub(crate) fn forget_owner_token(token: &str) {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for session in sessions.get_or_insert_with(HashMap::new).values_mut() {
        if session.token.as_deref() == Some(token) {
            session.token = None;
            session.last_attempt = None;
        }
    }
}

/// The owner session for `transport`, opened now on this thread if it isn't yet.
fn require_owner_session(transport: &Transport) -> Result<String, String> {
    if let Some(token) = with_session(&transport.key(), |session| session.token.clone()) {
        return Ok(token);
    }
    let token = open_owner_session(transport, true)?;
    with_session(&transport.key(), |session| {
        session.token = Some(token.clone())
    });
    Ok(token)
}

fn rpc(
    transport: &Transport,
    endpoint: &str,
    params: Value,
) -> Result<Value, (Option<String>, String)> {
    match transport {
        Transport::Local => gpui_gxserver_rpc_result_with_code(endpoint, &params, CALL_TIMEOUT)
            .map_err(|error| (error.code, error.message)),
        Transport::Remote(target) => {
            let (status, body) =
                gpui_remote_gxserver_post_typed_operation(target, endpoint, &params, CALL_TIMEOUT)
                    .map_err(|message| (None, message))?;
            let envelope: Value = serde_json::from_str(&body)
                .map_err(|_| (None, "gxserver returned invalid JSON.".to_string()))?;
            if (200..300).contains(&status) {
                return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
            }
            Err((
                text(&envelope, "error"),
                text(&envelope, "message")
                    .unwrap_or_else(|| format!("gxserver request failed with HTTP {status}.")),
            ))
        }
    }
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

/// Enrolls the owner key on `transport`'s gxserver (trusted on first use there) and opens an
/// owner session with it.
fn open_owner_session(transport: &Transport, may_ask: bool) -> Result<String, String> {
    in_security_flow(|| {
        let key =
            ghostex_owner_keys::load_or_create_owner_key().map_err(|error| error.to_string())?;
        let public_key = key.public_key_b64();
        remember_own_owner_key(&public_key);
        let mut params = json!({ "key": key_json(&public_key, &public_key, "Ghostex app") });
        // The approver key goes along, so the one approval that lets this app act as the owner
        // there also lets it answer requests there (server/src/server/security_http.rs,
        // `APPROVER_KEY_PARAM`); a gxserver from before it ignores the field.
        if let Ok(approver) = ghostex_owner_keys::approver_public_key_or_create() {
            params["approverKey"] = key_json(&approver, &public_key, "approvals");
        }
        if !may_ask {
            params["askOwner"] = json!(false);
        }
        let enrolled = enroll_owner_key(transport, params)?;
        let key_id = enrolled
            .get("key")
            .and_then(|key| text(key, "keyId"))
            .ok_or("gxserver didn't return the owner key")?;
        let status =
            rpc(transport, "/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
        let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
        let challenge =
            rpc(transport, "/api/ownerChallenge", json!({})).map_err(|(_, message)| message)?;
        let challenge = text(&challenge, "challenge").ok_or("gxserver gave no challenge")?;
        let payload = owner_session_signing_payload(&server_id, &challenge, &key_id)
            .map_err(|error| error.to_string())?;
        let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
        let opened = rpc(
            transport,
            "/api/openOwnerSession",
            json!({ "keyId": key_id, "challenge": challenge, "signature": signature }),
        )
        .map_err(|(_, message)| message)?;
        text(&opened, "token").ok_or_else(|| "gxserver opened no owner session".to_string())
    })
}

/*
CDXC:Security 2026-11-08 WHY:
When another owner key is already enrolled on a computer (a second computer connecting to it), enrolling this app's owner key there asks for that owner's approval. The owner session is retried every minute, and each request lapsed after ten minutes, so the Mac's log gained a new "Let M7-DESKTOP (Ghostex app) (windows) act as the owner of Ghostex on this computer" request every ten minutes all day (30 on 8 Nov). The app now asks once: it waits on that request, sends its key again with the approval once it is approved, and stops asking there until it restarts when it is denied or lapses.
*/
/// The key id gxserver gives this app's owner key (hex of the first 16 bytes of SHA-256 over the
/// public key), learned the first time the app sends it anywhere.
static OWN_OWNER_KEY_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn remember_own_owner_key(public_key_b64: &str) {
    if OWN_OWNER_KEY_ID.get().is_some() {
        return;
    }
    if let Some(key_id) = key_id_of(public_key_b64) {
        let _ = OWN_OWNER_KEY_ID.set(key_id);
    }
}

/// gxserver's key id for a public key: hex of the first 16 bytes of SHA-256 over its bytes.
fn key_id_of(public_key_b64: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64)
        .ok()?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    Some(hex::encode(&digest.as_ref()[..16]))
}

/// Whether `request` came back from one of this app's own calls (its own click, approved where
/// it happens, or its background work, which shows its own card): never by a key's device id,
/// which two Macs share after Migration Assistant (CDXC:Security 2026-11-10 in `plain.rs`).
pub(crate) fn asked_by_this_app(request: &ApprovalRequest) -> bool {
    plain::is_own_request(&request.id)
}

/// Whether `request` asks a connected computer to let this very app act as the owner there: only
/// that computer's own owner can answer it, so this app never shows it as a card.
pub(crate) fn asks_for_this_app(request: &ApprovalRequest) -> bool {
    request.action == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
        && request.target.is_some()
        && request.target.as_deref() == OWN_OWNER_KEY_ID.get().map(String::as_str)
}

/*
CDXC:Security 2026-11-09 WHY:
Discord report (11.2.0): "I click approve, and then get this little error popup that tells me its Not approved. It tells me to do it on the phone or other computers. Nothing comes up on those computers and I dont see a way to do it there?" The request a computer waits on showed as a card only from its `approvalRequestsChanged` event, sent once when the request is made: an app that was restarting, updating or reconnecting at that moment never showed it, and nothing listed it later, so the request waited a day with no way to answer it. Each computer's waiting requests are now read when its stream goes live and shown as cards, where this app may answer them (this computer, or one where it already acts as the owner).
*/
/// The requests waiting for the owner on `transport`'s gxserver. Empty on a connected computer
/// where this app doesn't act as the owner yet: listing needs that, and it is never asked for
/// from here.
pub(crate) fn pending_requests(transport: &Transport) -> Vec<ApprovalRequest> {
    use ghostex_gx_protocol::approval::ApprovalStatus;
    in_security_flow(|| {
        match transport {
            // A gxserver that approves plainly lists them for this app's token file.
            _ if plain::is_plain(transport) => {}
            Transport::Local => {
                require_owner_session(transport)?;
            }
            // Opened here when that needs no approval: the stream usually goes live before the
            // background opener ran, which listed nothing and left requests made meanwhile with
            // no card.
            Transport::Remote(_) => {
                if quiet_owner_session(transport).is_none() {
                    return Ok(Vec::new());
                }
            }
        }
        let answer = rpc(transport, "/api/listApprovalRequests", json!({}))
            .map_err(|(_, message)| message)?;
        Ok::<_, String>(
            answer
                .get("requests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|request| {
                    serde_json::from_value::<ApprovalRequest>(request.clone()).ok()
                })
                .filter(|request| request.status == ApprovalStatus::Pending)
                .collect(),
        )
    })
    .unwrap_or_else(|error| {
        log::info!("waiting approval requests not read: {error}");
        Vec::new()
    })
}

/// gxserver's routes for the user's own actions on a session (Stop, answering its prompt, typing
/// into it): the owner's devices do them without asking, anyone else asks each time.
const STEER_ENDPOINTS: &[&str] = &[
    "/api/interruptSessionChat",
    "/api/answerSessionChatPrompt",
    "/api/sendSessionText",
    "/api/sendSessionEnter",
    "/api/sendSessionMessage",
];

/// The code of the answer [`steer_needs_owner`] gives instead of sending the call.
pub(crate) const OWNER_APPROVAL_NEEDED_ERROR: &str = "ownerApprovalNeeded";

/*
CDXC:Security 2026-11-08 WHY:
User, after Escape in the chat of a session on their connected Windows PC showed a red "Waiting for your approval … Stop what session G506f is doing … repeat this call with approvalId": "wtf is this line". The owner's own Stop, answers and typing on a connected computer's sessions pass without asking once this app holds an owner session there; this Mac had none, because Windows has its own owner and the request to let this Mac act as the owner there was never approved, so each action went as that computer's token file (agent level) and asked on its own. Now such an action first makes sure of the owner session; while it waits on that one approval, the action is not sent (no request per action) and the chat says once, in plain words, what to approve and where.
*/
/// For a Stop, prompt answer or typing on the computer `target` reaches: `None` when it may go
/// (an owner session there is open, or can be opened now), or the plain refusal to show when this
/// app is not yet approved as the owner there.
pub(crate) fn steer_needs_owner(
    target: &GpuiRemoteGxserverRequestTarget,
    endpoint: &str,
) -> Option<String> {
    if !STEER_ENDPOINTS.contains(&endpoint) || IN_SECURITY_FLOW.with(Cell::get) {
        return None;
    }
    let transport = Transport::Remote(target.clone());
    let key = transport.key();
    if with_session(&key, |session| session.token.is_some() || session.plain)
        || plain::known_approvals(&transport).is_some_and(|answer| answer.plain)
    {
        return None;
    }
    // Never block the window: the background opener (`cached_token`) tries meanwhile.
    if std::thread::current().name() != Some("main") {
        if let Ok(token) = require_owner_session(&transport) {
            with_session(&key, |session| session.token = Some(token));
            return None;
        }
    }
    let waiting = with_session(&key, |session| {
        session.waiting_approval.is_some() || session.gave_up
    });
    waiting.then(|| {
        format!(
            "Ghostex on {} isn't approved as you on the computer this session runs on yet, so it can't stop, answer or type there for you. Approve \"Let {} (Ghostex app) act as the owner\" once on that computer (Ghostex shows it as a card) or on your phone in that computer's Approvals; after that it works without asking.",
            this_computer_name(),
            this_computer_name(),
        )
    })
}

/// [`enroll_owner_key`]'s answer while that computer's own owner hasn't let this app act as the
/// owner there; the approval card replaces it with words naming both computers.
pub(crate) const OWNER_WAITING_ERROR: &str = "Another owner key is enrolled on that computer's Ghostex. Approve this app there or on your phone.";

/// What Approve on `computer`'s card says while that computer's owner hasn't let this app in.
pub(crate) fn owner_waiting_message(computer: &str) -> String {
    let me = this_computer_name();
    format!(
        "{computer} has its own owner, so Ghostex here can't answer its requests yet. In Ghostex on {computer}, or on a phone paired with {computer}, approve \"Let {me} (Ghostex app) act as the owner\" (it shows there as a card once Ghostex on {computer} is open), or answer this request there instead."
    )
}

/// `/api/enrollOwnerKey` on `transport`, waiting on the approval it asked for there instead of
/// asking again.
fn enroll_owner_key(transport: &Transport, params: Value) -> Result<Value, String> {
    const WAITING: &str = OWNER_WAITING_ERROR;
    const STOPPED: &str = "This app wasn't approved as an owner on that computer; it uses that computer's token file until Ghostex restarts.";
    let key = transport.key();
    let (waiting, gave_up) = with_session(&key, |session| {
        (session.waiting_approval.clone(), session.gave_up)
    });
    if gave_up {
        return Err(STOPPED.to_string());
    }
    let mut params = params;
    if let Some(id) = waiting {
        match request_state(transport, &id) {
            RequestState::Pending(_) | RequestState::Unknown => return Err(WAITING.to_string()),
            RequestState::Approved => {
                params = with_approval_id(&params, &id).unwrap_or(params);
            }
            // Approved and then lapsed unspent reads the same as denied, yet the key went in when
            // it was approved (CDXC:Security 2026-11-10 in server/src/security/approvals.rs): ask
            // once more without making a request, and stop only if it isn't there.
            RequestState::Declined => {
                with_session(&key, |session| session.waiting_approval = None);
                params["askOwner"] = json!(false);
                let (status, body) = raw_rpc(transport, "/api/enrollOwnerKey", &params)?;
                let envelope: Value = serde_json::from_str(&body).unwrap_or_default();
                if (200..300).contains(&status)
                    && envelope.get("ok").and_then(Value::as_bool) == Some(true)
                {
                    return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
                }
                with_session(&key, |session| session.gave_up = true);
                return Err(STOPPED.to_string());
            }
            RequestState::Gone => {}
        }
        with_session(&key, |session| session.waiting_approval = None);
    }
    let (status, body) = raw_rpc(transport, "/api/enrollOwnerKey", &params)?;
    if serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|envelope| text(&envelope, "error"))
        .as_deref()
        == Some(OWNER_APPROVAL_NEEDED_ERROR)
    {
        return Err(
            "This app isn't approved as an owner on that computer yet; it asks there once."
                .to_string(),
        );
    }
    if let Some(request) = waiting_request_in(status, &body) {
        plain::note_own_request(&request.id);
        with_session(&key, |session| {
            session.waiting_approval = Some(request.id.clone())
        });
        return Err(WAITING.to_string());
    }
    let envelope: Value =
        serde_json::from_str(&body).map_err(|_| "gxserver returned invalid JSON.".to_string())?;
    if (200..300).contains(&status) && envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
    }
    Err(text(&envelope, "message")
        .unwrap_or_else(|| format!("gxserver request failed with HTTP {status}.")))
}

/// One call on `transport`, answered as HTTP status and body (the app's own transport: it sends
/// the owner session when one is open).
fn raw_rpc(transport: &Transport, endpoint: &str, params: &Value) -> Result<(u16, String), String> {
    match transport {
        Transport::Local => {
            crate::app::helpers::gxserver_post_typed_operation(endpoint, params, CALL_TIMEOUT)
        }
        Transport::Remote(target) => {
            gpui_remote_gxserver_post_typed_operation(target, endpoint, params, CALL_TIMEOUT)
        }
    }
}

/// The owner session on `transport` when it is open or opens without asking anyone (this app's key
/// is enrolled there, or it is the first); `None` otherwise. Never makes a request.
fn quiet_owner_session(transport: &Transport) -> Option<String> {
    let key = transport.key();
    if let Some(token) = with_session(&key, |session| session.token.clone()) {
        return Some(token);
    }
    if with_session(&key, |session| session.gave_up) {
        return None;
    }
    let token = in_security_flow(|| open_owner_session(transport, false)).ok()?;
    with_session(&key, |session| session.token = Some(token.clone()));
    Some(token)
}

/// That computer let this app act as the owner there (the request asking it was approved): the
/// next call opens the owner session instead of waiting for the owner to act there first.
pub(crate) fn note_owner_key_approved(transport: &Transport) {
    with_session(&transport.key(), |session| {
        session.gave_up = false;
        session.waiting_approval = None;
        session.last_attempt = None;
    });
}

/// Who may answer a computer's requests.
pub(crate) enum Answerer {
    /// This app: Approve here signs it.
    ThisApp,
    /// Only the named devices there (Ghostex on another computer, a paired phone; empty when
    /// gxserver doesn't say). `asked`: a request waits there to let this app answer from here.
    Elsewhere { devices: Vec<String>, asked: bool },
}

/*
CDXC:Security 2026-11-10 WHY:
A Ghostex user with four computers: "Clicking approve on that gave me the error that the other machines is where that should happen." This app showed every connected computer's requests as cards with Approve, but it can only sign for a computer where it acts as the owner and its approver key is enrolled, which a computer with its own Ghostex app grants only once its owner approves it. So a connected computer's request is an approvable card only where this app can answer it; elsewhere it is a notice that names where to approve it, and this app asks that computer once (one card there) to let it answer its requests from here too.
*/
/// Whether this app can answer `transport`'s requests now, and if not, who can. Runs on a
/// background thread; may ask that computer once to let this app act as the owner there.
pub(crate) fn answerer(transport: &Transport) -> Answerer {
    if matches!(transport, Transport::Local) || plain::is_plain(transport) {
        return Answerer::ThisApp;
    }
    in_security_flow(|| {
        let own_approver = ghostex_owner_keys::approver_public_key_or_create()
            .ok()
            .and_then(|public_key| key_id_of(&public_key));
        let mut session = quiet_owner_session(transport);
        if session.is_none() && !with_session(&transport.key(), |session| session.gave_up) {
            // One request there, reused until answered (`enroll_owner_key`).
            if let Ok(token) = open_owner_session(transport, true) {
                with_session(&transport.key(), |state| state.token = Some(token.clone()));
                session = Some(token);
            }
        }
        let asked = with_session(&transport.key(), |session| {
            session.waiting_approval.is_some()
        });
        let status = rpc(transport, "/api/securityStatus", json!({})).unwrap_or_default();
        let labels = |field: &str, suffix: &str| -> Vec<String> {
            status
                .get(field)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|key| own_approver.is_none() || text(key, "keyId") != own_approver)
                .filter_map(|key| {
                    // A phone's key is labelled with whatever the phone sent ("Phone" on 11.2.1).
                    if matches!(text(key, "platform").as_deref(), Some("ios" | "android")) {
                        return Some("your phone".to_string());
                    }
                    text(key, "label")
                        .map(|label| label.trim_end_matches(suffix).trim().to_string())
                })
                .fold(Vec::new(), |mut names: Vec<String>, name| {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    names
                })
        };
        let approvers = labels("approverKeys", "(approvals)");
        let approver_ids: Vec<String> = status
            .get("approverKeys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|key| text(key, "keyId"))
            .collect();
        let elsewhere = || {
            if approvers.is_empty() {
                labels("ownerKeys", "(Ghostex app)")
            } else {
                approvers.clone()
            }
        };
        if session.is_none() {
            return Answerer::Elsewhere {
                devices: elsewhere(),
                asked,
            };
        }
        if approver_ids.is_empty()
            || own_approver
                .as_ref()
                .is_some_and(|own| approver_ids.contains(own))
        {
            return Answerer::ThisApp;
        }
        // The owner there let this app in before its approver key came along with it: ask for
        // that key once (a card there, `addApproverKey`), and send people there meanwhile.
        if let (Ok(owner_key), Ok(approver)) = (
            ghostex_owner_keys::load_or_create_owner_key(),
            ghostex_owner_keys::approver_public_key_or_create(),
        ) {
            let wire_key = key_json(&approver, &owner_key.public_key_b64(), "approvals");
            match rpc(
                transport,
                "/api/enrollApproverKey",
                json!({ "key": wire_key }),
            ) {
                Ok(_) => return Answerer::ThisApp,
                Err((code, _)) => {
                    return Answerer::Elsewhere {
                        devices: elsewhere(),
                        asked: code.as_deref() == Some(APPROVAL_REQUIRED_ERROR),
                    };
                }
            }
        }
        Answerer::Elsewhere {
            devices: elsewhere(),
            asked: false,
        }
    })
}

/// Shows the system prompt with the request's summary, signs the request with this computer's
/// approver key (enrolling it on that gxserver the first time) and approves it there.
pub(crate) fn approve_request(
    transport: &Transport,
    request: &ApprovalRequest,
) -> Result<(), String> {
    if let Some(answer) = plain::server_approvals(transport).filter(|answer| answer.plain) {
        return plain::approve_plain(transport, request, answer.require_presence);
    }
    in_security_flow(|| {
        require_owner_session(transport)?;
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
        let enrolled = rpc(
            transport,
            "/api/enrollApproverKey",
            json!({ "key": wire_key }),
        )
        .map_err(|(code, message)| match code.as_deref() {
            Some(APPROVAL_REQUIRED_ERROR) => {
                "Approve adding this computer as an approver on your phone first, then try again."
                    .to_string()
            }
            _ => message,
        })?;
        let key_id = enrolled
            .get("key")
            .and_then(|key| text(key, "keyId"))
            .ok_or("gxserver didn't return the approver key")?;
        let status =
            rpc(transport, "/api/securityStatus", json!({})).map_err(|(_, message)| message)?;
        let server_id = text(&status, "serverId").ok_or("gxserver didn't say its id")?;
        let payload =
            approval_signing_payload(&server_id, request).map_err(|error| error.to_string())?;
        let signature = key.sign_b64(&payload).map_err(|error| error.to_string())?;
        rpc(
            transport,
            "/api/approveApprovalRequest",
            json!({ "id": request.id, "keyId": key_id, "signature": signature }),
        )
        .map(|_| ())
        .map_err(|(_, message)| message)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
A card only leaves when its gxserver says the request was answered or lapsed, and a card whose word never arrived (the app was away from its event stream when it happened) stayed for good: Approve asked for Touch ID and then failed, and the agent's next try added another card beside it. Before the prompt, the card's request is read back; one no longer waiting just closes.
*/
/// Whether request `id` still waits for the owner; `true` when gxserver can't say, so the
/// decision goes ahead and reports its own error.
pub(crate) fn request_still_waiting(transport: &Transport, id: &str) -> bool {
    let plain = plain::is_plain(transport);
    in_security_flow(|| {
        if !plain {
            require_owner_session(transport)?;
        }
        match rpc(transport, "/api/readApprovalRequest", json!({ "id": id })) {
            Ok(answer) => Ok::<bool, String>(
                answer
                    .pointer("/request/status")
                    .and_then(Value::as_str)
                    .is_none_or(|status| status == "pending"),
            ),
            Err((Some(code), _)) if code == "notFound" => Ok(false),
            Err(_) => Ok(true),
        }
    })
    .unwrap_or(true)
}

/// [`approve_request`] for request `id` of this computer's gxserver (a phone or computer waiting
/// to pair, from Settings > Remote or the 11.0.1 notice): reads it, then prompts and signs.
pub(crate) fn approve_local_request_id(id: &str) -> Result<(), String> {
    let transport = Transport::Local;
    let plain = plain::is_plain(&transport);
    let request = in_security_flow(|| {
        if !plain {
            require_owner_session(&transport)?;
        }
        let answer = rpc(&transport, "/api/readApprovalRequest", json!({ "id": id }))
            .map_err(|(_, message)| message)?;
        serde_json::from_value::<ApprovalRequest>(
            answer.get("request").cloned().unwrap_or_default(),
        )
        .map_err(|_| "This request is no longer waiting.".to_string())
    })?;
    approve_request(&transport, &request)
}

/// Denies a request; no prompt, saying no only closes a door.
pub(crate) fn deny_request(transport: &Transport, id: &str) -> Result<(), String> {
    let plain = plain::is_plain(transport);
    in_security_flow(|| {
        if !plain {
            require_owner_session(transport)?;
        }
        rpc(transport, "/api/denyApprovalRequest", json!({ "id": id }))
            .map(|_| ())
            .map_err(|(_, message)| message)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
A sensitive change the user starts in the app (turning on Easy Connect, a security-relaxing setting; letting a connected computer in waits as a card instead, CDXC:Security 2026-11-08 at `without_prompts`) reaches gxserver as an owner call that still needs a signed approval. Showing the system prompt right then, with gxserver's summary, and sending the call again is what the user expects from their own click; a card would make them approve their own click twice. Never on the main thread (the prompt would freeze the window) and never inside an approval or owner-session flow already running.
*/
/// For a call of the app's own that `transport`'s gxserver answered with `approvalRequired`
/// (`body`): prompts, approves, and returns the approval id to send the call again with. `None`
/// when the answer isn't one, the user didn't approve, or this thread can't prompt.
pub(crate) fn approve_own_call(transport: &Transport, status: u16, body: &str) -> Option<String> {
    if status != 403 || IN_SECURITY_FLOW.with(Cell::get) || NO_PROMPTS.with(Cell::get) {
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
    plain::note_own_request(&request.id);
    // One system prompt at a time, whatever asks: a second call meanwhile returns its answer.
    if PROMPT_OPEN
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        log::info!("own call not prompted: another approval prompt is open");
        return None;
    }
    let approved = approve_request(transport, &request);
    PROMPT_OPEN.store(false, std::sync::atomic::Ordering::Release);
    match approved {
        Ok(()) => Some(request.id),
        Err(error) => {
            log::info!("own call not approved: {error}");
            None
        }
    }
}

/// `params` with `approvalId` set, for the call sent again after its approval.
pub(crate) fn with_approval_id(params: &Value, approval_id: &str) -> Option<Value> {
    let mut params = params.clone();
    params
        .as_object_mut()?
        .insert("approvalId".to_string(), json!(approval_id));
    Some(params)
}
