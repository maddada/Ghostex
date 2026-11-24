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

use std::{fs, io::Cursor, path::Path};

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

/// A picture larger than this is refused; the desktop stores up to a 1600×1000 PNG, well under it.
/// As base64 in a JSON body it still fits the general 1 MiB request limit.
const THUMBNAIL_MAX_BYTES: usize = 600 * 1024;
/// The largest picture side accepted, in pixels.
const THUMBNAIL_MAX_SIDE: u32 = 2000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn thumbnail_path(gxserver_state_dir: &Path, id: &str) -> std::path::PathBuf {
    visual_pages_dir(gxserver_state_dir).join(format!("{id}.png"))
}

fn themed_thumbnail_path(gxserver_state_dir: &Path, id: &str, theme: &str) -> std::path::PathBuf {
    visual_pages_dir(gxserver_state_dir).join(format!("{id}.{theme}.png"))
}

fn theme_param(params: &serde_json::Map<String, Value>) -> Option<&'static str> {
    match params.get("theme").and_then(Value::as_str) {
        Some("dark") => Some("dark"),
        Some("light") => Some("light"),
        _ => None,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("png.tmp");
    fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path))
}

/// Composites a transparent PNG onto a clean solid surface matching `theme` so older clients that
/// read `<id>.png` without specifying a theme (including phones at 11.2.1+) always receive an
/// opaque, legible preview even when their own theme differs from the capture theme.
fn flatten_transparent_png(png: &[u8], theme: &str) -> Option<Vec<u8>> {
    let rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let bg: [u8; 3] = if theme == "light" {
        [0xf0, 0xf0, 0xf0]
    } else {
        [0x13, 0x13, 0x13]
    };
    let (width, height) = rgba.dimensions();
    let mut rgb = image::RgbImage::new(width, height);
    for (x, y, px) in rgba.enumerate_pixels() {
        let [r, g, b, a] = px.0;
        let alpha = u16::from(a);
        let inv = 255 - alpha;
        let blend = |fg: u8, bg_c: u8| -> u8 {
            ((u16::from(fg) * alpha + u16::from(bg_c) * inv + 127) / 255) as u8
        };
        rgb.put_pixel(
            x,
            y,
            image::Rgb([blend(r, bg[0]), blend(g, bg[1]), blend(b, bg[2])]),
        );
    }
    use image::ImageEncoder as _;
    let mut out = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Default,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
    .ok()?;
    Some(out.into_inner())
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

/// `/api/storeVisualPageThumbnail` `{pageId, png, answered?, theme?, transparent?, primary?}`: the
/// desktop's picture of a page, as base64 PNG. `answered` says whether the page showed its answers;
/// left out, the page's record decides. When `theme` (`"dark"` or `"light"`) is given, the picture
/// is stored beside the page as `<id>.<theme>.png`, and `<id>.png` is kept as an opaque fallback
/// for clients that do not pass `theme` (such as phones at 11.2.1+).
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
    let theme = theme_param(&params);
    let transparent = params
        .get("transparent")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let primary = params
        .get("primary")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let written = (|| {
        let fallback_path = thumbnail_path(state_dir, &id);
        match theme {
            Some(theme) => {
                let themed_path = themed_thumbnail_path(state_dir, &id, theme);
                write_atomic(&themed_path, &png).map_err(StoreAnswersError::Io)?;
                if primary || !fallback_path.is_file() {
                    let fallback_bytes = if transparent {
                        flatten_transparent_png(&png, theme)
                            .filter(|bytes| bytes.len() <= THUMBNAIL_MAX_BYTES)
                            .unwrap_or_else(|| png.clone())
                    } else {
                        png.clone()
                    };
                    write_atomic(&fallback_path, &fallback_bytes).map_err(StoreAnswersError::Io)?;
                }
                note_thumbnail(state_dir, &id, answered, Some(theme), transparent)
            }
            None => {
                write_atomic(&fallback_path, &png).map_err(StoreAnswersError::Io)?;
                let _ = fs::remove_file(themed_thumbnail_path(state_dir, &id, "dark"));
                let _ = fs::remove_file(themed_thumbnail_path(state_dir, &id, "light"));
                note_thumbnail(state_dir, &id, answered, None, false)
            }
        }
    })();
    match written {
        Ok(_) => {
            let mut result = json!({ "pageId": id, "stored": true });
            if let Some(theme) = theme {
                result["theme"] = json!(theme);
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(StoreAnswersError::NotFound) => {
            let _ = fs::remove_file(thumbnail_path(state_dir, &id));
            if let Some(theme) = theme {
                let _ = fs::remove_file(themed_thumbnail_path(state_dir, &id, theme));
            }
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

/// `/api/readVisualPageThumbnail` `{pageId, withPicture?, theme?}`: the page's picture for its
/// chat card, as `{pageId, png, capturedAt, stale, gone, backgroundPicture, theme, transparent}`.
/// `png` is base64 or null (always null with `withPicture: false`, which the desktop uses to ask
/// whether to take one); `stale` means the page has no picture or was answered since it, `gone`
/// that the page no longer exists, and `backgroundPicture` that the page has no picture and was
/// published late enough to be pictured without being opened. When `theme` (`"dark"` or `"light"`)
/// is requested and a matching themed picture exists, that picture is returned with its
/// `transparent` flag; otherwise the opaque fallback `<id>.png` is returned.
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
    let theme = theme_param(&params);
    let result = picture_state(&state.paths.root_dir, &id, with_picture, theme);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

/// The read's answer for page `id` (`handle_read_http`).
fn picture_state(state_dir: &Path, id: &str, with_picture: bool, theme: Option<&str>) -> Value {
    match read_record(state_dir, id) {
        None => {
            json!({
                "pageId": id,
                "png": null,
                "capturedAt": null,
                "stale": false,
                "gone": true,
                "backgroundPicture": false,
                "theme": null,
                "transparent": false,
            })
        }
        Some(record) => {
            let themed = theme
                .map(|t| (t, themed_thumbnail_path(state_dir, id, t)))
                .filter(|(_, path)| path.is_file());
            let (chosen_path, resolved_theme, transparent) = match themed {
                Some((t, path)) => {
                    let key = if t == "light" {
                        "thumbnailLightTransparent"
                    } else {
                        "thumbnailDarkTransparent"
                    };
                    let transparent = record.get(key).and_then(Value::as_bool).unwrap_or(false);
                    (path, Some(t), transparent)
                }
                None => (thumbnail_path(state_dir, id), None, false),
            };
            let missing_themed = theme.is_some() && resolved_theme.is_none();
            let captured_at = record
                .get("thumbnailAt")
                .and_then(Value::as_str)
                .filter(|_| chosen_path.is_file());
            let png = captured_at
                .filter(|_| with_picture)
                .and_then(|_| fs::read(&chosen_path).ok())
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
            let needs_themed = missing_themed && captured_at.is_some();
            json!({
                "pageId": id,
                "png": png,
                "capturedAt": captured_at,
                "stale": captured_at.is_none() || needs_themed || answered_since_picture(&record),
                "gone": false,
                "backgroundPicture": (captured_at.is_none() || needs_themed)
                    && record.get("pictureInBackground").and_then(Value::as_bool) == Some(true),
                "needsThemedPicture": needs_themed,
                "theme": resolved_theme,
                "transparent": transparent,
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
            picture_state(dir.path(), &id, false, None)["backgroundPicture"],
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
            picture_state(dir.path(), &id, false, None)["backgroundPicture"],
            false
        );
        // Gone.
        assert_eq!(
            picture_state(dir.path(), "0123456789abcdef0123456789abcdef", false, None)
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
            picture_state(dir.path(), &id, false, None)["backgroundPicture"],
            false
        );
    }
}
