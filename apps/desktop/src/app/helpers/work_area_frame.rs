//! The work area frame (`workAreaFrame`, Settings > Interface > Frame the work area): the work area
//! drawn as one card inside the window, with a gutter all around it, a hairline border and a light
//! shadow. The elements that draw it are in `app/element/work_area_frame.rs`.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{BoxShadow, Hsla, point, px};

use crate::app::helpers::*;

/// CDXC:Workarea 2026-11-05 DECISION:
/// Maintainer on GitHub issue #407: "Sure go ahead and implement the frame effect as an option in settings but please test it well and send me screenshots of the final thing in the PR. I want the current one I have now to remain default though please (should remain as it looks now)". The frame is an opt-in setting, off by default, and off the window's elements, styles and native clipping are exactly what they were.
///
/// CDXC:Workarea 2026-11-05 WHY:
/// The gutter is 6pt on every side, enough to read the card as its own surface without taking room from the work area. The card's radius is the window's corner radius less the gutter (10.5pt), so the two corners run at the same distance, and it stays 10.5pt in full screen, where the window has no corner; what sits inside the card's 1pt border rounds at 9.5pt. The border is the chrome ink at 6% in dark mode and under dark glass and the theme's divider line in light mode, and the shadow is light. The card keeps its background and the glass keeps its tints, and the frame stays with the sidebar hidden and in full screen. macOS only: elsewhere the work area's web pages cannot be clipped to the card's corners (packages/settings-catalog/src/availability.rs).
/// SEE-ALSO: app/element/work_area_frame.rs (the card and its glass gutter).
static WORK_AREA_FRAME: AtomicBool = AtomicBool::new(false);

/// The gutter between the card and the window's edges (and the sidebar), in points.
pub(crate) const WORK_AREA_FRAME_MARGIN: f32 = 6.0;

/// The card's border width, in points.
pub(crate) const WORK_AREA_FRAME_BORDER: f32 = 1.0;

/// The card's corner radius: the window's 16.5pt corner (`MACOS_WINDOW_CORNER_RADIUS`) less the
/// gutter, so the card's corner and the window's run at the same distance.
pub(crate) const WORK_AREA_FRAME_RADIUS: f32 = 16.5 - WORK_AREA_FRAME_MARGIN;

/// Reads `workAreaFrame` (off when unset).
pub(crate) fn refresh_work_area_frame(object: &serde_json::Map<String, serde_json::Value>) {
    WORK_AREA_FRAME.store(
        object
            .get("workAreaFrame")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        Ordering::Relaxed,
    );
}

/// Whether the work area is drawn as a card. The setting exists on macOS only.
pub(crate) fn work_area_frame_active() -> bool {
    cfg!(target_os = "macos") && WORK_AREA_FRAME.load(Ordering::Relaxed)
}

/// The card's 1pt border: the theme's divider line in light mode (its glass variant under glass),
/// the chrome ink at 6% in dark mode and under dark glass.
pub(crate) fn work_area_frame_border_color() -> Hsla {
    if chrome_uses_light_appearance() {
        sidebar_divider_line_color()
    } else {
        Hsla::from(chrome_ink()).opacity(0.06)
    }
}

/// The card's light shadow.
pub(crate) fn work_area_frame_shadow() -> Vec<BoxShadow> {
    let shadow = |alpha: f32, blur: f32| BoxShadow {
        color: gpui::black().opacity(alpha),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    if window_glass_active() {
        // GPUI paints a shadow under its element's whole shape. Under glass the card is
        // see-through, so a shadow would darken the wallpaper inside it; the border alone draws
        // the card's edge there.
        Vec::new()
    } else if chrome_uses_light_appearance() {
        vec![shadow(0.06, 2.0), shadow(0.05, 4.0)]
    } else {
        vec![shadow(0.45, 3.0)]
    }
}

/// The opaque gutter: the sidebar's own chrome gradient, painted by the body row from the window's
/// top edge to its bottom edge exactly as the sidebar paints it, so the gutter reads as sidebar.
pub(crate) fn work_area_frame_gutter_fill() -> gpui::Background {
    sidebar_chrome_fill(false, 180.0)
}
