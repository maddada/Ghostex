use super::drag::SidebarDropTarget;
use super::drag_source::SidebarDragSource;
use super::{
    appearance::SidebarAppearance,
    model::{NativeSidebarCollection, NativeSidebarSnapshot},
};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::{
    h_flex,
    input::{Escape, Input},
    v_flex,
};
use serde_json::json;

impl GhostexGpuiApp {
    /// CDXC:Projects 2026-09-19 DECISION:
    /// User: keep only the Branched rail look for project groups and default everyone to it (the Quiet and Header rail options are gone); the group header shows no chevron, and double-clicking it no longer renames it (Rename stays in its context menu).
    pub(crate) fn render_native_collection(
        &self,
        collection: &NativeSidebarCollection,
        snapshot: &NativeSidebarSnapshot,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let id = collection.collection_id.clone();
        let hovered = self.native_sidebar.hovered_collection.as_ref() == Some(&id);
        let hover_id = id.clone();
        let drop_position = self.native_sidebar_drop_position("targetId", &id);
        let drop_inside = self
            .native_sidebar
            .drop_command
            .as_ref()
            .is_some_and(|command| command["collectionId"] == id);
        let drop_id = id.clone();
        let bulk_id = id.clone();
        let menu = collection.menu.clone();
        let scale = appearance.scale;
        let color = u32::from_str_radix(collection.color.trim_start_matches('#'), 16)
            .map(rgb)
            .map(gpui::Hsla::from)
            .unwrap_or(appearance.muted);
        let dragged = super::drag::SidebarDrag {
            kind: "collection",
            preview: super::drag::SidebarDragPreview::Row(super::row_drag::RowDragPreview {
                identity: super::row_drag::RowDragIdentity::Collection {
                    color,
                    background: color.opacity(0.18),
                },
                appearance: appearance.clone(),
                width: px(0.0),
                pointer_x: px(0.0),
            }),
            id: id.clone(),
            title: collection.title.clone(),
            scale: appearance.scale,
        };
        let rail_width = 2.0;
        let active = collection.collapsed && collection.contains_active_session;
        let name = match self
            .native_sidebar
            .name_editor
            .as_ref()
            .filter(|editor| editor.kind == "collection" && editor.id == id)
        {
            Some(editor) => div()
                .flex_1()
                .min_w_0()
                .on_action(cx.listener(|app, _: &Escape, _, cx| {
                    cx.stop_propagation();
                    app.finish_native_sidebar_rename(false, cx);
                }))
                .child(Input::new(&editor.input).h(px(24.0 * scale)))
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(15.55 * scale))
                .font_weight(FontWeight::LIGHT)
                .child(collection.title.clone())
                .into_any_element(),
        };
        v_flex().relative().when(self.native_sidebar.is_dragging("collection", &id), |row| row.opacity(0.28)).flex_shrink_0().ml(px(3.0 * scale)).mr(px(5.0 * scale)).mb(px(10.0 * scale)).pb(px(5.0 * scale)).pl(px((rail_width + 10.0) * scale))
            .child(div().absolute().left_0().top_0().bottom(px(5.0 * scale)).w(px(rail_width * scale)).bg(color.opacity(0.18)))
            .child(h_flex().id(format!("native-collection-{id}")).role(gpui::Role::TreeItem).aria_label(collection.title.clone()).aria_expanded(!collection.collapsed).relative().ml(px(-10.0 * scale)).h(px(30.0 * scale)).pl(px(8.0 * scale)).pr(px(8.0 * scale)).gap(px(5.0 * scale))
                .bg(color.opacity(0.18))
                .hover(|row| row.bg(color.opacity(0.22)))
                .when(active, |row| row.bg(appearance.selected).rounded(px(5.0 * scale)).child(super::decorations::selected_outline(appearance)))
                .when_some(drop_position, |row, position| row.child(super::drag::drop_line(position, scale)))
                .when(drop_inside, |row| row.bg(color.opacity(0.28)))
                .child(name)
                .when(collection.collapsed && collection.working_count > 0, |row| row.child(div().text_size(px(10.0 * scale)).text_color(rgb(super::status::WORKING_COLOR)).child(collection.working_count.to_string())))
                .when(collection.collapsed && collection.attention_count > 0, |row| row.child(div().text_size(px(10.0 * scale)).text_color(rgb(0x95d7f6)).child(collection.attention_count.to_string())))
                .when(collection.collapsed && collection.working_count == 0 && collection.background_work_count > 0, |row| row.child(div().text_size(px(10.0 * scale)).text_color(super::status::background_work_color()).child(collection.background_work_count.to_string())))
                .when(collection.collapsed && collection.working_count == 0 && collection.attention_count == 0 && collection.awake_count > 0, |row| row.child(div().text_size(px(10.0 * scale)).child(collection.awake_count.to_string())))
                .when(!collection.collapsed && hovered, |row| row.child(div().id(format!("native-collection-bulk-{id}")).role(gpui::Role::Button).aria_label("Collection actions").size(px(22.0 * scale)).flex().items_center().justify_center().child(titlebar_svg_icon("titlebar/arrows-diagonal.svg", 14.0 * scale, appearance.muted))
                    .on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.dispatch_native_sidebar_ui(json!({ "type": "collectionAction", "collectionId": bulk_id, "action": "toggleProjects" }), cx); }))))
                .on_hover(cx.listener(move |app, hovered, _, cx| {
                    if *hovered { app.native_sidebar.hovered_collection = Some(hover_id.clone()); }
                    else if app.native_sidebar.hovered_collection.as_ref() == Some(&hover_id) { app.native_sidebar.hovered_collection = None; }
                    cx.notify();
                }))
                .sidebar_drag_source(dragged, cx)
.sidebar_drop_target("collection", drop_id, None, cx)
                .on_mouse_down(MouseButton::Right, move |event, window, cx| { cx.stop_propagation(); Self::show_native_sidebar_menu(&menu, event.position, scale, window, cx); })
                .on_click(cx.listener(move |app, _, _, cx| { cx.stop_propagation(); app.dispatch_native_sidebar_ui(json!({ "type": "collectionAction", "collectionId": id, "action": "toggle" }), cx); })))
            .when(self.native_sidebar.disclosures.present(&format!("collection:{}", collection.collection_id), collection.collapsed), |column| {
                let content = v_flex().w_full().pt(px(5.0 * scale)).pl(px(8.0 * scale))
                    .children(collection.group_ids.iter().filter_map(|id| snapshot.groups.iter().find(|group| &group.group_id == id)).map(|group| div().mb(px(if group.collapsed { 5.0 } else { 7.0 } * scale)).child(self.render_native_sidebar_group(group, &snapshot.hud, appearance, cx))));
                column.child(self.render_native_disclosure(format!("collection:{}", collection.collection_id), content.into_any_element(), cx))
            })
            .into_any_element()
    }
}
