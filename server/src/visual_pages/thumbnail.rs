//! Page card pictures: a small PNG of a published page, taken by the desktop app from the page
//! while the user has it open (or, for a page published since 2026-11-02, in a hidden browser once
//! its card is on screen) and stored here, so the chat card on every client (desktop, the GPUI
//! web build, the phone) shows the same picture.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User picked "B · Window preview" for the card a published page gets in the chat (docs/2026-10-10/chat-cards-questions/01-page-card.html): a window bar with dots and the file name, a scaled-down picture of the page fading out at its bottom, then the title, "Interactive page · file.html" and Open; the green Answered pill stays. For the picture the user chose "the desktop app snapshots it". An answered page is pictured again the next time it is opened (a republished page is a new page with a new id). Until a page has a picture its card shows a plain placeholder: the window bar, the file name and a page icon.
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User: "we don't show the preview until after i open it. let's please open it in the bg just to capture the screenshot (if cef is already installed). but only do this for new ones that come into view in the active session basically not old ones before this update so we dont go and capture a ton of old ones and slow the machine". This supersedes the 2026-10-11 rule that a page is pictured only from the floating page window the user opened (no hidden browser, no runtime start for it): a page published since this change carries `pictureInBackground` in its record, and while it has no picture this read answers `backgroundPicture: true`, which the desktop takes as leave to load the page in a hidden browser and picture it once its card is drawn in the chat on screen (apps/desktop/src/app/visual_page_background_pictures.rs). Pages published before carry no mark and are still pictured only when opened.
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The capture is apps/desktop/src/app/visual_page_thumbnails.rs (with apps/desktop/src/cef/shell/page_screenshot.rs); the card's asking rules are packages/gx-chat-core/src/transcript/page_thumbnails.rs and visual/page-card.json; the cards are drawn by apps/desktop/src/app/native_chat/page_card.rs and apps/mobile/app/src/chat/native/transcript/PageCard.tsx.
//! CDXC:ServerApi 2026-10-11 WHY:
//! Both calls stay on the loopback listener (protocol.rs) and behind the auth token, not under `/visual/<id>/` with the page's own routes: the page's content policy lets it reach its own routes, and a page must not be able to replace its own picture.

use std::{fs, path::Path};

use axum::http::StatusCode;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::store::{is_page_id, note_thumbnail, read_record, visual_pages_dir, StoreAnswersError};

pub(crate) const READ_VISUAL_PAGE_THUMBNAIL_ENDPOINT: &str = "/api/readVisualPageThumbnail";
pub(crate) const STORE_VISUAL_PAGE_THUMBNAIL_ENDPOINT: &str = "/api/storeVisualPageThumbnail";

/// A picture larger than this is refused; the desktop stores an 800×500 PNG, well under it. As
/// base64 in a JSON body it still fits the general 1 MiB request limit.
const THUMBNAIL_MAX_BYTES: usize = 600 * 1024;
/// The largest picture side accepted, in pixels.
const THUMBNAIL_MAX_SIDE: u32 = 2000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn thumbnail_path(gxserver_state_dir: &Path, id: &str) -> std::path::PathBuf {
    visual_pages_dir(gxserver_state_dir).join(format!("{id}.png"))
}

/// The page was answered, or refreshed from its file (`source.rs`), after its picture was taken.
fn answered_since_picture(record: &Value) -> bool {
    (is_answered(record) && record.get("thumbnailAnswered").and_then(Value::as_bool) != Some(true))
        || record.get("thumbnailOutdated").and_then(Value::as_bool) == Some(true)
}

fn is_answered(record: &Value) -> bool {
    record.get("answeredAt").and_then(Value::as_str).is_some()
}

fn page_id_param(params: &serde_json::Map<String, Value>) -> Option<&str> {
    params
        .get("pageId")
        .and_then(Value::as_str)
        .filter(|id| is_page_id(id))
}

/// `/api/storeVisualPageThumbnail` `{pageId, png, answered?}`: the desktop's picture of a page, as
/// base64 PNG. `answered` says whether the page showed its answers; left out, the page's record
/// decides.
pub(crate) fn handle_store_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let Some(id) = page_id_param(&params).map(str::to_string) else {
        return bad_request(
            endpoint_path,
            request_id,
            "storeVisualPageThumbnail requires pageId.",
        );
    };
    let Some(png) = params
        .get("png")
        .and_then(Value::as_str)
        .and_then(|png| base64::engine::general_purpose::STANDARD.decode(png).ok())
    else {
        return bad_request(
            endpoint_path,
            request_id,
            "storeVisualPageThumbnail requires png as base64.",
        );
    };
    if png.len() > THUMBNAIL_MAX_BYTES || !is_reasonable_png(&png) {
        return bad_request(
            endpoint_path,
            request_id,
            "The picture must be a PNG of at most 600 KiB and 2000 pixels a side.",
        );
    }
    let state_dir = &state.paths.root_dir;
    let Some(record) = read_record(state_dir, &id) else {
        return not_found(endpoint_path, request_id);
    };
    let answered = params
        .get("answered")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| is_answered(&record));
    let written = (|| {
        let path = thumbnail_path(state_dir, &id);
        let temporary = path.with_extension("png.tmp");
        fs::write(&temporary, &png)
            .and_then(|()| fs::rename(&temporary, &path))
            .map_err(StoreAnswersError::Io)?;
        note_thumbnail(state_dir, &id, answered)
    })();
    match written {
        Ok(_) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "pageId": id, "stored": true })),
        ),
        Err(StoreAnswersError::NotFound) => {
            let _ = fs::remove_file(thumbnail_path(state_dir, &id));
            not_found(endpoint_path, request_id)
        }
        Err(StoreAnswersError::AlreadyAnswered(_)) | Err(StoreAnswersError::Io(_)) => {
            domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "Could not save the picture.".to_string(),
                },
            )
        }
    }
}

