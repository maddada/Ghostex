use super::drag::SidebarDropTarget;
use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarGroup, NativeSidebarSection},
};
use crate::{
    GhostexGpuiApp,
    app::{consts::*, helpers::*},
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use serde_json::json;

impl GhostexGpuiApp {
    pub(crate) fn render_native_section_header(
        &self,
        group: &NativeSidebarGroup,
        section: &NativeSidebarSection,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let storage_id = group.storage_id.clone();
        let section_id = section.id.clone();
        let key = format!("{}-{}", group.group_id, section.id);
        let hovered = self.native_sidebar.hovered_section.as_ref() == Some(&key);
        let drop_target = self.native_sidebar_section_drop_target(&group.group_id, &section.id);
        // CDXC:Sidebar 2026-09-19 DECISION:
        // User: the working, attention and question dots next to a section title show only while that section is collapsed, not always; an expanded section already shows the state on its rows.
        let summarize = section.collapsed;
        let indicator = h_flex()
            .gap(px(4.0 * scale))
            .when(summarize && section.working_count > 0, |row| {
                row.child(
                    div()
                        .size(px(8.0 * scale))
                        .rounded_full()
                        .bg(rgb(super::status::WORKING_COLOR)),
                )
            })
            .when(summarize && section.attention_count > 0, |row| {
                row.child(div().size(px(8.0 * scale)).rounded_full().bg(rgb(0x95d7f6)))
            })
            .when(
                summarize && section.working_count == 0 && section.background_work_count > 0,
                |row| {
                    row.child(
                        div()
                            .size(px(8.0 * scale))
                            .rounded_full()
                            .bg(super::status::background_work_color()),
                    )
                },
            )
            .when(summarize && section.question_count > 0, |row| {
                row.child(div().size(px(8.0 * scale)).rounded_full().bg(rgb(0xf472b6)))
            })
            .when(
                section.collapsed && section.contains_active_session,
                |row| {
                    row.child(
                        div()
                            .size(px(9.0 * scale))
                            .rounded_full()
                            .border(px(1.5 * scale))
                            .border_color(chrome_color(0xa3a3a3, 0x858585)),
                    )
                },
            );
        // CDXC:Sidebar 2026-09-18 DECISION:
        // User: a section heading is exactly as wide as a session card (the same 3px insets) with the card's 5px/6px side padding, and it has bottom padding too so the label sits centred in its rounded hover fill.
        // This supersedes the 2026-09-16 rule of letting the heading run through the sidebar's right edge for the native sidebar.
        h_flex()
            .id(format!("native-sidebar-section-{key}"))
            .role(gpui::Role::Button)
            .aria_label(format!("{} section", section.id))
            .aria_expanded(!section.collapsed)
            .mx(px(3.0 * scale))
            .h(px(20.0 * scale))
            .py(px(3.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .gap(px(5.0 * scale))
            .text_size(px(12.0 * scale))
            .font_weight(FontWeight::LIGHT)
            .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.34))
            // CDXC:Sidebar 2026-09-18 DECISION: User: the section heading's hover fill has rounded corners, like the session rows; this supersedes the 2026-09-16 square-corner decision for the native sidebar.
            .rounded(px(5.0 * scale))
            .hover(|row| {
                row.bg(chrome_ink().opacity(0.06))
                    .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.58))
            })
            .when(drop_target, |row| section_drop_highlight(row, scale))
            .sidebar_drop_target(
                "section",
                section.id.clone(),
                Some(group.group_id.clone()),
                cx,
            )
            .child(section.id.to_uppercase())
            .child(if hovered {
                titlebar_svg_icon(
                    if section.collapsed {
                        COMMAND_ICON_CHEVRON_RIGHT
                    } else {
                        COMMAND_ICON_CHEVRON_DOWN
                    },
                    12.0 * scale,
                    appearance.muted,
                )
                .into_any_element()
            } else {
                indicator.into_any_element()
            })
            .child(div().flex_1())
            .when(section.collapsed, |row| {
                row.child(section.count.to_string())
            })
            .on_hover(cx.listener(move |app, hovered, _, cx| {
                if *hovered {
                    app.native_sidebar.hovered_section = Some(key.clone());
                } else if app.native_sidebar.hovered_section.as_ref() == Some(&key) {
                    app.native_sidebar.hovered_section = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.dispatch_native_sidebar_ui(
                    json!({"type": "toggleSection", "groupId": storage_id, "section": section_id}),
                    cx,
                );
            }))
            .into_any_element()
    }

    /// The heading of a section this group does not draw yet, shown only while a session drag
    /// can land in it (section_move.rs).
    pub(super) fn render_native_section_drop_placeholder(
        &self,
        group: &NativeSidebarGroup,
        section_id: &'static str,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let drop_target = self.native_sidebar_section_drop_target(&group.group_id, section_id);
        h_flex()
            .id(format!(
                "native-sidebar-section-drop-{}-{section_id}",
                group.group_id
            ))
            .mx(px(3.0 * scale))
            .h(px(20.0 * scale))
            .py(px(3.0 * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .text_size(px(12.0 * scale))
            .font_weight(FontWeight::LIGHT)
            .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.34))
            .rounded(px(5.0 * scale))
            .border_1()
            .border_dashed()
            .border_color(chrome_ink().opacity(0.12))
            .when(drop_target, |row| section_drop_highlight(row, scale))
            .sidebar_drop_target(
                "section",
                section_id.to_string(),
                Some(group.group_id.clone()),
                cx,
            )
            .child(section_id.to_uppercase())
            .into_any_element()
    }
}

/// The accent a section heading takes while a dragged session would land in that section.
fn section_drop_highlight<E: Styled>(row: E, _scale: f32) -> E {
    row.bg(rgb(0x60a5fa).opacity(0.18))
        .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.9))
}
