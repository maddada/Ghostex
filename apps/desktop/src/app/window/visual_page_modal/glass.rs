//! The picture behind the app's glass and the work area's tint over it, handed to the floating
//! window's page so the page's backdrop looks like the window it floats over: the reader's own
//! wallpaper or custom picture under the same tint (server/src/visual_pages/theme.rs paints it
//! under the page's glass cards). While the window is open it follows the app: a new wallpaper,
//! another glass setting, a new tint or the app's theme reach the page within a second, and a
//! move or resize of the window at once. The window paints the same picture, placed the same way,
//! behind its own header (`Backdrop`), so the header and the page read as one surface.
//!
//! CDXC:SessionChat 2026-11-02 DECISION:
//! User picked "A" on Q987: hand the page the picture behind its glass, so every page shows the reader's wallpaper (or their custom picture) the way the app's own glass does, instead of the painted dusk. Later the same day: "The bg of this isn't changing per different bg that I apply on my desktop" and "make it darker bg on dark mode": the live blur ("Desktop and windows") now hands over the desktop picture too, the page dims it with the work area's own tint (`windowGlassWorkAreaTintDark` / `Light` over the workspace colour) instead of a fixed purple, and the page follows changes while it is open. Video and live-style glass, glass off, the web build, the phone and pages opened elsewhere keep the painted dusk or dawn.
//! CDXC:SessionChat 2026-11-02 WHY:
//! The page can't be see-through (a windowed CEF browser always paints opaque), so it gets the main window's own blurred picture instead: GPUI's `background_wallpaper_snapshot`, a JPEG of a few dozen kilobytes, put into the page by a page script as a data URL. A gxserver address would be refused by the page's content policy (`img-src data: blob: https:`), and the picture belongs to this computer, not to the gxserver that serves the page. The picture is sent again only when its bytes change (GPUI keeps the last JPEG, so asking every second costs no encoding); the tint, placement and theme go in a small script of their own.
//! CDXC:SessionChat 2026-11-04 WHY:
//! User: "why are the bgs of inline htmls still not showing my real wallpaper but blurred like we agreed?". The scripts above never took: a script added for new documents (`Page.addScriptToEvaluateOnNewDocument`) runs before the parser has made `<html>`, so `document.documentElement` was null and both scripts threw (`TypeError: Cannot read properties of null (reading 'setAttribute')`, reproduced in Chromium with the same DevTools call), and the page kept the painted dusk. They were only ever sent again when the picture or the look changed, so the page window never showed the picture unless the wallpaper changed while it was open. Each script now waits for `<html>` (`on_root`), which still comes before the page's first script and first paint.
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User: "can we make the titlebar part of the inline html instead of being a different color?" and "want to be able to resize and move them". The window paints the page's backdrop behind its header too: this picture under this tint at the same place on screen, or the page's own top colour without one (`PAGE_TOP_DARK` / `PAGE_TOP_LIGHT`). The picture is placed for the whole window and the page is told its part of it (`picture_rect`), so it runs on unbroken from the header into the page. While the window moves or is resized the new placement goes straight into the current page (`execute_app_owned_script`); the scripts kept for later documents are only added again on the once-a-second check, so a long drag does not pile them up.
//! SEE-ALSO: .dependencies/zed/crates/gpui_macos/src/window_wallpaper.rs (`jpeg`), server/src/visual_pages/theme.rs (`data-gx-glass`, `--gx-glass-tint-dark` / `-light`, `--gx-backdrop-top`).

use super::{GpuiVisualPageModalWindow, HEADER_HEIGHT};
use base64::Engine as _;
use gpui::{App, Bounds, Context, Pixels, Point, Rgba, Size, Window, point, px, size};
use std::{sync::Arc, time::Duration};

/// The picture's largest width in pixels: under the glass's blur no finer detail survives.
const PICTURE_MAX_WIDTH: u32 = 640;

/// How often the open window checks whether the app's glass or theme changed.
const GLASS_CHECK_EVERY: Duration = Duration::from_secs(1);