/// `/api/readVisualPageThumbnail` `{pageId, withPicture?}`: the page's picture for its chat card,
/// as `{pageId, png, capturedAt, stale, gone, backgroundPicture}`. `png` is base64 or null (always
/// null with `withPicture: false`, which the desktop uses to ask whether to take one); `stale`
/// means the page has no picture or was answered since it, `gone` that the page no longer exists,
/// and `backgroundPicture` that the page has no picture and was published late enough to be
/// pictured without being opened.
pub(crate) fn handle_read_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let Some(id) = page_id_param(&params).map(str::to_string) else {
        return bad_request(
            endpoint_path,
            request_id,
            "readVisualPageThumbnail requires pageId.",
        );
    };
    let with_picture = params.get("withPicture").and_then(Value::as_bool) != Some(false);
    let result = picture_state(&state.paths.root_dir, &id, with_picture);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

/// The read's answer for page `id` (`handle_read_http`).
fn picture_state(state_dir: &Path, id: &str, with_picture: bool) -> Value {
    match read_record(state_dir, id) {
        None => {
            json!({
                "pageId": id,
                "png": null,
                "capturedAt": null,
                "stale": false,
                "gone": true,
                "backgroundPicture": false,
            })
        }
        Some(record) => {
            let captured_at = record
                .get("thumbnailAt")
                .and_then(Value::as_str)
                .filter(|_| thumbnail_path(state_dir, id).is_file());
            let png = captured_at
                .filter(|_| with_picture)
                .and_then(|_| fs::read(thumbnail_path(state_dir, id)).ok())
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
            json!({
                "pageId": id,
                "png": png,
                "capturedAt": captured_at,
                "stale": captured_at.is_none() || answered_since_picture(&record),
                "gone": false,
                "backgroundPicture": captured_at.is_none()
                    && record.get("pictureInBackground").and_then(Value::as_bool) == Some(true),
            })
        }
    }
}

/// A PNG whose header names a picture of a sensible size.
fn is_reasonable_png(bytes: &[u8]) -> bool {
    // Signature, then the IHDR chunk: length (4), type (4), width (4), height (4).
    if bytes.len() < 24 || !bytes.starts_with(PNG_SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return false;
    }
    let side =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (width, height) = (side(16), side(20));
    (1..=THUMBNAIL_MAX_SIDE).contains(&width) && (1..=THUMBNAIL_MAX_SIDE).contains(&height)
}

fn not_found(endpoint_path: String, request_id: String) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError {
            code: "notFound",
            message: "No such page.".to_string(),
        },
    )
}

fn bad_request(endpoint_path: String, request_id: String, message: &str) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError::bad_request(message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_published_since_background_pictures_may_be_pictured_unopened() {
        let dir = tempfile::tempdir().unwrap();
        let id =
            super::super::store::write_page(dir.path(), "Page", "<p>page</p>", None, None).unwrap();
        assert_eq!(
            picture_state(dir.path(), &id, false)["backgroundPicture"],
            true
        );
        // A page published before carries no mark.
        let path = visual_pages_dir(dir.path()).join(format!("{id}.json"));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record
            .as_object_mut()
            .unwrap()
            .remove("pictureInBackground");
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            picture_state(dir.path(), &id, false)["backgroundPicture"],
            false
        );
        // Gone.
        assert_eq!(
            picture_state(dir.path(), "0123456789abcdef0123456789abcdef", false)
                ["backgroundPicture"],
            false
        );
    }

    #[test]
    fn a_pictured_page_is_not_pictured_unopened_again() {
        let dir = tempfile::tempdir().unwrap();
        let id =
            super::super::store::write_page(dir.path(), "Page", "<p>page</p>", None, None).unwrap();
        fs::write(thumbnail_path(dir.path(), &id), b"png").unwrap();
        let path = visual_pages_dir(dir.path()).join(format!("{id}.json"));
        let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["thumbnailAt"] = json!("2026-11-02T00:00:00.000Z");
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            picture_state(dir.path(), &id, false)["backgroundPicture"],
            false
        );
    }
}
