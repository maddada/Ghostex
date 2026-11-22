//! Preview of the sidebar ⋯ menu's links (packages/gx-core/src/custom_links.rs).
//!
//! `GHOSTEX_NATIVE_MODAL_DEMO=custom-link-dialog` opens the app's own Add link dialog
//! (window/custom_link_modal.rs); `GHOSTEX_NATIVE_MODAL_DEMO_STATE=edit` opens it on a saved link,
//! `error` shows what an address it cannot read says.
//!
//! `GHOSTEX_NATIVE_MODAL_DEMO=custom-links` draws a 1650x962 window: `menu` (default) the classic
//! sidebar's ⋯ menu with three links and Add link… (the rows come from gx-core's own builders, laid
//! into the menu by `insert_custom_link_rows`, drawn in the sidebar menu's metrics,
//! native_sidebar/menus.rs), `row-menu` the first link's Edit… and Remove open beside it, and
//! `page` a link's page with the app's own address bar (helpers/titlebar/custom_link_bar_look.rs)
//! over a stand-in page, since the preview runs no browser engine. The theme follows
//! `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`.
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use ghostex_gx_core::custom_links::{
    custom_link_menu_rows, custom_link_row_menu, custom_links_from_value, insert_custom_link_rows,
    normalize_custom_link_url,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, Hsla, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, prelude::FluentBuilder as _, px, rgb,
    svg,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, h_flex, v_flex};
use serde_json::{Value, json};

use super::custom_link_modal::*;

static LIGHT: AtomicBool = AtomicBool::new(false);

fn light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

// Stand-ins for the `crate::app` items the bar's look file reads, with the app's colours.
fn chrome_color(dark: u32, light_color: u32) -> gpui::Rgba {
    rgb(if light() { light_color } else { dark })
}
fn overlay() -> gpui::Rgba {
    chrome_color(0xffffff, 0x000000)
}
fn titlebar_icon_color() -> Hsla {
    chrome_color(0xfcfcfc, 0x292929).opacity(0.84).into()
}
fn browser_toolbar_disabled_icon_color() -> Hsla {
    chrome_color(0xcfcfcf, 0x404040).opacity(0.4).into()
}
fn titlebar_button_hover_color() -> Hsla {
    overlay().opacity(0.08).into()
}
fn browser_toolbar_background() -> Hsla {
    chrome_color(0x000000, 0xffffff).into()
}
fn browser_toolbar_text_color() -> Hsla {
    chrome_color(0xf0f0f0, 0x262626).opacity(0.95).into()
}
fn browser_toolbar_security_icon_color() -> Hsla {
    chrome_color(0xc7c7c7, 0x525252).opacity(0.9).into()
}
fn titlebar_svg_icon(path: &'static str, icon_size: f32, color: Hsla) -> impl IntoElement {
    svg().size(px(icon_size)).path(path).text_color(color)
}
fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::new(text).build(window, cx)
}
const TITLEBAR_CONTROL_HEIGHT: f32 = 27.0;
const TITLEBAR_BUTTON_RADIUS: f32 = 7.0;
const BROWSER_TOOLBAR_BUTTON_WIDTH: f32 = 30.0;
const BROWSER_TOOLBAR_HEIGHT: f32 = 35.0;
const BROWSER_TOOLBAR_LEADING_PADDING: f32 = 6.0;
const BROWSER_TOOLBAR_ITEM_GAP: f32 = 2.0;
const BROWSER_ADDRESS_MINIMUM_WIDTH: f32 = 180.0;
const BROWSER_ADDRESS_HEIGHT: f32 = 20.0;
const WORKAREA_HEADER_EDGE_PADDING: f32 = 9.0;
const WORKAREA_HEADER_PINNED_GAP: f32 = 2.0;
const TITLEBAR_ICON_CHEVRON_LEFT: &str = "titlebar/chevron-left.svg";
const BROWSER_ICON_CHEVRON_RIGHT: &str = "titlebar/chevron-right.svg";
const BROWSER_ICON_RELOAD: &str = "titlebar/reload.svg";
const TITLEBAR_ICON_X: &str = "titlebar/x.svg";

