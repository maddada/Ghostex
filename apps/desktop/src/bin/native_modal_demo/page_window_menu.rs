//! Preview of the Ghostex page window's ⋯ menu open under its button: the band's round controls
//! (⋯, Hide, close) at the band's sizes, and the menu panel the app draws
//! (`visual_page_modal/more_menu_panel.rs`) in the dark menu colours with glass off, placed as
//! `more_menu.rs` places it (right edge on the ⋯ button's, 6pt below it). Run it with
//! `GHOSTEX_NATIVE_MODAL_DEMO=page-window-menu`.
use super::page_band_geometry::{CONTROL_INSET, CONTROL_SIZE, TOP_BAND_HEIGHT};
use super::page_window_menu_panel::{MENU_WIDTH, MenuInk, height, panel};
use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    div, px, rgb, rgba, svg,
};

struct PageWindowMenuDemo;

const WIDTH: f32 = 560.0;

fn control(icon: &'static str, pressed: bool) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .bg(if pressed {
            rgba(0x404040b8)
        } else {
            rgba(0x26262699)
        })
        .border_1()
        .border_color(rgba(0xe9e9e924))
        .child(svg().path(icon).size(px(14.0)).text_color(rgba(0xc8cdd5ff)))
}

impl Render for PageWindowMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let ink = MenuInk {
            foreground: rgb(0xfcfcfc).into(),
            hover: rgb(0xffffff).opacity(0.08).into(),
            border: rgb(0xffffff).opacity(0.12).into(),
            fill: rgb(0x2b2c30).into(),
            row_height: 34.0,
        };
        // The ⋯ button is the first of the three on the right: its right edge sits two buttons
        // and two gaps in from the right inset.
        let more_right = WIDTH - CONTROL_INSET - 2.0 * (CONTROL_SIZE + 6.0);
        let menu_left = more_right - MENU_WIDTH;
        let menu_top = CONTROL_INSET + CONTROL_SIZE + 6.0;
        div()
            .relative()
            .size_full()
            .bg(rgb(0x131313))
            .text_color(rgb(0xe7e7ea))
            .child(
                div()
                    .absolute()
                    .top(px(CONTROL_INSET))
                    .right(px(CONTROL_INSET))
                    .flex()
                    .gap(px(6.0))
                    .child(control("titlebar/dots.svg", true))
                    .child(control("titlebar/window-minimize.svg", false))
                    .child(control("titlebar/x.svg", false)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(TOP_BAND_HEIGHT + 34.0))
                    .left(px(40.0))
                    .w(px(360.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x9b9ba3))
                            .child("THREAD BRIEF · ISSUE #593 · DESIGN"),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Thread brief: every session keeps an objective"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xa1a1aa))
                            .child("Coordinator work items, the Work panel and the dock, opened up to every session."),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(menu_left))
                    .top(px(menu_top))
                    .w(px(MENU_WIDTH))
                    .h(px(height(ink.row_height)))
                    .child(panel(ink, std::rc::Rc::new(|_, _| {}))),
            )
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let _ = demo;
    let _ = super::open_modal_window(
        WIDTH,
        340.0,
        move |_, cx| cx.new(|_| PageWindowMenuDemo),
        cx,
    );
}
