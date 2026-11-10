//! A department's sub-heading under Main Hub's GBots heading, and its bots under it: the
//! department's name in the heading's quiet look, a click folds its bots, folded it shows their
//! status and how many there are, and its right-click menu has Department Settings… (the lead's
//! GBot Settings on its Department tab) and Remove Department….
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 6, mockup Q2101 A): GBots in a department sit under the department's sub-heading in Main Hub, under the GBots heading; its lead is marked with a small crown (gbots_rows.rs). Removing a department keeps its GBots, so its confirmation says so.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/gbots.rs (`GbotsDepartmentView`, the order), apps/mobile/app/src/components/sessions/GbotsRows.tsx (the phone's same sub-headings).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use gpui_component::{ElementExt as _, h_flex, v_flex};
use serde_json::{Value, json};

use super::appearance::SidebarAppearance;
use super::gbots_rows::{folded_summary, gbots_command};
use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::app::{consts::*, helpers::*};

impl GhostexGpuiApp {
    /// One department: its sub-heading and, while it is open or folding, its bots.
    pub(super) fn render_native_gbots_department(
        &self,
        department: &ghostex_gx_core::GbotsDepartmentView,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = format!("group:{}", department.storage_id);
        let bots = self
            .native_sidebar
            .disclosures
            .present(&key, department.collapsed)
            .then(|| {
                let rows = v_flex()
                    .w_full()
                    .children(self.render_native_gbot_blocks(
                        &department.group_ids,
                        snapshot,
                        appearance,
                        cx,
                    ))
                    .into_any_element();
                self.render_native_disclosure(key, rows, cx)
            });
        v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                self.render_native_gbots_department_heading(department, snapshot, appearance, cx),
            )
            .children(bots)
            .into_any_element()
    }

    fn render_native_gbots_department_heading(
        &self,
        department: &ghostex_gx_core::GbotsDepartmentView,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let storage_id = department.storage_id.clone();
        let hover_key = format!("gbots-department-{storage_id}");
        let hovered = self.native_sidebar.hovered_section.as_ref() == Some(&hover_key);
        let collapsed = department.collapsed;
        let folded = folded_summary(snapshot, &department.group_ids);
        let menu = department_menu(department, snapshot);
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let fold_id = storage_id.clone();
        let stop_key = format!("gbots-department:{storage_id}");
        h_flex()
            .id(format!("native-sidebar-gbots-department-{storage_id}"))
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(department.name.clone())
            .aria_expanded(!collapsed)
            .mx(px(3.0 * scale))
            .mt(px(2.0 * scale))
            .h(px(20.0 * scale))
            .pl(px(9.0 * scale))
            .pr(px(4.0 * scale))
            .gap(px(5.0 * scale))
            .text_size(px(11.5 * scale))
            .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.42))
            .rounded(px(5.0 * scale))
            .hover(|row| {
                row.bg(chrome_ink().opacity(0.06))
                    .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.62))
            })
            .when_some(department.icon.clone(), |row, icon| {
                row.child(
                    svg()
                        .flex_shrink_0()
                        .size(px(11.0 * scale))
                        .path(format!("titlebar/{icon}.svg"))
                        .text_color(appearance.muted),
                )
            })
            .child(div().min_w_0().truncate().child(department.name.clone()))
            .when(hovered, |row| {
                row.child(titlebar_svg_icon(
                    if collapsed {
                        COMMAND_ICON_CHEVRON_RIGHT
                    } else {
                        COMMAND_ICON_CHEVRON_DOWN
                    },
                    11.0 * scale,
                    appearance.muted,
                ))
            })
            .when(collapsed && !hovered && folded.any(), |row| {
                row.child(super::status::folded_status_indicator(
                    folded.working,
                    folded.attention,
                    folded.background_work,
                    folded.question,
                    folded.approval,
                    scale,
                    titlebar_background(),
                ))
            })
            .child(div().flex_1())
            .when(collapsed && !hovered, |row| {
                row.child(folded.bots.to_string())
            })
            .on_hover(cx.listener(move |app, hovered, _, cx| {
                if *hovered {
                    app.native_sidebar.hovered_section = Some(hover_key.clone());
                } else if app.native_sidebar.hovered_section.as_ref() == Some(&hover_key) {
                    app.native_sidebar.hovered_section = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "toggleGroup", "groupId": fold_id}),
                    cx,
                );
            }))
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                Self::show_native_sidebar_context_menu(
                    &menu,
                    menu_owner.get(),
                    event.position,
                    scale,
                    window,
                    cx,
                );
            })
            .map(|row| {
                self.sidebar_focus_stop(
                    row,
                    stop_key,
                    super::focus_order::SidebarStopPlace::List,
                    appearance,
                    cx,
                )
            })
            .into_any_element()
    }

    /// Remove Department…: asks, then removes it on `machine`; its GBots stay, in no department.
    pub(super) fn remove_native_sidebar_gbots_department(
        &mut self,
        machine: String,
        department_id: String,
        name: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Remove the department {name}?"),
            Some("Its GBots stay, in no department, with their chats and memory."),
            &["Cancel", "Remove Department"],
            cx,
        );
        cx.spawn(async move |app, cx| {
            if answer.await == Ok(1) {
                let _ = app.update(cx, |app, cx| {
                    app.call_native_sidebar_gbots(
                        &machine,
                        json!({ "op": "departmentRemove", "department": department_id }),
                        "Department not removed",
                        |_, _, _| {},
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}

/// A department sub-heading's menu: its settings open in its lead's (or first GBot's) GBot
/// Settings, on the Department tab.
fn department_menu(
    department: &ghostex_gx_core::GbotsDepartmentView,
    snapshot: &NativeSidebarSnapshot,
) -> Value {
    let first = department.group_ids.first().and_then(|group_id| {
        snapshot
            .groups
            .iter()
            .find(|group| &group.group_id == group_id)
    });
    let machine_id = first
        .map(|group| group.machine_id())
        .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string());
    let mut items = Vec::new();
    if let Some(look) = first.and_then(|group| group.gbot()) {
        items.push(json!({
            "label": "Department Settings…",
            "icon": "users",
            "command": gbots_command("department", json!({ "botId": look.bot_id, "name": look.name, "machineId": machine_id })),
        }));
        items.push(json!({ "separator": true }));
    }
    items.push(json!({
        "label": "Remove Department…",
        "icon": "trash",
        "command": gbots_command("removeDepartment", json!({
            "departmentId": department.department_id,
            "name": department.name,
            "machineId": machine_id,
        })),
    }));
    Value::Array(items)
}