/// The top of the page's painted dusk (dark) and dawn (light), which the header paints when there
/// is no picture: `--gx-backdrop-top` in server/src/visual_pages/theme.rs, where the floating
/// window's page fades its backdrop into this colour at its top edge.
pub(super) const PAGE_TOP_DARK: u32 = 0x131313;
pub(super) const PAGE_TOP_LIGHT: u32 = 0xf0f0f0;

/// What the page was last handed.
#[derive(Default)]
pub(super) struct PageGlass {
    /// The picture's JPEG, empty for none; `None` before the first hand-over.
    picture: Option<Vec<u8>>,
    /// The look script last added for the documents the page loads later.
    kept_look: Option<String>,
    /// The look the current document was last given.
    shown_look: Option<String>,
    /// What the window paints behind its header, while the page has a picture.
    pub(super) backdrop: Option<Backdrop>,
    /// The app's theme as of the last check, which picks the header's colour without a picture.
    pub(super) light: bool,
}

/// The picture behind the header: the page's own, at the same place, under the same tint.
pub(super) struct Backdrop {
    pub(super) image: Arc<gpui::Image>,
    /// Where the picture lies, in the window's coordinates.
    pub(super) rect: Bounds<Pixels>,
    pub(super) tint: Rgba,
}

/// The glass the page gets: its picture and where it lies in the window, and the work area's tint.
struct Glass {
    jpeg: Vec<u8>,
    rect: Bounds<Pixels>,
    tint: Rgba,
}

impl PageGlass {
    /// Nothing handed yet, in the app's theme as the window opens (`light`).
    pub(super) fn new(light: bool) -> Self {
        Self {
            light,
            ..Self::default()
        }
    }
}

impl GpuiVisualPageModalWindow {
    /// Hands the new page its glass and keeps it in step while the window is open.
    pub(super) fn follow_page_glass(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_page_glass(true, window, cx);
        self._glass_watch = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(GLASS_CHECK_EVERY).await;
                if this
                    .update_in(cx, |this, window, cx| {
                        this.sync_page_glass(true, window, cx)
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
        self._glass_bounds = Some(cx.observe_window_bounds(window, |this, window, cx| {
            this.sync_page_glass(false, window, cx);
        }));
    }

    /// `keep`: the look also goes into the documents the page loads later (a reload, Refresh);
    /// otherwise only into the current one.
    fn sync_page_glass(&mut self, keep: bool, window: &Window, cx: &mut Context<Self>) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        let light = crate::app::helpers::chrome_uses_light_appearance();
        let glass = page_glass(window, cx);
        let jpeg = glass
            .as_ref()
            .map(|glass| glass.jpeg.clone())
            .unwrap_or_default();
        // Page scripts run in the current document and in every one it loads later, in the order
        // they were added, so a reload ends on the latest of each.
        if self.glass.picture.as_ref() != Some(&jpeg) {
            surface.read(cx).add_page_script(&picture_script(&jpeg));
            self.glass.picture = Some(jpeg);
        }
        let look = look_script(
            if light { "light" } else { "dark" },
            glass.as_ref().map(|glass| (glass.rect, glass.tint)),
        );
        if keep && self.glass.kept_look.as_ref() != Some(&look) {
            surface.read(cx).add_page_script(&look);
            self.glass.kept_look = Some(look.clone());
            self.glass.shown_look = Some(look);
        } else if self.glass.shown_look.as_ref() != Some(&look) {
            surface.update(cx, |surface, _| surface.execute_app_owned_script(&look));
            self.glass.shown_look = Some(look);
        }
        let backdrop = glass.map(|glass| Backdrop {
            image: match &self.glass.backdrop {
                Some(backdrop) if backdrop.image.bytes() == glass.jpeg.as_slice() => {
                    backdrop.image.clone()
                }
                _ => Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Jpeg, glass.jpeg)),
            },
            rect: glass.rect,
            tint: glass.tint,
        });
        let changed = match (&self.glass.backdrop, &backdrop) {
            (Some(old), Some(new)) => {
                old.rect != new.rect || old.tint != new.tint || !Arc::ptr_eq(&old.image, &new.image)
            }
            (None, None) => false,
            _ => true,
        } || self.glass.light != light;
        self.glass.backdrop = backdrop;
        self.glass.light = light;
        if changed {
            cx.notify();
        }
    }
}

