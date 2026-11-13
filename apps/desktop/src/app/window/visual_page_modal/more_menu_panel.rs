//! The page window's ⋯ menu panel (`more_menu.rs`), built as the sessions sidebar's menu is
//! (`native_sidebar/menus.rs`): its rows' height, a 6px inset, 8px corners, 16px icons, and the
//! shared popup menu colours, which the caller passes in. It depends on nothing of the window, so
//! the native modal preview draws this same panel (`bin/native_modal_demo/page_window_menu.rs`).

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use std::rc::Rc;

/// The menu's width and inset.
pub(crate) const MENU_WIDTH: f32 = 210.0;
const MENU_PADDING: f32 = 6.0;

/// The rows: label and icon, in order.
pub(crate) const ROWS: [(&str, &str); 3] = [
    ("Export page", "titlebar/download.svg"),
    ("Import answers", "titlebar/upload.svg"),
    ("Open in browser", "titlebar/external-link.svg"),
];

/// The menu's colours and row height.
#[derive(Clone, Copy)]
pub(crate) struct MenuInk {
    pub(crate) foreground: gpui::Hsla,
    pub(crate) hover: gpui::Hsla,
    pub(crate) border: gpui::Hsla,
    pub(crate) fill: gpui::Hsla,
    pub(crate) row_height: f32,
}

/// The panel's height for rows `row_height` tall, its border included.
pub(crate) fn height(row_height: f32) -> f32 {
    ROWS.len() as f32 * row_height + 2.0 * MENU_PADDING + 2.0
}

/// The panel, filling what holds it; `pick` gets the index of the row pressed.
pub(crate) fn panel(ink: MenuInk, pick: Rc<dyn Fn(usize, &mut App)>) -> AnyElement {
    let hover = ink.hover;
    div()
        .id("visual-page-more-menu")
        .role(gpui::Role::Menu)
        .aria_label("Page actions")
        .font_family(crate::ui_fonts::UI_FONT)
        .size_full()
        .flex()
        .flex_col()
        .p(px(MENU_PADDING))
        .rounded(px(8.0))
        .border_1()
        .border_color(ink.border)
        .bg(ink.fill)
        .text_color(ink.foreground)
        .font_weight(gpui::FontWeight::NORMAL)
        .text_size(px(13.0))
        .children(ROWS.into_iter().enumerate().map(|(index, (label, icon))| {
            let pick = pick.clone();
            div()
                .id(gpui::SharedString::from(format!(
                    "visual-page-more:{index}"
                )))
                .role(gpui::Role::MenuItem)
                .aria_label(label)
                .w_full()
                .h(px(ink.row_height))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(10.0))
                .gap(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .child(
                    svg()
                        .path(icon)
                        .size(px(16.0))
                        .flex_shrink_0()
                        .text_color(ink.foreground),
                )
                .child(div().flex_1().min_w_0().truncate().child(label))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    pick(index, cx);
                })
        }))
        .into_any_element()
}
