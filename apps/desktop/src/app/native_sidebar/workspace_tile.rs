//! The workspace tile at the left end of the Spaces row: a split button. The left part is the
//! window's workspace as its letter on the workspace's color, and a click switches to the other
//! workspace (gx-core `workspace_switch_target`); the slim chevron on the right opens the
//! workspace menu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: one tile to the left of the Spaces row (not a rail), then that workspace's Spaces. The menu
//! comes from gx-core (`sidebar_menu/workspace.rs`).
//!
//! CDXC:Workspaces 2026-10-10 DECISION:
//! User picked option A of the one-click switch mockup (the switch rule is gx-core `workspace_switch_target`) and asked to "make the look of the profile switcher match the size etc of the space buttons and don't add a | there between them". So the split button has the Space buttons' height, radius, hairline border and hover, its letter block is sized like a Space icon, and the hairline that followed the tile is gone (this supersedes the 2026-10-09 hairline).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The letter part's width: a Space button's 28px face less the split button's left border.
const LETTER_PART_WIDTH: f32 = 27.0;
/// The chevron part's width, its right border included.
const CHEVRON_PART_WIDTH: f32 = 15.0;

/// The split button and the row gap after it: what the Space row gives up for it.
pub(super) const WORKSPACE_TILE_ROOM: f32 = LETTER_PART_WIDTH + CHEVRON_PART_WIDTH + 4.0;

impl GhostexGpuiApp {
    /// Whether the window draws the workspace tile: this computer's section is shown and its
    /// daemon has workspaces. With Spaces off the row is drawn for the tile alone only once there
    /// is more than one workspace, so an install that never made one looks as it did.
    pub(super) fn native_sidebar_shows_workspace_tile(
        &self,
        selected_machine_id: &str,
        spaces_enabled: bool,
    ) -> bool {
        selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            && self
                .gx_store_workspaces_state()
                .is_some_and(|state| spaces_enabled || state.workspaces.len() > 1)
    }

    pub(super) fn render_native_sidebar_workspace_tile(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let tile = self.gx_store_workspace_tile()?;
        let scale = appearance.scale;
        let color = u32::from_str_radix(tile.color.trim_start_matches('#'), 16)
            .map(rgb)
            .map(gpui::Hsla::from)
            .unwrap_or(appearance.muted);
        let border = appearance.foreground.opacity(0.08);
        let tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        let tooltip_delay = appearance.tooltip_delay;
        let (switch_to, letter_tooltip) = match tile.switch_to {
            Some((workspace_id, name)) => (Some(workspace_id), format!("Switch to {name}")),
            None => (None, tile.name.clone()),
        };
        let menu_tooltip = tile.name.clone();
        let open_menu = move |app: &mut Self,
                              event: &gpui::ClickEvent,
                              window: &mut gpui::Window,
                              cx: &mut gpui::Context<Self>| {
            let Some(menu) = app.gx_store_workspace_menu(cx) else {
                return;
            };
            Self::show_native_sidebar_menu(&menu, event.position(), scale, window, cx);
        };
        let letter = div()
            .id("native-sidebar-workspace-switch")
            .role(gpui::Role::Button)
            .aria_label(letter_tooltip.clone())
            .w(px(LETTER_PART_WIDTH * scale))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded_l(px(5.0 * scale))
            .cursor_pointer()
            .hover(|part| part.bg(appearance.hover))
            .child(
                div()
                    .size(px(18.0 * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * scale))
                    .bg(color)
                    .text_size(px(11.0 * scale))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(gpui::white())
                    .child(tile.letter.clone()),
            )
            .when(tooltips, |part| {
                part.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_free_width_tooltip(
                            letter_tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .on_click(
                cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    match switch_to.clone() {
                        Some(workspace_id) => app.gx_store_select_workspace(workspace_id, cx),
                        // A single workspace has nowhere to switch to: the letter opens the menu.
                        None => open_menu(app, event, window, cx),
                    }
                }),
            );
        let chevron = div()
            .id("native-sidebar-workspace-menu")
            .role(gpui::Role::Button)
            .aria_label(format!("Workspace {} menu", tile.name))
            .w(px((CHEVRON_PART_WIDTH - 1.0) * scale))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(border)
            .rounded_r(px(5.0 * scale))
            .cursor_pointer()
            .hover(|part| part.bg(appearance.hover))
            .child(titlebar_svg_icon(
                "titlebar/chevron-down.svg",
                10.0 * scale,
                appearance.muted,
            ))
            .when(tooltips, |part| {
                part.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_free_width_tooltip(
                            menu_tooltip.clone(),
                            tooltip_span,
                            scale,
                            window,
                            cx,
                        )
                    },
                )
            })
            .on_click(
                cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    open_menu(app, event, window, cx);
                }),
            );
        Some(
            div()
                .id("native-sidebar-workspace-tile")
                .flex()
                .flex_shrink_0()
                .h(px(28.0 * scale))
                .rounded(px(6.0 * scale))
                .border_1()
                .border_color(border)
                .child(letter)
                .child(chevron)
                .into_any_element(),
        )
    }
}
