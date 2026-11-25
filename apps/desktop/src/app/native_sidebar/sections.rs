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
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{ElementExt as _, h_flex};
use serde_json::json;

/*
CDXC:Sidebar 2026-11-21 DECISION:
User: the sidebar's gaps, buttons and text like another app's sidebar (spacing and sizes only).
There a heading ("Pinned") is a row of its own at the rows' text size, in sentence case and the
muted grey, 35pt above its first row's centre where rows are 31pt apart, and a group starts about
15pt below the last row of the one above. The section headings (Pinned, Sessions, Parked and the
others) and Main Hub's GBots heading take those measures; they were 20pt, 12pt capitals, 8pt apart.
*/
/*
CDXC:Sidebar 2026-11-21 DECISION:
User, on the spacing above: the gap above "Sessions" is too big, and the gaps around the section
headings (Pinned and its sessions, Sessions, Parked) are a bit too large overall; tighten them,
about halfway between the old and the new values, consistent with each other. Halfway: the heading
row 25pt (was 20, then 30), 2pt to its first row (0, then 4), and 6pt above a heading whether the
section above is open or folded (0 and 8, then 12 and 4). So a heading is 31pt from the folded one
above it (the rows' own pitch), about 30pt from its first row's centre, and 34.5pt from the last
row above it.
*/
/// A section heading's row, unscaled.
pub(super) const SECTION_HEADING_HEIGHT: f32 = 25.0;
/// Between an open section's heading and its first row.
pub(super) const SECTION_HEADING_GAP_BELOW: f32 = 2.0;
/// Above a heading under another section, open or folded.
pub(super) const SECTION_GAP_ABOVE: f32 = 6.0;

/// A section's heading text: its id in sentence case ("pinned" → "Pinned"), except Main Hub's
/// "sessions" section which reads "Chats" (2026-11-24 Main Hub Part 2 option A).
pub(super) fn section_heading_title(id: &str, is_home: bool) -> String {
    if is_home && id == "sessions" {
        return "Chats".to_string();
    }
    let mut chars = id.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

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
        // CDXC:Sidebar 2026-11-24 DECISION:
        // User: multiple status dots always overlap like a coordinator row's stack (`status::folded_status_indicator`), never side by side with a gap; on section headers they sit at the far right in place of the count, and hovering swaps them for the count.
        let has_status_dots = section.collapsed
            && (section.working_count > 0
                || section.attention_count > 0
                || section.background_work_count > 0
                || section.question_count > 0
                || section.approval_count > 0);
        let dot_ring = if drop_target {
            titlebar_background().blend(rgb(0x60a5fa).opacity(0.18).into())
        } else {
            titlebar_background()
        };
        let active_ring = (section.collapsed && section.contains_active_session).then(|| {
            div()
                .size(px(9.0 * scale))
                .rounded_full()
                .border(px(1.5 * scale))
                .border_color(chrome_color(0xa3a3a3, 0x858585))
                .into_any_element()
        });
        // CDXC:Sidebar 2026-09-18 DECISION:
        // User: a section heading is exactly as wide as a session card (the same 3px insets) with the card's 5px/6px side padding, and it has bottom padding too so the label sits centred in its rounded hover fill.
        // This supersedes the 2026-09-16 rule of letting the heading run through the sidebar's right edge for the native sidebar.
        let owner = super::menus::MenuOwner::default();
        let menu_owner = owner.clone();
        let stop_key = format!("section:{key}");
        let title = section_heading_title(&section.id, group.is_home);
        h_flex()
            .id(format!("native-sidebar-section-{key}"))
            .on_prepaint(super::menus::record_menu_owner(&owner))
            .role(gpui::Role::Button)
            .aria_label(format!("{title} section"))
            .aria_expanded(!section.collapsed)
            .mx(px(3.0 * scale))
            .h(px(SECTION_HEADING_HEIGHT * scale))
            .pl(px(5.0 * scale))
            .pr(px(6.0 * scale))
            .gap(px(5.0 * scale))
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
            .child(title)
            .children(if hovered {
                Some(
                    titlebar_svg_icon(
                        if section.collapsed {
                            COMMAND_ICON_CHEVRON_RIGHT
                        } else {
                            COMMAND_ICON_CHEVRON_DOWN
                        },
                        14.0 * scale,
                        appearance.muted,
                    )
                    .into_any_element(),
                )
            } else {
                active_ring
            })
            .child(div().flex_1())
            .when(section.collapsed, |row| {
                if has_status_dots && !hovered {
                    row.child(super::status::folded_status_indicator(
                        section.working_count > 0,
                        section.attention_count > 0,
                        section.background_work_count > 0,
                        section.question_count > 0,
                        section.approval_count > 0,
                        scale,
                        dot_ring,
                    ))
                } else {
                    row.child(super::sessions::section_count_label(section.count, scale))
                }
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
            // The Parked heading's Sleep All and Close All (gx-core `sidebar_menu/section.rs`).
            .when_some(section.menu.clone(), |row, menu| {
                row.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        Self::show_native_sidebar_context_menu(
                            &menu,
                            menu_owner.get(),
                            event.position,
                            scale,
                            window,
                            cx,
                        );
                    }),
                )
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
}

/// The accent a section heading (or a group header, `project_header.rs`) takes while a dragged
/// session would land in it.
pub(super) fn section_drop_highlight<E: Styled>(row: E, _scale: f32) -> E {
    row.bg(rgb(0x60a5fa).opacity(0.18))
        .text_color(chrome_color(0xd8d8d8, 0x292929).opacity(0.9))
}
