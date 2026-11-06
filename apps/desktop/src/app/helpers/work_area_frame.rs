//! The work area frame (`workAreaFrame`, Settings > Interface > Frame the work area): the work area
//! drawn as one card inside the window, with a gutter all around it, a hairline border and a light
//! shadow. The elements that draw it are in `app/element/work_area_frame.rs`.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{BoxShadow, Hsla, Pixels, Point, point, px};

use crate::app::helpers::*;

/// CDXC:Workarea 2026-11-05 DECISION:
/// Maintainer on GitHub issue #407: "Sure go ahead and implement the frame effect as an option in settings but please test it well and send me screenshots of the final thing in the PR. I want the current one I have now to remain default though please (should remain as it looks now)". The frame is an opt-in setting, off by default, and off the window's elements, styles and native clipping are exactly what they were.
///
/// CDXC:Workarea 2026-11-05 WHY:
/// The gutter is 6pt on every side, enough to read the card as its own surface without taking room from the work area. The card's radius is the window's corner radius less the gutter (10.5pt), so the two corners run at the same distance, and it stays 10.5pt in full screen, where the window has no corner; what sits inside the card's 1pt border rounds at 9.5pt. The border is the chrome ink at 6% in dark mode and under dark glass and the theme's divider line in light mode, and the shadow is light. The card keeps its background and the glass keeps its tints, and the frame stays with the sidebar hidden and in full screen. macOS only: elsewhere the work area's web pages cannot be clipped to the card's corners (packages/settings-catalog/src/availability.rs).
/// SEE-ALSO: app/element/work_area_frame.rs (the card and its glass gutter), app/element/window_corner_pane.rs (panes round to the card), native/macos/GpuiWindowCorners.h (web pages clip to the card).
static WORK_AREA_FRAME: AtomicBool = AtomicBool::new(false);

/// The gutter between the card and the window's edges (and the sidebar), in points.
pub(crate) const WORK_AREA_FRAME_MARGIN: f32 = 6.0;

/// The card's border width, in points.
pub(crate) const WORK_AREA_FRAME_BORDER: f32 = 1.0;

/// The card's corner radius: the window's 16.5pt corner (`MACOS_WINDOW_CORNER_RADIUS`) less the
/// gutter, so the card's corner and the window's run at the same distance.
pub(crate) const WORK_AREA_FRAME_RADIUS: f32 = 16.5 - WORK_AREA_FRAME_MARGIN;

/// The radius of what sits inside the card's border (panes, web pages).
pub(crate) const WORK_AREA_FRAME_INNER_RADIUS: f32 =
    WORK_AREA_FRAME_RADIUS - WORK_AREA_FRAME_BORDER;

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

