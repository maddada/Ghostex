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
/// The floating x: the toasts' round x at this size, overhanging the tab's corner by `X_OUTSET`
/// sideways and `X_RISE` upwards.
///
/// CDXC:SessionChat 2026-11-11 WHY: User, of the x drawn 6pt above the tab: "This is cut off". The tabs sat in the title's row, which is only as tall as the header's buttons (27pt) and clips its text, so the x lost its upper half. The tabs now sit beside that row, and the x rises 4pt above a tab whose top is 7pt below the header's top edge (36pt header, 24pt tab, the title's 1pt nudge), so 3pt are left; the row ends with `X_OUTSET` of room for an x on the right.
const X_SIZE: f32 = 16.0;
const X_OUTSET: f32 = 6.0;
const X_RISE: f32 = 4.0;
const DOT_SIZE: f32 = 7.0;

impl GhostexGpuiApp {
    /// The hidden pages' tabs after the breadcrumb's title, oldest first.
    pub(crate) fn render_hidden_visual_page_tabs(
        &self,
        tabs: Vec<HiddenVisualPageTab>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        gpui::div()
            .flex()
            .flex_shrink_0()
            .items_center()
            // The title's 1pt nudge (breadcrumb.rs), so a tab's text lines up with it.
            .relative()
            .top(px(1.0))
            .when(!close_buttons_on_left(), |row| row.pr(px(X_OUTSET)))
            .children(
                tabs.into_iter()
                    .map(|tab| self.render_hidden_visual_page_tab(tab, cx)),
            )
            .into_any_element()
    }

    fn render_hidden_visual_page_tab(
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
                .top(px(-X_RISE))
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
