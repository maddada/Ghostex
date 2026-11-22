//! Preview of the user's links (packages/gx-core/src/custom_links.rs): the sidebar ⋯ menu's rows,
//! the Add link dialog, the link page's toolbar and links pinned to the rail.
//!
//! `GHOSTEX_NATIVE_MODAL_DEMO=custom-link-dialog` opens the app's own Add link dialog
//! (window/custom_link_modal.rs); `GHOSTEX_NATIVE_MODAL_DEMO_STATE=edit` opens it on a saved link,
//! `error` shows what an address it cannot read says.
//!
//! `GHOSTEX_NATIVE_MODAL_DEMO=custom-links` draws a 1650x962 window. Its states:
//! - `menu` (default): the classic sidebar's ⋯ menu with three links and Add link… (the rows come
//!   from gx-core's own builders, laid in by `insert_custom_link_rows`), drawn in the sidebar
//!   menu's metrics (native_sidebar/menus.rs); `row-menu` adds the first link's own menu.
//! - `page`: the rail with two pinned links, the first one's page open, under the side panel
//!   Browser's toolbar (Back, Forward, Reload, Home, the address, Page Size, Annotate, History,
//!   Profile, DevTools, ⋯) and the link page's Close. The toolbar is drawn here in
//!   render_browser_toolbar's icons, order and sizes, and the page under it is a stand-in, since
//!   the preview runs neither the app nor a browser engine.
//! - `rail`: the same, with the pointer on the second pinned link and its tooltip.
//! - `rail-menu`: the rail's ⋯ menu with its Links group (each link row's pin, as every rail row
//!   has) and an unpinned link's own menu open with Pin to rail.
//! - `rail-pin-menu`: a pinned link's right-click menu on the rail: Move up, Move down, Unpin from
//!   rail.
//!
//! The theme follows `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`.
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

// The app's chrome colours for the demo's theme (helpers/chrome_palette.rs, helpers/browser.rs).
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

// render_browser_toolbar's measures (app/consts.rs).
const TITLEBAR_CONTROL_HEIGHT: f32 = 27.0;
const TITLEBAR_BUTTON_RADIUS: f32 = 7.0;
const BROWSER_TOOLBAR_BUTTON_ICON_SIZE: f32 = 18.0;
const BROWSER_TOOLBAR_BUTTON_WIDTH: f32 = 18.0 + 2.0 * 7.0;
const BROWSER_TOOLBAR_HEIGHT: f32 = 35.0;
const BROWSER_TOOLBAR_LEADING_PADDING: f32 = 6.0;
const BROWSER_TOOLBAR_ITEM_GAP: f32 = 2.0;
const BROWSER_TOOLBAR_ADDRESS_GAP: f32 = 18.0;
const BROWSER_TOOLBAR_ADDRESS_RIGHT_GAP: f32 = 14.0;
const BROWSER_ADDRESS_MINIMUM_WIDTH: f32 = 180.0;
const BROWSER_ADDRESS_HEIGHT: f32 = 20.0;
const WORKAREA_HEADER_EDGE_PADDING: f32 = 9.0;
const WORKAREA_HEADER_PINNED_GAP: f32 = 2.0;

// The rail's measures (app/sidebar_rail/model.rs).
const RAIL_WIDTH: f32 = 52.0;
const RAIL_BUTTON: f32 = 36.0;
const RAIL_ICON: f32 = 20.0;
const RAIL_GAP: f32 = 8.0;
const RAIL_BUTTON_RADIUS: f32 = 9.0;
const RAIL_TOP: f32 = 44.0;

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
    /// The rail's ⋯ menu, its Links group, and the unpinned link's own menu.
    rail_menu: Vec<Value>,
    rail_row_menu: Vec<Value>,
    address: Entity<InputState>,
}

