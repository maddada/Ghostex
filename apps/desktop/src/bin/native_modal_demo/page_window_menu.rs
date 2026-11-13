//! Preview of the Ghostex page window's ⋯ menu open under its button: the band's round controls
//! (⋯, Hide, close) at the band's sizes, and the menu panel the app draws
//! (`visual_page_modal/more_menu_panel.rs`) in the dark menu colours with glass off, placed as
//! `more_menu.rs` places it (right edge on the ⋯ button's, 6pt below it). Run it with
//! `GHOSTEX_NATIVE_MODAL_DEMO=page-window-menu`.
use super::page_band_geometry::{CONTROL_INSET, CONTROL_SIZE, TOP_BAND_HEIGHT};
use super::page_window_menu_panel::{
    MENU_WIDTH, MenuInk, SNOOZE_HOURS, SUBMENU_WIDTH, height, menu_rows, panel, snooze_rows,
};
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
            foreground: rgb(0xf4f4f5).into(),
            muted: rgb(0xb4b4bb).into(),
            hover: rgba(0xffffff1c).into(),
            border: rgba(0xffffff2e).into(),
            fill: rgba(0x52525acc).into(),
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
                    .h(px(height(4, ink.row_height)))
                    .child(panel(
                        "visual-page-more-menu",
                        ink,
                        menu_rows(true),
                        Some(0),
                        std::rc::Rc::new(|_, _| {}),
                    )),
            )
            // The Snooze lengths, beside the Snooze row toward the page (more_menu.rs).
            .child(
                div()
                    .absolute()
                    .left(px(menu_left - SUBMENU_WIDTH - 4.0))
                    .top(px(menu_top + 1.0))
                    .w(px(SUBMENU_WIDTH))
                    .h(px(height(SNOOZE_HOURS.len(), ink.row_height)))
                    .child(panel(
                        "visual-page-snooze-menu",
                        ink,
                        snooze_rows(),
                        None,
                        std::rc::Rc::new(|_, _| {}),
                    )),
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
