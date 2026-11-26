//! The workspace tile: the first tile of the Spaces row, after the computer tiles. It shows the
//! window's workspace as a plain glyph in the Spaces row's icon size and tone. A click switches to
//! the other workspace (gx-core `workspace_switch_target`) and a right click opens the workspace
//! menu (gx-core `workspace_menu`). The menus' workspace rows keep `workspace_badge` (white glyph
//! on a disc of the workspace's colour).
//!
//! CDXC:Workspaces 2026-10-10 DECISION:
//! User picked option A of the one-click switch mockup (the switch rule is gx-core `workspace_switch_target`): "i want 1 click to switch workspaces between work and personal one in my case (but still have access to the dropdown we have now basically)".
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User (picked on the 11.12.0 smoke improvements page): the tile's right-click menu opens through accessibility too (AXShowMenu, gpui `ShowContextMenu`), so screen readers and automation reach it without a pixel press. macOS offers the action only through the patched `.dependencies/accesskit_macos`.
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User picked "F. The first tile of the Spaces row" (Q4296): "single click switches workspace; right click shows the workspace menu; each workspace gets its own ICON (not just a colour dot), and those icons must look different from the Spaces icons". The tile leaves the bottom left for the start of the Spaces row (after the computers, which switch machines; the workspace belongs to this computer), and with Spaces off it still leads that row. Workspace icons are a set of their own (gx-protocol `WORKSPACE_ICONS`). With a single workspace nothing is drawn (`gx_store_workspace_tile`). This supersedes the dot-and-name button at the bottom left of the same day (Q4254 A) and its later card-fill tile.
//!
//! CDXC:Workspaces 2026-11-23 DECISION:
//! User: "please let's bring back the work workspace to be a toggle in the spaces row i can't keep clicking on it bottom left of the rail". The workspace tile stays at the start of the Spaces row in both Rail and Classic layouts (no longer hidden when the rail is on).
//!
//! CDXC:Workspaces 2026-11-24 DECISION:
//! User: draw the Spaces-row workspace toggle as a plain glyph (just the briefcase or person icon, no colored circle or card fill behind it) in the same tone (`appearance.chrome_icon`) and size (`16pt`) as the neighbouring Spaces-row icons, with the row's normal tile hairline and hover/active chrome. Remove the duplicate workspace switch from the bottom of the left rail (`app/sidebar_rail/render.rs`); the Spaces-row tile replaces it. The workspace right-click menu keeps `workspace_badge`.
//!
//! CDXC:Workspaces 2026-11-26 DECISION:
//! User: "if i pick show work's spaces please hide the toggle between workspaces, not needed anymore". While the window's Spaces row shows another workspace's Spaces (Settings > Workspaces > Show Work's Spaces, with Spaces on) the tile and its hairline are not drawn; they come back when that switch is off. The workspace menu and the Go to Workspace hotkeys still switch.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The tile's width before the zoom: a square tile matching the Spaces row's height, computer
/// tiles, and Space tile minimum width.
pub(super) const WORKSPACE_TILE_WIDTH: f32 = 28.0;
/// The room the tile takes in the Spaces row: itself, its gap, the hairline after it and that
/// hairline's gap.
pub(super) const WORKSPACE_TILE_ROOM: f32 = WORKSPACE_TILE_WIDTH + 4.0 + 1.0 + 4.0;
/// The plain glyph's size before the zoom: matches the neighbouring Space tiles (`selectors.rs`).
const WORKSPACE_ICON_SIZE: f32 = 16.0;

/// A workspace's mark for menu rows: its icon (an id of gx-protocol `WORKSPACE_ICONS`, drawn from
/// `assets/titlebar/<id>.svg`) in white on a filled disc of its colour, `size` across (already
/// scaled). Settings > Workspaces (built into the native-modal demo too) draws its own copy.
pub(crate) fn workspace_badge(icon: &str, color: gpui::Hsla, size: f32) -> AnyElement {
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(color)
        .child(
            gpui::svg()
                .path(gpui_sidebar_command_icon_asset_path(Some(icon)))
                .size(px((size * 0.62).round()))
                .text_color(gpui::white()),
        )
        .into_any_element()
}

