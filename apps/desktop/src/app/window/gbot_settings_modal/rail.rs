//! The GBot Settings rail: the bot's icon in its colour and its name on top (live as the Profile
//! changes), then the tabs, drawn like Settings' rail rows (settings_modal/rail.rs).
use super::super::native_modal_kit::*;
use super::model::{gbot_color_hex, gbot_icon_path};
use super::window::{GbotSettingsTab, GpuiGbotSettingsModalWindow};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use gpui_component::{h_flex, v_flex};

/// Settings' rail width.
const RAIL_WIDTH: f32 = 192.0;

impl GpuiGbotSettingsModalWindow {
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let name = match self.draft.name.trim() {
            "" => "GBot".to_string(),
            name => name.to_string(),
        };
        let role = self.draft.title.trim().to_string();
        let header = v_flex()
            .w_full()
            .min_w_0()
            .px(px(10.0))
            .pt(px(4.0))
            .pb(px(12.0))
            .mb(px(6.0))
            .gap(px(4.0))
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        svg()
                            .path(SharedString::from(gbot_icon_path(&self.draft.icon)))
                            .flex_shrink_0()
                            .size(px(18.0))
                            .text_color(hsla(rgb(gbot_color_hex(&self.draft.color)))),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(14.0))
                            .line_height(px(19.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(name),
                    ),
            )
            .when(!role.is_empty(), |this| {
                this.child(
                    div()
                        .pl(px(26.0))
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(role),
                )
            });
        let accent = p.raised_hover;
        let resting = p.foreground_alpha(0.82);
        let tabs = GbotSettingsTab::ALL.into_iter().map(|tab| {
            let active = self.tab == tab;
            let tone = if active { p.foreground } else { resting };
            h_flex()
                .id(SharedString::from(format!(
                    "gbot-settings-tab-{}",
                    tab.title()
                )))
                .role(gpui::Role::Button)
                .aria_label(tab.title())
                .aria_selected(active)
                .w_full()
                .min_w_0()
                .min_h(px(32.0))
                .px(px(10.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .cursor_pointer()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(hsla(tone))
                .when(active, |this| {
                    this.bg(hsla(accent)).font_weight(FontWeight::MEDIUM)
                })
                .hover(move |this| this.bg(hsla(accent)).text_color(hsla(p.foreground)))
                .on_press(cx, move |this: &mut Self, window, cx| {
                    this.select_tab(tab, window, cx)
                })
                .child(
                    svg()
                        .path(tab.icon())
                        .flex_shrink_0()
                        .size(px(16.0))
                        .text_color(hsla(tone)),
                )
                .child(tab.title())
        });
        v_flex()
            .w(px(RAIL_WIDTH))
            .flex_shrink_0()
            .h_full()
            .mt(px(-1.0))
            .px(px(8.0))
            .py(px(10.0))
            .gap(px(4.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .overflow_hidden()
            .child(header)
            .children(tabs)
            .into_any_element()
    }
}
