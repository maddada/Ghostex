//! Pictures for the chat's page cards, taken from a page the user opened: when the floating page
//! window has loaded a page from the gxserver that published it (this computer's, or a remote
//! machine's through this computer's tunnel to it) and the page has no picture yet (or was
//! answered since its picture), the window's own browser takes a screenshot of the top of the page
//! (`CefSurface::capture_page_top`), which is shrunk and encoded off the main thread and stored
//! with the page (`server/src/visual_pages/thumbnail.rs`) for every client's card. A page published
//! since 2026-11-02 is pictured the same way before anyone opens it, from a hidden browser
//! (`visual_page_background_pictures.rs`).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The user's decision is on server/src/visual_pages/thumbnail.rs; the screenshot is apps/desktop/src/cef/shell/page_screenshot.rs; the window is apps/desktop/src/app/window/visual_page_modal.rs.

use std::{
    io::Cursor,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use base64::Engine as _;
use futures::FutureExt as _;
use serde_json::json;

use crate::app::gx_store::gx_rpc;
use crate::app::helpers::gpui_local_gxserver_api_port;
use crate::*;

/// The stored picture; exact 2× Retina copy of `thumbnailWidth`/`thumbnailHeight` (800×500) in
/// packages/gx-chat-core/visual/page-card.json so 11.2.1+ phones keep the exact 1.6 aspect ratio
/// while desktop and mobile render crisp Retina previews.
const PICTURE: (u32, u32) = (1600, 1000);
/// Consistent CSS viewport used for hidden-browser captures: matches `--gx-wide-width` (1120px)
/// plus gutters at exact 1.6 aspect ratio so KPI grids and cards frame cleanly regardless of the
/// user's main window width.
pub(super) const CAPTURE_VIEWPORT: (f32, f32) = (1200.0, 750.0);
/// The picture is made smaller until it is under this (gxserver takes up to 600 KiB).
const PICTURE_MAX_BYTES: usize = 560 * 1024;
/// How long a page gets after it loaded for its scripts to build it, before the wait for its web
/// fonts and pictures.
const SETTLE: Duration = Duration::from_millis(800);
/// The longest the page waits for its web fonts and pictures before it is pictured anyway.
const FONTS_AND_PICTURES_TIMEOUT_MS: u64 = 4_000;
/// A screenshot not answered by then is given up on.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// Wait after flipping `data-gx-theme` so styles repaint before capturing the second theme.
const THEME_SWITCH_SETTLE_MS: u64 = 120;

/// Marks `<html>` for thumbnail capture (clearing Ghostex's default `--gx-backdrop` while keeping
/// any page-authored background intact, and using the compact 20px top padding) and reports
/// whether the resulting canvas is transparent and whether the page supports both themes.
const PREPARE_THUMBNAIL_SCRIPT: &str = r#"(async () => {
  const d = document.documentElement;
  d.setAttribute('data-gx-thumbnail', '');
  await new Promise((done) => setTimeout(done, 60));
  const isClear = (el) => {
    if (!el) return true;
    const cs = getComputedStyle(el);
    if (cs.backgroundImage && cs.backgroundImage !== 'none') return false;
    const m = (cs.backgroundColor || '').match(/[\d.]+/g);
    return !m || (m.length > 3 && parseFloat(m[3]) < 0.05);
  };
  const clear = isClear(d) && isClear(document.body);
  const fixedTheme = d.getAttribute('data-gx-theme-from') === 'page';
  return (clear ? 'transparent' : 'opaque') + ':' + (fixedTheme ? 'fixed' : 'both');
})()"#;

/// One picture at a time, from the page window or a hidden browser.
pub(super) static CAPTURING: AtomicBool = AtomicBool::new(false);

/// The page id when `url` is a page the gxserver at `port` on this computer's loopback serves
/// (`http://127.0.0.1:<port>/visual/<id>`): this computer's own, or the tunnel to a remote one.
pub(super) fn served_page_id(url: &str, port: u16) -> Option<String> {
    let url = gpui::http_client::Url::parse(url).ok()?;
    let local_host = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    if url.scheme() != "http" || !local_host || url.port() != Some(port) {
        return None;
    }
    let id = url.path().strip_prefix("/visual/")?.trim_end_matches('/');
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| id.to_ascii_lowercase())
}

