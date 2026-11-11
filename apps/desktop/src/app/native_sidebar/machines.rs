//! The computers in the sidebar's top row: one icon each, before the Spaces, or one button that
//! lists them when the row is short of room.
//!
//! CDXC:RemoteMachines 2026-11-11 DECISION:
//! User: "I told u to make the machines show as icons next to the spaces but i still dont see that??". The computers and the Spaces share one row: the computers first, as icon tiles without words (this computer's monitor, a remote computer's Windows, Apple or Ubuntu mark where the app knows its system, else the cloud), then a hairline, then the Spaces sharing the rest. The picked computer is filled like a picked Space; each tile's tooltip names it and its state ("Windows · connected"), its right-click menu is the one the tab had, and a remote computer's status dots sit small on its tile's top-right corner. With only this computer there are no computer tiles. When the row cannot hold every computer and every Space at their narrowest, the computers fold into one button showing the picked computer, which lists them all (each row keeps its menu); after that the Spaces fold into their More tile as before. This supersedes the joined Local | Windows control of the same day and the machine tabs before it.
//! SEE-ALSO: native_sidebar/selectors.rs (the row and its room), remote_conn/app_update.rs (`remote_machine_systems`), unified/header.rs (`unified_filter_row_menu`, which opens a listed computer's menu).

use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarMachine, NativeSidebarSnapshot},
};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;
use std::collections::HashMap;

/// A computer tile's face and the gap after it, before the zoom: a Space tile's.
pub(crate) const MACHINE_ICON_SIZE: f32 = 28.0;
pub(crate) const MACHINE_ICON_GAP: f32 = 4.0;
/// The hairline between the computers and the Spaces and the gap after it (the gap before it is
/// the last tile's).
pub(crate) const MACHINE_DIVIDER_ROOM: f32 = 5.0;

/// How much of the row `shown` computer tiles and their hairline take, before the zoom.
pub(crate) fn machine_icons_room(shown: usize) -> f32 {
    shown as f32 * (MACHINE_ICON_SIZE + MACHINE_ICON_GAP) + MACHINE_DIVIDER_ROOM
}

/// A computer's state, in the words its tooltip uses.
fn machine_state_words(machine: &NativeSidebarMachine) -> &'static str {
    if machine.id == ghostex_gx_core::LOCAL_MACHINE_ID {
        return "this computer";
    }
    match machine.state.as_str() {
        ghostex_gx_core::MACHINE_STATE_CONNECTED => "connected",
        "connecting" => "connecting",
        "disconnected" => "not connected",
        state => ghostex_gx_core::machine_state_copy(state).0,
    }
}

