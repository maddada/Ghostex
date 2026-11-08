//! Preview of the header's "…" extensions menu (the app's own row file,
//! app/helpers/titlebar/extension_menu_row.rs) and of the context menu a pinned extension's header
//! icon opens. `GHOSTEX_NATIVE_MODAL_DEMO_STATE=unpinned|pinned` (default `unpinned`) sets whether
//! Lazygit's pin is filled, `header-menu` shows the icon's right-click menu. The theme follows
//! `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`.
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, Hsla, Image, ImageFormat, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px, rgb, svg,
};
use gpui_component::Side;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance, PopupMenuItem};

// Stand-ins for the `crate::app` items the row file reads, with the app's colours for the demo's theme.
static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn light() -> bool {
    LIGHT.load(std::sync::atomic::Ordering::Relaxed)
}
fn titlebar_overlay_base() -> gpui::Rgba {
    rgb(if light() { 0x000000 } else { 0xffffff })
}
fn titlebar_popup_menu_foreground() -> Hsla {
    rgb(if light() { 0x292929 } else { 0xfcfcfc }).into()
}
fn titlebar_active_text_color() -> Hsla {
    rgb(if light() { 0x292929 } else { 0xfcfcfc }).into()
}
fn titlebar_inactive_text_color() -> Hsla {
    let color = if light() { 0x292929 } else { 0xfcfcfc };
    rgb(color).opacity(0.68).into()
}
fn titlebar_button_hover_color() -> Hsla {
    titlebar_overlay_base().opacity(0.08).into()
}
fn titlebar_active_segment_color() -> Hsla {
    titlebar_overlay_base().opacity(0.11).into()
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
const TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT: f32 = 40.0;
const TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE: f32 = 13.0;
const TITLEBAR_POPUP_MENU_ROW_ICON_SIZE: f32 = 14.0;

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct ToggleGpuiExtensionPin {
    extension_id: String,
    pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct LaunchGpuiExtension {
    extension_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct DisableGpuiExtension {
    extension_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
struct OpenGpuiExtensionsModal;

/// The app's `GpuiInstalledExtension`, cut to what the row reads.
#[derive(Clone)]
struct GpuiInstalledExtension {
    id: String,
    title: String,
    icon_image: Arc<Image>,
    pinned: bool,
}

impl GpuiInstalledExtension {
    fn launch_placement_label(&self) -> &'static str {
        "Terminal"
    }
}

#[path = "../../app/helpers/titlebar/extension_menu_row.rs"]
mod row;
use row::titlebar_popup_extension_menu_row;

fn menu_appearance() -> PopupMenuAppearance {
    let (background, border, hover) = if light() {
        (0xf5f5f5, 0x000000, 0.06)
    } else {
        (0x1c1c1c, 0xffffff, 0.08)
    };
    PopupMenuAppearance {
        shadow: false,
        panel_radius: px(8.0),
        item_radius: px(6.0),
        padding: px(6.0),
        item_padding_x: px(10.0),
        item_height: px(34.0),
        separator_margin: px(6.0),
        background: rgb(background).into(),
        foreground: titlebar_popup_menu_foreground(),
        border: rgb(border).opacity(0.12).into(),
        hover: rgb(border).opacity(hover).into(),
    }
}

/// A plain text row as the context menu draws one (app/context_menu.rs `row_element`).
fn context_row(label: &'static str) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .whitespace_nowrap()
            .items_center()
            .min_h(px(34.0))
            .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
            .text_color(titlebar_popup_menu_foreground())
            .child(label)
    })
}

struct ExtensionMenuDemo {
    menu: Entity<PopupMenu>,
    width: f32,
}

impl ExtensionMenuDemo {
    fn new(state: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let header_menu = state == "header-menu";
        let pinned = state == "pinned" || header_menu;
        let icon = include_str!("../../../assets/titlebar/git-branch.svg")
            .replace("currentColor", if light() { "#333333" } else { "#e6e6e6" });
        let extension = GpuiInstalledExtension {
            id: "lazygit".to_string(),
            title: "Lazygit".to_string(),
            icon_image: Arc::new(Image::from_bytes(ImageFormat::Svg, icon.into_bytes())),
            pinned,
        };
        let width = if header_menu { 200.0 } else { 320.0 };
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let menu = menu
                .appearance(menu_appearance())
                .min_w(px(width))
                .max_w(px(width))
                .check_side(Side::Right);
            if header_menu {
                return menu
                    .item(context_row("Unpin from header"))
                    .item(context_row("Disable extension"))
                    .separator()
                    .item(context_row("Extension Store"));
            }
            menu.menu_element(
                Box::new(LaunchGpuiExtension {
                    extension_id: extension.id.clone(),
                }),
                move |_, _| titlebar_popup_extension_menu_row(extension.clone()),
            )
            .separator()
            .menu_element(Box::new(OpenGpuiExtensionsModal), |_, _| {
                div()
                    .flex()
                    .min_h(px(34.0))
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                    .text_color(titlebar_popup_menu_foreground())
                    .child(titlebar_svg_icon(
                        "titlebar/puzzle.svg",
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        titlebar_popup_menu_foreground(),
                    ))
                    .child("Browse extensions…")
            })
        });
        menu.focus_handle(cx).focus(window, cx);
        Self { menu, width }
    }
}

impl Render for ExtensionMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(if light() {
                rgb(0xe9e9ec)
            } else {
                rgb(0x2a2b2f)
            })
            .p(px(10.0))
            .child(div().w(px(self.width + 2.0)).child(self.menu.clone()))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    LIGHT.store(demo.palette.light, std::sync::atomic::Ordering::Relaxed);
    let state = demo.state.clone();
    super::open_modal_window(
        360.0,
        if state == "header-menu" { 150.0 } else { 130.0 },
        move |window, cx| cx.new(|cx| ExtensionMenuDemo::new(state, window, cx)),
        cx,
    );
}
