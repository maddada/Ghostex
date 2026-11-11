use crate::app::helpers::gpui_sidebar_command_icon_asset_path;
use gpui::{
    AnyElement, Bounds, Hsla, IntoElement, ParentElement, Pixels, Point, Styled, Window, div, px,
    rgb,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct SpaceDragPreview {
    pub(crate) visible_ids: Vec<String>,
    /// From one Space tile's start to the next's, before the zoom: the tiles stretch across their
    /// row (CDXC:Spaces 2026-11-11 DECISION in selectors.rs), so it follows the sidebar's width.
    pub(crate) pitch: f32,
    /// What the computers and their hairline take before the first Space tile, before the zoom.
    pub(crate) leading: f32,
    pub(crate) icon: String,
    pub(crate) color: Hsla,
    pub(crate) background: Hsla,
    pub(crate) outline: Hsla,
    pub(crate) pointer_y: Pixels,
}

impl SpaceDragPreview {
    pub(crate) fn render(&self, scale: f32, window: &Window) -> AnyElement {
        div()
            .relative()
            .top(self.pointer_y - window.mouse_position().y)
            .size(px(28.0 * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(super::navigation::SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .border_1()
            .border_color(self.outline)
            .bg(self.background)
            .child(
                gpui::svg()
                    .path(gpui_sidebar_command_icon_asset_path(Some(&self.icon)))
                    .size(px(16.0 * scale))
                    .text_color(self.color),
            )
            .into_any_element()
    }

    pub(crate) fn drop_command(
        &self,
        source_id: &str,
        target_kind: &str,
        target_id: &str,
        pointer: Point<Pixels>,
        bounds: Bounds<Pixels>,
        scale: f32,
    ) -> Option<Value> {
        let pitch = self.pitch;
        let left = if target_kind == "space-row" {
            bounds.left() + px((super::navigation::SIDEBAR_TOP_EDGE_INSET + self.leading) * scale)
        } else {
            let index = self
                .visible_ids
                .iter()
                .position(|id| id == target_id)
                .or_else(|| (target_id == "other").then_some(self.visible_ids.len()))?;
            bounds.left() - px(index as f32 * pitch * scale)
        };
        let candidate = self.visible_ids.iter().enumerate().find(|(index, _)| {
            pointer.x < left + px(((pitch - 4.0) / 2.0 + *index as f32 * pitch) * scale)
        });
        let (target_index, target_id, after) = match candidate {
            Some((index, id)) => (index, id, false),
            None => (
                self.visible_ids.len().checked_sub(1)?,
                self.visible_ids.last()?,
                true,
            ),
        };
        let source_index = self.visible_ids.iter().position(|id| id == source_id)?;
        let insertion = target_index + usize::from(after);
        if insertion == source_index || insertion == source_index + 1 {
            return None;
        }
        Some(
            json!({"type": "moveSpace", "spaceId": source_id, "targetSpaceId": target_id,
            "visibleSpaceIds": self.visible_ids, "position": if after { "after" } else { "before" }}),
        )
    }
}

pub(super) fn insertion_line(position: &str, scale: f32) -> AnyElement {
    let line = div()
        .absolute()
        .top(px(5.0 * scale))
        .bottom(px(5.0 * scale))
        .w(px(2.0 * scale))
        .rounded(px(scale))
        .bg(rgb(0xc8cdd5).opacity(0.72));
    if position == "after" {
        line.right(px(-4.0 * scale))
    } else {
        line.left(px(-4.0 * scale))
    }
    .into_any_element()
}
