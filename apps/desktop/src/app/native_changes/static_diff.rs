//! A patch drawn read-only with the Changes view's rows: Files' "changed on disk" comparison uses
//! it, so a diff looks the same wherever the app shows one.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

use super::model::parse_patch;
use super::rows::{code_cell, is_prose, kind_colors, marker_cell, number_cell, stat};
use super::style::*;
use crate::app::window::native_modal_kit::{MODAL_MONO_FONT, ModalPalette, hsla};

/// Every hunk of `patch` (one file's unified diff, starting with its `diff --git` line) with both
/// line numbers, in a scrolling column. `id` names the scroll area.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn render_static_diff(
    id: impl Into<SharedString>,
    patch: &str,
    palette: ModalPalette,
    wrap: bool,
) -> AnyElement {
    let s = ChangesStyle::resolve(palette);
    let Some(file) = parse_patch(patch).into_iter().next() else {
        return div()
            .p(px(16.0))
            .text_size(px(12.0))
            .text_color(hsla(s.muted))
            .child("No differences.")
            .into_any_element();
    };
    let plain = is_prose(&file.path);
    let mut rows: Vec<AnyElement> = vec![
        h_flex()
            .px(px(10.0))
            .h(px(FILE_HEADER_HEIGHT))
            .gap(px(8.0))
            .bg(hsla(s.header_bg))
            .border_b_1()
            .border_color(hsla(s.hairline))
            .text_size(px(12.0))
            .text_color(hsla(s.muted))
            .child("On disk \u{2192} your changes")
            .child(stat(&s, file.additions, file.deletions))
            .into_any_element(),
    ];
    for hunk in &file.hunks {
        rows.push(
            div()
                .w_full()
                .h(px(diff_hunk_height()))
                .pl(px(diff_number_width() * 2.0 + diff_marker_width()))
                .flex()
                .items_center()
                .bg(hsla(s.hunk_row))
                .font_family(MODAL_MONO_FONT)
                .text_size(px(11.5))
                .text_color(hsla(s.hunk_text))
                .child(hunk.header.clone())
                .into_any_element(),
        );
        for line in &hunk.lines {
            let (row_bg, gutter_bg, number_color) = kind_colors(&s, line.kind);
            rows.push(
                h_flex()
                    .w_full()
                    .items_start()
                    .min_h(px(diff_row_height()))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(diff_code_size()))
                    .line_height(px(diff_row_height()))
                    .bg(hsla(s.code_bg))
                    .when_some(row_bg, |this, bg| this.bg(hsla(bg)))
                    .child(number_cell(&s, line.old, gutter_bg, number_color))
                    .child(number_cell(&s, line.new, gutter_bg, number_color))
                    .child(marker_cell(&s, line.kind))
                    .child(code_cell(&s, line, wrap, plain))
                    .into_any_element(),
            );
        }
    }
    v_flex()
        .id(id.into())
        .size_full()
        .overflow_y_scroll()
        .bg(hsla(s.code_bg))
        .children(rows)
        .into_any_element()
}