/// The pinned links, as `sidebarRailPins` holds them among the other pins.
const RAIL_PINS: &[&str] = &[
    "search",
    "notifications",
    "mainHub",
    "agentsHub",
    "allAutomations",
    "link:proxy",
    "link:wiki",
    "claude",
    "grok",
];

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
        // The rail's ⋯ menu as app/sidebar_rail/menu.rs builds it: a group of items, each with
        // its pin, then the Links group with each link row's pin and Pin to rail in its own menu.
        let pins: Vec<String> = RAIL_PINS.iter().map(|pin| pin.to_string()).collect();
        let pin = |key: &str| {
            let pinned = pins.iter().any(|pin| pin == key);
            json!({ "icon": if pinned { "pin-filled" } else { "pin-outline" }, "hoverOnly": !pinned })
        };
        let item = |label: &str, icon: &str, key: &str| json!({ "label": label, "icon": icon, "secondary": pin(key) });
        let heading = |label: &str| json!({ "label": label, "heading": true });
        let mut rail_menu = vec![
            heading("Go to"),
            item("Search", "search", "search"),
            item("Notifications", "bell", "notifications"),
            item("Ask Ghostex AI", "ghostex-flower", "mainHub"),
            item("Agents Hub", "users-group", "agentsHub"),
            item("All Automations", "clock", "allAutomations"),
            item("Commands", "bolt", "commands"),
            heading("Views"),
            item("Files", "folder", "files"),
            item("Browser", "browser", "browser"),
            heading("Links"),
        ];
        let mut rail_row_menu = Vec::new();
        for mut row in rows.clone() {
            if let Some(id) = row["rowMenu"]["key"].as_str().map(str::to_string) {
                let key = ghostex_gx_core::sidebar_rail::rail_link_pin_key(&id);
                let pinned = pins.contains(&key);
                row["rowMenu"]["pinned"] = json!(pinned);
                row["secondary"] = pin(&key);
                if !pinned {
                    rail_row_menu = custom_link_row_menu(&row["rowMenu"])
                        .and_then(|menu| menu.as_array().cloned())
                        .unwrap_or_default();
                }
            }
            rail_menu.push(row);
        }
        Self {
            state,
            menu,
            row_menu,
            rail_menu,
            rail_row_menu,
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
        if row["heading"] == true {
            panel = panel.child(
                div()
                    .h(px(24.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .opacity(0.6)
                    .child(label),
            );
            continue;
        }
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
                // A rail row's pin: filled while pinned, the outline on the row under the pointer.
                .when_some(
                    row["secondary"]["icon"]
                        .as_str()
                        .filter(|_| selected || row["secondary"]["hoverOnly"] != true)
                        .map(|icon| gpui::SharedString::from(format!("titlebar/{icon}.svg"))),
                    |row, icon| {
                        row.child(
                            svg()
                                .path(icon)
                                .size(px(14.0))
                                .flex_shrink_0()
                                .text_color(foreground().opacity(0.7)),
                        )
                    },
                )
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

    /// The side panel Browser's toolbar as render_browser_toolbar lays it out for the link page:
    /// Back, Forward, Reload, Home; the site icon and address; Page Size, Annotate, History,
    /// Profile, DevTools and ⋯; then the link page's Close.
    fn render_toolbar(&self) -> AnyElement {
        let button = |icon: &'static str, enabled: bool| {
            div()
                .flex()
                .flex_shrink_0()
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
                .items_center()
                .justify_center()
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .child(titlebar_svg_icon(
                    icon,
                    BROWSER_TOOLBAR_BUTTON_ICON_SIZE,
                    if enabled {
                        titlebar_icon_color()
                    } else {
                        browser_toolbar_disabled_icon_color()
                    },
                ))
        };
        // The tab strip's ⋯: three 3pt dots (render_browser_tab_overflow_icon).
        let overflow = div()
            .flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .child(h_flex().gap(px(2.0)).children(
                (0..3).map(|_| div().size(px(3.0)).rounded_full().bg(titlebar_icon_color())),
            ));
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
            .text_color(browser_toolbar_text_color());
        h_flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT))
            .w_full()
            .items_center()
            .pl(px(BROWSER_TOOLBAR_LEADING_PADDING))
            .pr(px(WORKAREA_HEADER_EDGE_PADDING + WORKAREA_HEADER_PINNED_GAP))
            .bg(browser_toolbar_background())
            .border_b_1()
            .border_color(chrome_color(0x252525, 0xd4d4d4))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .child(button("titlebar/chevron-left.svg", true))
                    .child(button("titlebar/chevron-right.svg", false))
                    .child(button("titlebar/reload.svg", true))
                    .child(button("titlebar/home.svg", true)),
            )
            .child(div().flex_shrink_0().w(px(BROWSER_TOOLBAR_ADDRESS_GAP)))
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(BROWSER_ADDRESS_MINIMUM_WIDTH))
                    .h(px(BROWSER_ADDRESS_HEIGHT))
                    .items_center()
                    .child(titlebar_svg_icon(
                        "titlebar/world.svg",
                        14.0,
                        browser_toolbar_security_icon_color(),
                    ))
                    .child(
                        div()
                            .ml(px(8.0))
                            .flex_1()
                            .min_w_0()
                            .h(px(BROWSER_ADDRESS_HEIGHT))
                            .overflow_hidden()
                            .child(address),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(BROWSER_TOOLBAR_ADDRESS_RIGHT_GAP)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .child(button("modals/settings/device-mobile.svg", true))
                    .child(button("titlebar/pointer.svg", true))
                    .child(button("titlebar/history.svg", true))
                    .child(button("titlebar/user-circle.svg", true))
                    .child(button("titlebar/tools.svg", true))
                    .child(overflow)
                    .child(button("titlebar/x.svg", true)),
            )
            .into_any_element()
    }

    /// The rail (app/sidebar_rail/render.rs): its pins, the showing link's filled tile, ⋯, and at
    /// its foot the usage toggle and Toggle sidebar. `hovered` is the pin under the pointer.
    fn render_rail(&self, showing: Option<&str>, hovered: Option<&str>) -> AnyElement {
        let fill: Hsla = overlay().opacity(if light() { 0.09 } else { 0.11 }).into();
        let hover: Hsla = overlay().opacity(if light() { 0.06 } else { 0.08 }).into();
        let tile = |content: AnyElement, on: bool, under_pointer: bool| {
            div()
                .flex_shrink_0()
                .size(px(RAIL_BUTTON))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RAIL_BUTTON_RADIUS))
                .when(on, |tile| tile.bg(fill))
                .when(under_pointer && !on, |tile| tile.bg(hover))
                .child(content)
        };
        let mut column = v_flex().w_full().flex_1().items_center().gap(px(RAIL_GAP));
        for key in RAIL_PINS {
            let on = showing == Some(*key);
            let under_pointer = hovered == Some(*key);
            let color = if on {
                foreground()
            } else {
                titlebar_icon_color()
            };
            let content = match ghostex_gx_core::sidebar_rail::rail_link_pin_id(key) {
                // A pinned link: its site icon once cached, else the globe.
                Some(id) if id == "wiki" => crate::data_url_image::image(WIKI_ICON, false)
                    .map(|image| gpui::img(image).size(px(16.0)).into_any_element())
                    .unwrap_or_else(|| div().into_any_element()),
                Some(_) => {
                    titlebar_svg_icon("titlebar/world.svg", RAIL_ICON, color).into_any_element()
                }
                None => {
                    let (icon, size): (&'static str, f32) = match *key {
                        "search" => ("titlebar/search.svg", RAIL_ICON),
                        "notifications" => ("titlebar/bell.svg", RAIL_ICON),
                        "mainHub" => ("titlebar/ghostex-flower.svg", RAIL_ICON),
                        "agentsHub" => ("titlebar/users-group.svg", RAIL_ICON),
                        "allAutomations" => ("titlebar/clock.svg", RAIL_ICON),
                        "claude" => ("agent-icons/claude.svg", 15.0),
                        _ => ("agent-icons/grok-build.svg", 16.0),
                    };
                    titlebar_svg_icon(icon, size, color).into_any_element()
                }
            };
            column = column.child(tile(
                div()
                    .size(px(RAIL_ICON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content)
                    .into_any_element(),
                on,
                under_pointer,
            ));
        }
        column = column.child(tile(
            titlebar_svg_icon("titlebar/dots.svg", RAIL_ICON, titlebar_icon_color())
                .into_any_element(),
            false,
            hovered == Some("more"),
        ));
        v_flex()
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .pb(px(8.0))
            .bg(chrome_color(0x121212, 0xebebeb))
            .border_r_1()
            .border_color(foreground().opacity(0.06))
            .child(div().h(px(RAIL_TOP)).flex_shrink_0())
            .child(column)
            .child(tile(
                titlebar_svg_icon("titlebar/chart-bar.svg", RAIL_ICON, titlebar_icon_color())
                    .into_any_element(),
                false,
                false,
            ))
            .child(div().h(px(RAIL_GAP)))
            .child(tile(
                titlebar_svg_icon(
                    "titlebar/layout-sidebar.svg",
                    RAIL_ICON,
                    titlebar_icon_color(),
                )
                .into_any_element(),
                false,
                false,
            ))
            .into_any_element()
    }

    /// A rail pin's top, from the window's top.
    fn rail_pin_top(key: &str) -> f32 {
        let index = RAIL_PINS.iter().position(|pin| *pin == key).unwrap_or(0);
        RAIL_TOP + index as f32 * (RAIL_BUTTON + RAIL_GAP)
    }

    /// The link page in the sessions column's place: the toolbar over a stand-in page.
    fn render_link_page(&self) -> AnyElement {
        v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(chrome_color(0x161616, 0xf1f1f1))
            .pt(px(RAIL_TOP - 4.0))
            .child(
                v_flex()
                    .flex_1()
                    .mx(px(6.0))
                    .mb(px(6.0))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(overlay().opacity(0.1))
                    .child(self.render_toolbar())
                    .child(page_stand_in()),
            )
            .into_any_element()
    }

    /// The rail with the first link's page open; `tooltip` puts the pointer on the second link.
    fn render_page(&self, tooltip: bool, pin_menu: bool) -> AnyElement {
        let hovered =
            (tooltip || pin_menu).then_some(if pin_menu { "link:proxy" } else { "link:wiki" });
        div()
            .size_full()
            .relative()
            .child(
                h_flex()
                    .size_full()
                    .child(self.render_rail(Some("link:proxy"), hovered))
                    .child(self.render_link_page()),
            )
            .when(tooltip, |frame| {
                // The sidebar-style tooltip, right of the button (native_sidebar/link_chips.rs).
                let top = Self::rail_pin_top("link:wiki") + RAIL_BUTTON / 2.0 - 13.0;
                frame.child(
                    div()
                        .absolute()
                        .left(px(RAIL_WIDTH - 4.0))
                        .top(px(top))
                        .h(px(26.0))
                        .px(px(9.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .bg(chrome_color(0x262626, 0xffffff))
                        .border_1()
                        .border_color(overlay().opacity(0.12))
                        .shadow_md()
                        .text_size(px(12.0))
                        .text_color(foreground())
                        .child("Team wiki"),
                )
            })
            .when(pin_menu, |frame| {
                // The pin's right-click menu (app/sidebar_rail/menu.rs `sidebar_rail_item_menu`).
                let rows = vec![
                    json!({ "label": "Move up", "icon": "arrow-up" }),
                    json!({ "label": "Move down", "icon": "arrow-down" }),
                    json!({ "label": "Unpin from rail", "icon": "pin-outline" }),
                ];
                frame.child(
                    div()
                        .absolute()
                        .left(px(RAIL_WIDTH - 8.0))
                        .top(px(Self::rail_pin_top("link:proxy") + 20.0))
                        .child(menu_panel(&rows, Some(2))),
                )
            })
            .into_any_element()
    }

    /// The rail's ⋯ menu beside its button with the Links group, and the unpinned link's menu.
    fn render_rail_menu(&self) -> AnyElement {
        let unpinned = self
            .rail_menu
            .iter()
            .position(|row| row["rowMenu"]["pinned"] == false);
        let menu_left = RAIL_WIDTH + 4.0;
        let menu_top = 24.0;
        let row_top = unpinned.map_or(0.0, |index| {
            6.0 + self.rail_menu[..index]
                .iter()
                .map(|row| if row["heading"] == true { 24.0 } else { 34.0 })
                .sum::<f32>()
        });
        div()
            .size_full()
            .relative()
            .child(
                h_flex()
                    .size_full()
                    .child(self.render_rail(None, Some("more")))
                    .child(sidebar_stand_in())
                    .child(div().flex_1().h_full().bg(chrome_color(0x0f0f0f, 0xffffff))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(menu_left))
                    .top(px(menu_top))
                    .child(menu_panel(&self.rail_menu, unpinned)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(menu_left + MENU_WIDTH + 3.0))
                    .top(px(menu_top + row_top - 6.0))
                    .child(menu_panel(&self.rail_row_menu, Some(1))),
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
                "page" => self.render_page(false, false),
                "rail" => self.render_page(true, false),
                "rail-pin-menu" => self.render_page(false, true),
                "rail-menu" => self.render_rail_menu(),
                "row-menu" => self.render_menu(true),
                _ => self.render_menu(false),
            })
    }
}
