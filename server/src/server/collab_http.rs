//! Ghostex Collab's calls.
//!
//! `/api/collab` is the owner's (Settings > Collab, the chat's Share popover, the CLI, the phone):
//! - `op: "read"` (default): the choices, the relay, the guest page check (`guestPage`) and every
//!   share with its mode, expiry and viewers. Never a link or a code.
//! - `op: "set"`: `patch` holds the choices to change (`enabled`, `expiry` hour/fourHours/day,
//!   `mode` approve/view, `ownerName`). Turning Collab on needs the owner's signed approval;
//!   turning it off ends every share. `relayUrl`, `requireCode: false` and a Send default are
//!   refused.
//! - `op: "share"`: `projectId`, `sessionId`; the chat's share, made with the choices if it has
//!   none, with its `link` and `code`. Needs the owner's signed approval every time (it is also
//!   how the owner sees an existing share's link again), and a relay serving the pinned page.
//! - `op: "stop"` (`shareId`), `op: "stopAll"`, `op: "removeViewer"` (`shareId`, `viewerId`):
//!   always allowed. `op: "setMode"` (`shareId`, `mode`): to Send, or from View only to Ask me,
//!   needs the owner's signed approval; narrowing doesn't.
//! - `op: "resolve"`: `projectId`, `sessionId`, `heldId`, `action` (`send`, `edit` or `decline`) and
//!   for `send` an optional edited `text`: the owner's answer to a guest message held for approval.
//!   `send` needs the owner's signed approval.
//!
//! A call that needs an approval answers `approvalRequired` with the request (GitHub #495,
//! `ai/security-tokens-and-approvals.md`); gxserver makes the change itself once the owner
//! approves, and the same call repeated with `approvalId` answers with its result.
//!
//! `/api/collabTyping` (`projectId`, `sessionId`, `typing`) is sent by the owner's chat and a guest's
//! while they type into a shared chat.
//!
//! A guest reaches this gxserver only through the relay connector (collab_connector.rs) with its
//! share's token; [`route_guest`] answers what that token may do.

use super::*;

use crate::collab::{self, shares, GuestMode};
use crate::security::{ActionRequest, PresenceGrant, PresenceOutcome, Principal, SensitiveAction};
use ghostex_gx_protocol::approval::APPROVAL_ID_PARAM;

use super::security_http::{presence_outcome, presence_outcome_response};

pub(crate) const COLLAB_ENDPOINTS: &[&str] = &[
    "/api/collab",
    "/api/collabTyping",
    GUEST_PAGE_ENDPOINT,
    GUEST_PAGE_ANSWERS_ENDPOINT,
];

/// The header the relay connector names the guest with; only the connector holds a share's token,
/// so only it can set it.
pub(crate) const VIEWER_HEADER: &str = "x-ghostex-collab-viewer";

fn ok(endpoint_path: String, request_id: String, result: Value) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

fn fail(
    endpoint_path: String,
    request_id: String,
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> RoutedResponse {
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(code, message.into(), Some(request_id)),
    )
}

pub(crate) fn guest_refusal(endpoint_path: String, request_id: String) -> RoutedResponse {
    fail(
        endpoint_path,
        request_id,
        StatusCode::FORBIDDEN,
        "forbidden",
        "A guest can only follow and write in the chat that was shared.",
    )
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_title(state: &AppState, project_id: &str, session_id: &str) -> Option<String> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository.get_session(project_id, session_id).ok()??;
    ["title", "displayTitle", "name"].iter().find_map(|key| {
        session
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    })
}

/// A chat's title for its share, also what the guest's join page names.
pub(crate) fn share_title(state: &AppState, share: &shares::StoredShare) -> String {
    session_title(state, &share.project_id, &share.session_id)
        .unwrap_or_else(|| "A chat".to_string())
}

fn republish(state: &Arc<AppState>, share: &shares::StoredShare) {
    crate::server::republish_session_chat(state, &share.project_id, &share.session_id);
}

/// What the agent reads: the guest's words, labelled with their name so it knows they did not
/// come from the owner.
pub(crate) fn labelled_guest_text(name: &str, text: &str) -> String {
    format!("{name} (a guest in Ghostex Collab) says:\n{text}")
}

async fn deliver(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    text: &str,
    image_paths: &[String],
) -> Result<(), String> {
    let mut body = json!({
        "params": {
            "projectId": share.project_id,
            "sessionId": share.session_id,
            "text": text,
            "sendRequestId": Uuid::new_v4().to_string(),
        }
    });
    if !image_paths.is_empty() {
        body["params"]["imagePaths"] = json!(image_paths);
    }
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if routed.response.status().is_success() {
        return Ok(());
    }
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    Err(serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| "The message didn't reach the agent.".to_string()))
}

/// Clears a typing line when it runs out, without waiting for another frame to carry it.
fn clear_typing_later(state: &Arc<AppState>, share: &shares::StoredShare) {
    let state = state.clone();
    let share = share.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(shares::TYPING_MS as u64 + 150)).await;
        let now = chrono::Utc::now().timestamp_millis();
        if shares::next_typing_deadline(&share.id).is_none_or(|deadline| deadline <= now) {
            republish(&state, &share);
        }
    });
}