impl GhostexGpuiApp {
    /// Whether the Spaces row leads with the workspace tile now: this computer's section is shown,
    /// its daemon has two or more workspaces (gxserver publishes none while the Workspaces
    /// feature is off), and the row shows no other workspace's Spaces. Shown in both Rail and
    /// Classic layouts.
    pub(super) fn native_sidebar_workspace_tile_shown(&self) -> bool {
        self.native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            })
            && !self.gx_store_window_shows_joined_spaces()
            && self.gx_store_workspace_tile().is_some()
    }

    /// The workspace tile at the start of the Spaces row (`native_sidebar_workspace_tile_shown`).
    pub(super) fn render_native_sidebar_workspace_tile(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.native_sidebar_workspace_tile_shown() {
            return None;
        }
        let tile = self.gx_store_workspace_tile()?;
        let scale = appearance.scale;
        let tooltip = match &tile.switch_to {
            Some((_, name)) => match tile.switch_hotkey.as_deref() {
                Some(hotkey) => format!("{}: click for {name} {hotkey}", tile.name),
                None => format!("{}: click for {name}", tile.name),
            },
            None => tile.name.clone(),
        };
        let tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let tooltip_span = self.native_sidebar_tooltip_span(scale);
        let tooltip_delay = appearance.tooltip_delay;
        let switch_to = tile.switch_to.map(|(workspace_id, _)| workspace_id);
        let owner = super::menus::MenuOwner::default();
        let click_owner = owner.clone();
        let right_owner = owner.clone();
        let a11y_owner = owner.clone();
        let sidebar = cx.entity().downgrade();
        let aria_label = format!("Workspace {}", tile.name);
        let element = div()
            .id("native-sidebar-workspace-tile")
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(aria_label)
            .flex_shrink_0()
            .w(px(WORKSPACE_TILE_WIDTH * scale))
            .h(px(28.0 * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .border_1()
            .border_color(appearance.foreground.opacity(0.08))
            .cursor_pointer()
            .hover(|tile| tile.bg(appearance.hover))
            .active(|tile| tile.bg(appearance.selected))
            .child(
                gpui::svg()
                    .path(gpui_sidebar_command_icon_asset_path(Some(&tile.icon)))
                    .size(px(WORKSPACE_ICON_SIZE * scale))
                    .text_color(appearance.chrome_icon),
            )
            .when(tooltips, |element| {
                element.managed_discrete_tooltip_with_placement(
                    tooltip_span.placement(),
                    tooltip_delay,
                    move |window, cx| {
                        super::tooltips::sidebar_free_width_tooltip(
                            tooltip.clone(),
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
                        // No other workspace to go to: the menu instead.
                        None => {
                            let Some(menu) = app.gx_store_workspace_menu(cx) else {
                                return;
                            };
                            Self::show_native_sidebar_context_menu(
                                &menu,
                                click_owner.get(),
                                event.position(),
                                scale,
                                window,
                                cx,
                            );
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let Some(menu) = app.gx_store_workspace_menu(cx) else {
                        return;
                    };
                    Self::show_native_sidebar_context_menu(
                        &menu,
                        right_owner.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                }),
            )
            // The right-click menu for VoiceOver and automation (AXShowMenu), hung below the tile.
            .on_a11y_action(
                gpui::AccessibleAction::ShowContextMenu,
                move |_, window, cx| {
                    let _ = sidebar.update(cx, |app, cx| {
                        let Some(menu) = app.gx_store_workspace_menu(cx) else {
                            return;
                        };
                        let owner = a11y_owner.get();
                        let at =
                            owner.map_or(gpui::Point::default(), |bounds| bounds.bottom_left());
                        Self::show_native_sidebar_context_menu(&menu, owner, at, scale, window, cx);
                    });
                },
            );
        Some(
            self.sidebar_focus_stop(
                element,
                "workspace-tile",
                super::focus_order::SidebarStopPlace::Fixed,
                appearance,
                cx,
            )
            .into_any_element(),
        )
    }
}
