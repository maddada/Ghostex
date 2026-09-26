use super::drag::SidebarDrag;
use super::drag::SidebarDropTarget;
use super::drag_source::SidebarDragSource;
use ghostex_gx_core::SpaceSleepScope;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::{h_flex, v_flex};
use serde_json::json;

use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarSnapshot, NativeSidebarSpace},
};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

impl GhostexGpuiApp {
    pub(crate) fn render_native_sidebar_selectors(
        &self,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        // Space buttons have a fixed 28px face and 4px gap in both renderers.
        let capacity = (((self.sidebar_width / scale - 13.0) + 4.0) / 32.0)
            .floor()
            .max(2.0) as usize;
        let user_spaces: Vec<_> = snapshot
            .spaces
            .iter()
            .filter(|space| space.id != "other")
            .collect();
        let overflow = user_spaces.len() + 1 > capacity;
        let mut visible: Vec<_> = user_spaces
            .iter()
            .copied()
            .take(if overflow {
                capacity - 2
            } else {
                user_spaces.len()
            })
            .collect();
        if let Some(selected) = user_spaces.iter().find(|space| space.selected) {
            if !visible.iter().any(|space| space.id == selected.id) && overflow {
                visible.pop();
                visible.push(selected);
            }
        }
        let visible_ids: Vec<String> = visible.iter().map(|space| space.id.clone()).collect();
        let light = appearance.light;
        let hidden: Vec<_> = user_spaces
            .iter()
            .filter(|space| !visible.iter().any(|item| item.id == space.id))
            .map(|space| (*space).clone())
            .collect();
        v_flex().w_full().flex_shrink_0()
            .children(self.render_native_machine_tabs(snapshot, appearance, cx))
            .child(div().h(px(13.0 * scale)).flex_shrink_0())
            .when(snapshot.spaces_enabled, |column| column.child(h_flex().id("native-sidebar-space-track").sidebar_drop_target("space-row", String::new(), None, cx).w_full().h(px(40.0 * scale)).pb(px(12.0 * scale)).px(px(6.5 * scale)).justify_center().gap(px(4.0 * scale))
                .when(user_spaces.is_empty(), |row| row.child(h_flex().id("native-sidebar-create-space").role(gpui::Role::Button).aria_label("Create space").flex_1().h(px(28.0 * scale)).px(px(10.0 * scale)).gap(px(6.0 * scale)).rounded(px(6.0 * scale)).border_1().border_color(appearance.muted.opacity(0.25)).text_size(px(12.0 * scale))
                    .child(titlebar_svg_icon("titlebar/plus.svg", 14.0 * scale, appearance.muted)).child("Create space")
                    .on_click(cx.listener(|app, _, _, cx| { cx.stop_propagation(); app.dispatch_native_sidebar_ui(json!({"type": "editSpace"}), cx); }))))
                .children(visible.into_iter().chain(snapshot.spaces.iter().filter(|space| space.id == "other")).map(|space| self.render_native_sidebar_space(space, &visible_ids, appearance, cx)))
                .when(overflow, |row| row.child(div().id("native-sidebar-more-spaces").role(gpui::Role::Button).aria_label("More spaces").size(px(28.0 * scale)).flex().items_center().justify_center().rounded(px(6.0 * scale)).hover(|row| row.bg(appearance.hover))
                    .child(titlebar_svg_icon("titlebar/dots.svg", 16.0 * scale, appearance.muted))
                    .on_click(cx.listener(move |_, event: &gpui::ClickEvent, window, cx| {
                        // CDXC:Spaces 2026-09-20 DECISION: User: the More menu lists the overflowing Spaces first and puts New Space last, under a separator.
                        let mut items: Vec<_> = hidden.iter().map(|space| json!({"label": space.name, "icon": space.icon, "iconColor": crate::app::window::space_editor_modal::space_display_color(&space.color, light), "checked": space.selected, "command": {"type": "selectSpace", "spaceId": space.id}})).collect();
                        if !items.is_empty() { items.push(json!({"separator": true})); }
                        items.push(json!({"label": "New Space", "icon": "plus", "command": {"type": "editSpace"}}));
                        Self::show_native_sidebar_menu(&json!(items), event.position(), scale, window, cx);
                    }))))))
            .into_any_element()
    }