/// The icon a remote computer's system gets: the brand marks Settings > Remote shows.
fn system_icon(system: Option<&&'static str>) -> &'static str {
    match system.copied() {
        Some("windows") => "modals/settings/brand-windows.svg",
        Some("macos") => "modals/settings/brand-apple.svg",
        Some("linux") => "modals/settings/brand-ubuntu.svg",
        _ => "titlebar/cloud.svg",
    }
}

impl GhostexGpuiApp {
    /// The computer tiles (or, `folded`, the one button that lists them) for the top row; empty
    /// with only this computer. The Unified sidebar draws none: its filter lists the computers.
    pub(crate) fn render_native_machine_icons(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        folded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        if snapshot.machines.len() <= 1 {
            return Vec::new();
        }
        let systems = self.remote_machine_systems();
        if folded {
            return vec![self.render_native_machines_button(snapshot, &systems, appearance, cx)];
        }
        snapshot
            .machines
            .iter()
            .map(|machine| {
                self.render_native_machine_icon(snapshot, machine, &systems, appearance, cx)
            })
            .collect()
    }

    /// The icon a computer shows: this computer's monitor, a spinner while a remote one connects,
    /// else its system's mark.
    fn native_machine_icon(
        machine: &NativeSidebarMachine,
        systems: &HashMap<String, &'static str>,
        appearance: &SidebarAppearance,
        size: f32,
    ) -> AnyElement {
        let busy = ghostex_gx_core::machine_state_is_busy(&machine.state);
        let local = machine.id == ghostex_gx_core::LOCAL_MACHINE_ID;
        let failed = !local && ghostex_gx_core::machine_state_is_failure(&machine.state);
        let icon = gpui::svg()
            .path(if local {
                "titlebar/device-desktop.svg"
            } else if busy {
                "titlebar/loader2.svg"
            } else {
                system_icon(systems.get(&machine.id))
            })
            .size(px(size))
            .flex_shrink_0()
            .text_color(if failed {
                rgb(0xff9494).into()
            } else {
                appearance.chrome_icon
            });
        if busy && !local {
            icon.with_throttled_animation(
                format!("native-machine-busy-{}", machine.id),
                std::time::Duration::from_millis(900),
                |icon, progress| {
                    icon.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                        progress,
                    )))
                },
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        }
    }

    /// What a computer's tooltip says: its name and state, its error, a waiting Ghostex update.
    fn native_machine_tooltip(&self, machine: &NativeSidebarMachine) -> String {
        let mut text = format!("{} · {}", machine.label, machine_state_words(machine));
        if let Some(message) = &machine.message {
            text.push_str(&format!(": {message}"));
        }
        if machine.id != ghostex_gx_core::LOCAL_MACHINE_ID
            && let Some(update) = self.remote_app_update_tab_badge(&machine.id)
        {
            text.push_str(&format!(" · {}", update.tooltip));
        }
        text
    }

    /// One computer's tile.
    fn render_native_machine_icon(
        &self,
        snapshot: &NativeSidebarSnapshot,
        machine: &NativeSidebarMachine,
        systems: &HashMap<String, &'static str>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let id = machine.id.clone();
        let reconnect = id.clone();
        let local = id == ghostex_gx_core::LOCAL_MACHINE_ID;
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let tab_menu = (!local).then(|| self.native_machine_tab_menu(&id));
        let selected = snapshot.selected_machine_id == id;
        let reconnects = !local
            && machine.state != ghostex_gx_core::MACHINE_STATE_CONNECTED
            && !ghostex_gx_core::machine_state_is_busy(&machine.state);
        let dot_group = format!("native-machine-dots-{id}");
        let (selected_background, selected_outline) =
            super::selectors::space_tile_selected_colors(appearance);
        let tooltip = self.native_machine_tooltip(machine);
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        // An update waiting there: the tab's arrow, small on the tile's lower corner.
        let update = (!local)
            .then(|| self.remote_app_update_tab_badge(&id))
            .flatten()
            .map(|update| {
                gpui::svg()
                    .path(if update.busy {
                        "titlebar/loader2.svg"
                    } else {
                        "titlebar/download.svg"
                    })
                    .size(px(9.0 * scale))
                    .text_color(appearance.foreground)
            });
        let has_status = !selected
            && (machine.working_count > 0
                || machine.attention_count > 0
                || machine.background_work_count > 0
                || machine.question_count > 0
                || machine.approval_count > 0);
        div()
            .id(format!("native-sidebar-machine-{id}"))
            .group(dot_group.clone())
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Tab)
            .aria_label(tooltip.clone())
            .aria_selected(selected)
            .relative()
            .size(px(MACHINE_ICON_SIZE * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .border_1()
            .border_color(appearance.foreground.opacity(0.08))
            .cursor_pointer()
            .when(selected, |tile| {
                tile.bg(selected_background)
                    .border_color(selected_outline)
                    .when(appearance.light && !appearance.glass, |tile| {
                        tile.shadow_sm()
                    })
            })
            .hover(|tile| tile.bg(appearance.hover))
            .child(Self::native_machine_icon(
                machine,
                systems,
                appearance,
                15.0 * scale,
            ))
            /*
            CDXC:Sidebar 2026-09-25 DECISION:
            User: stop showing the status on the machine we're on and show it on the other machines as dots, not numbers, exactly like the Spaces. The picked computer's sessions are already listed below.
            Since 2026-11-11 the dots sit small on the tile's top-right corner (CDXC:RemoteMachines 2026-11-11 DECISION at the top of this file), so the icon stays centred.
            */
            .when(has_status, |tile| {
                tile.child(
                    div()
                        .absolute()
                        .top(px(-4.0 * scale))
                        .right(px(-4.0 * scale))
                        .child(super::status::status_dot_stack(
                            machine.working_count,
                            machine.attention_count,
                            machine.background_work_count,
                            machine.question_count,
                            machine.approval_count,
                            scale,
                            super::status::DotBackdrop::new(appearance, None, dot_group.clone()),
                        )),
                )
            })
            .children(update.map(|glyph| {
                div()
                    .absolute()
                    .right(px(2.0 * scale))
                    .bottom(px(2.0 * scale))
                    .child(glyph)
            }))
            .when(
                self.native_sidebar.pointer_inside
                    && self.native_sidebar.menu.is_none()
                    && !cx.has_active_drag(),
                |tile| {
                    tile.managed_discrete_tooltip_with_placement(
                        tooltip_span.placement(),
                        appearance.tooltip_delay,
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
                },
            )
            // A click picks the computer; one that is not connected (and not already trying) is
            // asked to connect too, as a click on the old tab's icon did.
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "selectMachine", "machineId": id}),
                    cx,
                );
                if reconnects {
                    app.remote_reconnect_from_sidebar(&reconnect, cx);
                }
            }))
            .when_some(tab_menu, |tile, items| {
                tile.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    Self::show_native_sidebar_context_menu(
                        &items,
                        menu_owner.get(),
                        event.position,
                        scale,
                        window,
                        cx,
                    );
                })
            })
            .into_any_element()
    }

    /// The computers folded into one button: the picked computer's icon, the other computers'
    /// status dots on its corner, and a menu that lists every computer (a click picks it, and a
    /// remote one's row opens the menu its tile has).
    fn render_native_machines_button(
        &self,
        snapshot: &NativeSidebarSnapshot,
        systems: &HashMap<String, &'static str>,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let picked = snapshot
            .machines
            .iter()
            .find(|machine| machine.id == snapshot.selected_machine_id)
            .unwrap_or(&snapshot.machines[0]);
        let others = || {
            snapshot
                .machines
                .iter()
                .filter(|machine| machine.id != snapshot.selected_machine_id)
        };
        let sum =
            |count: fn(&NativeSidebarMachine) -> usize| -> usize { others().map(count).sum() };
        let (working, attention, background, question, approval) = (
            sum(|machine| machine.working_count),
            sum(|machine| machine.attention_count),
            sum(|machine| machine.background_work_count),
            sum(|machine| machine.question_count),
            sum(|machine| machine.approval_count),
        );
        let has_status = working + attention + background + question + approval > 0;
        let items: Vec<_> = snapshot
            .machines
            .iter()
            .map(|machine| {
                let local = machine.id == ghostex_gx_core::LOCAL_MACHINE_ID;
                let mut item = json!({
                    "label": machine.label,
                    "detail": machine_state_words(machine),
                    "icon": if local { "deviceDesktop" } else { "cloud" },
                    "checked": machine.id == snapshot.selected_machine_id,
                    "command": {"type": "selectMachine", "machineId": machine.id},
                });
                if !local {
                    item["rowMenu"] =
                        json!({"kind": "machine", "key": machine.id, "label": machine.label});
                }
                item
            })
            .collect();
        let items = serde_json::Value::Array(items);
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let tooltip = format!("Computers: {}", self.native_machine_tooltip(picked));
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        let dot_group = "native-machines-button-dots";
        div()
            .id("native-sidebar-machines")
            .group(dot_group)
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(tooltip.clone())
            .relative()
            .size(px(MACHINE_ICON_SIZE * scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .border_1()
            .border_color(appearance.foreground.opacity(0.08))
            .cursor_pointer()
            .hover(|tile| tile.bg(appearance.hover))
            .child(Self::native_machine_icon(
                picked,
                systems,
                appearance,
                15.0 * scale,
            ))
            .when(has_status, |tile| {
                tile.child(
                    div()
                        .absolute()
                        .top(px(-4.0 * scale))
                        .right(px(-4.0 * scale))
                        .child(super::status::status_dot_stack(
                            working,
                            attention,
                            background,
                            question,
                            approval,
                            scale,
                            super::status::DotBackdrop::new(
                                appearance,
                                None,
                                dot_group.to_string(),
                            ),
                        )),
                )
            })
            .when(
                self.native_sidebar.pointer_inside
                    && self.native_sidebar.menu.is_none()
                    && !cx.has_active_drag(),
                |tile| {
                    tile.managed_discrete_tooltip_with_placement(
                        tooltip_span.placement(),
                        appearance.tooltip_delay,
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
                },
            )
            .on_click(cx.listener(move |_, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_menu_hung(
                    &items,
                    menu_owner.get(),
                    None,
                    Some(("computers".to_owned(), super::menus::press_position(event))),
                    event.position(),
                    scale,
                    window,
                    cx,
                );
            }))
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// A remote machine tab's right-click menu, which the Unified filter dropdown's machine rows open
    /// too. This computer's tab has none.
    pub(crate) fn native_machine_tab_menu(&self, machine_id: &str) -> serde_json::Value {
        let mut items = Vec::new();
        if let Some(label) = self
            .remote_app_update_tab_badge(machine_id)
            .and_then(|update| update.menu_label)
        {
            items.push(json!({"label": label, "icon": "download", "command": {"type": "remoteAppUpdate", "op": "install", "remoteMachineId": machine_id}}));
            items.push(json!({"separator": true}));
        }
        items.push(json!({"label": "Hide Machine", "icon": "eye-off", "command": {"type": "machineAction", "action": "hide", "machineId": machine_id}}));
        // The tab's workspace on this computer (gx-core `machine_workspace_menu`).
        if let Some(workspace_menu) = self.gx_store_machine_workspace_menu(machine_id) {
            items.push(workspace_menu);
        }
        items.push(json!({"separator": true}));
        items.push(json!({"label": "Configure Machines", "icon": "settings", "command": {"type": "machineAction", "action": "configure", "machineId": machine_id}}));
        serde_json::Value::Array(items)
    }
}
