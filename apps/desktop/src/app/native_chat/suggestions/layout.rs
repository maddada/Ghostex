//! The `/`, `$` and `@` popup's measurements, read from
//! `packages/gx-chat-core/visual/composer-suggestions.json`.

use gpui::{AnyElement, IntoElement, ParentElement as _, Pixels, Styled as _, div, px};
use serde::Deserialize;
use std::{cell::Cell, rc::Rc, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SuggestionPopupSpec {
    pub(super) gap_above_px: f32,
    pub(super) radius_px: f32,
    pub(super) border_px: f32,
    pub(super) list_max_height_px: f32,
    pub(super) list_padding_px: f32,
    pub(super) padding_inline_px: f32,
    pub(super) heading_font_size_px: f32,
    pub(super) heading_line_height_px: f32,
    pub(super) heading_letter_spacing_px: f32,
    pub(super) heading_padding_top_px: f32,
    pub(super) heading_padding_bottom_px: f32,
    pub(super) row_font_size_px: f32,
    pub(super) row_line_height_px: f32,
    pub(super) row_padding_block_px: f32,
    pub(super) row_gap_px: f32,
    pub(super) row_radius_px: f32,
    pub(super) row_border_px: f32,
    pub(super) label_column_px: f32,
    pub(super) icon_px: f32,
    pub(super) retry_height_px: f32,
    pub(super) retry_padding_inline_px: f32,
    pub(super) retry_radius_px: f32,
    pub(super) shadows: Vec<SuggestionShadow>,
}

/// One layer of React's `shadow-xl`, in unzoomed CSS pixels.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SuggestionShadow {
    pub(super) offset_y_px: f32,
    pub(super) blur_px: f32,
    pub(super) spread_px: f32,
    pub(super) alpha: f32,
}

/// CDXC:SessionChat 2026-09-19 SEE-ALSO:
/// React's picker kept these values as Tailwind classes in session-chat-composer.tsx, which the
/// JSON mirrors. Borders stay whole pixels at every zoom, like the composer card's own border.
pub(super) static SPEC: LazyLock<SuggestionPopupSpec> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../../packages/gx-chat-core/visual/composer-suggestions.json"
    ))
    .expect("shared composer suggestion popup geometry")
});

impl SuggestionPopupSpec {
    pub(super) fn heading_height(&self, scale: f32) -> Pixels {
        px((self.heading_padding_top_px
            + self.heading_line_height_px
            + self.heading_padding_bottom_px)
            * scale)
    }

    /// A pickable row: React's bare `<button>` carried the legacy 1px outline on both edges.
    pub(super) fn row_height(&self, scale: f32) -> Pixels {
        px(
            (self.row_padding_block_px * 2.0 + self.row_line_height_px) * scale
                + self.row_border_px * 2.0,
        )
    }

    /// The loading, empty or error row, which was a plain `div` in React and so has no outline.
    pub(super) fn status_height(&self, retry: bool, scale: f32) -> Pixels {
        let content = if retry {
            self.row_line_height_px.max(self.retry_height_px)
        } else {
            self.row_line_height_px
        };
        px((self.row_padding_block_px * 2.0 + content) * scale)
    }
}

/// The popup's full height for one projection: React's `max-h-72` caps the scrolling list, and the
/// container's border sits outside that cap.
pub(in crate::app::native_chat) fn suggestion_panel_height(
    projection: &serde_json::Value,
    scale: f32,
) -> Pixels {
    let spec = &*SPEC;
    let rows = projection["rows"].as_array().map_or(0, Vec::len) as f32;
    let status = if projection["status"].is_string() {
        spec.status_height(projection["retry"] == true, scale)
    } else {
        px(0.0)
    };
    let list = px(spec.list_padding_px * 2.0 * scale)
        + spec.heading_height(scale)
        + spec.row_height(scale) * rows
        + status;
    list.min(px(spec.list_max_height_px * scale)) + px(spec.border_px * 2.0)
}

/// The top edge of the panels kept inside the composer's field above the card (task list,
/// subagents, the not-ready card or error, the saved-draft notice), collected over one frame.
/// The list opens above the topmost of them instead of covering them, as React's did.
pub(in crate::app::native_chat) type StackTop = Rc<Cell<Option<Pixels>>>;

/// Reports its parent's top edge into `top`.
pub(in crate::app::native_chat) fn stack_marker(top: &StackTop) -> impl IntoElement {
    let top = top.clone();
    gpui::canvas(
        move |bounds, _, _| {
            let edge = bounds.top();
            top.set(Some(top.get().map_or(edge, |old| old.min(edge))));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

pub(in crate::app::native_chat) fn stack_member(element: AnyElement, top: &StackTop) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(element)
        .child(stack_marker(top))
        .into_any_element()
}