/// The floating page window's page finished loading in `surface`, which shows `viewport` (logical
/// pixels, which are CSS pixels at the page's default zoom). Pictures it when its card needs one;
/// `remote` is the machine whose gxserver published it, `None` for this computer's.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// The picture is the top of the page, and DevTools paints nothing for a clip outside the viewport: a page scrolled when the shot is taken came out as an empty band (the scrolled-off part, in the page's plain background colour) above a strip of its middle, which is what the user saw on many cards. The page's own script scrolls back to where the reader left it as soon as it loads (window/visual_page_modal/page_script.js), and readers scroll a page they just opened. So when a picture is about to be taken the app asks the page to wait at its top (`HOLD_PAGE_TOP`), which also stops the reader's wheel and puts back any other scroll until the picture is taken (`RELEASE_PAGE_TOP`), and the page's settle answer says whether it really is at its top; a scrolled page is not pictured. `captureBeyondViewport` pictures the top of a scrolled page, but leaves its fixed backgrounds where the viewport is and resizes the on-screen page for the shot.
#[allow(dead_code)]
pub(crate) fn picture_opened_page(
    surface: gpui::WeakEntity<CefSurface>,
    url: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    viewport: (f32, f32),
    cx: &mut gpui::App,
) {
    let port = remote
        .as_ref()
        .map_or_else(gpui_local_gxserver_api_port, |remote| remote.local_port);
    let Some(page_id) = served_page_id(url, port) else {
        return;
    };
    if CAPTURING.swap(true, Ordering::AcqRel) {
        return;
    }
    cx.spawn(async move |cx| {
        let _ = picture_page(&surface, &page_id, remote, viewport, cx).await;
        CAPTURING.store(false, Ordering::Release);
    })
    .detach();
}

/// Asks the page's script to show its top until `RELEASE_PAGE_TOP`, which takes it back to where
/// the reader was (window/visual_page_modal/page_script.js).
const HOLD_PAGE_TOP: &str = "window.__ghostexPageScroll && window.__ghostexPageScroll.hold();";
const RELEASE_PAGE_TOP: &str =
    "window.__ghostexPageScroll && window.__ghostexPageScroll.release();";

/// What the page's gxserver says about its picture (`readVisualPageThumbnail` without the
/// picture): `stale`, `gone`, `backgroundPicture` and `needsThemedPicture`.
pub(super) async fn read_picture_state(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    page_id: &str,
    theme: Option<&str>,
) -> Option<serde_json::Value> {
    let mut params = json!({"pageId": page_id, "withPicture": false});
    if let Some(theme) = theme {
        params["theme"] = json!(theme);
    }
    gx_rpc(remote, "/api/readVisualPageThumbnail", params)
        .await
        .ok()
}

