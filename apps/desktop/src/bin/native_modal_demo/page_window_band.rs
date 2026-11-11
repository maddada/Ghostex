//! Preview of the Ghostex page window's top band: the round controls (open in browser, Hide,
//! close) at the band's sizes and insets (`visual_page_modal/band_geometry.rs`), on the right as
//! with Close buttons on the right, and on the left with the close button first as without it.
//! Run it with `GHOSTEX_NATIVE_MODAL_DEMO=page-window-band`; dashed guides mark the inset from the
//! top and from the side, which match.
use super::page_band_geometry::{CONTROL_INSET, CONTROL_SIZE, TOP_BAND_HEIGHT};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    div, px, rgb, rgba,
};

struct PageWindowBandDemo;

fn control() -> impl IntoElement {
    div()
        .flex_shrink_0()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .bg(rgba(0xffffff1f))
        .border_1()
        .border_color(rgba(0xffffff2e))
}

fn band(left: bool) -> impl IntoElement {
    let controls = div()
        .flex()
        .gap(px(6.0))
        .child(control())
        .child(control())
        .when(!left, |row| row.child(control()));
    div()
        .relative()
        .w(px(460.0))
        .h(px(TOP_BAND_HEIGHT + 60.0))
        .rounded(px(12.0))
        .bg(rgb(0x1d1b1e))
        .border_1()
        .border_color(rgba(0xffffff1a))
        .child(
            div()
                .absolute()
                .top(px(TOP_BAND_HEIGHT))
                .left_0()
                .right_0()
                .bottom_0()
                .rounded_b(px(12.0))
                .bg(rgb(0x252327)),
        )
        .child(
            div()
                .absolute()
                .top(px(CONTROL_INSET))
                .map(|corner| {
                    if left {
                        corner.left(px(CONTROL_INSET))
                    } else {
                        corner.right(px(CONTROL_INSET))
                    }
                })
                .child(controls),
        )
        .when(left, |band| {
            band.child(
                div()
                    .absolute()
                    .top(px(CONTROL_INSET))
                    .right(px(CONTROL_INSET))
                    .child(control()),
            )
        })
}

impl Render for PageWindowBandDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x2a2a30))
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .text_color(rgb(0xe7e7ea))
            .text_size(px(12.0))
            .child(format!(
                "Band {TOP_BAND_HEIGHT}pt, controls {CONTROL_SIZE}pt, {CONTROL_INSET}pt from the top and from the side"
            ))
            .child("Close buttons on the right: on")
            .child(band(false))
            .child("Close buttons on the right: off (close first, then Hide)")
            .child(band(true))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let _ = demo;
    let _ = super::open_modal_window(
        520.0,
        360.0,
        move |_, cx| cx.new(|_| PageWindowBandDemo),
        cx,
    );
}