/// The picture behind the main window's glass, where it lies in the page window `window`, and the
/// work area's tint; `None` while the glass shows no picture (glass off, a video or a live style).
fn page_glass(window: &Window, cx: &mut App) -> Option<Glass> {
    let (snapshot, main_bounds) = crate::app::helpers::window_glass_active()
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
        .and_then(|(snapshot, main_bounds)| Some((snapshot?, main_bounds)))?;
    let bounds = window.bounds();
    // Held against the screen (and always under the live blur, which is the screen behind the
    // window), the page window shows the part of the screen it sits over, like the main window
    // does; attached to the window, the picture covers the page window. The page window and the
    // main window share one screen and one coordinate space, so the page window sits on the
    // screen where the main window does, moved by their difference.
    let on_screen = (crate::app::helpers::window_glass_shows_live_blur()
        || crate::app::helpers::window_glass_picture_follows_screen())
    .then(|| {
        let offset: Point<Pixels> = bounds.origin - main_bounds.origin;
        snapshot.window.origin + offset
    });
    let tint = Rgba::from(
        crate::app::helpers::workspace_background_color()
            .opacity(crate::app::helpers::work_area_glass_alpha()),
    );
    Some(Glass {
        jpeg: snapshot.jpeg,
        rect: picture_rect(bounds.size, snapshot.screen, on_screen),
        tint,
    })
}

/// Where a picture laid out on a whole screen of `screen` lies in a window of `window_size`, in
/// the window's coordinates: held against the screen when the window's top-left corner is
/// `on_screen` there, else covering the window, centred.
fn picture_rect(
    window_size: Size<Pixels>,
    screen: Size<Pixels>,
    on_screen: Option<Point<Pixels>>,
) -> Bounds<Pixels> {
    if let Some(origin) = on_screen {
        return Bounds::new(point(-origin.x, -origin.y), screen);
    }
    let scale = (f32::from(window_size.width) / f32::from(screen.width).max(1.0))
        .max(f32::from(window_size.height) / f32::from(screen.height).max(1.0));
    let picture = size(screen.width * scale, screen.height * scale);
    Bounds::new(
        point(
            (window_size.width - picture.width) / 2.0,
            (window_size.height - picture.height) / 2.0,
        ),
        picture,
    )
}

/// Runs `body` with `d` set to the page's `<html>`, as soon as there is one. A script added for
/// new documents runs before the parser has made it (CDXC:SessionChat 2026-11-04 WHY above); the
/// observer's callback still runs before the page's first script.
fn on_root(body: &str) -> String {
    format!(
        "(function(){{function run(d){{{body}}}var d=document.documentElement;if(d){{run(d);return;}}\
new MutationObserver(function(_,o){{var d=document.documentElement;if(d){{o.disconnect();run(d);}}}})\
.observe(document,{{childList:true}});}})();"
    )
}

/// Shows `jpeg` behind the page, or takes the picture away when it is empty.
fn picture_script(jpeg: &[u8]) -> String {
    if jpeg.is_empty() {
        return on_root(
            "d.removeAttribute(\"data-gx-glass\");d.style.removeProperty(\"--gx-glass-picture\");",
        );
    }
    let picture = base64::engine::general_purpose::STANDARD.encode(jpeg);
    on_root(&format!(
        "d.setAttribute(\"data-gx-glass\",\"\");\
d.style.setProperty(\"--gx-glass-picture\",'url(\"data:image/jpeg;base64,{picture}\")');"
    ))
}