async fn picture_page(
    surface: &gpui::WeakEntity<CefSurface>,
    page_id: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Option<()> {
    // Only a page with no picture, or one answered since its picture, is pictured.
    let known = read_picture_state(remote.clone(), page_id, None).await?;
    if known["stale"] != true || known["gone"] == true {
        return None;
    }
    surface
        .update(cx, |surface, _| {
            surface.execute_app_owned_script(HOLD_PAGE_TOP)
        })
        .ok()?;
    cx.background_executor().timer(SETTLE).await;
    let base64 = screenshot_page_top(surface, viewport, cx).await;
    let _ = surface.update(cx, |surface, _| {
        surface.execute_app_owned_script(RELEASE_PAGE_TOP)
    });
    store_picture(base64?, page_id, remote, cx).await
}

/// Shrinks `base64` (a screenshot) off the main thread and stores it as the page's picture.
/// Returns `Some(true)` when gxserver confirmed themed storage, `Some(false)` for legacy storage.
pub(super) async fn store_themed_picture(
    base64: String,
    page_id: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    theme: Option<&str>,
    transparent: bool,
    primary: bool,
    cx: &mut gpui::AsyncApp,
) -> Option<bool> {
    let png = cx
        .background_executor()
        .spawn(async move { encode_picture(&base64, transparent) })
        .await?;
    let mut payload = json!({
        "pageId": page_id,
        "png": base64::engine::general_purpose::STANDARD.encode(png),
    });
    if let Some(theme) = theme {
        payload["theme"] = json!(theme);
        payload["transparent"] = json!(transparent);
        payload["primary"] = json!(primary);
    }
    let response = gx_rpc(remote, "/api/storeVisualPageThumbnail", payload)
        .await
        .ok()?;
    Some(response.get("theme").and_then(serde_json::Value::as_str).is_some())
}

/// Captures crisp Retina (1600×1000) dark and light card thumbnails from a hidden browser surface,
/// using a transparent viewport background when the page uses Ghostex's theme (and keeping any
/// page-authored background intact).
pub(super) async fn capture_and_store_themed_pictures(
    surface: &gpui::WeakEntity<CefSurface>,
    page_id: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    viewport: (f32, f32),
    primary_light: bool,
    cx: &mut gpui::AsyncApp,
) -> Option<()> {
    if !wait_settled_at_top(surface, cx).await {
        return None;
    }
    let mode = run_page_script(surface, PREPARE_THUMBNAIL_SCRIPT, cx)
        .await
        .unwrap_or_else(|| "opaque:fixed".to_string());
    let mut parts = mode.split(':');
    let transparent = parts.next() == Some("transparent");
    let capture_both = parts.next() == Some("both");
    if transparent {
        let _ = set_transparent_override(surface, true, cx).await;
    }
    let primary_theme = if primary_light { "light" } else { "dark" };
    let primary_shot = capture_clip(surface, viewport, cx).await?;
    let server_supports_themes = store_themed_picture(
        primary_shot,
        page_id,
        remote.clone(),
        Some(primary_theme),
        transparent,
        true,
        cx,
    )
    .await?;
    if server_supports_themes && capture_both {
        let other_theme = if primary_light { "dark" } else { "light" };
        let switch_script = format!(
            "(async () => {{\
                const d = document.documentElement;\
                d.setAttribute('data-gx-theme', '{other_theme}');\
                Array.prototype.forEach.call(document.querySelectorAll('.gx-theme-switch [data-gx-set-theme]'), (b) => {{\
                    b.setAttribute('aria-pressed', b.getAttribute('data-gx-set-theme') === '{other_theme}' ? 'true' : 'false');\
                }});\
                await new Promise((done) => setTimeout(done, {THEME_SWITCH_SETTLE_MS}));\
                return 'ok';\
            }})()"
        );
        if run_page_script(surface, &switch_script, cx).await.is_some()
            && let Some(other_shot) = capture_clip(surface, viewport, cx).await
        {
            let _ = store_themed_picture(
                other_shot,
                page_id,
                remote,
                Some(other_theme),
                transparent,
                false,
                cx,
            )
            .await;
        }
    }
    Some(())
}

/// Shrinks `base64` (a screenshot) off the main thread and stores it as the page's picture.
pub(super) async fn store_picture(
    base64: String,
    page_id: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    cx: &mut gpui::AsyncApp,
) -> Option<()> {
    store_themed_picture(base64, page_id, remote, None, false, true, cx)
        .await
        .map(|_| ())
}

async fn wait_settled_at_top(
    surface: &gpui::WeakEntity<CefSurface>,
    cx: &mut gpui::AsyncApp,
) -> bool {
    let Some(mut settled) = surface
        .update(cx, |surface, _| {
            surface.wait_for_page_settled(FONTS_AND_PICTURES_TIMEOUT_MS)
        })
        .ok()
        .flatten()
    else {
        return false;
    };
    let timeout = cx.background_executor().timer(ANSWER_TIMEOUT);
    let where_it_is = futures::select! {
        answer = (&mut settled.settled).fuse() => answer.ok().flatten(),
        _ = timeout.fuse() => None,
    };
    drop(settled);
    where_it_is.as_deref() == Some("top")
}

async fn run_page_script(
    surface: &gpui::WeakEntity<CefSurface>,
    script: &str,
    cx: &mut gpui::AsyncApp,
) -> Option<String> {
    let mut pending = surface
        .update(cx, |surface, _| surface.evaluate_page_script(script))
        .ok()??;
    let timeout = cx.background_executor().timer(ANSWER_TIMEOUT);
    let answer = futures::select! {
        answer = (&mut pending.settled).fuse() => answer.ok()?,
        _ = timeout.fuse() => None,
    };
    drop(pending);
    answer
}

async fn set_transparent_override(
    surface: &gpui::WeakEntity<CefSurface>,
    transparent: bool,
    cx: &mut gpui::AsyncApp,
) -> Option<()> {
    let mut pending = surface
        .update(cx, |surface, _| {
            surface.set_default_background_transparent(transparent)
        })
        .ok()??;
    let timeout = cx.background_executor().timer(ANSWER_TIMEOUT);
    let answer = futures::select! {
        answer = (&mut pending.settled).fuse() => answer.ok()?,
        _ = timeout.fuse() => None,
    };
    drop(pending);
    answer.map(|_| ())
}

async fn capture_clip(
    surface: &gpui::WeakEntity<CefSurface>,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Option<String> {
    let aspect = PICTURE.0 as f64 / PICTURE.1 as f64;
    let (width, height) = (viewport.0 as f64, viewport.1 as f64);
    let (width, height) = if height >= width / aspect {
        (width, width / aspect)
    } else {
        (height * aspect, height)
    };
    if width < 1.0 {
        return None;
    }
    // CDXC:SessionChat 2026-11-13 WHY:
    // User, on opening a Ghostex page: "it shows html then it becomes smaller then expands again"
    // (their recording, frames 305 to 313: the whole page drawn at about two thirds of its size in
    // the page window's top-left corner for 150 ms, about a second after it loaded, then back at
    // full size). That was this screenshot: a clip `scale` other than 1 makes Chromium emulate that
    // scale for the shot, and for a page on screen the emulation redraws the visible page too. The
    // page window takes the picture only when the page has none or was answered since its last
    // one, which is why it came and went. The shot is taken at the page's own scale, which
    // Chromium draws without changing the page on screen, and `encode_picture` shrinks it to the
    // stored size off the main thread as it already did.
    let mut screenshot = surface
        .update(cx, |surface, _| {
            surface.capture_page_top(width, height, 1.0)
        })
        .ok()??;
    let timeout = cx.background_executor().timer(ANSWER_TIMEOUT);
    let base64 = futures::select! {
        picture = (&mut screenshot.picture).fuse() => picture.ok()??,
        _ = timeout.fuse() => return None,
    };
    // The observer is dropped here, on the main thread, once its answer is in.
    drop(screenshot);
    Some(base64)
}

/// The top of the page in the picture's shape, as wide as `viewport` allows (base64 PNG), once the
/// page has its web fonts and pictures (or ran out of time waiting); `None` when the page is not at
/// its top.
pub(super) async fn screenshot_page_top(
    surface: &gpui::WeakEntity<CefSurface>,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Option<String> {
    if !wait_settled_at_top(surface, cx).await {
        return None;
    }
    capture_clip(surface, viewport, cx).await
}

/// The screenshot (base64 PNG) at up to the stored Retina size, sharpened with Lanczos3 and
/// compressed small enough to store. Preserves the alpha channel when `preserve_alpha` is true.
fn encode_picture(base64: &str, preserve_alpha: bool) -> Option<Vec<u8>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    for scale in [1.0_f32, 0.85, 0.72, 0.6, 0.5] {
        let width = (PICTURE.0 as f32 * scale).round() as u32;
        let height = (PICTURE.1 as f32 * scale).round() as u32;
        use image::ImageEncoder as _;
        let mut png = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            image::codecs::png::CompressionType::Default,
            image::codecs::png::FilterType::Adaptive,
        );
        if preserve_alpha {
            let full = decoded.to_rgba8();
            let picture =
                image::imageops::resize(&full, width, height, image::imageops::FilterType::Lanczos3);
            encoder
                .write_image(&picture, width, height, image::ExtendedColorType::Rgba8)
                .ok()?;
        } else {
            let full = decoded.to_rgb8();
            let picture =
                image::imageops::resize(&full, width, height, image::imageops::FilterType::Lanczos3);
            encoder
                .write_image(&picture, width, height, image::ExtendedColorType::Rgb8)
                .ok()?;
        }
        let png = png.into_inner();
        if png.len() <= PICTURE_MAX_BYTES {
            return Some(png);
        }
    }
    None
}
