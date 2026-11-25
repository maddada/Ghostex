//! Preview of the Ghostex page window's ⋯ menu open under its button: the band's round controls
//! (⋯, Hide, close) at the band's sizes, and the menu panel the app draws
//! (`visual_page_modal/more_menu_panel.rs`) with its Snooze lengths open, placed as `more_menu.rs`
//! places them (right edge on the ⋯ button's, 6pt below it), in the frosted colours of
//! `more_menu.rs` `menu_ink`. The app's macOS window shadow is stood in for by a drawn one, and the
//! glass shows the preview's backdrop unblurred. Run it with
//! `GHOSTEX_NATIVE_MODAL_DEMO=page-window-menu` (`GHOSTEX_NATIVE_MODAL_DEMO_THEME=light` for light;
//! `GHOSTEX_NATIVE_MODAL_DEMO_STATE=hover` marks Star page as if hovered).
use super::page_band_geometry::{CONTROL_INSET, CONTROL_SIZE, TOP_BAND_HEIGHT};
use super::page_window_menu_panel::{
    MENU_RADIUS, MENU_WIDTH, MenuInk, SUBMENU_WIDTH, first_row_top, height, menu_lines, panel,
    snooze_lines,
};
use gpui::{
    App, AppContext as _, BoxShadow, Context, Hsla, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, point, px, rgb, rgba, svg,
};

struct PageWindowMenuDemo {
    light: bool,
    hover: bool,
}

const WIDTH: f32 = 620.0;
const HEIGHT: f32 = 440.0;

fn control(icon: &'static str, pressed: bool, light: bool) -> impl IntoElement {
    // The neutral filled button's rest and hover fills, no border (`page_controls.rs`).
    let (fill, on, ink) = if light {
        (0x00000012, 0x0000001c, 0x3f3f46ff)
    } else {
        (0xffffff1a, 0xffffff24, 0xc8cdd5ff)
    };
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .bg(rgba(if pressed { on } else { fill }))
        .child(svg().path(icon).size(px(14.0)).text_color(rgba(ink)))
}

/// `more_menu.rs` `menu_ink` under window glass.
fn ink(light: bool) -> MenuInk {
    if light {
        MenuInk {
            foreground: rgb(0x18181b).into(),
            muted: rgb(0x71717b).into(),
            icon: rgb(0x3f3f46).into(),
            hover: rgba(0x0000000e).into(),
            border: rgba(0x00000014).into(),
            line: rgba(0x00000014).into(),
            top_edge: rgba(0xffffffe6).into(),
            fill: rgba(0xffffffb3).into(),
        }
    } else {
        MenuInk {
            foreground: rgb(0xf4f4f5).into(),
            muted: rgb(0xa6a6ae).into(),
            icon: rgb(0xd4d4d8).into(),
            hover: rgba(0xffffff1a).into(),
            border: rgba(0xffffff24).into(),
            line: rgba(0xffffff17).into(),
            top_edge: rgba(0xffffff17).into(),
            fill: rgba(0x2c2c32b3).into(),
        }
    }
}

/// The mockup's soft shadow for option A, standing in for the window shadow macOS casts.
fn soft_shadow(light: bool) -> Vec<BoxShadow> {
    let shadow = |color: u32, y: f32, blur: f32| BoxShadow {
        color: Hsla::from(rgba(color)),
        offset: point(px(0.0), px(y)),
        // GPUI's blur radius is the spread of the fade, half of CSS's.
        blur_radius: px(blur / 2.0),
        spread_radius: px(0.0),
        inset: false,
    };
    if light {
        vec![shadow(0x0f172a29, 16.0, 40.0), shadow(0x0f172a14, 2.0, 6.0)]
    } else {
        vec![shadow(0x0000006b, 16.0, 40.0), shadow(0x0000003d, 2.0, 8.0)]
    }
}

fn bar(width: f32, color: u32) -> impl IntoElement {
    div()
        .w(px(width))
        .h(px(7.0))
        .mb(px(9.0))
        .rounded(px(4.0))
        .bg(rgba(color))
}

