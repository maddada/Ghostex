//! What a guest writes, answered here in the component, never by gxserver with a share token:
//! a message (`sendSessionChatMessage`), a page's answers (`collabGuestPageAnswers`), a pasted
//! picture (`saveSessionChatImage`) and the typing line (`setSessionChatDraft`, `collabTyping`).
//! The connector (connector.rs) sends these paths here and replays only reads to gxserver.
//!
//! The rules are the Collab rebuild's (#495): View only refuses writing; Ask me holds each
//! message for the owner's signed approval; Send delivers at once (turned on per share with the
//! owner's approval); a guest may name only pictures they uploaded to this share; per-guest and
//! per-share limits.

use std::time::Duration;

use ghostex_gx_protocol::collab_component::HostCall;
use serde_json::{json, Map, Value};

use crate::config::{self, GuestMode};
use crate::host::{self, share_title};
use crate::shares::{self, StoredShare, Viewer};

/// The paths a guest's page may call that are answered here.
pub const GUEST_WRITE_PATHS: &[&str] = &[
    "/api/sendSessionChatMessage",
    "/api/setSessionChatDraft",
    "/api/collabTyping",
    "/api/saveSessionChatImage",
    "/api/collabGuestPageAnswers",
];

/// The paths replayed to gxserver with the share's token (it allows only these, and only for
/// the shared chat).
pub const GUEST_READ_PATHS: &[&str] = &[
    "/api/readSessionChat",
    "/api/readSessionChatImage",
    "/api/readSessionChatResource",
    "/api/collabGuestPage",
];

/// The longest page answers a guest may send; a page's own answers may be up to 64 KiB.
const MAX_GUEST_PAGE_ANSWERS_CHARS: usize = 16_000;
/// Largest picture a guest may upload (base64 in the request, about 10 MB of image).
const MAX_GUEST_IMAGE_BASE64: usize = 14 * 1024 * 1024;

/// An HTTP status and gxserver's JSON envelope, as the guest page reads it.
pub type Answer = (u16, Value);

fn ok(result: Value) -> Answer {
    (
        200,
        json!({
            "ok": true,
            "product": ghostex_gx_protocol::GXSERVER_PRODUCT,
            "protocolVersion": host::address().gxserver_protocol_version,
            "requestId": uuid::Uuid::new_v4().to_string(),
            "result": result,
        }),
    )
}

fn fail(status: u16, code: &str, message: impl Into<String>) -> Answer {
    (
        status,
        json!({
            "ok": false,
            "product": ghostex_gx_protocol::GXSERVER_PRODUCT,
            "protocolVersion": host::address().gxserver_protocol_version,
            "error": code,
            "message": message.into(),
        }),
    )
}

