//! A sidebar extension's own mode: its button in the sidebar's top row, drawn like the Bots
//! toggle (bots.rs), and the panel that takes the list's place while it is on.

use ghostex_gx_core::extension_panel::{ExtensionModeButton, ExtensionSurfaceView};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::icons::panel_icon;
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_sidebar::appearance::SidebarAppearance;
use crate::app::native_sidebar::navigation::{
    SIDEBAR_TOP_ROW_BUTTON_HEIGHT, SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH, SIDEBAR_TOP_ROW_BUTTON_RADIUS,
    SIDEBAR_TOP_ROW_BUTTON_WIDTH,
};

impl GhostexGpuiApp {
    /// One top-row button per extension mode: the extension's icon, a back arrow while its
    /// mode is showing, filled then like the Bots toggle.
    pub(crate) fn render_native_sidebar_extension_mode_button(
        &self,
        button: &ExtensionModeButton,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let selected = button.selected;
        let tooltip = if selected {
            "Show projects".to_string()
        } else {
            format!("Show {}", button.title)
        };
        let extension_id = button.extension_id.clone();
        let icon = if selected {
            titlebar_svg_icon(
                "titlebar/arrow-left.svg",
                15.0 * scale,
                appearance.foreground,
            )
            .into_any_element()
        } else {
            panel_icon(
                Some(button.icon.as_deref().unwrap_or("puzzle")),
                &button.extension_id,
                15.0 * scale,
                appearance.chrome_icon,
            )
            .unwrap_or_else(|| div().into_any_element())
        };
        div()
            .id(SharedString::from(format!(
                "native-sidebar-extension-mode-{}",
                button.extension_id
            )))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(tooltip.clone()))
            .aria_selected(selected)
            .flex()
            .flex_shrink(1.0)
            .items_center()
            .justify_center()
            .h(px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
            .w(px(SIDEBAR_TOP_ROW_BUTTON_WIDTH * scale))
            .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
            .rounded(px(SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .cursor_default()
            .when(cfg!(target_os = "windows"), |button| button.occlude())
            .hover(|button| button.bg(appearance.hover))
            .when(selected, |button| button.bg(appearance.selected))
            .child(icon)
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_native_sidebar_extension_mode(&extension_id, cx);
            }))
            .managed_discrete_tooltip_with_placement(
                ManagedTooltipPlacement::Right,
                appearance.tooltip_delay,
                move |window, cx| {
                    crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                        tooltip.clone(),
                        window,
                        cx,
                    )
                },
            )
            .into_any_element()
    }

    /// The extension's panel in the list's place.
    pub(crate) fn render_native_sidebar_extension_mode(
        &self,
        view: &ExtensionSurfaceView,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .pr(px(4.0 * appearance.scale))
            .child(self.render_native_extension_surface(view, "sidebar", true, appearance, cx))
            .into_any_element()
    }
}
