//! The Screenshot mode chip at the foot of the sidebar, where the account tiles it hides were: it
//! says screenshot mode is on, and a click turns it off (CDXC:ScreenshotMode 2026-11-20 in
//! app/screenshot_mode.rs: "clearly shown as on so it isn't left on by mistake").

use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

impl GhostexGpuiApp {
    /// The chip, while screenshot mode is on.
    pub(super) fn render_native_sidebar_screenshot_chip(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !crate::app::screenshot_mode::screenshot_mode_on() {
            return None;
        }
        let scale = appearance.scale;
        // Amber, so it reads as a state that is on rather than as a quiet label.
        let tone: gpui::Hsla = gpui::rgb(if appearance.light { 0xa86b00 } else { 0xe5b33b }).into();
        let chip = h_flex()
            .id("native-sidebar-screenshot-mode")
            .role(gpui::Role::Button)
            .aria_label("Screenshot mode is on. Turn it off")
            .gap(px(5.0 * scale))
            .h(px(22.0 * scale))
            .px(px(8.0 * scale))
            .rounded_full()
            .border_1()
            .border_color(tone.opacity(0.45))
            .bg(tone.opacity(0.12))
            .hover(move |chip| chip.bg(tone.opacity(0.2)))
            .active(move |chip| chip.bg(tone.opacity(0.28)))
            .cursor_pointer()
            .text_size(px(11.5 * scale))
            .text_color(tone)
            .child(titlebar_svg_icon("titlebar/camera.svg", 12.0 * scale, tone))
            .child("Screenshot mode")
            .managed_tooltip(|window, cx| {
                super::link_chips::sidebar_style_tooltip(
                    "Account names, account tiles and other workspaces are hidden. Click to turn off."
                        .to_string(),
                    window,
                    cx,
                )
            })
            .on_click(cx.listener(|app, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                app.handle_gpui_app_modal_update_settings_patch_message(
                    &json!({
                        "patch": { crate::app::screenshot_mode::SCREENSHOT_MODE_KEY: false },
                        "source": "sidebar:screenshotMode",
                        "type": "updateSettingsPatch",
                    }),
                    cx,
                );
            }));
        Some(
            div()
                .w_full()
                .flex()
                .justify_center()
                .pt(px(6.0 * scale))
                .pb(px(8.0 * scale))
                .child(chip)
                .into_any_element(),
        )
    }
}