#[path = "../../app/helpers/titlebar/custom_link_bar_look.rs"]
mod look;
use look::*;

const WIDTH: f32 = 1650.0;
const HEIGHT: f32 = 962.0;
const SIDEBAR_WIDTH: f32 = 286.0;
const MENU_WIDTH: f32 = 252.0;
const PAGE_ADDRESS: &str = "http://localhost:8317/management.html";

/// A small square favicon, as the Browser's cache hands the menu one.
const WIKI_ICON: &str = "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><rect width='16' height='16' rx='3' fill='%233b82f6'/><path d='M4 4h8v2H4zM4 7h8v2H4zM4 10h5v2H4z' fill='white'/></svg>";

fn sample_links() -> Value {
    json!([
        { "id": "proxy", "name": "Proxy dashboard", "url": "localhost:8317/management.html" },
        { "id": "router", "name": "", "url": "192.168.1.1" },
        { "id": "wiki", "name": "Team wiki", "url": "https://wiki.example.com/" },
    ])
}

pub(super) fn open_dialog(demo: &super::DemoEnv, cx: &mut App) {
    let editing = demo.state == "edit";
    let host: CustomLinkModalHost = Rc::new(|command, cx: &mut App| {
        match command {
            CustomLinkModalCommand::Save { id, name, url } => {
                eprintln!("save: {id:?} {name:?} {url:?}")
            }
            CustomLinkModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let config = CustomLinkModalConfig {
        id: editing.then(|| "proxy".to_string()),
        name: if editing {
            "Proxy dashboard".into()
        } else {
            String::new()
        },
        url: match demo.state.as_str() {
            "edit" => "http://localhost:8317/management.html".into(),
            "error" => "localhost:99999".into(),
            _ => "localhost:8317".into(),
        },
        read_address: normalize_custom_link_url,
        palette: demo.palette,
    };
    // `error`: press Enter (GHOSTEX_NATIVE_MODAL_DEMO_KEYS=enter) to see what an address it cannot
    // read says.
    let _ = super::open_modal_window(
        CUSTOM_LINK_MODAL_WIDTH,
        CUSTOM_LINK_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiCustomLinkModalWindow::new(config, host, window, cx)),
        cx,
    );
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    LIGHT.store(demo.palette.light, Ordering::Relaxed);
    let state = if demo.state.is_empty() {
        "menu".to_string()
    } else {
        demo.state.clone()
    };
    let _ = super::open_modal_window(
        WIDTH,
        HEIGHT,
        move |window, cx| cx.new(|cx| CustomLinksDemo::new(state, window, cx)),
        cx,
    );
}

struct CustomLinksDemo {
    state: String,
    menu: Vec<Value>,
    row_menu: Vec<Value>,
    address: Entity<InputState>,
}

impl CustomLinksDemo {
    fn new(state: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let links = custom_links_from_value(Some(&sample_links()));
        let rows = custom_link_menu_rows(&links, |link| {
            (link.id == "wiki").then(|| WIKI_ICON.to_string())
        });
        let mut menu = base_more_menu();
        insert_custom_link_rows(&mut menu, rows.clone());
        let row_menu = custom_link_row_menu(&rows[0]["rowMenu"])
            .and_then(|menu| menu.as_array().cloned())
            .unwrap_or_default();
        let address = cx.new(|cx| InputState::new(window, cx).default_value(PAGE_ADDRESS));
        Self {
            state,
            menu,
            row_menu,
            address,
        }
    }
}

/// The classic sidebar's ⋯ menu as gx-core builds it today (sidebar_menu/navigation.rs
/// `more_menu`), the rows the link group is laid into.
fn base_more_menu() -> Vec<Value> {
    let row = |label: &str, icon: &str| json!({ "label": label, "icon": icon, "command": {"type": "sidebarAction"} });
    let submenu = |label: &str, icon: &str| json!({ "label": label, "icon": icon, "children": [] });
    let separator = || json!({ "separator": true });
    vec![
        row("Add Project", "plus"),
        submenu("Sort & Filter", "filter"),
        row("Collapse All", "arrows-diagonal"),
        separator(),
        json!({ "label": "Commands", "icon": "bolt", "suffix": "⌘K", "command": {"type": "sidebarAction"} }),
        row("Agents Hub", "users-group"),
        row("All Automations", "clock"),
        row("Connect a Phone", "devices"),
        row("Connect a Computer", "device-laptop"),
        submenu("Keep Awake", "moon"),
        separator(),
        json!({ "label": "Ask Ghostex AI", "icon": "help-circle", "command": {"type": "command", "message": {"type": ghostex_gx_core::MENU_PANEL_MESSAGE_TYPE, "panel": ghostex_gx_core::MENU_PANEL_HELP}} }),
        json!({ "label": "Join Discord", "icon": "users-group", "command": {"type": "command", "message": {"type": "openExternalUrl"}} }),
        row("Send Bug / Feedback", "message-circle"),
        separator(),
        submenu("Tools", "tools"),
        submenu("Ghostex", "ghostex"),
        separator(),
        row("Settings", "settings"),
    ]
}

fn foreground() -> Hsla {
    chrome_color(0xfcfcfc, 0x292929).into()
}

fn menu_surface() -> Hsla {
    chrome_color(0x1f1f1f, 0xfbfbfb).into()
}

fn menu_border() -> Hsla {
    overlay().opacity(0.12).into()
}

fn menu_hover() -> Hsla {
    overlay().opacity(if light() { 0.06 } else { 0.08 }).into()
}

/// One menu panel, in the sidebar menu's metrics: 6pt padding, 8pt corners, 34pt rows with a 16pt
/// icon, 13pt separators, a link row's ⋯ on the row under the pointer.
fn menu_panel(rows: &[Value], hovered: Option<usize>) -> AnyElement {
    let mut panel = v_flex()
        .w(px(MENU_WIDTH))
        .p(px(6.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(menu_border())
        .bg(menu_surface())
        .shadow_lg()
        .text_size(px(13.0))
        .text_color(foreground());
    for (index, row) in rows.iter().enumerate() {
        if row["separator"] == true {
            panel = panel.child(
                div()
                    .h(px(13.0))
                    .flex()
                    .items_center()
                    .child(div().w_full().h(px(1.0)).bg(menu_border())),
            );
            continue;
        }
        let label = row["label"].as_str().unwrap_or_default().to_string();
        let icon = row["icon"]
            .as_str()
            .map(|icon| gpui::SharedString::from(format!("titlebar/{icon}.svg")));
        let image = row["imageDataUrl"]
            .as_str()
            .and_then(|value| crate::data_url_image::image(value, false));
        let selected = hovered == Some(index);
        let danger = row["danger"] == true;
        panel = panel.child(
            h_flex()
                .w_full()
                .min_h(px(34.0))
                .px(px(10.0))
                .gap(px(8.0))
                .rounded(px(6.0))
                .when(selected, |row| row.bg(menu_hover()))
                .when(danger, |row| {
                    row.text_color(chrome_color(0xff7b72, 0xd32f2f))
                })
                .when_some(icon, |row, icon| {
                    row.child(svg().path(icon).size(px(16.0)).flex_shrink_0().text_color(
                        if danger {
                            chrome_color(0xff7b72, 0xd32f2f).into()
                        } else {
                            foreground().opacity(0.84)
                        },
                    ))
                })
                .when_some(image, |row, image| {
                    row.child(gpui::img(image).size(px(16.0)).flex_shrink_0())
                })
                .child(div().flex_1().min_w_0().truncate().child(label))
                .when_some(row["suffix"].as_str(), |row, suffix| {
                    row.child(
                        div()
                            .text_size(px(11.0))
                            .opacity(0.6)
                            .child(suffix.to_string()),
                    )
                })
                .when(row.get("children").is_some(), |row| {
                    row.child(
                        svg()
                            .path("titlebar/chevron-right.svg")
                            .size(px(12.0))
                            .text_color(foreground().opacity(0.55)),
                    )
                })
                .when(row.get("rowMenu").is_some() && selected, |row| {
                    row.child(
                        div()
                            .size(px(22.0))
                            .mr(px(-6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(menu_hover())
                            .child(
                                svg()
                                    .path("titlebar/dots.svg")
                                    .size(px(14.0))
                                    .text_color(foreground().opacity(0.55)),
                            ),
                    )
                }),
        );
    }
    panel.into_any_element()
}

/// A stand-in for an app's sidebar: its top row, a few session rows.
fn sidebar_stand_in() -> AnyElement {
    let ink = foreground();
    let bar = |width: f32, alpha: f32| {
        div()
            .h(px(9.0))
            .w(px(width))
            .rounded(px(4.5))
            .bg(ink.opacity(alpha))
    };
    let mut list = v_flex().w_full().px(px(12.0)).pt(px(14.0)).gap(px(18.0));
    for (title, detail) in [
        (150.0, 90.0),
        (180.0, 70.0),
        (120.0, 100.0),
        (165.0, 60.0),
        (140.0, 85.0),
    ] {
        list = list.child(
            v_flex()
                .gap(px(7.0))
                .child(bar(title, 0.32))
                .child(bar(detail, 0.14)),
        );
    }
    let button = |icon: &'static str, on: bool| {
        div()
            .size(px(27.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .when(on, |button| button.bg(overlay().opacity(0.11)))
            .child(titlebar_svg_icon(icon, 16.0, titlebar_icon_color()))
    };
    v_flex()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(chrome_color(0x161616, 0xf1f1f1))
        .border_r_1()
        .border_color(overlay().opacity(0.08))
        .child(
            h_flex()
                .h(px(44.0))
                .px(px(10.0))
                .gap(px(4.0))
                .justify_end()
                .child(button("titlebar/layout-sidebar.svg", false))
                .child(div().flex_1())
                .child(button("titlebar/search.svg", false))
                .child(button("titlebar/bell.svg", false))
                .child(button("titlebar/dots.svg", true)),
        )
        .child(list)
        .into_any_element()
}

fn page_stand_in() -> AnyElement {
    let ink: Hsla = chrome_color(0xe8e8e8, 0x1f2328).into();
    let card_bg: Hsla = chrome_color(0x1b1d21, 0xf6f8fa).into();
    let border: Hsla = overlay().opacity(0.1).into();
    let card = |title: &'static str, value: &'static str| {
        v_flex()
            .flex_1()
            .p(px(18.0))
            .gap(px(8.0))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(border)
            .child(div().text_size(px(12.0)).opacity(0.6).child(title))
            .child(
                div()
                    .text_size(px(26.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value),
            )
    };
    let mut table = v_flex()
        .rounded(px(10.0))
        .border_1()
        .border_color(border)
        .bg(card_bg);
    for (index, (name, status, count)) in [
        ("Account", "Status", "Requests today"),
        ("work", "Ready", "1,284"),
        ("personal", "Ready", "312"),
        ("backup", "Cooling down", "0"),
    ]
    .into_iter()
    .enumerate()
    {
        table = table.child(
            h_flex()
                .px(px(18.0))
                .h(px(44.0))
                .when(index > 0, |row| row.border_t_1().border_color(border))
                .when(index == 0, |row| row.opacity(0.6).text_size(px(12.0)))
                .child(div().w(px(260.0)).child(name))
                .child(div().w(px(220.0)).child(status))
                .child(div().child(count)),
        );
    }
    v_flex()
        .flex_1()
        .size_full()
        .p(px(36.0))
        .gap(px(22.0))
        .bg(chrome_color(0x0f1012, 0xffffff))
        .text_color(ink)
        .text_size(px(14.0))
        .child(
            div()
                .text_size(px(22.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Proxy dashboard"),
        )
        .child(
            h_flex()
                .gap(px(16.0))
                .child(card("Accounts", "3"))
                .child(card("Requests today", "1,596"))
                .child(card("Errors", "0")),
        )
        .child(table)
        .into_any_element()
}

impl CustomLinksDemo {
    fn render_menu(&self, row_menu_open: bool) -> AnyElement {
        let first_link = self
            .menu
            .iter()
            .position(|row| row.get("rowMenu").is_some());
        // The menu hangs from the ⋯ button, right edges lined up, as the sidebar opens it.
        let menu_left = SIDEBAR_WIDTH - 10.0 - MENU_WIDTH;
        let menu_top = 44.0 + 3.0;
        // The first link row's top: rows above it at 34pt, separators at 13pt, plus the padding.
        let first_link_top = first_link.map_or(0.0, |index| {
            6.0 + self.menu[..index]
                .iter()
                .map(|row| if row["separator"] == true { 13.0 } else { 34.0 })
                .sum::<f32>()
        });
        div()
            .size_full()
            .relative()
            .child(
                h_flex()
                    .size_full()
                    .child(sidebar_stand_in())
                    .child(div().flex_1().h_full().bg(chrome_color(0x0f0f0f, 0xffffff))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(menu_left))
                    .top(px(menu_top))
                    .child(menu_panel(&self.menu, first_link)),
            )
            .when(row_menu_open, |frame| {
                frame.child(
                    div()
                        .absolute()
                        .left(px(menu_left + MENU_WIDTH + 3.0))
                        .top(px(menu_top + first_link_top - 6.0))
                        .child(menu_panel(&self.row_menu, None)),
                )
            })
            .into_any_element()
    }

    fn render_page(&self) -> AnyElement {
        let address = Input::new(&self.address)
            .with_size(gpui_component::Size::XSmall)
            .appearance(false)
            .bordered(false)
            .focus_bordered(false)
            .w_full()
            .px(px(0.0))
            .py(px(0.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .line_height(px(BROWSER_ADDRESS_HEIGHT))
            .text_color(browser_toolbar_text_color())
            .into_any_element();
        let nothing = || -> CustomLinkBarPress { Box::new(|_, _, _| {}) };
        let bar = custom_link_bar(
            CustomLinkBarState {
                can_go_back: true,
                can_go_forward: false,
                loading: false,
            },
            titlebar_svg_icon(
                "titlebar/world.svg",
                14.0,
                browser_toolbar_security_icon_color(),
            )
            .into_any_element(),
            Some(address),
            CustomLinkBarActions {
                back: nothing(),
                forward: nothing(),
                reload: nothing(),
                close: nothing(),
                address: nothing(),
                address_key: Box::new(|_, _, _| {}),
            },
        );
        // With the sidebar hidden, the header keeps the sidebar's top row where it was
        // (claude_web/header.rs) and names no project.
        let header_button = |icon: &'static str| {
            div()
                .size(px(27.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .child(titlebar_svg_icon(icon, 16.0, titlebar_icon_color()))
        };
        v_flex()
            .size_full()
            .bg(chrome_color(0x161616, 0xf1f1f1))
            .child(
                h_flex()
                    .h(px(40.0))
                    .px(px(10.0))
                    .gap(px(4.0))
                    .child(div().w(px(70.0)))
                    .child(header_button("titlebar/layout-sidebar.svg"))
                    .child(header_button("titlebar/search.svg"))
                    .child(header_button("titlebar/bell.svg"))
                    .child(header_button("titlebar/dots.svg")),
            )
            .child(
                v_flex()
                    .flex_1()
                    .mx(px(6.0))
                    .mb(px(6.0))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(overlay().opacity(0.1))
                    .child(bar)
                    .child(page_stand_in()),
            )
            .into_any_element()
    }
}

impl Render for CustomLinksDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .text_color(foreground())
            .child(match self.state.as_str() {
                "page" => self.render_page(),
                "row-menu" => self.render_menu(true),
                _ => self.render_menu(false),
            })
    }
}
