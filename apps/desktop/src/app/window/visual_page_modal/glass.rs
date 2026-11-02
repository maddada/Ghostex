//! The picture behind the app's glass and the work area's tint over it, handed to the floating
//! window's page so the page's backdrop looks like the window it floats over: the reader's own
//! wallpaper or custom picture under the same tint (server/src/visual_pages/theme.rs paints it
//! under the page's glass cards). While the window is open it follows the app: a new wallpaper,
//! another glass setting, a new tint or the app's theme reach the page within a second.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User picked "A" on Q987: hand the page the picture behind its glass, so every page shows the reader's wallpaper (or their custom picture) the way the app's own glass does, instead of the painted dusk. Later the same day: "The bg of this isn't changing per different bg that I apply on my desktop" and "make it darker bg on dark mode": the live blur ("Desktop and windows") now hands over the desktop picture too, the page dims it with the work area's own tint (`windowGlassWorkAreaTintDark` / `Light` over the workspace colour) instead of a fixed purple, and the page follows changes while it is open. Video and live-style glass, glass off, the web build, the phone and pages opened elsewhere keep the painted dusk or dawn.
//! CDXC:SessionChat 2026-11-02 WHY:
//! The page can't be see-through (a windowed CEF browser always paints opaque), so it gets the main window's own blurred picture instead: GPUI's `background_wallpaper_snapshot`, a JPEG of a few dozen kilobytes, put into the page by a page script as a data URL. A gxserver address would be refused by the page's content policy (`img-src data: blob: https:`), and the picture belongs to this computer, not to the gxserver that serves the page. The picture is sent again only when its bytes change (GPUI keeps the last JPEG, so asking every second costs no encoding); the tint, placement and theme go in a small script of their own.
//! SEE-ALSO: .dependencies/zed/crates/gpui_macos/src/window_wallpaper.rs (`jpeg`), server/src/visual_pages/theme.rs (`data-gx-glass`, `--gx-glass-tint-dark` / `-light`).

use super::{GpuiVisualPageModalWindow, HEADER_HEIGHT};
use base64::Engine as _;
use gpui::{App, Bounds, Context, Pixels, Point, Window, px};
use std::time::Duration;

/// The picture's largest width in pixels: under the glass's blur no finer detail survives.
const PICTURE_MAX_WIDTH: u32 = 640;

/// How often the open window checks whether the app's glass or theme changed.
const GLASS_CHECK_EVERY: Duration = Duration::from_secs(1);

/// What the page was last handed.
#[derive(Default)]
pub(super) struct PageGlass {
    /// The picture's JPEG, empty for none; `None` before the first hand-over.
    picture: Option<Vec<u8>>,
    look: Option<String>,
}

impl GpuiVisualPageModalWindow {
    /// Hands the new page its glass and keeps it in step while the window is open.
    pub(super) fn follow_page_glass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_page_glass(window, cx);
        self._glass_watch = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(GLASS_CHECK_EVERY).await;
                if this
                    .update_in(cx, |this, window, cx| this.sync_page_glass(window, cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    fn sync_page_glass(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        let mut page = window.bounds();
        page.origin.y += px(HEADER_HEIGHT);
        page.size.height = (page.size.height - px(HEADER_HEIGHT)).max(px(1.0));
        let (picture, look) = page_glass(page, cx);
        // Page scripts run in the current document and in every one it loads later, in the order
        // they were added, so a reload ends on the latest of each.
        if self.glass.picture.as_ref() != Some(&picture) {
            surface.read(cx).add_page_script(&picture_script(&picture));
            self.glass.picture = Some(picture);
        }
        if self.glass.look.as_ref() != Some(&look) {
            surface.read(cx).add_page_script(&look);
            self.glass.look = Some(look);
        }
    }
}

/// The picture behind the main window's glass (empty while it shows none: glass off, a video or a
/// live style), and the script that sets its placement, the work area's tint and the app's theme
/// on a page laid out at `page` (its window's bounds, moved to where the page itself starts).
fn page_glass(page: Bounds<Pixels>, cx: &mut App) -> (Vec<u8>, String) {
    let light = crate::app::helpers::chrome_uses_light_appearance();
    let theme = if light { "light" } else { "dark" };
    let snapshot = crate::app::helpers::window_glass_active()
        .then(|| crate::app::helpers::main_window_handle(cx))
        .flatten()
        .and_then(|main| {
            main.update(cx, |_, window, _| {
                (
                    window.background_wallpaper_snapshot(PICTURE_MAX_WIDTH),
                    window.bounds(),
                )
            })
            .ok()
        })
        .and_then(|(snapshot, main_bounds)| Some((snapshot?, main_bounds)));
    let Some((snapshot, main_bounds)) = snapshot else {
        return (Vec::new(), look_script(theme, None));
    };
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
    let tint = gpui::Rgba::from(
        crate::app::helpers::workspace_background_color()
            .opacity(crate::app::helpers::work_area_glass_alpha()),
    );
    let tint = format!(
        "rgba({},{},{},{:.3})",
        (tint.r * 255.0).round(),
        (tint.g * 255.0).round(),
        (tint.b * 255.0).round(),
        tint.a
    );
    (
        snapshot.jpeg,
        look_script(theme, Some((&size, &position, &tint))),
    )
}

/// Shows `jpeg` behind the page, or takes the picture away when it is empty.
fn picture_script(jpeg: &[u8]) -> String {
    if jpeg.is_empty() {
        return "(function(){var d=document.documentElement;d.removeAttribute(\"data-gx-glass\");\
d.style.removeProperty(\"--gx-glass-picture\");})();"
            .to_string();
    }
    let picture = base64::engine::general_purpose::STANDARD.encode(jpeg);
    format!(
        "(function(){{var d=document.documentElement;d.setAttribute(\"data-gx-glass\",\"\");\
d.style.setProperty(\"--gx-glass-picture\",'url(\"data:image/jpeg;base64,{picture}\")');}})();"
    )
}

/// The app's theme (unless the reader or the page itself chose one, `data-gx-theme-from`, kit.rs),
/// and with a picture its size, position and the tint for that theme; the other theme's tint is
/// left to the page's default.
fn look_script(theme: &str, glass: Option<(&str, &str, &str)>) -> String {
    let other = if theme == "light" { "dark" } else { "light" };
    let glass = match glass {
        Some((size, position, tint)) => format!(
            "s.setProperty(\"--gx-glass-size\",\"{size}\");s.setProperty(\"--gx-glass-position\",\"{position}\");\
s.setProperty(\"--gx-glass-tint-{theme}\",\"{tint}\");s.removeProperty(\"--gx-glass-tint-{other}\");"
        ),
        None => "s.removeProperty(\"--gx-glass-tint-light\");s.removeProperty(\"--gx-glass-tint-dark\");"
            .to_string(),
    };
    format!(
        "(function(){{var d=document.documentElement,s=d.style;{glass}\
if(d.hasAttribute(\"data-gx-theme-from\")||d.getAttribute(\"data-gx-theme\")===\"{theme}\")return;\
d.setAttribute(\"data-gx-theme\",\"{theme}\");\
Array.prototype.forEach.call(document.querySelectorAll(\".gx-theme-switch [data-gx-set-theme]\"),function(b){{\
b.setAttribute(\"aria-pressed\",b.getAttribute(\"data-gx-set-theme\")===\"{theme}\"?\"true\":\"false\");}});}})();"
    )
}
