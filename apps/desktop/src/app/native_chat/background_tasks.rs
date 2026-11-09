//! The background tasks, in the floating menu the Background pill on the working row opens
//! (chat_pills.rs): one row per task with its own clock, and an (i) beside the clock that says the
//! work continues in the background (`extras::background_tasks` in gx-chat-core words it).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The phone draws the same rows in apps/mobile/app/src/chat/native/cards/ChatPills.tsx; the
//! decision and its wording live in packages/gx-chat-core/src/extras/background_tasks.rs.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::helpers::ThrottledAnimationExt;
use gpui::StatefulInteractiveElement as _;
use gpui::{
    AnyElement, FontFeatures, InteractiveElement, IntoElement, ParentElement, Styled, div, px, svg,
};
use serde_json::Value;
use std::time::Duration;

/// One task row, the "+N more" line and the gap between them, in unscaled pixels: fixed heights,
/// so the floating menu knows its size before it opens (`background_menu_height`).
const ROW_HEIGHT: f32 = 26.0;
const ROW_GAP: f32 = 4.0;

/// The Background pill's menu: the header line, one row per named task and "+N more".
pub(super) fn background_menu_height(tasks: &Value) -> f32 {
    let rows =
        tasks["rows"].as_array().map_or(0, Vec::len) + usize::from(tasks["more"].is_string());
    let list = rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32 * ROW_GAP;
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list
}

impl NativeChatView {
    /// The Background pill's floating menu: the rows the background-task card above the composer
    /// showed, under a header line with the running count.
    pub(super) fn background_menu_body(&self, p: &ChatAppearance) -> Option<AnyElement> {
        let tasks = self.snapshot["backgroundTasksPanel"].clone();
        if !tasks.is_object() {
            return None;
        }
        let s = p.scale;
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        let info = tasks["info"].as_str().unwrap_or_default();
        let header = super::chat_pills::pill_menu_header(
            "titlebar/terminal-2.svg",
            "Background tasks",
            tasks["meta"].as_str().unwrap_or_default().to_owned(),
            None,
            p,
        );
        let mut rows = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s))
            .min_w_0()
            .px(px(4.0 * s));
        let named = tasks["rows"].as_array().into_iter().flatten();
        let mut any_named = false;
        for (index, row) in named.enumerate() {
            any_named = true;
            rows = rows.child(self.background_task_row(index, row, info, p, reduced_motion));
        }
        if let Some(more) = tasks["more"].as_str() {
            rows = rows.child(
                div()
                    .h(px(ROW_HEIGHT * s))
                    .flex()
                    .items_center()
                    // A remainder under named rows lines up with their titles; alone it is the line.
                    .pl(px(if any_named { 22.0 * s } else { 0.0 }))
                    .text_size(px(13.0 * s))
                    .text_color(p.muted)
                    .child(more.to_owned()),
            );
        }
        Some(super::chat_pills::pill_menu_column(
            "chat-background-tasks",
            "Background tasks",
            header,
            Some(rows.into_any_element()),
            None,
            p,
        ))
    }

    fn background_task_row(
        &self,
        index: usize,
        row: &Value,
        info: &str,
        p: &ChatAppearance,
        reduced_motion: bool,
    ) -> AnyElement {
        let s = p.scale;
        let glyph = svg()
            .path("titlebar/loader2.svg")
            .size(px(12.0 * s))
            .text_color(p.control_primary);
        let glyph = if reduced_motion {
            glyph.into_any_element()
        } else {
            glyph
                .with_throttled_animation(
                    ("chat-background-task", index),
                    Duration::from_secs(1),
                    |glyph, progress| {
                        glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                            progress,
                        )))
                    },
                )
                .into_any_element()
        };
        // CDXC:SessionChat 2026-10-11 DECISION: User: the task title takes the same regular weight as the Threads card's title, not bold.
        let prefix = row["prefix"]
            .as_str()
            .map(|prefix| format!("{prefix} "))
            .unwrap_or_default();
        let title = row["title"].as_str().unwrap_or_default();
        let mut name = div()
            .id(("chat-background-task-title", index))
            .flex_1()
            .min_w_0()
            .truncate()
            .text_color(p.foreground)
            .child(format!("{prefix}{title}"));
        // The command the agent ran, for whoever wants to know exactly what it is.
        if let Some(command) = row["command"]
            .as_str()
            .filter(|command| !command.is_empty())
        {
            let command = command.to_owned();
            name = name.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(command.clone()).build(window, cx)
            });
        }
        let line = div()
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .text_size(px(14.0 * s))
            .child(
                div()
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph),
            )
            .child(name);
        // The (i) sits right against the clock it explains.
        let mut trailing = div().flex().items_center().gap(px(6.0 * s)).flex_shrink_0();
        if !info.is_empty() {
            let info = info.to_owned();
            trailing = trailing.child(
                div()
                    .id(("chat-background-task-info", index))
                    .role(gpui::Role::Image)
                    .aria_label(info.clone())
                    .flex()
                    .items_center()
                    .justify_center()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(info.clone()).build(window, cx)
                    })
                    .child(
                        svg()
                            .path("titlebar/info-circle.svg")
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            );
        }
        if let Some(elapsed) = row["elapsed"].as_str() {
            trailing = trailing.child(
                div()
                    .text_color(p.muted)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .child(elapsed.to_owned()),
            );
        }
        let line = line.child(trailing);
        line.into_any_element()
    }
}
