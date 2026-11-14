//! Preview of the Browser address bar's route icon: only the logo of the computer the page loads
//! from (no words), its name in the tooltip (CDXC:Browser 2026-11-15 in
//! app/titlebar/icon_button_and_browser_toolbar.rs). Run it with
//! `GHOSTEX_NATIVE_MODAL_DEMO=browser-route-icon` and `GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light`;
//! it draws the toolbar's left end for a Mac, a Windows computer, an Arch one (`m7-arch`) and a
//! Cloud Box, each with the same icon markup and colour as the real toolbar.
use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    div, px, rgb, svg,
};

struct BrowserRouteIconDemo {
    light: bool,
}

impl Render for BrowserRouteIconDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let light = self.light;
        let (bar, border, label, icon) = if light {
            (0xf4f4f5, 0xd4d4d4, 0x18181b, 0x525252)
        } else {
            (0x161616, 0x252525, 0xe7e7ea, 0xb7b7b7)
        };
        let row = |name: &'static str, asset: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(300.0))
                        .h(px(36.0))
                        .bg(rgb(bar))
                        .border_b_1()
                        .border_color(rgb(border))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .px(px(9.0))
                                .items_center()
                                .child(svg().path(asset).size(px(14.0)).text_color(rgb(icon))),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(icon))
                                .child("‹  ›  ⟳  ⌂"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(label))
                        .child(format!("tooltip: {name}")),
                )
        };
        div()
            .size_full()
            .bg(rgb(if light { 0xffffff } else { 0x0c0c0c }))
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(row("MacBook Pro", "titlebar/brand-apple.svg"))
            .child(row("DESKTOP-WIN", "titlebar/brand-windows.svg"))
            .child(row("m7-arch", "titlebar/brand-arch.svg"))
            .child(row("box-c31", "titlebar/cloud.svg"))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let light = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_THEME")
        .map(|theme| theme.trim().eq_ignore_ascii_case("light"))
        .unwrap_or(false);
    let _ = demo;
    let _ = super::open_modal_window(
        560.0,
        240.0,
        move |_, cx| cx.new(|_| BrowserRouteIconDemo { light }),
        cx,
    );
}
