//! The tab a hidden Ghostex page leaves after the session's title in the chat's header: the page's
//! name, a dot while a question on it is unanswered, and while pointed at, the toasts' round x
//! floating over its corner. A click brings the page back; the x closes it for good.
//! SEE-ALSO: apps/desktop/src/app/visual_page_hidden.rs (the user's picks and the hidden pages).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use gpui_component::tooltip::Tooltip;

use crate::app::helpers::*;
use crate::app::visual_page_hidden::HiddenVisualPageTab;
use crate::app::window::round_close_button::*;
use crate::*;

/// The tab's height, its gap from the title, and the widest its name gets.
const TAB_HEIGHT: f32 = 24.0;
const TAB_GAP: f32 = 8.0;
const TAB_NAME_MAX_WIDTH: f32 = 200.0;
/// The floating x: the toasts' round x at this size, overhanging the tab's corner by `X_OUTSET`.
const X_SIZE: f32 = 16.0;
const X_OUTSET: f32 = 6.0;
const DOT_SIZE: f32 = 7.0;

impl GhostexGpuiApp {
    pub(crate) fn render_hidden_visual_page_tab(
        &self,
        tab: HiddenVisualPageTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let light = chrome_uses_light_appearance();
        let key = tab.key;
        let tip = format!("Show {}", tab.title);
        let close = tab.hovered.then(|| {
            div()
                .id(("hidden-page-tab-close", key))
                .absolute()
                .top(px(-X_OUTSET))
                .map(|button| {
                    if close_buttons_on_left() {
                        button.left(px(-X_OUTSET))
                    } else {
                        button.right(px(-X_OUTSET))
                    }
                })
                .size(px(X_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(gpui_app_toast_close_button_border_color(light))
                .bg(gpui_app_toast_close_button_color(light))
                .hover(move |style| style.bg(gpui_app_toast_close_button_hover_color(light)))
                .cursor_default()
                .tooltip(|window, cx| Tooltip::new("Close page").build(window, cx))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.close_hidden_visual_page(key, cx);
                    }),
                )
                .child(round_close_x_icon(
                    X_SIZE / ROUND_CLOSE_BUTTON_SIZE,
                    gpui_app_toast_close_button_icon_color(light),
                ))
        });
        div()
            .id(("hidden-page-tab", key))
            .relative()
            .flex_shrink_0()
            .ml(px(TAB_GAP))
            .h(px(TAB_HEIGHT))
            .pl(px(6.0))
            .pr(px(7.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(6.0))
            .bg(titlebar_split_button_hover_color())
            .hover(|style| style.bg(titlebar_split_button_open_color()))
            .text_size(px(12.0))
            .line_height(px(TAB_HEIGHT))
            .text_color(titlebar_text_color())
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hidden_visual_page_tab_hovered(key, *hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.show_hidden_visual_page(key, cx)))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .child(
                svg()
                    .flex_shrink_0()
                    .size(px(13.0))
                    .path("titlebar/file-type-html.svg")
                    .text_color(titlebar_icon_color()),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w(px(TAB_NAME_MAX_WIDTH))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(tab.title),
            )
            .when(tab.questions_left, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .size(px(DOT_SIZE))
                        .rounded_full()
                        .bg(rgb(crate::app::native_sidebar::status::QUESTION_COLOR)),
                )
            })
            .children(close)
            .into_any_element()
    }
}