    fn render_native_sidebar_space(
        &self,
        space: &NativeSidebarSpace,
        visible_ids: &[String],
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = space.id.clone();
        let menu_id = id.clone();
        let drag_id = id.clone();
        let name = space.name.clone();
        let scale = appearance.scale;
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        let path = gpui_sidebar_command_icon_asset_path(Some(&space.icon));
        let display_color = crate::app::window::space_editor_modal::space_display_color(
            &space.color,
            appearance.light,
        );
        let color = u32::from_str_radix(display_color.trim_start_matches('#'), 16)
            .map(rgb)
            .map(gpui::Hsla::from)
            .unwrap_or(appearance.foreground);
        /*
        CDXC:Spaces 2026-09-21 DECISION:
        User: the selected Space's grey box looked bad in light mode, make it nicer. In light mode it
        is a raised white tile (white fill, hairline outline, small shadow); dark mode keeps its
        translucent fill.
        */
        let (selected_background, selected_outline) = if appearance.light {
            (
                appearance.selected,
                appearance.selected_outline.opacity(0.14 / 0.12),
            )
        } else {
            (
                appearance.foreground.opacity(0.12),
                appearance.foreground.opacity(0.16),
            )
        };
        let dragged = SidebarDrag {
            kind: "space",
            id: id.clone(),
            title: space.name.clone(),
            scale,
            preview: super::drag::SidebarDragPreview::Space(super::space_drag::SpaceDragPreview {
                visible_ids: visible_ids.to_vec(),
                icon: space.icon.clone(),
                color,
                background: if space.selected {
                    selected_background
                } else if space.contains_active_session {
                    appearance.selected
                } else {
                    appearance.foreground.opacity(0.08)
                },
                outline: if space.selected {
                    selected_outline
                } else if space.contains_active_session {
                    appearance.selected_outline
                } else {
                    appearance.foreground.opacity(0.08)
                },
                pointer_y: px(0.0),
            }),
        };
        let drop_position = self.native_sidebar_drop_position("targetSpaceId", &id);
        let has_status =
            space.working_count > 0 || space.attention_count > 0 || space.background_work_count > 0;
        let icon = gpui::svg()
            .path(path)
            .size(px(16.0 * scale))
            .text_color(color)
            .opacity(if has_status { 0.8 } else { 1.0 });
        /*
        CDXC:Spaces 2026-09-21 DECISION:
        User: the Spaces row's buttons are centred in the sidebar, and every Space's icon has a very
        light outline. A selected Space, or one holding the active session, keeps its stronger outline.
        */
        v_flex().id(format!("native-sidebar-space-{id}")).role(gpui::Role::Button).aria_label(format!("Space {name}")).aria_selected(space.selected).relative().size(px(28.0 * scale)).flex_shrink_0().items_center().justify_center().rounded(px(6.0 * scale)).border_1().border_color(appearance.foreground.opacity(0.08)).cursor_pointer()
            .when(self.native_sidebar.is_dragging("space", &id), |row| row.opacity(0.3))
            .when(space.contains_active_session && !space.selected, |row| row.bg(appearance.selected).border_color(appearance.selected_outline))
            .when(space.selected, |row| row.bg(selected_background).border_color(selected_outline).when(appearance.light && !appearance.glass, |row| row.shadow_sm()))
            .hover(|row| row.bg(appearance.hover))
            .child(icon)
            .children(drop_position.map(|position| super::space_drag::insertion_line(position, scale)))
            // CDXC:Spaces 2026-09-22 DECISION: User: remove the numbers for now and match the dots exactly to the session card status dots. The 2026-09-25 overlap layout lives on `status_dot_stack`.
            .when(has_status, |row| row.child(h_flex().absolute().top(px(20.5 * scale)).left_0().w_full().justify_center().h(px(11.0 * scale))
                .child(super::status::status_dot_stack(space.working_count, space.attention_count, space.background_work_count, scale))))
            .when(self.native_sidebar.pointer_inside && self.native_sidebar.menu.is_none() && !cx.has_active_drag(), |row| row.managed_discrete_tooltip_with_placement(tooltip_span.placement(), appearance.tooltip_delay, move |window, cx| super::tooltips::sidebar_free_width_tooltip(name.to_string(), tooltip_span, scale, window, cx)))
            .when(id != "other", |row| row.sidebar_drag_source(dragged, cx))
            .sidebar_drop_target("space", drag_id, None, cx)
            .on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.select_native_space(&id, cx); }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                let mut items = Vec::new();
                // CDXC:Spaces 2026-09-22 DECISION: User: a Space icon's menu is two labelled groups: "Manage" at the top (Edit Space, New Space) and "Sleep" at the bottom, and the Sleep group shows only Sleep Inactive. This supersedes the same-day request for Sleep Space and Sleep Others on this menu. Sleep Inactive is the Space's idle sessions only. The Other button offers the same rows for the projects no Space claims.
                items.push(json!({"label": "Manage", "heading": true}));
                if menu_id != "other" { items.push(json!({"label": "Edit Space", "icon": "pencil", "command": {"type": "editSpace", "spaceId": menu_id}})); }
                items.push(json!({"label": "New Space", "icon": "plus", "command": {"type": "editSpace"}}));
                items.push(json!({"separator": true}));
                items.push(json!({"label": "Sleep", "heading": true}));
                let plans = app.gx_store_space_sleep_plans(&menu_id);
                items.push(json!({"label": "Sleep Inactive", "icon": "moon", "disabled": !app.gx_store_space_sleep_has_work(plans.as_ref(), SpaceSleepScope::Inactive), "command": {"type": "sleepSpace", "spaceId": menu_id, "scope": SpaceSleepScope::Inactive.as_str()}}));
                Self::show_native_sidebar_menu(&json!(items), event.position, scale, window, cx);
            }))
            .into_any_element()
    }
}