pub fn refusal() -> Answer {
    fail(
        403,
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

fn referenced_image_paths(text: &str) -> Vec<String> {
    crate::split_image_references(text).1
}

/// One guest write. `body` is the request body the page sent (`{params: …}`).
pub async fn answer(share: &StoredShare, viewer: &Viewer, path: &str, body: &Value) -> Answer {
    let params = body
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if text(&params, "projectId").as_deref() != Some(share.project_id.as_str())
        || text(&params, "sessionId").as_deref() != Some(share.session_id.as_str())
    {
        return refusal();
    }
    // The share and the guest as they are now (the connector's copies may be older).
    let Some(share) = shares::get(&share.id) else {
        return fail(403, "collabEnded", "This link has ended.");
    };
    let Some(viewer) = shares::viewer(&share.id, &viewer.id) else {
        return refusal();
    };
    match path {
        "/api/saveSessionChatImage" => guest_image(&share, &viewer, &params).await,
        "/api/setSessionChatDraft" => {
            let typing = params
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| !content.trim().is_empty());
            note_typing(&share, Some(&viewer.id), typing);
            ok(json!({}))
        }
        "/api/collabTyping" => {
            let typing = params
                .get("typing")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            note_typing(&share, Some(&viewer.id), typing);
            ok(json!({}))
        }
        "/api/sendSessionChatMessage" => guest_message(&share, &viewer, &params).await,
        "/api/collabGuestPageAnswers" => guest_page_answers(&share, &viewer, &params).await,
        _ => refusal(),
    }
}

/// Clears a typing line when it runs out, without waiting for another change to carry it.
fn clear_typing_later(share: &StoredShare) {
    let share = share.clone();
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    runtime.spawn(async move {
        tokio::time::sleep(Duration::from_millis(shares::TYPING_MS as u64 + 150)).await;
        let now = chrono::Utc::now().timestamp_millis();
        if shares::next_typing_deadline(&share.id).is_none_or(|deadline| deadline <= now) {
            crate::sync::chat_changed(&share.project_id, &share.session_id);
        }
    });
}

fn note_typing(share: &StoredShare, viewer: Option<&str>, typing: bool) {
    if shares::note_typing(&share.id, viewer, typing) {
        crate::sync::chat_changed(&share.project_id, &share.session_id);
    }
    if typing {
        clear_typing_later(share);
    }
}

/// The owner typing in a shared chat (gxserver forwards the owner's `collabTyping`).
pub fn owner_typing(project_id: &str, session_id: &str, typing: bool) {
    if let Some(share) = shares::for_session(project_id, session_id) {
        note_typing(&share, None, typing);
    }
}

async fn guest_message(
    share: &StoredShare,
    viewer: &Viewer,
    params: &Map<String, Value>,
) -> Answer {
    let owner = config::owner_display_name(&config::read_config());
    if share.mode == GuestMode::View {
        return fail(
            403,
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
        return refusal();
    }
    // A picture reference in the words (`[Image #1](/path)`, what the chat box writes for a paste)
    // reaches the agent as that file, so it too may only name this guest's own uploads.
    let words_raw = text(params, "text").unwrap_or_default();
    if referenced_image_paths(&words_raw)
        .iter()
        .any(|image| !viewer.images.contains(image))
    {
        return refusal();
    }
    if params.get("key").is_some() || params.get("interrupt").and_then(Value::as_bool) == Some(true)
    {
        return refusal();
    }
    let words: String = words_raw.chars().take(8_000).collect();
    if words.is_empty() && images.is_empty() {
        return fail(400, "badRequest", "Write a message first.");
    }
    send_guest_words(share, viewer, words, images).await
}

/// A guest's checked words (a message, or a page's answers) on their way to the chat: held for
/// the owner's approval in Approve mode, sent at once in Send mode.
async fn send_guest_words(
    share: &StoredShare,
    viewer: &Viewer,
    words: String,
    images: Vec<String>,
) -> Answer {
    let owner = config::owner_display_name(&config::read_config());
    let title = share_title(share);
    let refused = |message: String| {
        crate::audit::record(
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
        return fail(429, "collabBusy", message);
    }
    if !shares::allow_message(&share.id, &viewer.id) {
        let message =
            refused("You're sending messages too fast. Wait a moment and try again.".to_string());
        return fail(429, "collabRateLimited", message);
    }
    shares::note_typing(&share.id, Some(&viewer.id), false);
    match share.mode {
        GuestMode::Approve => {
            let held = shares::hold(&share.id, viewer, &words, &images);
            crate::audit::record(
                share,
                &title,
                "message",
                &viewer.name,
                Some(&words),
                images.len(),
                Some("held"),
            );
            crate::sync::chat_changed(&share.project_id, &share.session_id);
            ok(json!({ "collab": { "held": true, "heldId": held.id } }))
        }
        _ => {
            crate::sync::chat_changed(&share.project_id, &share.session_id);
            match crate::owner::deliver(
                share,
                &crate::labelled_guest_text(&viewer.name, &words),
                &images,
            )
            .await
            {
                Ok(()) => {
                    crate::audit::record(
                        share,
                        &title,
                        "message",
                        &viewer.name,
                        Some(&words),
                        images.len(),
                        Some("sent"),
                    );
                    ok(json!({ "collab": { "held": false } }))
                }
                Err(message) => fail(409, "sendFailed", message),
            }
        }
    }
}

async fn guest_page_answers(
    share: &StoredShare,
    viewer: &Viewer,
    params: &Map<String, Value>,
) -> Answer {
    if share.mode == GuestMode::View {
        let owner = config::owner_display_name(&config::read_config());
        return fail(
            403,
            "collabViewOnly",
            format!("{owner} shared this chat to watch only."),
        );
    }
    let Some(page_id) = text(params, "pageId") else {
        return fail(400, "badRequest", "pageId is required.");
    };
    let answers = params.get("answers").cloned().unwrap_or(Value::Null);
    let message = match host::call_async(HostCall::GuestPageAnswersMessage {
        project_id: share.project_id.clone(),
        session_id: share.session_id.clone(),
        page_id,
        answers,
    })
    .await
    {
        Ok(value) => value["message"].as_str().unwrap_or_default().to_string(),
        Err(reason) => return fail(409, "pageNotAnswerable", reason),
    };
    // A picture pasted into a page's note is the guest's own upload; as in a message, the answers
    // may name no other file.
    if referenced_image_paths(&message)
        .iter()
        .any(|image| !viewer.images.contains(image))
    {
        return refusal();
    }
    if message.chars().count() > MAX_GUEST_PAGE_ANSWERS_CHARS {
        return fail(
            413,
            "payloadTooLarge",
            "These answers are too long to send to a shared chat. Shorten the notes and try again.",
        );
    }
    send_guest_words(share, viewer, message, Vec::new()).await
}

/// A guest's pasted picture: saved by gxserver like the owner's own (`saveSessionChatImage`),
/// and remembered as theirs, so they may read it back and send it with a message.
async fn guest_image(share: &StoredShare, viewer: &Viewer, params: &Map<String, Value>) -> Answer {
    let size = params
        .get("base64Data")
        .and_then(Value::as_str)
        .map_or(0, str::len);
    if size > MAX_GUEST_IMAGE_BASE64 {
        return fail(
            413,
            "payloadTooLarge",
            "Pictures sent to a shared chat can be up to 10 MB.",
        );
    }
    if viewer.images.len() >= shares::MAX_GUEST_IMAGES {
        return fail(
            429,
            "collabRateLimited",
            "You've sent as many pictures as this chat takes.",
        );
    }
    match host::call_async(HostCall::SaveGuestImage {
        project_id: share.project_id.clone(),
        session_id: share.session_id.clone(),
        params: params.clone(),
    })
    .await
    {
        Ok(result) => {
            if let Some(saved) = result["path"].as_str() {
                shares::note_guest_image(&share.id, &viewer.id, saved);
                crate::notify_changed();
            }
            ok(result)
        }
        Err(message) => fail(400, "badRequest", message),
    }
}
