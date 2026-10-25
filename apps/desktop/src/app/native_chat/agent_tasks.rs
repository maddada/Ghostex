//! The agent's task plan, in the floating menu the Tasks pill on the working row opens when the
//! chat has no Session details panel to show it in: the list the CLI shows under its own
//! transcript. Rows, order, the header line and the completed-task fold all come from the core's
//! task panel (packages/gx-chat-core/src/extras/agent_tasks.rs).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, relative,
};
use serde_json::{Value, json};
use std::time::Duration;

/// One task row and the list's cap, in unscaled pixels: fixed heights, so the floating menu knows
/// its size before it opens (`tasks_menu_height`).
const ROW_HEIGHT: f32 = 22.0;
const ROW_GAP: f32 = 2.0;
const LIST_MAX: f32 = 220.0;

/// The Tasks pill's menu: the header line, the rows and the "N more tasks" fold.
pub(super) fn tasks_menu_height(panel: &Value) -> f32 {
    let rows = panel["rows"].as_array().map_or(0, Vec::len);
    let list = (rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32 * ROW_GAP).min(LIST_MAX);
    let footer = if text(panel, "foldLabel").is_empty() {
        0.0
    } else {
        super::chat_pills::MENU_SECTION_GAP + super::chat_pills::MENU_FOOTER_HEIGHT
    };
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list + footer
}

impl NativeChatView {
    /// The Tasks pill's floating menu: what the Tasks panel above the composer showed, the progress
    /// bar beside its header line and the same rows and fold.
    pub(super) fn tasks_menu_body(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.snapshot["agentTasksPanel"].clone();
        if !panel.is_object() {
            return None;
        }
        let s = p.scale;
        let percent = panel["percent"].as_f64().unwrap_or(0.0) as f32 / 100.0;
        let bar = div()
            .w(px(48.0 * s))
            .h(px(4.0 * s))
            .rounded_full()
            .overflow_hidden()
            .flex_shrink_0()
            .bg(p.foreground.opacity(0.1))
            .child(
                div()
                    .h_full()
                    .w(relative(percent))
                    .rounded_full()
                    .bg(p.control_primary),
            )
            .into_any_element();
        let header = super::chat_pills::pill_menu_header(
            "titlebar/list-check.svg",
            "Tasks",
            text(&panel, "meta"),
            Some(bar),
            p,
        );
        let mut rows = div()
            .id("chat-agent-tasks-rows")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s))
            .max_h(px(LIST_MAX * s))
            .overflow_y_scroll();
        for row in panel["rows"].as_array().into_iter().flatten() {
            rows = rows.child(self.task_row(row, p));
        }
        let expanded = panel["showCompleted"] == true;
        let fold = text(&panel, "foldLabel");
        // CDXC:SessionChat 2026-10-15 DECISION: User (2026-09-16): the fold is a text line, not a button, and reads "N more task(s)" / "Show less tasks". Rows are the Subagents row size. Since the Tasks pill (2026-10-15) the list is the pill's floating menu, on the chat's menu surface under a header line with the list icon, instead of the shared status card (orchestrator's call, answering for the user).
        let footer = (!fold.is_empty()).then(|| {
            div()
                .id("chat-agent-tasks-fold")
                .role(gpui::Role::Button)
                .aria_label(fold.clone())
                .h(px(super::chat_pills::MENU_FOOTER_HEIGHT * s))
                .flex()
                .items_center()
                .chat_cursor_pointer()
                .text_size(px(12.0 * s))
                .text_color(p.card_muted)
                .hover(|style| style.text_color(p.foreground))
                .child(fold)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(
                        json!({"type":"toggleAgentTasksCompleted","expanded":!expanded}),
                        cx,
                    )
                }))
                .into_any_element()
        });
        Some(super::chat_pills::pill_menu_column(
            "chat-agent-tasks",
            "Agent tasks",
            header,
            Some(rows.into_any_element()),
            footer,
            p,
        ))
    }

    fn task_row(&self, row: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let group = row["group"].as_str().unwrap_or("pending");
        let marker = match group {
            "in_progress" => {
                let glyph = gpui::svg()
                    .path("titlebar/loader2.svg")
                    .size(px(13.0 * s))
                    .text_color(p.control_primary);
                if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                    glyph.into_any_element()
                } else {
                    glyph
                        .with_throttled_animation(
                            "chat-agent-tasks-spinner",
                            Duration::from_secs(1),
                            |glyph, progress| {
                                glyph.with_transformation(gpui::Transformation::rotate(
                                    gpui::percentage(progress),
                                ))
                            },
                        )
                        .into_any_element()
                }
            }
            // React: a filled check for done work (`IconCircleCheckFilled`), not the outline ring
            // that a waiting task wears.
            "completed" => gpui::svg()
                .path("titlebar/circle-check-filled.svg")
                .size(px(13.0 * s))
                .text_color(p.primary)
                .into_any_element(),
            // The waiting marker is a hollow ring, the shape the CLI uses for a pending box
            // (`.ghostex-chat-agent-tasks-dot`).
            _ => div()
                .size(px(8.0 * s))
                .rounded_full()
                .border(px(1.5 * s))
                .border_color(p.muted.opacity(0.7))
                .into_any_element(),
        };
        let title = text(row, "title");
        let blocked = text(row, "blockedLabel");
        div()
            .id(gpui::SharedString::from(format!(
                "chat-agent-task:{}",
                text(row, "id")
            )))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .text_size(px(12.0 * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(title.clone()).build(window, cx)
            })
            .child(
                div()
                    .size(px(13.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(marker),
            )
            .child(
                // React's `flex: 0 1 auto`: the title takes only the room it needs, so what a task
                // waits for reads as part of the line instead of drifting to the far edge.
                div()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .truncate()
                    .when(group == "completed", |this| {
                        this.text_color(p.card_muted).line_through()
                    })
                    .when(group != "completed", |this| this.text_color(p.prose))
                    .child(text(row, "subject")),
            )
            .when(!blocked.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * s))
                        .text_color(p.card_muted)
                        .child(blocked),
                )
            })
            .into_any_element()
    }
}