fn note_typing(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: Option<&str>,
    typing: bool,
) {
    if shares::note_typing(&share.id, viewer, typing) {
        republish(state, share);
    }
    if typing {
        clear_typing_later(state, share);
    }
}

pub(crate) async fn handle_collab_http(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    // Only a guest's page reads pages through the tunnel (`route_guest`); the owner opens them
    // at their own address.
    if endpoint_path == GUEST_PAGE_ENDPOINT || endpoint_path == GUEST_PAGE_ANSWERS_ENDPOINT {
        return fail(
            endpoint_path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Only a Ghostex Collab guest's page calls this.",
        );
    }
    if endpoint_path == "/api/collabTyping" {
        let (Some(project_id), Some(session_id)) =
            (text(&params, "projectId"), text(&params, "sessionId"))
        else {
            return fail(
                endpoint_path,
                request_id,
                StatusCode::BAD_REQUEST,
                "badRequest",
                "projectId and sessionId are required.",
            );
        };
        if let Some(share) = shares::for_session(&project_id, &session_id) {
            let typing = params
                .get("typing")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            note_typing(state, &share, None, typing);
        }
        return ok(endpoint_path, request_id, json!({}));
    }
    // A repeated call for an approval gxserver already acted on gets that answer back.
    if let Some(approval_id) = text(&params, APPROVAL_ID_PARAM) {
        if let Some(answer) = approvals::claim_retry(&approval_id, caller).await {
            return owner_answer(state, caller, endpoint_path, request_id, &params, answer);
        }
    }
    let answer = handle_owner(state, caller, &params).await;
    owner_answer(state, caller, endpoint_path, request_id, &params, answer)
}

