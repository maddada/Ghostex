//! The `buttons` block: a row of buttons, each running its action.

use ghostex_gx_core::extension_panel::panel_action_allowed;
use ghostex_gx_protocol::extension_panel::{PanelBlock, PanelButton};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::h_flex;

use super::PanelPlace;
use super::icons::panel_icon;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    pub(super) fn render_native_panel_buttons(
        &self,
        block: &PanelBlock,
        items: &[PanelButton],
        place: &PanelPlace,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .px(px(6.0 * scale))
            .gap(px(6.0 * scale))
            .children(items.iter().enumerate().map(|(index, button)| {
                let primary = button.style.as_deref() == Some("primary");
                let allowed = panel_action_allowed(&button.action, &place.granted_permissions);
                let click_place = place.clone();
                let click_block = block.id.clone();
                let click_item = button.id.clone();
                let action = button.action.clone();
                h_flex()
                    .id(SharedString::from(format!(
                        "ext-button-{}-{}-{index}",
                        place.ui_key(),
                        block.id
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(button.title.clone()))
                    .h(px(26.0 * scale))
                    .px(px(10.0 * scale))
                    .gap(px(6.0 * scale))
                    .rounded(px(6.0 * scale))
                    .border_1()
                    .border_color(appearance.foreground.opacity(0.12))
                    .text_size(px(12.0 * scale))
                    .when(primary, |button| button.bg(appearance.selected))
                    .hover(|button| button.bg(appearance.hover))
                    .when(!allowed, |button| button.opacity(0.5))
                    .children(panel_icon(
                        button.icon.as_deref(),
                        &place.extension_id,
                        13.0 * scale,
                        appearance.chrome_icon,
                    ))
                    .child(div().min_w_0().truncate().child(button.title.clone()))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.run_native_panel_action(
                            &click_place,
                            &click_block,
                            click_item.as_deref(),
                            &action,
                            window,
                            cx,
                        );
                    }))
                    .into_any_element()
            }))
            .into_any_element()
    }
}
