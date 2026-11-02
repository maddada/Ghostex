//! The picture behind the app's glass, handed to the floating window's page so the page's backdrop
//! is the reader's own wallpaper or custom picture (server/src/visual_pages/theme.rs paints it,
//! dimmed, under the page's glass cards).
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User picked "A" on Q987: hand the page the picture behind its glass, so every page shows the reader's wallpaper (or their custom picture) the way the app's own glass does, instead of the painted dusk. Live and video glass, glass off, the web build, the phone and pages opened elsewhere keep the painted dusk or dawn.
//! CDXC:SessionChat 2026-11-02 WHY:
//! The page can't be see-through (a windowed CEF browser always paints opaque), so it gets the main window's own blurred picture instead: GPUI's `background_wallpaper_snapshot`, a JPEG of a few dozen kilobytes, put into the page by a page script as a data URL. A gxserver address would be refused by the page's content policy (`img-src data: blob: https:`), and the picture belongs to this computer, not to the gxserver that serves the page. It is read when the page opens, so a new wallpaper shows from the next open.
//! SEE-ALSO: .dependencies/zed/crates/gpui_macos/src/window_wallpaper.rs (`jpeg`), server/src/visual_pages/theme.rs (`data-gx-glass`).

use base64::Engine as _;
use gpui::{App, Bounds, Pixels, Point};

/// The picture's largest width in pixels: under the glass's blur no finer detail survives.
const PICTURE_MAX_WIDTH: u32 = 640;

/// The page script that gives the page the picture behind the main window's glass, for a page laid
/// out at `page` (its window's bounds, moved to where the page itself starts). `None` while the
/// glass shows no picture (glass off, the live blur, a video or a live style).
pub(super) fn page_glass_script(page: Bounds<Pixels>, cx: &mut App) -> Option<String> {
    if !crate::app::helpers::window_glass_active() {
        return None;
    }
    let main = crate::app::helpers::main_window_handle(cx)?;
    let (snapshot, main_bounds) = main
        .update(cx, |_, window, _| {
            (
                window.background_wallpaper_snapshot(PICTURE_MAX_WIDTH),
                window.bounds(),
            )
        })
        .ok()?;
    let snapshot = snapshot?;
    let picture = base64::engine::general_purpose::STANDARD.encode(&snapshot.jpeg);
    // Held against the screen, the page shows the part of the screen it sits over, like the
    // main window does; attached to the window, it covers the page.
    let (size, position) = if crate::app::helpers::window_glass_picture_follows_screen() {
        // The page window and the main window share one screen and one coordinate space, so the
        // page sits on the screen where the main window does, moved by their difference.
        let offset: Point<Pixels> = page.origin - main_bounds.origin;
        let x = f32::from(snapshot.window.origin.x + offset.x);
        let y = f32::from(snapshot.window.origin.y + offset.y);
        (
            format!(
                "{}px {}px",
                f32::from(snapshot.screen.width),
                f32::from(snapshot.screen.height)
            ),
            format!("{}px {}px", -x, -y),
        )
    } else {
        ("cover".to_string(), "center".to_string())
    };
    Some(format!(
        "(function(){{var s=document.documentElement;s.setAttribute(\"data-gx-glass\",\"\");s=s.style;\
s.setProperty(\"--gx-glass-picture\",'url(\"data:image/jpeg;base64,{picture}\")');\
s.setProperty(\"--gx-glass-size\",\"{size}\");s.setProperty(\"--gx-glass-position\",\"{position}\");}})();"
    ))
}
