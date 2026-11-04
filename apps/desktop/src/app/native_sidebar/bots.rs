//! The Hermes button that swaps the sidebar between the projects and the Hermes bots, and the
//! gateway dot a bot row draws in place of a project's status.
//!
//! CDXC:Bots 2026-09-27 DECISION:
//! User: the Bots entry is a Hermes-logo button.
//! With Spaces on it is the last slot of the Space row, after a hairline, and can't be edited, deleted or dragged like a Space; with Spaces off it is a toggle in the sidebar's top row between the Agents Panel toggle and Search.
//! Only one of the two exists at a time, and neither exists while Bots is switched off in Settings > Extensions.
//! While Bots is showing it draws a back arrow instead of the logo, so it reads as the way back to the projects; this supersedes the logo-only look of 2026-09-26.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use serde_json::json;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarGroup;
use super::navigation::SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The top-row toggle's width, which the row's compaction math counts.
pub(crate) const BOTS_TOGGLE_WIDTH: f32 = 30.0;
/// The hairline and its gap before the Space-row slot, which the slot capacity counts.
pub(crate) const BOTS_SPACE_SLOT_DIVIDER_WIDTH: f32 = 5.0;

impl GhostexGpuiApp {
    /// Swaps the list, and brings the bot projects up to date on the way into Bots.
    pub(crate) fn toggle_native_sidebar_bots_mode(&mut self, cx: &mut gpui::Context<Self>) {
        let entering = !self
            .native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.bots_mode);
        self.dispatch_native_sidebar_ui(
            json!({"type": "setSidebarMode", "mode": if entering { "bots" } else { "projects" }}),
            cx,
        );
        if entering {
            self.gx_store_sync_bot_projects(cx);
        }
    }

    /// The last slot of the Space row: a Space-sized tile, selected while Bots is showing.
    pub(crate) fn render_native_sidebar_bots_space_slot(
        &self,
        bots_mode: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let (selected_background, selected_outline) =
            super::selectors::space_tile_selected_colors(appearance);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0 * scale))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(16.0 * scale))
                    .bg(appearance.foreground.opacity(0.12)),
            )
            .child(
                self.native_sidebar_bots_button(
                    "native-sidebar-bots-space-slot",
                    bots_mode,
                    hermes_color(),
                    appearance,
                    cx,
                )
                .size(px(28.0 * scale))
                .rounded(px(6.0 * scale))
                .border_1()
                .border_color(appearance.foreground.opacity(0.08))
                .cursor_pointer()
                .when(bots_mode, |slot| {
                    slot.bg(selected_background)
                        .border_color(selected_outline)
                        .when(appearance.light && !appearance.glass, |slot| {
                            slot.shadow_sm()
                        })
                }),
            )
            .into_any_element()
    }

    /// The top-row toggle, filled while Bots is showing.
    pub(crate) fn render_native_sidebar_bots_toggle(
        &self,
        bots_mode: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        self.native_sidebar_bots_button(
            "native-sidebar-bots-toggle",
            bots_mode,
            appearance.chrome_icon,
            appearance,
            cx,
        )
        .when(cfg!(target_os = "windows"), |button| button.occlude())
        .h(px(28.0 * scale))
        .w(px(BOTS_TOGGLE_WIDTH * scale))
        .flex_shrink(1.0)
        .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
        .rounded(px(5.0 * scale))
        .cursor_default()
        .when(bots_mode, |button| button.bg(appearance.selected))
        .into_any_element()
    }

    /// What both entries share: the button's identity and state, the Hermes mark (a back arrow
    /// while Bots is showing), the toggle and its tooltip. Each entry adds its own size and
    /// selected look; `icon_color` colors only the Hermes mark.
    fn native_sidebar_bots_button(
        &self,
        id: &'static str,
        bots_mode: bool,
        icon_color: gpui::Hsla,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tooltip = if bots_mode {
            "Show projects"
        } else {
            "Show bots"
        };
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(tooltip)
            .aria_selected(bots_mode)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .hover(|button| button.bg(appearance.hover))
            .child(if bots_mode {
                titlebar_svg_icon(
                    "titlebar/arrow-left.svg",
                    15.0 * appearance.scale,
                    appearance.foreground,
                )
            } else {
                titlebar_svg_icon(
                    workspace_tab_agent_icon_path("hermes-agent").unwrap_or_default(),
                    15.0 * appearance.scale,
                    icon_color,
                )
            })
            .on_click(cx.listener(|app, _, _, cx| {
                cx.stop_propagation();
                app.toggle_native_sidebar_bots_mode(cx);
            }))
            .managed_discrete_tooltip_with_placement(
                ManagedTooltipPlacement::Right,
                appearance.tooltip_delay,
                move |window, cx| {
                    super::tooltips::sidebar_style_tooltip(tooltip.to_string(), window, cx)
                },
            )
    }
}

impl NativeSidebarGroup {
    /// Whether the bot's gateway runs, and `None` on every group that is not a bot's: the snapshot
    /// sets `botGatewayRunning` on bot projects only (gx_store/sidebar_snapshot.rs).
    pub(crate) fn bot_gateway_running(&self) -> Option<bool> {
        self.project_context
            .as_ref()?
            .get("botGatewayRunning")?
            .as_bool()
    }
}

/// The Hermes agent's own mark color, the one its tabs and launcher rows draw.
pub(crate) fn hermes_color() -> gpui::Hsla {
    rgb(workspace_tab_agent_icon_accent_color("hermes-agent")).into()
}

/// CDXC:Bots 2026-09-26 DECISION:
/// User: a bot row shows a letter tile in Hermes yellow and a gateway dot (green running, grey stopped) instead of a project's icon, git stats and status counts, so bots read as bots.
/// The tile and the dot are drawn by project_header.rs.
pub(crate) fn bot_gateway_dot(running: bool, appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .size(px(7.0 * scale))
        .flex_shrink_0()
        .rounded_full()
        .bg(rgb(if running { 0x4ade80 } else { 0x64748b }))
        .into_any_element()
}
