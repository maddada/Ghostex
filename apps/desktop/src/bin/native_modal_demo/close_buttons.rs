//! Preview of the round close (×) buttons: the board card's remove button and the toast's dismiss
//! button, drawn from the shared `round_close_button` module. Run it with
//! `GHOSTEX_NATIVE_MODAL_DEMO=close-buttons` and `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`; the
//! window draws both over a frosted-looking backdrop, at 1x and enlarged 4x beside it. The toast's ×
//! sits on the side the app puts it (`close_buttons_on_left`: the top left on macOS).
use super::round_close_button::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Hsla, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div, px, rgb,
};

struct CloseButtonsDemo {
    light: bool,
}

/// The button both the board card and the toast draw: the circle, its border, fill and ×.
fn button(light: bool, zoom: f32) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(ROUND_CLOSE_BUTTON_SIZE * zoom))
        .rounded_full()
        .border(px(zoom))
        .border_color(gpui_app_toast_close_button_border_color(light))
        .bg(gpui_app_toast_close_button_color(light))
        .child(round_close_x_icon(
            zoom,
            gpui_app_toast_close_button_icon_color(light),
        ))
}

impl Render for CloseButtonsDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let light = self.light;
        let (backdrop, card, text): (Hsla, Hsla, Hsla) = if light {
            (
                rgb(0xf1f1f3).into(),
                rgb(0xffffff).into(),
                rgb(0x18181b).into(),
            )
        } else {
            (
                rgb(0x2a2a30).into(),
                rgb(0x34343b).into(),
                rgb(0xe7e7ea).into(),
            )
        };
        let row = |label: &'static str, on_left: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(28.0))
                .child(
                    div()
                        .w(px(140.0))
                        .text_color(text)
                        .text_size(px(12.0))
                        .child(label),
                )
                .child(
                    div()
                        .w(px(200.0))
                        .h(px(54.0))
                        .rounded(px(10.0))
                        .bg(card)
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top(px(-7.0))
                                .map(|corner| {
                                    if on_left {
                                        corner.left(px(-7.0))
                                    } else {
                                        corner.right(px(-7.0))
                                    }
                                })
                                .child(button(light, 1.0)),
                        ),
                )
                .child(button(light, 4.0))
        };
        div()
            .size_full()
            .bg(backdrop)
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(28.0))
            .child(row("Board card remove", false))
            .child(row("Toast dismiss", close_buttons_on_left()))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let light = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_THEME")
        .map(|theme| theme.trim().eq_ignore_ascii_case("light"))
        .unwrap_or(false);
    let _ = demo;
    let _ = super::open_modal_window(
        560.0,
        260.0,
        move |_, cx| cx.new(|_| CloseButtonsDemo { light }),
        cx,
    );
}