fn owner_answer(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
    answer: OwnerResult,
) -> RoutedResponse {
    match answer {
        Ok(result) => ok(endpoint_path, request_id, result),
        Err(OwnerError::Presence(outcome)) => {
            if let PresenceOutcome::Pending(request) = &outcome {
                approvals::await_approval(state, &request.id, caller, params);
            }
            match presence_outcome_response(outcome, &endpoint_path, &request_id) {
                Err(response) => response,
                // A grant never comes back as an error.
                Ok(_) => fail(
                    endpoint_path,
                    request_id,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internalError",
                    "The approval was not used.",
                ),
            }
        }
        Err(OwnerError::Refused(code, message)) => fail(
            endpoint_path,
            request_id,
            match code {
                "badRequest" | "collabOff" => StatusCode::BAD_REQUEST,
                "notFound" => StatusCode::NOT_FOUND,
                "guestPageUnverified" => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            code,
            message,
        ),
    }
}

/// Why an owner call did not go through: refused outright, or waiting for (or denied) the
/// owner's signed approval.
#[derive(Clone, Debug)]
pub(crate) enum OwnerError {
    Refused(&'static str, String),
    Presence(PresenceOutcome),
}

impl From<(&'static str, String)> for OwnerError {
    fn from((code, message): (&'static str, String)) -> Self {
        Self::Refused(code, message)
    }
}

type OwnerResult = Result<Value, OwnerError>;

fn read_all(state: &Arc<AppState>) -> Value {
    let config = collab::read_config(&state.paths);
    let relay = collab::config::relay_url();
    let list: Vec<Value> = shares::list()
        .iter()
        .map(|share| shares::owner_view(share, &relay, Some(share_title(state, share))))
        .collect();
    json!({
        "config": {
            "enabled": config.enabled,
            "expiry": config.expiry.as_str(),
            "requireCode": true,
            "mode": config.mode.as_str(),
            "ownerName": config.owner_name,
            "ownerDisplayName": collab::config::owner_display_name(&config),
        },
        "relay": relay,
        "relayConnected": super::collab_connector::relay_connected(),
        "guestPage": collab::page_pin::status_json(),
        "shares": list,
    })
}

fn share_of(params: &Map<String, Value>) -> Result<shares::StoredShare, OwnerError> {
    if let Some(id) = text(params, "shareId") {
        return shares::get(&id)
            .ok_or_else(|| ("notFound", "This chat is no longer shared.".to_string()).into());
    }
    match (text(params, "projectId"), text(params, "sessionId")) {
        (Some(project_id), Some(session_id)) => shares::for_session(&project_id, &session_id)
            .ok_or_else(|| ("notFound", "This chat is no longer shared.".to_string()).into()),
        _ => Err((
            "badRequest",
            "shareId, or projectId and sessionId, is required.".to_string(),
        )
            .into()),
    }
}

fn session_exists(state: &AppState, project_id: &str, session_id: &str) -> Option<bool> {
    let db = open_gxserver_database(&state.paths).ok()?;
    DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(project_id, session_id)
        .ok()
        .map(|session| session.is_some())
}

/// The gate (`require_owner_presence`, GitHub #495) for one sensitive Collab change: the grant,
/// or the outcome to answer with. The card's words are gxserver's own, from the chat's title and
/// the stored share; `params` is exactly what the call acts on.
fn gate(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
    action: SensitiveAction,
    target: &str,
    summary: String,
    detail: String,
) -> Result<PresenceGrant, OwnerError> {
    let request = ActionRequest::new(action, summary)
        .target(target)
        .detail(detail)
        .params(Value::Object(params.clone()));
    let body = json!({ "params": params });
    match presence_outcome(state, caller, request, &body) {
        PresenceOutcome::Granted(grant) => Ok(grant),
        other => Err(OwnerError::Presence(other)),
    }
}

/// After an approved change ran: the security event the audit log and the notifications read.
fn performed(state: &AppState, caller: &Principal, grant: &PresenceGrant, ok: bool) {
    // TODO(#495 audit log): call `audit::record(action, caller, outcome, approval)` here once the
    // audit-log thread's module lands. Until then its log reads this same change from the
    // `SensitiveActionPerformed` event below (`state.security.subscribe()`).
    state.security.record_performed(
        grant.approval.action.clone(),
        grant.approval.target.clone(),
        caller,
        ok,
    );
}

fn quoted(title: &str) -> String {
    format!("\u{201c}{title}\u{201d}")
}

fn ends_in_words(ms: i64) -> String {
    match ms / 3_600_000 {
        0 | 1 => "an hour".to_string(),
        24 => "a day".to_string(),
        hours => format!("{hours} hours"),
    }
}

async fn handle_owner(
    state: &Arc<AppState>,
    caller: &Principal,
    params: &Map<String, Value>,
) -> OwnerResult {
    let op = text(params, "op").unwrap_or_else(|| "read".to_string());
    let saved = |result: std::io::Result<collab::CollabConfig>| {
        result.map_err(|error| {
            OwnerError::Refused(
                "internalError",
                format!("Ghostex couldn't save Ghostex Collab's settings: {error}"),
            )
        })
    };
    match op.as_str() {
        "read" => {}
        "set" => {
            let patch = params.get("patch").cloned().unwrap_or(Value::Null);
            let current = collab::read_config(&state.paths);
            let mut next = current.clone();
            collab::apply_patch(&mut next, &patch).map_err(|message| ("badRequest", message))?;
            // CDXC:Collab 2026-11-08 DECISION:
            // User (#495): turning Collab on needs the biometric/PIN approval; turning it off, and the defaults that can only narrow a share (an hour to a day, Ask me or View only), never do.
            let grant = if next.enabled && !current.enabled {
                Some(gate(
                    state,
                    caller,
                    params,
                    SensitiveAction::CollabEnable,
                    "collab",
                    "Turn on Ghostex Collab".to_string(),
                    "Chats can then be shared through links. Each share, and each guest message, still needs your approval.".to_string(),
                )?)
            } else {
                None
            };
            let next = saved(collab::update_config(&state.paths, |config| {
                let _ = collab::apply_patch(config, &patch);
            }))?;
            if let Some(grant) = &grant {
                performed(state, caller, grant, true);
            }
            if !next.enabled {
                for share in shares::stop_all() {
                    republish(state, &share);
                }
            }
            for share in shares::list() {
                republish(state, &share);
            }
            collab::notify_changed();
        }
        "share" => {
            let config = collab::read_config(&state.paths);
            if !config.enabled {
                return Err((
                    "collabOff",
                    "Turn on Ghostex Collab in Settings > Collab to share a chat.".to_string(),
                )
                    .into());
            }
            let (Some(project_id), Some(session_id)) =
                (text(params, "projectId"), text(params, "sessionId"))
            else {
                return Err((
                    "badRequest",
                    "projectId and sessionId are required.".to_string(),
                )
                    .into());
            };
            if session_exists(state, &project_id, &session_id) != Some(true) {
                return Err(("notFound", "That chat no longer exists.".to_string()).into());
            }
            // Checked before the approval is asked for, so a paused relay costs no touch.
            let relay = collab::config::relay_url();
            if !collab::page_pin::ensure_verified(&relay).await {
                let message = collab::page_pin::status_json()["message"]
                    .as_str()
                    .unwrap_or("The share relay's guest page couldn't be checked.")
                    .to_string();
                return Err(("guestPageUnverified", message).into());
            }
            let title = session_title(state, &project_id, &session_id)
                .unwrap_or_else(|| "A chat".to_string());
            let existing = shares::for_session(&project_id, &session_id);
            // CDXC:Collab 2026-11-08 DECISION:
            // User (#495): reading a share's link and code goes through the gate too, so it is the same approval as sharing: no read returns them, only the answer to an approved `share`.
            let (summary, detail) = match &existing {
                Some(share) => (
                    format!("Show the link and code for {}", quoted(&title)),
                    format!(
                        "Whoever gets them can join the shared chat until {}.",
                        chrono::DateTime::from_timestamp_millis(share.expires_at_ms.unwrap_or(0))
                            .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
                            .unwrap_or_else(|| "it ends".to_string())
                    ),
                ),
                None => (
                    format!("Share {} through a link", quoted(&title)),
                    format!(
                        "Anyone with the link and its code can follow this chat for {}{}.",
                        ends_in_words(config.expiry.millis()),
                        if config.mode == GuestMode::View {
                            ", without writing"
                        } else {
                            " and write messages that wait for your approval"
                        }
                    ),
                ),
            };
            let grant = gate(
                state,
                caller,
                params,
                SensitiveAction::CollabShareChat,
                &session_id,
                summary,
                detail,
            )?;
            let share = shares::share_session(&config, &project_id, &session_id);
            performed(state, caller, &grant, true);
            republish(state, &share);
            collab::notify_changed();
            let mut view = shares::owner_view(&share, &relay, Some(title));
            if let (Some(view), Value::Object(secrets)) =
                (view.as_object_mut(), shares::owner_secrets(&share, &relay))
            {
                view.extend(secrets);
            }
            return Ok(json!({ "share": view }));
        }
        "stop" => {
            let share = share_of(params)?;
            shares::stop(&share.id);
            republish(state, &share);
            collab::notify_changed();
        }
        "stopAll" => {
            for share in shares::stop_all() {
                republish(state, &share);
            }
            collab::notify_changed();
        }
        "setMode" => {
            let share = share_of(params)?;
            let mode = text(params, "mode")
                .as_deref()
                .and_then(GuestMode::parse)
                .ok_or(("badRequest", "mode is send, approve or view.".to_string()))?;
            let title = share_title(state, &share);
            // Narrowing (to Ask me, or View only) is always allowed; letting guests write more
            // needs the approval, Send most of all.
            let grant = match (share.mode, mode) {
                (from, to) if from == to => None,
                (_, GuestMode::Send) => Some(gate(
                    state,
                    caller,
                    params,
                    SensitiveAction::CollabEnableSendMode,
                    &share.session_id,
                    format!(
                        "Let guests of {} send messages straight to its agent",
                        quoted(&title)
                    ),
                    "Their messages reach the agent without your approval, and the agent may act on them with your tools and logins.".to_string(),
                )?),
                (GuestMode::View, GuestMode::Approve) => Some(gate(
                    state,
                    caller,
                    params,
                    SensitiveAction::CollabChangeShare,
                    &share.session_id,
                    format!("Let guests of {} write messages", quoted(&title)),
                    "Each message waits for your approval before it reaches the agent."
                        .to_string(),
                )?),
                _ => None,
            };
            shares::set_mode(&share.id, mode);
            if let Some(grant) = &grant {
                performed(state, caller, grant, true);
            }
            republish(state, &share);
        }
        "removeViewer" => {
            let share = share_of(params)?;
            let viewer = text(params, "viewerId").unwrap_or_default();
            // CDXC:Collab 2026-11-07 DECISION: the coordinator, for the user: "a block list so a removed guest can't rejoin": Remove keeps that guest's browser out of the share for good (`block: false` lets them back).
            let block = params.get("block").and_then(Value::as_bool).unwrap_or(true);
            if let Some(removed) = shares::remove_viewer(&share.id, &viewer, block) {
                collab::audit::record(
                    &share,
                    &share_title(state, &share),
                    "removed",
                    &removed.name,
                    None,
                    0,
                    None,
                );
            }
            republish(state, &share);
            collab::notify_changed();
        }
        "audit" => {
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(100)
                .min(1_000) as usize;
            return Ok(json!({ "entries": collab::audit::read_recent(limit) }));
        }
        "resolve" => {
            let share = share_of(params)?;
            let held_id = text(params, "heldId").unwrap_or_default();
            let action = text(params, "action").unwrap_or_default();
            let outcome = match action.as_str() {
                "send" => "sent",
                "edit" => "edited",
                _ => "declined",
            };
            let title = share_title(state, &share);
            // CDXC:Collab 2026-11-08 DECISION:
            // User (#495): approving a guest's message is a sensitive action ("approve guest messages" needs the biometric/PIN approval), so an agent holding a token can't wave a stranger's words through. Edit and Decline never send anything and stay free.
            let grant = if action == "send" {
                let held = shares::peek_held(&share.id, &held_id)
                    .ok_or(("notFound", "That message was already answered.".to_string()))?;
                let words = text(params, "text").unwrap_or_else(|| held.text.clone());
                let (shown, _) = collab::split_image_references(&words);
                let shown: String = shown.chars().take(600).collect();
                let pictures =
                    held.image_paths.len() + collab::split_image_references(&words).1.len();
                Some(gate(
                    state,
                    caller,
                    params,
                    SensitiveAction::CollabApproveGuestMessage,
                    &held_id,
                    format!("Send {}\u{2019}s message to {}", held.name, quoted(&title)),
                    format!(
                        "{}{}{}",
                        if words == held.text {
                            ""
                        } else {
                            "Edited before sending. "
                        },
                        quoted(&shown),
                        match pictures {
                            0 => String::new(),
                            1 => " With 1 picture.".to_string(),
                            count => format!(" With {count} pictures."),
                        }
                    ),
                )?)
            } else {
                None
            };
            let held = shares::take_held(&share.id, &held_id, outcome)
                .ok_or(("notFound", "That message was already answered.".to_string()))?;
            republish(state, &share);
            let audit = |outcome: &str| {
                collab::audit::record(
                    &share,
                    &title,
                    "message",
                    &held.name,
                    Some(&held.text),
                    held.image_paths.len(),
                    Some(outcome),
                );
            };
            return match action.as_str() {
                "send" => {
                    let words = text(params, "text").unwrap_or_else(|| held.text.clone());
                    let sent = deliver(
                        state,
                        &share,
                        &labelled_guest_text(&held.name, &words),
                        &held.image_paths,
                    )
                    .await;
                    if let Some(grant) = &grant {
                        performed(state, caller, grant, sent.is_ok());
                    }
                    sent.map_err(|message| OwnerError::Refused("sendFailed", message))?;
                    audit("approved");
                    Ok(json!({ "sent": true }))
                }
                "edit" => {
                    audit("edited");
                    Ok(json!({ "composerText": labelled_guest_text(&held.name, &held.text) }))
                }
                _ => {
                    audit("declined");
                    Ok(json!({ "declined": true }))
                }
            };
        }
        other => return Err(("badRequest", format!("Unknown op {other}.")).into()),
    }
    Ok(read_all(state))
}

/// Approvals Collab asked for, finished by gxserver once the owner signs them.
///
/// CDXC:Collab 2026-11-08 WHY:
/// The gate's contract has the caller repeat its call with `approvalId`. The chat's held-message card (desktop, web and phone, one chat core), the phone's sheet and an agent's CLI call would each need that wait-and-repeat loop; instead gxserver repeats the call itself, as that same caller, the moment the owner's signed approval arrives, so approving on the Mac or the phone is what makes the change. A caller that does repeat its call gets the same answer back (`claim_retry`), once the change ran, and never a second request.
pub(crate) mod approvals {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock, PoisonError};

    /// How long a finished answer waits for its caller to repeat the call.
    const KEEP_ANSWER_MS: u64 = 10 * 60_000;

    #[derive(Default)]
    struct Book {
        /// Pending approvals, with the call to repeat once approved.
        waiting: HashMap<String, (Principal, Map<String, Value>)>,
        /// Approved calls gxserver is running now.
        running: HashMap<String, Principal>,
        /// Calls gxserver ran, with their answer.
        done: HashMap<String, (Principal, OwnerResult, u64)>,
    }

    fn book() -> &'static Mutex<Book> {
        static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
        BOOK.get_or_init(Mutex::default)
    }

    fn with_book<T>(change: impl FnOnce(&mut Book) -> T) -> T {
        let mut book = book().lock().unwrap_or_else(PoisonError::into_inner);
        let now = crate::security::now_ms();
        book.done
            .retain(|_, (_, _, at)| now.saturating_sub(*at) < KEEP_ANSWER_MS);
        change(&mut book)
    }

    fn same_caller(asked: &Principal, caller: &Principal) -> bool {
        asked == caller || caller.is_owner_device()
    }

    pub(crate) fn await_approval(
        _state: &AppState,
        approval_id: &str,
        caller: &Principal,
        params: &Map<String, Value>,
    ) {
        let mut params = params.clone();
        params.remove(APPROVAL_ID_PARAM);
        with_book(|book| {
            book.waiting
                .insert(approval_id.to_string(), (caller.clone(), params));
        });
    }

    /// A caller repeating its call with `approvalId`: the answer gxserver already got, waiting a
    /// moment if it is running it now. `None` lets the call go through the gate as usual (the
    /// caller then spends the approval itself, and gxserver won't).
    pub(crate) async fn claim_retry(approval_id: &str, caller: &Principal) -> Option<OwnerResult> {
        for _ in 0..100 {
            let found = with_book(|book| {
                if let Some((asked, answer, _)) = book.done.get(approval_id) {
                    return Some(same_caller(asked, caller).then(|| answer.clone()));
                }
                if book.running.contains_key(approval_id) {
                    return None;
                }
                // Still pending, or ours to spend: the call goes through the gate, which answers
                // Pending again (and is awaited again) or spends the approval for this caller.
                book.waiting.remove(approval_id);
                Some(None)
            });
            match found {
                Some(answer) => return answer,
                None => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
        None
    }

    /// The answer stored for `approval_id`, for tests.
    #[cfg(test)]
    pub(crate) fn done_answer(approval_id: &str) -> Option<OwnerResult> {
        with_book(|book| {
            book.done
                .get(approval_id)
                .map(|(_, answer, _)| answer.clone())
        })
    }

    /// Runs one approved call as the caller that asked; `false` when it wasn't awaited here (or a
    /// repeated call took it first).
    pub(crate) async fn finish(state: &Arc<AppState>, approval_id: &str) -> bool {
        let Some((caller, mut params)) = with_book(|book| {
            let entry = book.waiting.remove(approval_id)?;
            book.running
                .insert(approval_id.to_string(), entry.0.clone());
            Some(entry)
        }) else {
            return false;
        };
        params.insert(APPROVAL_ID_PARAM.to_string(), json!(approval_id));
        let answer = Box::pin(handle_owner(state, &caller, &params)).await;
        if let Err(error) = &answer {
            eprintln!("[collab] the approved change {approval_id} did not go through: {error:?}");
        }
        with_book(|book| {
            book.running.remove(approval_id);
            book.done.insert(
                approval_id.to_string(),
                (caller, answer, crate::security::now_ms()),
            );
        });
        true
    }

    fn forget(approval_id: &str) {
        with_book(|book| {
            book.waiting.remove(approval_id);
        });
    }

    /// Follows the approval stream for the life of gxserver.
    pub(crate) fn start(state: Arc<AppState>) {
        let mut events = state.security.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(crate::security::SecurityEvent::ApprovalApproved { request, .. }) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            finish(&state, &request.id).await;
                        });
                    }
                    Ok(
                        crate::security::SecurityEvent::ApprovalDenied { request, .. }
                        | crate::security::SecurityEvent::ApprovalExpired { request },
                    ) => forget(&request.id),
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

/// A guest joined a shared chat: a row in the owner's notifications (the Mac's and the phone's),
/// beside the chat's live guest list in the Share popover.
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (#495): "Notify when a guest joins". The row is filed under the shared chat, so it opens it, but never replaces that chat's unread rows (an approval waiting there must stay).
pub(crate) fn notify_guest_joined(state: &AppState, share: &shares::StoredShare, guest: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(&share.project_id, &share.session_id) else {
        return;
    };
    let title = share_title(state, share);
    let subtitle = repository
        .get_project(&share.project_id)
        .ok()
        .flatten()
        .and_then(|project| project["name"].as_str().map(str::to_string))
        .unwrap_or_default();
    let row = crate::notification_feed::store::NewNotificationFeedRow {
        project_id: share.project_id.clone(),
        session_id: share.session_id.clone(),
        kind: crate::notification_feed::store::NOTIFICATION_FEED_KIND_CUSTOM,
        title: format!("{guest} joined {}", quoted(&title)),
        subtitle,
        body: "A guest is following this shared chat. Open Share to see who is watching or remove them."
            .to_string(),
        agent_name: crate::notification_feed::store::notification_agent_icon(&session),
        attention_event_id: None,
    };
    if crate::notification_feed::store::insert_notification_feed_row_beside(&db, row).is_ok() {
        crate::notification_feed::api::broadcast_notification_feed_changed(state);
    }
}

/// Ends the shares whose chat is gone from the database (deleted while gxserver was down, or by
/// a path that didn't end its share). `true` when one ended.
pub(crate) fn end_shares_of_missing_chats(state: &AppState) -> bool {
    let mut ended = false;
    for share in shares::list() {
        if session_exists(state, &share.project_id, &share.session_id) == Some(false) {
            end_share_of_deleted_chat(&share.project_id, &share.session_id);
            ended = true;
        }
    }
    ended
}

/// A deleted chat's share ends with it: its link is dead and its guests are dropped.
pub(crate) fn end_share_of_deleted_chat(project_id: &str, session_id: &str) {
    if let Some(share) = shares::stop_for_session(project_id, session_id) {
        collab::audit::record(&share, "A deleted chat", "ended", "", None, 0, None);
        collab::notify_changed();
    }
}

/// What a share token gets. `None` lets the request through to its normal route (a read of the
/// shared chat); `Some` is the whole answer.
pub(crate) async fn route_guest(
    state: &Arc<AppState>,
    share: shares::StoredShare,
    headers: &HeaderMap,
    endpoint_path: &str,
    request_id: String,
    body: &Value,
) -> Option<RoutedResponse> {
    let path = endpoint_path.to_string();
    if !collab::read_config(&state.paths).enabled {
        return Some(fail(
            path,
            request_id,
            StatusCode::FORBIDDEN,
            "collabEnded",
            "This link has ended.",
        ));
    }
    let params = read_domain_rpc_params(body).unwrap_or_default();
    if text(&params, "projectId").as_deref() != Some(share.project_id.as_str())
        || text(&params, "sessionId").as_deref() != Some(share.session_id.as_str())
    {
        return Some(guest_refusal(path, request_id));
    }
    let viewer = headers
        .get(VIEWER_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|id| shares::viewer(&share.id, id));
    let Some(viewer) = viewer else {
        return Some(guest_refusal(path, request_id));
    };
    match endpoint_path {
        "/api/readSessionChat" | "/api/readSessionChatResource" => None,
        "/api/readSessionChatImage" => {
            let wanted = text(&params, "path").unwrap_or_default();
            let shown = shares::guest_uploaded(&share.id, &wanted)
                || crate::session_files_links::session_shows_path(
                    state,
                    &share.project_id,
                    &share.session_id,
                    &wanted,
                )
                .await
                || crate::session_files_links::session_read_image(
                    state,
                    &share.project_id,
                    &share.session_id,
                    &wanted,
                )
                .await;
            if shown {
                None
            } else {
                Some(guest_refusal(path, request_id))
            }
        }
        "/api/saveSessionChatImage" => {
            Some(guest_image(state, &share, &viewer, body, path, request_id).await)
        }
        "/api/setSessionChatDraft" => {
            let typing = params
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| !content.trim().is_empty());
            note_typing(state, &share, Some(&viewer.id), typing);
            Some(ok(path, request_id, json!({})))
        }
        "/api/collabTyping" => {
            let typing = params
                .get("typing")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            note_typing(state, &share, Some(&viewer.id), typing);
            Some(ok(path, request_id, json!({})))
        }
        "/api/sendSessionChatMessage" => {
            Some(guest_message(state, &share, &viewer, &params, path, request_id).await)
        }
        GUEST_PAGE_ENDPOINT => Some(guest_page(state, &share, &params, path, request_id).await),
        GUEST_PAGE_ANSWERS_ENDPOINT => {
            Some(guest_page_answers(state, &share, &viewer, &params, path, request_id).await)
        }
        _ => Some(guest_refusal(path, request_id)),
    }
}

async fn guest_message(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: &shares::Viewer,
    params: &Map<String, Value>,
    path: String,
    request_id: String,
) -> RoutedResponse {
    let owner = collab::config::owner_display_name(&collab::read_config(&state.paths));
    if share.mode == GuestMode::View {
        return fail(
            path,
            request_id,
            StatusCode::FORBIDDEN,
            "collabViewOnly",
            format!("{owner} shared this chat to watch only."),
        );
    }
    // Only pictures this guest uploaded to this share go with their message.
    let images: Vec<String> = params
        .get("imagePaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|image| image.as_str().map(str::to_string))
        .collect();
    if images.iter().any(|image| !viewer.images.contains(image)) {
        return guest_refusal(path, request_id);
    }
    // A picture reference in the words (`[Image #1](/path)`, what the chat box writes for a paste)
    // reaches the agent as that file, so it too may only name this guest's own uploads.
    let words_raw = text(params, "text").unwrap_or_default();
    if referenced_image_paths(&words_raw)
        .iter()
        .any(|image| !viewer.images.contains(image))
    {
        return guest_refusal(path, request_id);
    }
    if params.get("key").is_some() || params.get("interrupt").and_then(Value::as_bool) == Some(true)
    {
        return guest_refusal(path, request_id);
    }
    let words: String = text(params, "text")
        .unwrap_or_default()
        .chars()
        .take(8_000)
        .collect();
    if words.is_empty() && images.is_empty() {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Write a message first.",
        );
    }
    send_guest_words(state, share, viewer, words, images, path, request_id).await
}

/// A guest's checked words (a message, or a page's answers) on their way to the chat: held for
/// the owner's approval in Approve mode, sent at once in Send mode.
async fn send_guest_words(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: &shares::Viewer,
    words: String,
    images: Vec<String>,
    path: String,
    request_id: String,
) -> RoutedResponse {
    let owner = collab::config::owner_display_name(&collab::read_config(&state.paths));
    let title = share_title(state, share);
    let refused = |message: String| {
        collab::audit::record(
            share,
            &title,
            "message",
            &viewer.name,
            Some(&words),
            images.len(),
            Some("refused"),
        );
        message
    };
    if share.mode == GuestMode::Approve && shares::held_count(&share.id) >= shares::MAX_HELD {
        let message = refused(format!(
            "{owner} has too many messages waiting. Try again once they have answered."
        ));
        return fail(
            path,
            request_id,
            StatusCode::TOO_MANY_REQUESTS,
            "collabBusy",
            message,
        );
    }
    if !shares::allow_message(&share.id, &viewer.id) {
        let message =
            refused("You're sending messages too fast. Wait a moment and try again.".to_string());
        return fail(
            path,
            request_id,
            StatusCode::TOO_MANY_REQUESTS,
            "collabRateLimited",
            message,
        );
    }
    shares::note_typing(&share.id, Some(&viewer.id), false);
    match share.mode {
        GuestMode::Approve => {
            let held = shares::hold(&share.id, viewer, &words, &images);
            collab::audit::record(
                share,
                &title,
                "message",
                &viewer.name,
                Some(&words),
                images.len(),
                Some("held"),
            );
            republish(state, share);
            ok(
                path,
                request_id,
                json!({ "collab": { "held": true, "heldId": held.id } }),
            )
        }
        _ => {
            republish(state, share);
            match deliver(
                state,
                share,
                &labelled_guest_text(&viewer.name, &words),
                &images,
            )
            .await
            {
                Ok(()) => {
                    collab::audit::record(
                        share,
                        &title,
                        "message",
                        &viewer.name,
                        Some(&words),
                        images.len(),
                        Some("sent"),
                    );
                    ok(path, request_id, json!({ "collab": { "held": false } }))
                }
                Err(message) => fail(
                    path,
                    request_id,
                    StatusCode::CONFLICT,
                    "sendFailed",
                    message,
                ),
            }
        }
    }
}

/// `{projectId, sessionId, pageId}` or `{projectId, sessionId, path}`: a Ghostex page the shared
/// chat published, or an HTML file it links to, as `{html, title, pageId, answerable}` for the
/// guest's page viewer (visual_pages/guest.rs).
pub(crate) const GUEST_PAGE_ENDPOINT: &str = "/api/collabGuestPage";
/// `{projectId, sessionId, pageId, answers}`: a guest's answers to a page, sent to the chat as the
/// guest's message (held for the owner in Approve mode).
pub(crate) const GUEST_PAGE_ANSWERS_ENDPOINT: &str = "/api/collabGuestPageAnswers";
/// The longest page answers a guest may send; a page's own answers may be up to 64 KiB.
const MAX_GUEST_PAGE_ANSWERS_CHARS: usize = 16_000;

async fn guest_page(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    params: &Map<String, Value>,
    path: String,
    request_id: String,
) -> RoutedResponse {
    let state_dir = state.paths.root_dir.clone();
    let (project_id, session_id) = (share.project_id.clone(), share.session_id.clone());
    let page = if let Some(page_id) = text(params, "pageId") {
        tokio::task::spawn_blocking(move || {
            Ok(crate::visual_pages::guest_page(
                &state_dir,
                &page_id,
                &project_id,
                &session_id,
            ))
        })
        .await
    } else if let Some(file) = text(params, "path") {
        if !crate::session_files_links::session_shows_path(
            state,
            &share.project_id,
            &share.session_id,
            &file,
        )
        .await
        {
            return guest_refusal(path, request_id);
        }
        tokio::task::spawn_blocking(move || {
            crate::visual_pages::guest_file_page(
                &state_dir,
                std::path::Path::new(&file),
                &project_id,
                &session_id,
            )
        })
        .await
    } else {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "pageId or path is required.",
        );
    };
    match page {
        Ok(Ok(Some(page))) => ok(
            path,
            request_id,
            json!({
                "html": page.html,
                "title": page.title,
                "pageId": page.page_id,
                "answerable": page.answerable,
            }),
        ),
        Ok(Ok(None)) => fail(
            path,
            request_id,
            StatusCode::NOT_FOUND,
            "notFound",
            "This page can't be opened here.",
        ),
        Ok(Err(message)) => fail(
            path,
            request_id,
            StatusCode::UNPROCESSABLE_ENTITY,
            "pageUnavailable",
            message,
        ),
        Err(_) => fail(
            path,
            request_id,
            StatusCode::INTERNAL_SERVER_ERROR,
            "pageUnavailable",
            "The page could not be read.",
        ),
    }
}

async fn guest_page_answers(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: &shares::Viewer,
    params: &Map<String, Value>,
    path: String,
    request_id: String,
) -> RoutedResponse {
    if share.mode == GuestMode::View {
        let owner = collab::config::owner_display_name(&collab::read_config(&state.paths));
        return fail(
            path,
            request_id,
            StatusCode::FORBIDDEN,
            "collabViewOnly",
            format!("{owner} shared this chat to watch only."),
        );
    }
    let Some(page_id) = text(params, "pageId") else {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "pageId is required.",
        );
    };
    let answers = params.get("answers").cloned().unwrap_or(Value::Null);
    let message = crate::visual_pages::guest_page_answers_message(
        &state.paths.root_dir,
        &page_id,
        &share.project_id,
        &share.session_id,
        &answers,
    );
    let message = match message {
        Ok(message) => message,
        Err(reason) => {
            return fail(
                path,
                request_id,
                StatusCode::CONFLICT,
                "pageNotAnswerable",
                reason,
            )
        }
    };
    // A picture pasted into a page's note is the guest's own upload; as in a message, the answers
    // may name no other file.
    if referenced_image_paths(&message)
        .iter()
        .any(|image| !viewer.images.contains(image))
    {
        return guest_refusal(path, request_id);
    }
    if message.chars().count() > MAX_GUEST_PAGE_ANSWERS_CHARS {
        return fail(
            path,
            request_id,
            StatusCode::PAYLOAD_TOO_LARGE,
            "payloadTooLarge",
            "These answers are too long to send to a shared chat. Shorten the notes and try again.",
        );
    }
    send_guest_words(state, share, viewer, message, Vec::new(), path, request_id).await
}

