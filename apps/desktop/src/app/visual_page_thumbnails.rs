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

/// The stored picture; `thumbnailWidth`/`thumbnailHeight` in packages/gx-chat-core/visual/page-card.json.
const PICTURE: (u32, u32) = (800, 500);
/// The picture is made smaller until it is under this (gxserver takes up to 600 KiB).
const PICTURE_MAX_BYTES: usize = 560 * 1024;
/// How long a page gets after it loaded for its scripts to build it, before the wait for its web
/// fonts and pictures.
const SETTLE: Duration = Duration::from_millis(800);
/// The longest the page waits for its web fonts and pictures before it is pictured anyway.
const FONTS_AND_PICTURES_TIMEOUT_MS: u64 = 4_000;
/// A screenshot not answered by then is given up on.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

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
/// picture): `stale`, `gone` and `backgroundPicture`.
pub(super) async fn read_picture_state(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    page_id: &str,
) -> Option<serde_json::Value> {
    gx_rpc(
        remote,
        "/api/readVisualPageThumbnail",
        json!({"pageId": page_id, "withPicture": false}),
    )
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
    let known = read_picture_state(remote.clone(), page_id).await?;
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
pub(super) async fn store_picture(
    base64: String,
    page_id: &str,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    cx: &mut gpui::AsyncApp,
) -> Option<()> {
    let png = cx
        .background_executor()
        .spawn(async move { encode_picture(&base64) })
        .await?;
    gx_rpc(
        remote,
        "/api/storeVisualPageThumbnail",
        json!({
            "pageId": page_id,
            "png": base64::engine::general_purpose::STANDARD.encode(png),
        }),
    )
    .await
    .ok()
    .map(|_| ())
}

/// The top of the page in the picture's shape, as wide as `viewport` allows (base64 PNG), once the
/// page has its web fonts and pictures (or ran out of time waiting); `None` when the page is not at
/// its top.
pub(super) async fn screenshot_page_top(
    surface: &gpui::WeakEntity<CefSurface>,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Option<String> {
    let mut settled = surface
        .update(cx, |surface, _| {
            surface.wait_for_page_settled(FONTS_AND_PICTURES_TIMEOUT_MS)
        })
        .ok()??;
    let timeout = cx.background_executor().timer(ANSWER_TIMEOUT);
    let where_it_is = futures::select! {
        answer = (&mut settled.settled).fuse() => answer.ok()??,
        _ = timeout.fuse() => return None,
    };
    drop(settled);
    if where_it_is != "top" {
        return None;
    }
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

/// The screenshot (base64 PNG) at exactly the stored size, as an opaque PNG small enough to store.
fn encode_picture(base64: &str) -> Option<Vec<u8>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .ok()?;
    let full = image::load_from_memory(&bytes).ok()?.to_rgb8();
    for scale in [1.0_f32, 0.8, 0.64] {
        let width = (PICTURE.0 as f32 * scale) as u32;
        let height = (PICTURE.1 as f32 * scale) as u32;
        let picture =
            image::imageops::resize(&full, width, height, image::imageops::FilterType::Triangle);
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(picture)
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        let png = png.into_inner();
        if png.len() <= PICTURE_MAX_BYTES {
            return Some(png);
        }
    }
    None
}
