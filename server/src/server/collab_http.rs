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

pub(crate) const COLLAB_ENDPOINTS: &[&str] = &["/api/collab", "/api/collabTyping"];

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
) -> Result<(), String> {
    let body = json!({
        "params": {
            "projectId": share.project_id,
            "sessionId": share.session_id,
            "text": text,
            "sendRequestId": Uuid::new_v4().to_string(),
        }
    });
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
            shares::leave(&share.id, &viewer);
            republish(state, &share);
            collab::notify_changed();
        }
        "resolve" => {
            let share = share_of(params)?;
            let held_id = text(params, "heldId").unwrap_or_default();
            let action = text(params, "action").unwrap_or_default();
            let held = shares::take_held(&share.id, &held_id)
                .ok_or(("notFound", "That message was already answered.".to_string()))?;
            republish(state, &share);
            return match action.as_str() {
                "send" => {
                    let words = text(params, "text").unwrap_or_else(|| held.text.clone());
                    deliver(state, &share, &labelled_guest_text(&held.name, &words))
                        .await
                        .map_err(|message| ("sendFailed", message))?;
                    Ok(json!({ "sent": true }))
                }
                "edit" => {
                    Ok(json!({ "composerText": labelled_guest_text(&held.name, &held.text) }))
                }
                _ => Ok(json!({ "declined": true })),
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
        "/api/readSessionChat" => None,
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
    if params
        .get("imagePaths")
        .and_then(Value::as_array)
        .is_some_and(|images| !images.is_empty())
    {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "collabNoImages",
            "Images can't be sent to a shared chat yet.",
        );
    }
    if params.get("key").is_some() || params.get("interrupt").and_then(Value::as_bool) == Some(true)
    {
        return guest_refusal(path, request_id);
    }
    let Some(words) = text(params, "text") else {
        return fail(
            path,
            request_id,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "Write a message first.",
        );
    };
    let words: String = words.chars().take(8_000).collect();
    shares::note_typing(&share.id, Some(&viewer.id), false);
    match share.mode {
        GuestMode::Approve => {
            let held = shares::hold(&share.id, viewer, &words);
            republish(state, share);
            ok(
                path,
                request_id,
                json!({ "collab": { "held": true, "heldId": held.id } }),
            )
        }
        _ => {
            republish(state, share);
            match deliver(state, share, &labelled_guest_text(&viewer.name, &words)).await {
                Ok(()) => ok(path, request_id, json!({ "collab": { "held": false } })),
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
