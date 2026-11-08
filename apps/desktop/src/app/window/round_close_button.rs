//! The round close (×) buttons drawn over a corner: the toast's dismiss button and the coordinator
//! board card's remove button share these colours and this glyph, so the two always match.
//! Pure gpui (no `crate::*`), so `native_modal_demo` can include this file for its preview.
use gpui::{Hsla, Styled as _, Svg, px, rgb, svg};

/// The toast's round × is a see-through light gray in both appearances, with a dark gray ×.
///
/// CDXC:Theming 2026-11-04 DECISION:
/// 2026-11-08 follow-up, User: "the x button here is so dim i can't tell there's an x without squinting please make it 1px thicker here and for the toasts in the gpui app". The × is drawn 1px thicker on both buttons (`round-close-x-board.svg`: 1.83px at its 11px size; `round-close-x-toast.svg`: 1.67px at its 8px size, up from 0.83px and 0.67px) and its gray is a step darker (dark: 0x2b2b2b to 0x151515; light: 0x4a4a4a to 0x2a2a2a) so it separates from the circle; the circle keeps its fill and border.
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

/// Which round close button a glyph is drawn for.
#[derive(Clone, Copy)]
pub(crate) enum RoundCloseXSize {
    /// The toast's 18px dismiss button (the glyph size is scaled by `scale`).
    Toast { scale: f32 },
    /// The board card's 20px remove button (the glyph size is scaled by `scale`).
    BoardCard { scale: f32 },
}

pub(crate) fn round_close_x_icon(size: RoundCloseXSize, color: Hsla) -> Svg {
    let (path, px_size) = match size {
        RoundCloseXSize::Toast { scale } => ("titlebar/round-close-x-toast.svg", 8.0 * scale),
        RoundCloseXSize::BoardCard { scale } => ("titlebar/round-close-x-board.svg", 11.0 * scale),
    };
    svg().size(px(px_size)).path(path).text_color(color)
}
