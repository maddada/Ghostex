//! The workspace switcher at the left end of the sidebar's bottom bar: a split button. The left part is
//! the window's workspace as a small dot in its colour and its name, and a click switches to the
//! other workspace (gx-core `workspace_switch_target`); the slim chevron on the right opens the
//! workspace menu.
//!
//! CDXC:Workspaces 2026-10-11 DECISION:
//! User: move the workspace picker out of the Spaces row to the bottom left of the app, in the sidebar's bottom bar. It shows whenever this computer's section is shown and the Workspaces feature is on, with or without Spaces. This supersedes the 2026-10-09 tile to the left of the Spaces row. The menu comes from gx-core (`sidebar_menu/workspace.rs`).
//!
//! CDXC:Workspaces 2026-10-10 DECISION:
//! User picked option A of the one-click switch mockup (the switch rule is gx-core `workspace_switch_target`) and asked to "make the look of the profile switcher match the size etc of the space buttons and don't add a | there between them". So the split button has the Space buttons' height, radius and hover, and the hairline that followed the tile is gone (this supersedes the 2026-10-09 hairline).
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User: "I figured out just clicking on the P or W button in the bottom left is enough, but I don't like the P or W icon in the bottom left", then picked "A. The name, quietly" (Q4254): the letter on a coloured square becomes an 8px dot in the workspace's colour and the workspace's name in the sidebar's own text, in the same place with the same one click; with a single workspace nothing is drawn (`gx_store_workspace_tile`). Supersedes the letter block of the 2026-10-10 decision.
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User: "make the bg behind this pill and its arrow on the right match the look of the accounts below it". The name and the arrow are one tile with exactly the account usage cards' surface: their resting, hover and pressed fills (`usage_card_fills`), their radius and their height (`SIDEBAR_USAGE_CARD_RADIUS`, `SIDEBAR_USAGE_METER_HEIGHT`, native_sidebar/usage.rs), and no border, as the cards have none. Each half lights alone on hover, so the switch and the menu stay two targets. Supersedes the separate quiet fill on the name and the unfilled arrow of the decision above.

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The chevron part's width.
const CHEVRON_PART_WIDTH: f32 = 18.0;
/// The name part's widest, so a long workspace name ends in an ellipsis instead of pushing the
/// line.
const NAME_PART_MAX_WIDTH: f32 = 180.0;

impl GhostexGpuiApp {
    /// Whether the sidebar's bottom has the workspace button now
    /// (`render_native_sidebar_footer_workspace_tile`).
    pub(super) fn native_sidebar_footer_workspace_tile_shown(&self) -> bool {
        self.native_sidebar
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            })
            && self.gx_store_workspace_tile().is_some()
    }

    /// The tile at the sidebar's bottom left, while this computer's section is shown and its
    /// daemon has workspaces (gxserver publishes none while the Workspaces feature is off).
    pub(super) fn render_native_sidebar_footer_workspace_tile(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        if snapshot.selected_machine_id != ghostex_gx_core::LOCAL_MACHINE_ID {
            return None;
        }
        self.render_native_sidebar_workspace_tile(appearance, cx)
    }

    fn render_native_sidebar_workspace_tile(
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
        // The account usage cards' own surface. Each half carries the fills, so a hovered half is
        // exactly a hovered card rather than a hover wash stacked on the resting fill.
        let fills = super::usage::usage_card_fills(appearance.light);
        let radius = px(super::usage::SIDEBAR_USAGE_CARD_RADIUS * scale);
        let tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        let tooltip_delay = appearance.tooltip_delay;
        let (switch_to, letter_tooltip) = match tile.switch_to {
            Some((workspace_id, name)) => {
                let tooltip = match tile.switch_hotkey.as_deref() {
                    Some(hotkey) => format!("Switch to {name} {hotkey}"),
                    None => format!("Switch to {name}"),
                };
                (Some(workspace_id), tooltip)
            }
            None => (None, tile.name.clone()),
        };
        let menu_tooltip = tile.name.clone();
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let open_menu = move |app: &mut Self,
                              event: &gpui::ClickEvent,
                              window: &mut gpui::Window,
                              cx: &mut gpui::Context<Self>| {
            let Some(menu) = app.gx_store_workspace_menu(cx) else {
                return;
            };
            // Hung from the whole split button (above it in the bottom bar), and either part
            // toggles it.
            Self::show_native_sidebar_menu_hung(
                &menu,
                menu_owner.get(),
                None,
                Some((
                    "workspace-tile".to_owned(),
                    super::menus::press_position(event),
                )),
                event.position(),
                scale,
                window,
                cx,
            );
        };
        let open_letter_menu = open_menu.clone();
        let letter = div()
            .id("native-sidebar-workspace-switch")
            .role(gpui::Role::Button)
            .aria_label(letter_tooltip.clone())
            .max_w(px(NAME_PART_MAX_WIDTH * scale))
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(7.0 * scale))
            .pl(px(8.0 * scale))
            .pr(px(6.0 * scale))
            .rounded_l(radius)
            .bg(fills.rest)
            .cursor_pointer()
            .hover(|part| part.bg(fills.hover))
            .active(|part| part.bg(fills.open))
            .child(
                div()
                    .size(px(8.0 * scale))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(color),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0 * scale))
                    .text_color(appearance.foreground)
                    .child(tile.name.clone()),
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
                        None => open_letter_menu(app, event, window, cx),
                    }
                }),
            );
        // The tile sits in the bottom bar, below the list.
        let letter = self.sidebar_focus_stop(
            letter,
            "workspace-tile:switch",
            super::focus_order::SidebarStopPlace::Fixed,
            appearance,
            cx,
        );
        let chevron = div()
            .id("native-sidebar-workspace-menu")
            .role(gpui::Role::Button)
            .aria_label(format!("Workspace {} menu", tile.name))
            .w(px(CHEVRON_PART_WIDTH * scale))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded_r(radius)
            .bg(fills.rest)
            .cursor_pointer()
            .hover(|part| part.bg(fills.hover))
            .active(|part| part.bg(fills.open))
            .child(titlebar_svg_icon(
                "titlebar/chevron-down.svg",
                10.0 * scale,
                appearance.chrome_icon,
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
        let chevron = self.sidebar_focus_stop(
            chevron,
            "workspace-tile:menu",
            super::focus_order::SidebarStopPlace::Fixed,
            appearance,
            cx,
        );
        Some(
            div()
                .id("native-sidebar-workspace-tile")
                .on_prepaint(super::menus::record_menu_owner(&owner))
                .flex()
                .min_w_0()
                .h(px(super::usage::SIDEBAR_USAGE_METER_HEIGHT * scale))
                .rounded(radius)
                .child(letter)
                .child(chevron)
                .into_any_element(),
        )
    }
}