/// Largest picture a guest may upload (base64 in the request, about 10 MB of image).
const MAX_GUEST_IMAGE_BASE64: usize = 14 * 1024 * 1024;

/// A guest's pasted picture: saved like the owner's own (`saveSessionChatImage`), and remembered
/// as theirs, so they may read it back and send it with a message.
async fn guest_image(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: &shares::Viewer,
    body: &Value,
    path: String,
    request_id: String,
) -> RoutedResponse {
    let size = body["params"]["base64Data"].as_str().map_or(0, str::len);
    if size > MAX_GUEST_IMAGE_BASE64 {
        return fail(
            path,
            request_id,
            StatusCode::PAYLOAD_TOO_LARGE,
            "payloadTooLarge",
            "Pictures sent to a shared chat can be up to 10 MB.",
        );
    }
    if viewer.images.len() >= shares::MAX_GUEST_IMAGES {
        return fail(
            path,
            request_id,
            StatusCode::TOO_MANY_REQUESTS,
            "collabRateLimited",
            "You've sent as many pictures as this chat takes.",
        );
    }
    let routed = crate::session_chat_files::handle_save_session_chat_image_http(
        state,
        path.clone(),
        request_id.clone(),
        body,
    );
    let status = routed.response.status();
    let bytes = to_bytes(routed.response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    let answer: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if let Some(saved) = answer["result"]["path"].as_str() {
        shares::note_guest_image(&share.id, &viewer.id, saved);
    }
    routed_json(Some(path), status, answer)
}

fn referenced_image_paths(text: &str) -> Vec<String> {
    collab::split_image_references(text).1
}
