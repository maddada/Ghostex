//! The page window's ⋯ menu panel and its Snooze submenu (`more_menu.rs`), built as the sessions
//! sidebar's menu is (`native_sidebar/menus.rs`): its rows' height, a 6px inset, 8px corners,
//! 16px icons, a chevron on a row that opens a submenu, and the shared popup menu colours, which
//! the caller passes in. It depends on nothing of the window, so the native modal preview draws
//! this same panel (`bin/native_modal_demo/page_window_menu.rs`).

use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use std::rc::Rc;

/// The menu's width and inset; the Snooze lengths' narrower width.
pub(crate) const MENU_WIDTH: f32 = 210.0;
pub(crate) const SUBMENU_WIDTH: f32 = 150.0;
const MENU_PADDING: f32 = 6.0;

/// One row: its label, its icon, and whether it opens a submenu (a chevron at its right).
#[derive(Clone, Copy)]
pub(crate) struct MenuRow {
    pub(crate) label: &'static str,
    pub(crate) icon: Option<&'static str>,
    pub(crate) submenu: bool,
}

/// The menu's rows, in order: Snooze first, shown only while the page waits for answers.
pub(crate) fn menu_rows(snooze: bool) -> Vec<MenuRow> {
    let row = |label, icon| MenuRow {
        label,
        icon: Some(icon),
        submenu: false,
    };
    // Snooze first, always (User: "put snooze as top option here").
    let mut rows = Vec::with_capacity(4);
    if snooze {
        rows.push(MenuRow {
            label: "Snooze",
            icon: Some("titlebar/alarm.svg"),
            submenu: true,
        });
    }
    rows.extend([
        row("Export page", "titlebar/download.svg"),
        row("Import answers", "titlebar/upload.svg"),
        row("Open in browser", "titlebar/external-link.svg"),
    ]);
    rows
}

/// Snooze's lengths, as the dock offers them (gx-chat-core extras/work_cards.rs `snooze_choices`).
pub(crate) const SNOOZE_HOURS: [(u64, &str); 3] = [(1, "1 hour"), (6, "6 hours"), (24, "24 hours")];

pub(crate) fn snooze_rows() -> Vec<MenuRow> {
    SNOOZE_HOURS
        .iter()
        .map(|(_, label)| MenuRow {
            label,
            icon: None,
            submenu: false,
        })
        .collect()
}

/// The menu's colours and row height.
#[derive(Clone, Copy)]
pub(crate) struct MenuInk {
    pub(crate) foreground: gpui::Hsla,
    pub(crate) muted: gpui::Hsla,
    pub(crate) hover: gpui::Hsla,
    pub(crate) border: gpui::Hsla,
    pub(crate) fill: gpui::Hsla,
    pub(crate) row_height: f32,
}

/// A panel's height for `rows` rows `row_height` tall, its border included.
pub(crate) fn height(rows: usize, row_height: f32) -> f32 {
    rows as f32 * row_height + 2.0 * MENU_PADDING + 2.0
}

/// A panel of `rows`, filling what holds it; `pick` gets the index of the row pressed, `open` marks
/// the submenu row whose submenu is open.
pub(crate) fn panel(
    id: &'static str,
    ink: MenuInk,
    rows: Vec<MenuRow>,
    open: Option<usize>,
    pick: Rc<dyn Fn(usize, &mut App)>,
) -> AnyElement {
    let hover = ink.hover;
    div()
        .id(id)
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
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            let pick = pick.clone();
            let mut item = div()
                .id(gpui::SharedString::from(format!("{id}:{index}")))
                .role(gpui::Role::MenuItem)
                .aria_label(row.label)
                .w_full()
                .h(px(ink.row_height))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(10.0))
                .gap(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover));
            if open == Some(index) {
                item = item.bg(hover);
            }
            if let Some(icon) = row.icon {
                item = item.child(
                    svg()
                        .path(icon)
                        .size(px(16.0))
                        .flex_shrink_0()
                        .text_color(ink.foreground),
                );
            }
            item = item.child(div().flex_1().min_w_0().truncate().child(row.label));
            if row.submenu {
                item = item.child(
                    svg()
                        .path("titlebar/chevron-right.svg")
                        .size(px(14.0))
                        .flex_shrink_0()
                        .text_color(ink.muted),
                );
            }
            item.on_click(move |_, _, cx| {
                cx.stop_propagation();
                pick(index, cx);
            })
        }))
        .into_any_element()
}
