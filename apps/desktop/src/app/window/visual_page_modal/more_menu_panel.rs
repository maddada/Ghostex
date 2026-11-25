//! The page window's ⋯ menu panel and its Snooze submenu (`more_menu.rs`): raised glass with the
//! page's title on top and its rows in labelled sections, 32px rows, a 6px inset, 12px corners,
//! 16px icons and a chevron on a row that opens a submenu, in the colours the caller passes in. It
//! depends on nothing of the window, so the native modal preview draws this same panel
//! (`bin/native_modal_demo/page_window_menu.rs`).
//!
//! CDXC:SessionChat 2026-11-25 DECISION:
//! User, of the Live page window's ⋯ menu ("color of the bg of that menu and it's style in general is bit too basic"), picked from the mockup page (page-window/page.html, Q4904): "A (but bit darker bg in darkmode pls) + D + G". A: raised glass, a thinner grey glass over a stronger blur with a 14% hairline, a lit 1px top edge, 12px corners and a soft shadow (the host window's own, `more_menu.rs`), a 10% hover wash with 8px corners, and in dark mode a fill a step darker than A's. D: the rows in sections under 11px headings in the chat box ⋯ menu's style, "This page" (Snooze, Star page) and "Share" (Share link, Export page, Import answers), then a line before Open in browser. G: a header above them with the page's title and "Live page · <age>". Supersedes the sidebar menu's look (34px rows, 8px corners, no sections) this menu had since 2026-11-13.

use gpui::{
    AnyElement, App, BoxShadow, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, point, px, svg,
};
use std::rc::Rc;

/// The menu's width and inset; the Snooze lengths' narrower width.
pub(crate) const MENU_WIDTH: f32 = 232.0;
pub(crate) const SUBMENU_WIDTH: f32 = 150.0;
const MENU_PADDING: f32 = 6.0;
/// The panels' corners, which their host windows take too (`frosted_host.rs`
/// `PAGE_WINDOW_MENU_HOST_RADIUS`).
pub(crate) const MENU_RADIUS: f32 = 12.0;
pub(crate) const ROW_HEIGHT: f32 = 32.0;
const TITLE_HEIGHT: f32 = 44.0;
const HEADING_HEIGHT: f32 = 24.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const ROW_PADDING_X: f32 = 10.0;

/// One row: its label, its icon, and whether it opens a submenu (a chevron at its right).
#[derive(Clone, Copy)]
pub(crate) struct MenuRow {
    pub(crate) label: &'static str,
    pub(crate) icon: Option<&'static str>,
    pub(crate) submenu: bool,
}