/// The app's theme (unless the reader or the page itself chose one, `data-gx-theme-from`, kit.rs),
/// and with a picture where it lies (`rect`, in the window's coordinates; the page starts under the
/// header) and the tint for that theme; the other theme's tint is left to the page's default.
fn look_script(theme: &str, glass: Option<(Bounds<Pixels>, Rgba)>) -> String {
    let other = if theme == "light" { "dark" } else { "light" };
    let glass = match glass {
        Some((rect, tint)) => format!(
            "s.setProperty(\"--gx-glass-size\",\"{}px {}px\");s.setProperty(\"--gx-glass-position\",\"{}px {}px\");\
s.setProperty(\"--gx-glass-tint-{theme}\",\"{}\");s.removeProperty(\"--gx-glass-tint-{other}\");",
            css_px(rect.size.width),
            css_px(rect.size.height),
            css_px(rect.origin.x),
            css_px(rect.origin.y - px(HEADER_HEIGHT)),
            css_rgba(tint),
        ),
        None => "s.removeProperty(\"--gx-glass-tint-light\");s.removeProperty(\"--gx-glass-tint-dark\");"
            .to_string(),
    };
    on_root(&format!(
        "var s=d.style;{glass}\
if(d.hasAttribute(\"data-gx-theme-from\")||d.getAttribute(\"data-gx-theme\")===\"{theme}\")return;\
d.setAttribute(\"data-gx-theme\",\"{theme}\");\
Array.prototype.forEach.call(document.querySelectorAll(\".gx-theme-switch [data-gx-set-theme]\"),function(b){{\
b.setAttribute(\"aria-pressed\",b.getAttribute(\"data-gx-set-theme\")===\"{theme}\"?\"true\":\"false\");}});"
    ))
}

fn css_px(value: Pixels) -> String {
    format!("{}", (f32::from(value) * 100.0).round() / 100.0)
}

fn css_rgba(tint: Rgba) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        (tint.r * 255.0).round(),
        (tint.g * 255.0).round(),
        (tint.b * 255.0).round(),
        tint.a
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_held_against_the_screen_sits_where_the_window_is_on_it() {
        let rect = picture_rect(
            size(px(900.0), px(700.0)),
            size(px(1728.0), px(1117.0)),
            Some(point(px(400.0), px(150.0))),
        );
        assert_eq!(rect.origin, point(px(-400.0), px(-150.0)));
        assert_eq!(rect.size, size(px(1728.0), px(1117.0)));
    }

    #[test]
    fn a_picture_attached_to_the_window_covers_it_centred() {
        // A tall window over a wide screen: the height decides, the sides are cut evenly.
        let rect = picture_rect(
            size(px(600.0), px(1000.0)),
            size(px(1600.0), px(1000.0)),
            None,
        );
        assert_eq!(rect.size, size(px(1600.0), px(1000.0)));
        assert_eq!(rect.origin, point(px(-500.0), px(0.0)));
        // A wide window: the width decides, top and bottom are cut evenly.
        let rect = picture_rect(
            size(px(2000.0), px(500.0)),
            size(px(1000.0), px(1000.0)),
            None,
        );
        assert_eq!(rect.size, size(px(2000.0), px(2000.0)));
        assert_eq!(rect.origin, point(px(0.0), px(-750.0)));
    }

    #[test]
    fn the_page_is_told_its_part_of_the_window_picture() {
        let look = look_script(
            "dark",
            Some((
                Bounds::new(point(px(-400.0), px(-150.0)), size(px(1728.0), px(1117.0))),
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.77,
                },
            )),
        );
        // The page starts under the header, so its picture starts that much higher.
        let top = -150.0 - HEADER_HEIGHT;
        assert!(
            look.contains(&format!("\"--gx-glass-position\",\"-400px {top}px\"")),
            "{look}"
        );
        assert!(
            look.contains("\"--gx-glass-size\",\"1728px 1117px\""),
            "{look}"
        );
        assert!(
            look.contains("\"--gx-glass-tint-dark\",\"rgba(0,0,0,0.770)\""),
            "{look}"
        );
    }

    #[test]
    fn every_page_script_waits_for_the_html_element() {
        for script in [
            picture_script(b"jpeg"),
            picture_script(&[]),
            look_script("light", None),
        ] {
            assert!(script.contains("var d=document.documentElement;if(d){run(d);return;}"));
            assert!(script.contains("new MutationObserver"));
            // Nothing touches `<html>` outside the function that is handed it.
            let outside = script.split_once("function run(d){").map(|(head, _)| head);
            assert_eq!(outside, Some("(function(){"));
        }
    }
}
