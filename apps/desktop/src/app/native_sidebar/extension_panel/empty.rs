//! The `empty` block: a centred message (the block's title and subtitle) with an icon and, when
//! the panel gives one, a button for its action.

use ghostex_gx_protocol::extension_panel::{PanelAction, PanelBlock};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{h_flex, v_flex};

use super::PanelPlace;
use super::icons::panel_icon;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_native_panel_empty(
        &self,
        block: &PanelBlock,
        subtitle: Option<&str>,
        icon: Option<&str>,
        action: Option<&PanelAction>,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let button = action.map(|action| {
            let action = action.clone();
            let click_place = place.clone();
            let click_block = block.id.clone();
            h_flex()
                .id(SharedString::from(format!(
                    "ext-empty-{}-{}",
                    place.ui_key(),
                    block.id
                )))
                .role(gpui::Role::Button)
                .h(px(26.0 * scale))
                .px(px(10.0 * scale))
                .rounded(px(6.0 * scale))
                .border_1()
                .border_color(appearance.foreground.opacity(0.12))
                .text_size(px(12.0 * scale))
                .hover(|button| button.bg(appearance.hover))
                .child("Open")
                .on_click(cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.run_native_panel_action(
                        &click_place,
                        &click_block,
                        None,
                        &action,
                        window,
                        cx,
                    );
                }))
        });
        v_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .py(px(14.0 * scale))
            .px(px(10.0 * scale))
            .gap(px(6.0 * scale))
            .children(panel_icon(
                icon,
                &place.extension_id,
                22.0 * scale,
                appearance.muted,
            ))
            .child(
                div()
                    .text_center()
                    .child(block.title.clone().unwrap_or_default()),
            )
            .when_some(subtitle, |column, subtitle| {
                column.child(
                    div()
                        .text_center()
                        .text_size(px(11.5 * scale))
                        .text_color(appearance.muted)
                        .child(subtitle.to_string()),
                )
            })
            .children(button)
            .into_any_element()
    }
}
