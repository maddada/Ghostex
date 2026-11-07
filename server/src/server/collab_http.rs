//! Ghostex Collab's calls.
//!
//! `/api/collab` is the owner's (Settings > Collab, the chat's Share popover, the CLI, the phone):
//! - `op: "read"` (default): the choices, the relay and every share with its link, code, mode and
//!   viewers.
//! - `op: "set"`: `patch` holds the choices to change (`enabled`, `expiry`, `requireCode`, `mode`,
//!   `ownerName`). Turning Ghostex Collab off ends every share.
//! - `op: "share"`: `projectId`, `sessionId`; the chat's share, made with the choices if it has none.
//! - `op: "stop"` (`shareId`), `op: "stopAll"`, `op: "setMode"` (`shareId`, `mode`),
//!   `op: "removeViewer"` (`shareId`, `viewerId`).
//! - `op: "resolve"`: `projectId`, `sessionId`, `heldId`, `action` (`send`, `edit` or `decline`) and
//!   for `send` an optional edited `text`: the owner's answer to a guest message held for approval.
//!
//! `/api/collabTyping` (`projectId`, `sessionId`, `typing`) is sent by the owner's chat and a guest's
//! while they type into a shared chat.
//!
//! A guest reaches this gxserver only through the relay connector (collab_connector.rs) with its
//! share's token; [`route_guest`] answers what that token may do.

use super::*;

use crate::collab::{self, shares, GuestMode};

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

fn session_title(state: &Arc<AppState>, project_id: &str, session_id: &str) -> Option<String> {
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
pub(crate) fn share_title(state: &Arc<AppState>, share: &shares::StoredShare) -> String {
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
    match handle_owner(state, &params).await {
        Ok(result) => ok(endpoint_path, request_id, result),
        Err((code, message)) => fail(
            endpoint_path,
            request_id,
            if code == "badRequest" || code == "collabOff" {
                StatusCode::BAD_REQUEST
            } else if code == "notFound" {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            code,
            message,
        ),
    }
}

type OwnerResult = Result<Value, (&'static str, String)>;

fn read_all(state: &Arc<AppState>) -> Value {
    let config = collab::read_config(&state.paths);
    let relay = collab::config::relay_url(&config);
    let list: Vec<Value> = shares::list()
        .iter()
        .map(|share| shares::owner_view(share, &relay, Some(share_title(state, share))))
        .collect();
    json!({
        "config": {
            "enabled": config.enabled,
            "expiry": config.expiry.as_str(),
            "requireCode": config.require_code,
            "mode": config.mode.as_str(),
            "ownerName": config.owner_name,
            "ownerDisplayName": collab::config::owner_display_name(&config),
        },
        "relay": relay,
        "relayConnected": super::collab_connector::relay_connected(),
        "shares": list,
    })
}

fn share_of(params: &Map<String, Value>) -> Result<shares::StoredShare, (&'static str, String)> {
    if let Some(id) = text(params, "shareId") {
        return shares::get(&id).ok_or(("notFound", "This chat is no longer shared.".to_string()));
    }
    match (text(params, "projectId"), text(params, "sessionId")) {
        (Some(project_id), Some(session_id)) => shares::for_session(&project_id, &session_id)
            .ok_or(("notFound", "This chat is no longer shared.".to_string())),
        _ => Err((
            "badRequest",
            "shareId, or projectId and sessionId, is required.".to_string(),
        )),
    }
}

async fn handle_owner(state: &Arc<AppState>, params: &Map<String, Value>) -> OwnerResult {
    let op = text(params, "op").unwrap_or_else(|| "read".to_string());
    let saved = |result: std::io::Result<collab::CollabConfig>| {
        result.map_err(|error| {
            (
                "internalError",
                format!("Ghostex couldn't save Ghostex Collab's settings: {error}"),
            )
        })
    };
    match op.as_str() {
        "read" => {}
        "set" => {
            let patch = params.get("patch").cloned().unwrap_or(Value::Null);
            let next = saved(collab::update_config(&state.paths, |config| {
                collab::apply_patch(config, &patch)
            }))?;
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
                ));
            }
            let (Some(project_id), Some(session_id)) =
                (text(params, "projectId"), text(params, "sessionId"))
            else {
                return Err((
                    "badRequest",
                    "projectId and sessionId are required.".to_string(),
                ));
            };
            if session_title(state, &project_id, &session_id).is_none()
                && open_gxserver_database(&state.paths)
                    .ok()
                    .and_then(|db| {
                        DomainRepository::new(&db, state.metadata.server_id.as_str())
                            .get_session(&project_id, &session_id)
                            .ok()
                            .flatten()
                    })
                    .is_none()
            {
                return Err(("notFound", "That chat no longer exists.".to_string()));
            }
            let share = shares::share_session(&config, &project_id, &session_id);
            republish(state, &share);
            collab::notify_changed();
            let relay = collab::config::relay_url(&config);
            return Ok(
                json!({ "share": shares::owner_view(&share, &relay, Some(share_title(state, &share))) }),
            );
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
            shares::set_mode(&share.id, mode);
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
            let held = shares::take_held(&share.id, &held_id)
                .ok_or(("notFound", "That message was already answered.".to_string()))?;
            republish(state, &share);
            let title = share_title(state, &share);
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
                    deliver(
                        state,
                        &share,
                        &labelled_guest_text(&held.name, &words),
                        &held.image_paths,
                    )
                    .await
                    .map_err(|message| ("sendFailed", message))?;
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
        other => return Err(("badRequest", format!("Unknown op {other}."))),
    }
    Ok(read_all(state))
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
