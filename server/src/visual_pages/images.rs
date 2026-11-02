use std::sync::Arc;

use axum::{body::Body, http::StatusCode};
use serde_json::json;

use crate::server::{AppState, RoutedResponse};
use crate::session_chat_files::{write_session_chat_image, SESSION_CHAT_IMAGE_MAX_BYTES};

use super::{
    answers::reply,
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{is_page_id, read_record},
};

pub(crate) const VISUAL_PAGE_IMAGE_SUFFIX: &str = "/image";

/// Whether `POST <path>` is a page's pasted-image route, `/visual/<id>/image`.
pub(crate) fn is_image_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_IMAGE_SUFFIX)
}

/// `POST /visual/<id>/image` with the image's bytes as the body: an image the reader pasted into one
/// of the page's note or text boxes (kit.rs). It is saved where the chat keeps pasted images
/// (`write_session_chat_image`) and its path comes back as `{"ok":true,"path":…}` for the kit to
/// write into the box as `[Image #N](path)`, so the answers carry it and the agent can read it.
///
/// CDXC:Clipboard 2026-11-02 WHY:
/// Answered before the auth gate like the page's answers and draft routes: the page has no token, its unguessable id is the grant, and its content policy lets it reach only its own `/visual/<id>/` routes. Only a page that still takes answers and knows the session that published it takes an image, and only bytes that start like an image are written, so the route cannot store arbitrary files. The kit posts the bytes as a typeless blob, a simple request with no preflight, and reads the reply through `Access-Control-Allow-Origin: *`.
pub(crate) async fn save(state: Arc<AppState>, path: String, body: Body) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_IMAGE_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
    else {
        return reply(
            StatusCode::NOT_FOUND,
            json!({"ok": false, "error": "No such page."}),
        );
    };
    let Ok(bytes) = axum::body::to_bytes(body, SESSION_CHAT_IMAGE_MAX_BYTES).await else {
        return reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"ok": false, "error": "The image is larger than 32 MiB. Try a smaller capture."}),
        );
    };
    if !looks_like_image(&bytes) {
        return reply(
            StatusCode::BAD_REQUEST,
            json!({"ok": false, "error": "Only PNG, JPEG, GIF, WebP and BMP images can be pasted here."}),
        );
    }
    let state_dir = state.paths.root_dir.clone();
    let data_dir = state.paths.app_data_dir.clone();
    let saved = tokio::task::spawn_blocking(move || {
        let record = read_record(&state_dir, &id).ok_or("No such page.")?;
        if record.get("answeredAt").is_some() {
            return Err("These answers were already sent.");
        }
        if record
            .get("sessionRef")
            .and_then(serde_json::Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err("The session that published this page is unknown.");
        }
        write_session_chat_image(&data_dir, &bytes, None)
            .map_err(|_| "The pasted image could not be saved.")
    })
    .await;
    match saved {
        Ok(Ok(path)) => reply(
            StatusCode::OK,
            json!({"ok": true, "path": path.to_string_lossy()}),
        ),
        Ok(Err(error)) => reply(
            if error == "No such page." {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::CONFLICT
            },
            json!({"ok": false, "error": error}),
        ),
        Err(_) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": "The pasted image could not be saved."}),
        ),
    }
}

/// PNG, JPEG, GIF, WebP or BMP, the formats `session_chat_image_extension` names.
fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF8")
        || (bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || (bytes.len() > 2 && bytes.starts_with(b"BM"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_page_image_route_matches() {
        assert!(is_image_route("/visual/abc/image"));
        assert!(!is_image_route("/visual/abc/answers"));
        assert!(!is_image_route("/api/saveSessionChatImage"));
    }

    #[test]
    fn only_image_bytes_are_taken() {
        assert!(looks_like_image(b"\x89PNG\r\n\x1a\n...."));
        assert!(looks_like_image(b"\xff\xd8\xff\xe0...."));
        assert!(looks_like_image(b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(!looks_like_image(b"#!/bin/sh\necho hi"));
        assert!(!looks_like_image(b""));
    }
}
