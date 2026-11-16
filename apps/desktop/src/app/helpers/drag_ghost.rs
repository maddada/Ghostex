//! The picture that follows the pointer while something is dragged: the sidebar's dragged session
//! row, shared by every drag ghost in the app so they all read as the same see-through glass.

use gpui::{
    AnyElement, Div, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Styled, div, px,
};

use crate::app::helpers::*;

/// CDXC:DragGhost 2026-11-13 DECISION:
/// User: "this isn't glassy looking when I drag it, please make the ghost of the dragged item match
/// the ghost of a dragged item from the sidebar." The Work panel's card, tabs, pane chips, board
/// and list rows drew their own opaque near-black or bordered, shadowed pills. Every drag ghost now
/// takes the sidebar session ghost's look from here: the chrome fill lifted 6% toward white, drawn
/// at 78% so what is under it (and the drop line beside the pointer) shows through, 5pt corners,
/// no border or shadow, and the sidebar's light Inter title.
pub(crate) const DRAG_GHOST_OPACITY: f32 = 0.78;
/// A sidebar session row's height, which a title-only ghost keeps.
pub(crate) const DRAG_GHOST_HEIGHT: f32 = 34.0;
pub(crate) const DRAG_GHOST_RADIUS: f32 = 5.0;
pub(crate) const DRAG_GHOST_TEXT_SIZE: f32 = 15.55;

/// The ghost's fill before its opacity.
pub(crate) fn drag_ghost_fill() -> Hsla {
    titlebar_background().blend(gpui::rgb(0xffffff).opacity(0.06).into())
}

/// The sidebar session title's colour.
pub(crate) fn drag_ghost_text_color() -> Hsla {
    chrome_color(0xb4b8c0, 0x262626).into()
}

/// The glass box alone (fill, corners, opacity), for a ghost that keeps its own content and size,
/// like a dragged tab.
pub(crate) fn drag_ghost_surface<E: Styled>(element: E, scale: f32) -> E {
    element
        .rounded(px(DRAG_GHOST_RADIUS * scale))
        .bg(drag_ghost_fill())
        .opacity(DRAG_GHOST_OPACITY)
}

/// The glass box with the sidebar's title text.
pub(crate) fn drag_ghost_styled<E: Styled>(element: E, scale: f32) -> E {
    drag_ghost_surface(element, scale)
        .font_family(crate::ui_fonts::UI_FONT)
        .font_weight(FontWeight::LIGHT)
        .text_size(px(DRAG_GHOST_TEXT_SIZE * scale))
        .text_color(drag_ghost_text_color())
}

/// A dragged item drawn as a sidebar session row: an optional leading icon and its title on one
/// truncated line, `width` wide (or as wide as its title, up to a default-width sidebar).
pub(crate) fn drag_ghost(
    title: impl Into<gpui::SharedString>,
    leading: Option<AnyElement>,
    width: Option<Pixels>,
    scale: f32,
) -> Div {
    let row = drag_ghost_styled(div(), scale)
        .flex()
        .items_center()
        .min_w_0()
        .h(px(DRAG_GHOST_HEIGHT * scale))
        .pr(px(10.0 * scale))
        .gap(px(6.0 * scale));
    let row = match width {
        Some(width) => row.w(width),
        None => row.max_w(px(crate::app::consts::DEFAULT_SIDEBAR_WIDTH * scale)),
    };
    let row = match leading {
        Some(icon) => row.pl(px(5.0 * scale)).child(
            div()
                .size(px(15.0 * scale))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(icon),
        ),
        None => row.pl(px(10.0 * scale)),
    };
    row.child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(title.into().into_any_element()),
    )
}
