//! The outlined split button every titlebar split button shares: Start, Open and Commit in the work area
//! header and the view panel's expand pair at the end of the view tab strip. The view tabs borrow its outline colour.

use gpui::Hsla;
use gpui::Styled;
use gpui::div;
use gpui::px;
use gpui::rgb;

use crate::app::helpers::chrome_uses_light_appearance;
use crate::app::helpers::titlebar_icon_hover_color;

/// Corner radius of the whole split button.
const TITLEBAR_SPLIT_BUTTON_RADIUS: f32 = 7.0;

/// The outline and the divider between the two halves.
///
/// CDXC:Titlebar 2026-09-25 DECISION:
/// User: "when we're in glass mode pls make the border color lighter". Under window glass the solid #252525 outline read as a dark groove on the frosted header, so it becomes a light see-through line (white 16% in dark mode, black 10% in light mode, paler than #d4d4d4); opaque keeps the solid colours.
pub(crate) fn titlebar_split_button_border_color() -> Hsla {
    let light = chrome_uses_light_appearance();
    if crate::app::helpers::window_glass_active() {
        return if light {
            gpui::black().opacity(0.10)
        } else {
            gpui::white().opacity(0.16)
        };
    }
    rgb(if light { 0xd4d4d4 } else { 0x252525 }).into()
}

/// CDXC:Titlebar 2026-09-25 DECISION:
/// User, of the frosted pill split buttons: "I don't like how they look", then from the split-button style mockup (docs/2026-09-23/titlebar-split-buttons): "do current for dark and interim for light mode pls". The four split buttons (Start, Open, Commit, and the view panel's expand pair) are a 7px-radius outline with a full-height divider: in dark mode a #252525 outline on the header's own surface, in light mode a #d4d4d4 outline on a white fill. A half's hover fill is 8% white in dark and 5% black in light, and an open or active half 11% white / 8% black. Supersedes the 2026-09-24 frosted "pill fill" (gradient, glass rim, inset divider, shadow).
pub(crate) fn titlebar_split_button_frame<E: Styled>(element: E, control_height: f32) -> E {
    element
        .h(px(control_height))
        .rounded(px(TITLEBAR_SPLIT_BUTTON_RADIUS))
        .border_1()
        .border_color(titlebar_split_button_border_color())
        .bg(if chrome_uses_light_appearance() {
            Hsla::white()
        } else {
            gpui::transparent_black()
        })
}

/// The corner radius a half's hover or open fill takes on the button's outer end, so the fill
/// follows the curve inside the 1px outline instead of showing square corners.
pub(crate) fn titlebar_split_button_segment_radius(_control_height: f32) -> gpui::Pixels {
    px(TITLEBAR_SPLIT_BUTTON_RADIUS - 1.0)
}

/// The full-height line between the two halves.
pub(crate) fn titlebar_split_button_divider(_control_height: f32) -> gpui::Div {
    div()
        .flex_shrink_0()
        .w(px(1.0))
        .h_full()
        .bg(titlebar_split_button_border_color())
}

/// A split button half under the pointer.
pub(crate) fn titlebar_split_button_hover_color() -> Hsla {
    if chrome_uses_light_appearance() {
        gpui::black().opacity(0.05)
    } else {
        gpui::white().opacity(0.08)
    }
}

/// A split button half whose menu is open, or the active expand cell.
pub(crate) fn titlebar_split_button_open_color() -> Hsla {
    if chrome_uses_light_appearance() {
        gpui::black().opacity(0.08)
    } else {
        gpui::white().opacity(0.11)
    }
}

/// The label and icon colour of a hovered or open half.
pub(crate) fn titlebar_split_button_hover_text_color() -> Hsla {
    titlebar_icon_hover_color()
}