/// The width the frame's gutter takes from the work area beside the sidebar: a gutter on each side
/// of the card, less the divider's 1pt that counts toward the left one. 0 with the frame off.
pub(crate) fn work_area_frame_gutter_width() -> f32 {
    if work_area_frame_active() {
        2.0 * WORK_AREA_FRAME_MARGIN - crate::app::consts::SIDEBAR_DIVIDER_WIDTH
    } else {
        0.0
    }
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

/// Where a workspace window's traffic lights sit: (11, 11.5) as always, one gutter lower and one
/// gutter further in with the work area frame on, where the body row, the sidebar's top row and the
/// card's header all start one gutter down and the card one gutter in.
///
/// CDXC:Workarea 2026-11-05 WHY:
/// With the frame on, the body row takes the gutter on top too, so the sidebar's top row and the card's header start one gutter lower and the card one gutter in. The lights move by the same gutter, down and in, so they keep the relation to both rows and to the window's edge that they have without the frame; left where they were, they sat 6pt above the buttons beside them.
/// SEE-ALSO: app/workspace_windows/open.rs (the window option), app/element/work_area_frame.rs (`sync_work_area_frame_traffic_lights`).
pub(crate) fn workspace_traffic_light_position() -> Point<Pixels> {
    let gutter = if work_area_frame_active() {
        WORK_AREA_FRAME_MARGIN
    } else {
        0.0
    };
    point(px(11.0 + gutter), px(11.5 + gutter))
}

/// The sidebar toggle's window x, whichever row draws it (the docked sidebar's top row or the
/// header while the sidebar is hidden), one gutter further in with the work area frame on, where
/// the traffic lights moved in by a gutter too (`workspace_traffic_light_position`).
/// `lights_reserved` is `GhostexGpuiApp::window_controls_reserved`.
///
/// CDXC:Titlebar 2026-11-05 WHY:
/// Both rows read this one value, so the toggle keeps the distance from the green light that the 88pt reserve gives it (CDXC:Titlebar 2026-10-11 DECISION in consts.rs) and does not move when the sidebar opens or closes (CDXC:Titlebar 2026-09-20; on macOS see the 2026-11-06 decision below); the row's fit check (`sidebar_top_row_fits`) reads it as well, so a narrow sidebar drops buttons before its menu slides off the edge.
///
/// CDXC:Titlebar 2026-11-06 DECISION:
/// GitHub #432, maintainer: "seems very minor but sure do it why not". On macOS the window's traffic lights hide with the sidebar and show in the floating sidebar's top row instead (`floating_reveal_carries_window_controls`), so with the lights gone the toggle takes the x it has on Windows and Linux, the edge strip's width (16pt with the frame). User, the same day: in full screen the header's left group starts there too, and neither the docked nor the floating sidebar's top row keeps room for the lights, because macOS draws them only in the title bar that slides down on hover (`window_controls_reserved`). This supersedes, on macOS, the 2026-11-05 rule that the toggle does not move when the sidebar closes; only the docked sidebar outside full screen keeps the 88pt reserve.
pub(crate) fn sidebar_toggle_leading_x(lights_reserved: bool) -> f32 {
    sidebar_toggle_window_x(lights_reserved, work_area_frame_active())
}

/// `sidebar_toggle_leading_x` for an explicit frame setting.
pub(crate) fn sidebar_toggle_window_x(lights_reserved: bool, framed: bool) -> f32 {
    let gutter = if framed { WORK_AREA_FRAME_MARGIN } else { 0.0 };
    let base = if lights_reserved {
        crate::app::consts::SIDEBAR_TOGGLE_LEADING_X
    } else {
        crate::app::floating_reveal::model::FLOATING_REVEAL_EDGE_WIDTH
    };
    base + gutter
}

/// The narrowest the sidebar may be: `SIDEBAR_MIN_WIDTH`, one gutter wider with the work area
/// frame on. Dragging the divider, the double-click reset, the width restored at start and the
/// clamp on every frame all read it, so turning the frame on widens a sidebar below it at once.
/// Turning the frame off lowers only the floor: the width stays where it is.
///
/// CDXC:Sidebar 2026-11-05 WHY:
/// With the frame on the top row's reserve is 94pt (`sidebar_toggle_leading_x`), 6pt more than without it, and at 190pt with the sidebar zoomed to 90% the card cut the bell in half. Search, the bell and the menu stay in the top row at every width (CDXC:Sidebar 2026-10-11 DECISION in native_sidebar/navigation.rs), so the minimum grows by the same gutter, to 196pt.
pub(crate) fn sidebar_min_width() -> f32 {
    let gutter = if work_area_frame_active() {
        WORK_AREA_FRAME_MARGIN
    } else {
        0.0
    };
    crate::app::consts::SIDEBAR_MIN_WIDTH + gutter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hidden_sidebar_toggle_sits_on_the_edge_strip() {
        // With no lights to clear the toggle sits where it does on Windows and Linux: 10pt, one
        // gutter further in with the frame on.
        assert_eq!(sidebar_toggle_window_x(false, false), 10.0);
        assert_eq!(sidebar_toggle_window_x(false, true), 16.0);
    }

    #[test]
    fn the_docked_sidebar_toggle_clears_the_lights() {
        let reserve = crate::app::consts::SIDEBAR_TOGGLE_LEADING_X;
        assert_eq!(sidebar_toggle_window_x(true, false), reserve);
        assert_eq!(
            sidebar_toggle_window_x(true, true),
            reserve + WORK_AREA_FRAME_MARGIN
        );
        #[cfg(target_os = "macos")]
        assert_eq!(reserve, 88.0);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(reserve, 10.0);
    }
}