impl Render for PageWindowMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let light = self.light;
        let ink = ink(light);
        let (page, card, card_border, text_bar, strong_bar) = if light {
            (0xe9ebf0, 0xf8f9fb, 0x43434321, 0x3f3f4633, 0x18181b8c)
        } else {
            (0x0f1219, 0x181a1f, 0xe9e9e91f, 0xc8cdd538, 0xfcfcfc80)
        };
        // The ⋯ button is the first of the three on the right: its right edge sits two buttons
        // and two gaps in from the right inset.
        let more_right = WIDTH - CONTROL_INSET - 2.0 * (CONTROL_SIZE + 6.0);
        let menu_left = more_right - MENU_WIDTH;
        let menu_top = CONTROL_INSET + CONTROL_SIZE + 6.0;
        let lines = menu_lines("Sidebar cleanup", Some("2h ago"), true, Some(false));
        let sub_lines = snooze_lines();
        let sub_top = menu_top + first_row_top(&lines) - first_row_top(&sub_lines);
        let menu_height = height(&lines);
        let sub_height = height(&sub_lines);
        let mut menu = panel(
            "visual-page-more-menu",
            ink,
            lines,
            Some(0),
            std::rc::Rc::new(|_, _| {}),
        );
        if self.hover {
            // Star page under the pointer, drawn as the row's hover draws it.
            menu = div()
                .relative()
                .size_full()
                .child(menu)
                .child(
                    div()
                        .absolute()
                        .left(px(7.0))
                        .right(px(7.0))
                        .top(px(
                            first_row_top(&menu_lines("", None, true, Some(false))) + 32.0
                        ))
                        .h(px(32.0))
                        .rounded(px(8.0))
                        .bg(ink.hover),
                )
                .into_any_element();
        }
        div()
            .relative()
            .size_full()
            .bg(rgb(page))
            .child(
                div()
                    .absolute()
                    .top(px(16.0))
                    .left(px(28.0))
                    .child(bar(300.0, text_bar))
                    .child(bar(320.0, text_bar))
                    .child(bar(190.0, text_bar)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(TOP_BAND_HEIGHT + 44.0))
                    .left(px(18.0))
                    .right(px(18.0))
                    .bottom(px(-30.0))
                    .p(px(18.0))
                    .rounded(px(14.0))
                    .bg(rgb(card))
                    .border_1()
                    .border_color(rgba(card_border))
                    .child(bar(240.0, strong_bar))
                    .child(bar(460.0, text_bar))
                    .child(bar(330.0, text_bar))
                    .child(bar(390.0, text_bar))
                    .child(bar(300.0, text_bar))
                    .child(bar(420.0, text_bar)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(CONTROL_INSET))
                    .right(px(CONTROL_INSET))
                    .flex()
                    .gap(px(6.0))
                    .child(control("titlebar/dots.svg", true, light))
                    .child(control("titlebar/window-minimize.svg", false, light))
                    .child(control("titlebar/x.svg", false, light)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(menu_left))
                    .top(px(menu_top))
                    .w(px(MENU_WIDTH))
                    .h(px(menu_height))
                    .rounded(px(MENU_RADIUS))
                    .shadow(soft_shadow(light))
                    .child(menu),
            )
            // The Snooze lengths, beside the Snooze row toward the page (more_menu.rs).
            .child(
                div()
                    .absolute()
                    .left(px(menu_left - SUBMENU_WIDTH - 4.0))
                    .top(px(sub_top))
                    .w(px(SUBMENU_WIDTH))
                    .h(px(sub_height))
                    .rounded(px(MENU_RADIUS))
                    .shadow(soft_shadow(light))
                    .child(panel(
                        "visual-page-snooze-menu",
                        ink,
                        sub_lines,
                        None,
                        std::rc::Rc::new(|_, _| {}),
                    )),
            )
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let light = demo.palette.light;
    let hover = demo.state == "hover";
    let _ = super::open_modal_window(
        WIDTH,
        HEIGHT,
        move |_, cx| cx.new(|_| PageWindowMenuDemo { light, hover }),
        cx,
    );
}