/// What a panel shows, top to bottom: the page's title, section headings, lines between the
/// sections, and the rows a press picks.
#[derive(Clone)]
pub(crate) enum MenuLine {
    Title {
        title: SharedString,
        detail: SharedString,
    },
    Heading(&'static str),
    Separator,
    Row(MenuRow),
}

impl MenuLine {
    fn height(&self) -> f32 {
        match self {
            MenuLine::Title { .. } => TITLE_HEIGHT,
            MenuLine::Heading(_) => HEADING_HEIGHT,
            MenuLine::Separator => SEPARATOR_HEIGHT,
            MenuLine::Row(_) => ROW_HEIGHT,
        }
    }
}

/// The menu's lines: the page's title and age, then "This page" (Snooze, shown only while the page
/// waits for answers, and Star page), "Share" (Share link, Export page, Import answers) and Open in
/// browser. `star`: whether the page is starred (Unstar page) or not (Star page), `None` when the
/// page's session is not known, which leaves the row out. `age`: how long ago the page was
/// published ("2h ago"), `None` when its gxserver does not say.
pub(crate) fn menu_lines(
    title: &str,
    age: Option<&str>,
    snooze: bool,
    star: Option<bool>,
) -> Vec<MenuLine> {
    let row = |label, icon| {
        MenuLine::Row(MenuRow {
            label,
            icon: Some(icon),
            submenu: false,
        })
    };
    let title = title.trim();
    let mut lines = vec![
        MenuLine::Title {
            title: if title.is_empty() {
                "Untitled page".into()
            } else {
                title.to_string().into()
            },
            detail: match age {
                Some(age) => format!("Live page · {age}").into(),
                None => "Live page".into(),
            },
        },
        MenuLine::Separator,
    ];
    let mut this_page = Vec::with_capacity(2);
    // Snooze first, always (User: "put snooze as top option here").
    if snooze {
        this_page.push(MenuLine::Row(MenuRow {
            label: "Snooze",
            icon: Some("modals/settings/moon.svg"),
            submenu: true,
        }));
    }
    // CDXC:SessionChat 2026-11-16 DECISION: the user picked the ⋯ menu for the page window's
    // star (Q3545 on the star-resources page); the rest is on gx-chat-core questions/later.rs
    // `starred_page_ids`. It follows Snooze, which stays first so its lengths line up with it.
    match star {
        Some(true) => this_page.push(row("Unstar page", "titlebar/star-filled.svg")),
        Some(false) => this_page.push(row("Star page", "titlebar/star.svg")),
        None => {}
    }
    if !this_page.is_empty() {
        lines.push(MenuLine::Heading("This page"));
        lines.extend(this_page);
        lines.push(MenuLine::Separator);
    }
    lines.extend([
        MenuLine::Heading("Share"),
        row("Share link…", "titlebar/link.svg"),
        row("Export page", "titlebar/file-down.svg"),
        row("Import answers", "titlebar/file-up.svg"),
        MenuLine::Separator,
        row("Open in browser", "titlebar/external-link.svg"),
    ]);
    lines
}

/// Snooze's lengths, as the dock offers them (gx-chat-core extras/work_cards.rs `snooze_choices`).
pub(crate) const SNOOZE_HOURS: [(u64, &str); 3] = [(1, "1 hour"), (6, "6 hours"), (24, "24 hours")];

pub(crate) fn snooze_lines() -> Vec<MenuLine> {
    SNOOZE_HOURS
        .iter()
        .map(|(_, label)| {
            MenuLine::Row(MenuRow {
                label,
                icon: None,
                submenu: false,
            })
        })
        .collect()
}

/// The menu's colours.
#[derive(Clone, Copy)]
pub(crate) struct MenuInk {
    pub(crate) foreground: gpui::Hsla,
    pub(crate) muted: gpui::Hsla,
    pub(crate) icon: gpui::Hsla,
    pub(crate) hover: gpui::Hsla,
    pub(crate) border: gpui::Hsla,
    /// The lines between sections.
    pub(crate) line: gpui::Hsla,
    /// The lit 1px edge along the panel's top.
    pub(crate) top_edge: gpui::Hsla,
    pub(crate) fill: gpui::Hsla,
}

/// A panel's height for `lines`, its border included.
pub(crate) fn height(lines: &[MenuLine]) -> f32 {
    lines.iter().map(MenuLine::height).sum::<f32>() + 2.0 * MENU_PADDING + 2.0
}

/// How far below the panel's top the first row's top is, its border and inset included (where the
/// Snooze lengths line up when Snooze is that row).
pub(crate) fn first_row_top(lines: &[MenuLine]) -> f32 {
    1.0 + MENU_PADDING
        + lines
            .iter()
            .take_while(|line| !matches!(line, MenuLine::Row(_)))
            .map(MenuLine::height)
            .sum::<f32>()
}

/// A panel of `lines`, filling what holds it; `pick` gets the index of the row pressed (counting
/// rows only), `open` marks the submenu row whose submenu is open.
pub(crate) fn panel(
    id: &'static str,
    ink: MenuInk,
    lines: Vec<MenuLine>,
    open: Option<usize>,
    pick: Rc<dyn Fn(usize, &mut App)>,
) -> AnyElement {
    let hover = ink.hover;
    let mut rows = 0;
    div()
        .id(id)
        .role(gpui::Role::Menu)
        .aria_label("Page actions")
        .font_family(crate::ui_fonts::UI_FONT)
        .size_full()
        .flex()
        .flex_col()
        .p(px(MENU_PADDING))
        .rounded(px(MENU_RADIUS))
        .border_1()
        .border_color(ink.border)
        .bg(ink.fill)
        .shadow(vec![BoxShadow {
            color: ink.top_edge,
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(0.0),
            spread_radius: px(0.0),
            inset: true,
        }])
        .text_color(ink.foreground)
        .font_weight(gpui::FontWeight::NORMAL)
        .text_size(px(13.0))
        .line_height(px(16.0))
        .children(lines.into_iter().map(|line| {
            match line {
                MenuLine::Title { title, detail } => div()
                    .flex_shrink_0()
                    .h(px(TITLE_HEIGHT))
                    .px(px(ROW_PADDING_X))
                    .pt(px(6.0))
                    .pb(px(4.0))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.5))
                            .line_height(px(14.0))
                            .text_color(ink.muted)
                            .child(detail),
                    )
                    .into_any_element(),
                MenuLine::Heading(label) => div()
                    .flex_shrink_0()
                    .h(px(HEADING_HEIGHT))
                    .px(px(ROW_PADDING_X))
                    .pb(px(4.0))
                    .flex()
                    .items_end()
                    .text_size(px(11.0))
                    .line_height(px(13.0))
                    .text_color(ink.foreground.opacity(0.58))
                    .child(label)
                    .into_any_element(),
                MenuLine::Separator => div()
                    .flex_shrink_0()
                    .h(px(SEPARATOR_HEIGHT))
                    .px(px(ROW_PADDING_X))
                    .flex()
                    .items_center()
                    .child(div().flex_1().h(px(1.0)).bg(ink.line))
                    .into_any_element(),
                MenuLine::Row(row) => {
                    let index = rows;
                    rows += 1;
                    let pick = pick.clone();
                    let mut item = div()
                        .id(gpui::SharedString::from(format!("{id}:{index}")))
                        .role(gpui::Role::MenuItem)
                        .aria_label(row.label)
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px(px(ROW_PADDING_X))
                        .gap(px(8.0))
                        .rounded(px(8.0))
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
                                .text_color(ink.icon),
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
                    .into_any_element()
                }
            }
        }))
        .into_any_element()
}
