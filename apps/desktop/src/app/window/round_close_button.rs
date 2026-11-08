//! The round close (×) buttons drawn over a corner: the toast's dismiss button and the coordinator
//! board card's remove button share these colours and this glyph, so the two always match.
//! Pure gpui (no `crate::*`), so `native_modal_demo` can include this file for its preview.
use gpui::{Hsla, Styled as _, Svg, px, rgb, svg};

/// The toast's round × is a see-through light gray in both appearances, with a dark gray ×.
///
/// CDXC:Theming 2026-11-04 DECISION:
/// 2026-11-08 follow-up, User: "the x button here is so dim i can't tell there's an x without squinting please make it 1px thicker here and for the toasts in the gpui app". The × is drawn 1px thicker on both buttons (`round-close-x.svg`: 1.83px at its 11px size; the toast's was 0.67px at 8px) and its gray is a step darker (dark: 0x2b2b2b to 0x151515; light: 0x4a4a4a to 0x2a2a2a) so it separates from the circle; the circle keeps its fill and border.
/// 2026-11-08 follow-up, User: "make both have the same as the board remove one". The toast's dismiss button is the board card's remove button: the same 20px circle, 11px × and colours, from this one module.
/// User, of the toast's close button: solid black stood out against the frosted toasts; it should be light gray so it fits the see-through UI. The fill keeps some of what is behind it in dark and light alike, its hover is a step stronger, and the × is dark gray so it still reads on the light fill.
pub(crate) fn gpui_app_toast_close_button_color(light: bool) -> Hsla {
    if light {
        rgb(0xe2e2e2).opacity(0.80).into()
    } else {
        rgb(0xc9c9c9).opacity(0.72).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_hover_color(light: bool) -> Hsla {
    if light {
        rgb(0xd2d2d2).opacity(0.92).into()
    } else {
        rgb(0xdedede).opacity(0.88).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_border_color(light: bool) -> Hsla {
    if light {
        rgb(0x000000).opacity(0.10).into()
    } else {
        rgb(0xffffff).opacity(0.28).into()
    }
}

pub(crate) fn gpui_app_toast_close_button_icon_color(light: bool) -> Hsla {
    if light {
        rgb(0x2a2a2a).into()
    } else {
        rgb(0x151515).into()
    }
}

/// The circle's size, shared by the board card's remove button and the toast's dismiss button.
pub(crate) const ROUND_CLOSE_BUTTON_SIZE: f32 = 20.0;

/// The × inside the circle, scaled by `scale` (the board's chat scale; the toast draws at 1.0).
pub(crate) fn round_close_x_icon(scale: f32, color: Hsla) -> Svg {
    svg()
        .size(px(11.0 * scale))
        .path("titlebar/round-close-x.svg")
        .text_color(color)
}
