use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSession},
};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::{
    ElementExt as _, h_flex,
    tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement},
};
use serde_json::{Value, json};

/// The buttons a hovered row draws, in order; empty when the user turned every hover button off, which is also when hovering must leave the row as it is (`ghostex_gx_core::hover_slots`).
/// A session row's hover button and its icon, before the zoom. The Unified filter dropdown's X
/// takes the same sizes.
pub(super) const SESSION_HOVER_BUTTON: f32 = 20.0;
pub(super) const SESSION_HOVER_ICON: f32 = 14.0;

pub(crate) fn session_hover_items(session: &NativeSidebarSession, expanded: bool) -> Vec<Value> {
    let chevron = session.details.get("hoverChevron").and_then(Value::as_bool) == Some(true);
    let list = |key: &str| {
        session
            .details
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let (before, after) = (list("hoverBefore"), list("hoverAfter"));
    ghostex_gx_core::hover_slots(before.len(), chevron, after.len(), expanded)
        .into_iter()
        .map(|slot| match slot {
            ghostex_gx_core::HoverSlot::Before(index) => before[index].clone(),
            ghostex_gx_core::HoverSlot::After(index) => after[index].clone(),
            ghostex_gx_core::HoverSlot::Chevron => json!({ "label": if expanded { "Collapse actions" } else { "Expand actions" }, "icon": if expanded { "chevron-right" } else { "chevron-left" }, "expand": true }),
        })
        .collect()
}

impl GhostexGpuiApp {
    pub(crate) fn render_native_session_hover_actions(
        &self,
        group: &NativeSidebarGroup,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.render_native_session_hover_strip(
            session,
            group.hover_actions_expanded,
            json!({"type": "toggleHoverActions", "groupId": group.storage_id}),
            appearance,
            cx,
        )
    }

    /// The buttons themselves, with what the chevron sends: a Project sidebar row's chevron opens
    /// its project's strip, a Unified card's opens every card's.
    pub(crate) fn render_native_session_hover_strip(
        &self,
        session: &NativeSidebarSession,
        expanded: bool,
        expand_command: Value,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = session.session_id.clone();
        let scale = appearance.scale;
        let actions = session_hover_items(session, expanded);
        h_flex()
            .id(format!("native-sidebar-hover-actions-{id}"))
            .flex_shrink_0()
            .h_full()
            .gap(px(2.0 * scale))
            // CDXC:Sidebar 2026-09-21 DECISION: User: "make pointer over an inner button of the session also remove the tooltip". The title's tooltip was anchored to the title's width before these buttons appeared and took part of it, so the pointer reaching them does not read as leaving the title; close it here.
            .on_hover(|hovered, window, cx| {
                if *hovered {
                    gpui_component::Root::hide_tooltip(window, cx);
                }
            })
            .children(actions.into_iter().enumerate().map(|(index, item)| {
                let label = item["label"].as_str().unwrap_or("").to_owned();
                let command = item.get("command").cloned();
                let menu = item.get("children").cloned();
                let expand = item["expand"].as_bool() == Some(true);
                let expand_command = expand_command.clone();
                let session_id = id.clone();
                let trigger_label = label.clone();
                let button_owner = super::menus::MenuOwner::default();
                div()
                    .on_prepaint(super::menus::record_menu_owner(&button_owner))
                    .id(format!("native-sidebar-hover-{id}-{index}"))
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .size(px(SESSION_HOVER_BUTTON * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * scale))
                    .cursor_pointer()
                    // On light-mode glass the buttons are white washes like the selected row.
                    .when(appearance.light, |item| {
                        item.bg(if appearance.glass {
                            gpui::Hsla::from(gpui::rgb(0xffffff)).opacity(0.55)
                        } else {
                            gpui::Hsla::from(gpui::rgb(0xf4f4f5))
                        })
                    })
                    .hover(|item| {
                        item.bg(if appearance.light && appearance.glass {
                            gpui::Hsla::from(gpui::rgb(0xffffff)).opacity(0.85)
                        } else if appearance.light {
                            gpui::Hsla::from(gpui::rgb(0xe4e4e7))
                        } else {
                            appearance.hover
                        })
                    })
                    .child(
                        gpui::svg()
                            .path(gpui_sidebar_command_icon_asset_path(item["icon"].as_str()))
                            .size(px(SESSION_HOVER_ICON * scale))
                            .text_color(appearance.muted),
                    )
                    .when(
                        self.native_sidebar.pointer_inside
                            && self.native_sidebar.menu.is_none()
                            && !cx.has_active_drag(),
                        |row| {
                            // CDXC:Tooltips 2026-09-15 DECISION: User: the small buttons inside a session card must not show their tooltip instantly; they wait as long as the card's own title tooltip does.
                            row.managed_discrete_tooltip_with_placement(
                                ManagedTooltipPlacement::BelowLeft,
                                appearance.tooltip_delay,
                                move |window, cx| {
                                    super::tooltips::sidebar_style_tooltip(
                                        label.to_string(),
                                        window,
                                        cx,
                                    )
                                },
                            )
                        },
                    )
                    .on_click(
                        cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            if expand {
                                if !app.gx_store_run_unified_sidebar_command(&expand_command, cx) {
                                    app.dispatch_native_sidebar_ui(expand_command.clone(), cx);
                                }
                            } else if let Some(menu) = &menu {
                                // The session card's menus hang below the card, lined up with this button (menus.rs).
                                Self::show_native_sidebar_menu_hung(
                                    menu,
                                    app.native_sidebar
                                        .session_card_bounds
                                        .get(&session_id)
                                        .copied(),
                                    button_owner.get(),
                                    Some((
                                        format!("session:{session_id}:{trigger_label}"),
                                        super::menus::press_position(event),
                                    )),
                                    event.position(),
                                    scale,
                                    window,
                                    cx,
                                );
                            } else if let Some(command) = &command {
                                app.dispatch_native_sidebar_ui(command.clone(), cx);
                            }
                        }),
                    )
            }))
            .into_any_element()
    }
}
