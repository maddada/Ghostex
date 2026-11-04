//! A coordinator's threads, in the floating menu the Threads pill on the working row opens: one list
//! of its threads, each row opening that thread. Order, labels and the steps ("Show recent", "N more")
//! come from the core (packages/gx-chat-core/src/extras/coordinator_threads.rs); the pill is
//! chat_pills.rs.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/AgentPanels.tsx (`CoordinatorThreadsMenu`, the
//! phone's twin), apps/desktop/src/app/session_chat/host_actions.rs (`openCoordinatorThread`).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

/// The sidebar's working colour (native_sidebar/status.rs), so a thread reads the same in both
/// places, and the amber of a row's "Needs your approval" tag (the header's dot takes the sidebar's
/// red, `chat_pills::waiting_color`).
const WORKING_COLOR: u32 = 0xc68a06;
const APPROVAL_COLOR: u32 = 0xf5a524;

/// One thread row, the "N more" line and the list's cap, in unscaled pixels: fixed heights, so the
/// floating menu knows its size before it opens (`threads_menu_height`).
const ROW_HEIGHT: f32 = 24.0;
const ROW_GAP: f32 = 2.0;
const LIST_MAX: f32 = 360.0;

/// The Workers pill's menu: the header line, the rows and "Show recent" / "N more" / "Show fewer".
pub(super) fn threads_menu_height(panel: &Value) -> f32 {
    let rows = panel["rows"].as_array().map_or(0, Vec::len)
        + usize::from(!text(panel, "emptyLabel").is_empty());
    let list = (rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32 * ROW_GAP).min(LIST_MAX);
    let footer = if text(panel, "moreLabel").is_empty() {
        0.0
    } else {
        super::chat_pills::MENU_SECTION_GAP + super::chat_pills::MENU_FOOTER_HEIGHT
    };
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list + footer
}

impl NativeChatView {
    /// The Workers pill's floating menu: what the Threads panel above the composer listed, with the
    /// same rows and steps. Drawn in the menu's window (`option_menu/pill_menu.rs`) with this
    /// view's listeners, so a row still talks to the chat it belongs to.
    pub(super) fn threads_menu_body(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.snapshot["coordinatorThreadsPanel"].clone();
        if !panel.is_object() {
            return None;
        }
        let s = p.scale;
        let header = super::chat_pills::pill_menu_header(
            "titlebar/users-group.svg",
            "Workers",
            text(&panel, "meta"),
            // The pill's waiting colour (CDXC:SessionChat 2026-10-27 in the core's
            // extras/chat_pills.rs): the sidebar's red for an approval, its pink for a question.
            super::chat_pills::waiting_color(if panel["attention"] == true {
                "approval"
            } else if panel["question"] == true {
                "question"
            } else {
                ""
            })
            .map(|color| {
                div()
                    .size(px(8.0 * s))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(color)
                    .into_any_element()
            }),
            p,
        );
        let rows_count = panel["rows"].as_array().map_or(0, Vec::len);
        let mut rows = div()
            .id("chat-coordinator-threads-rows")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s))
            .max_h(px(LIST_MAX * s))
            .overflow_y_scroll();
        for row in panel["rows"].as_array().into_iter().flatten() {
            rows = rows.child(self.coordinator_thread_row(row, p, cx));
        }
        let empty = text(&panel, "emptyLabel");
        let listed = rows_count > 0 || !empty.is_empty();
        if !empty.is_empty() {
            rows = rows.child(
                div()
                    .h(px(ROW_HEIGHT * s))
                    .flex()
                    .items_center()
                    .px(px(4.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(empty),
            );
        }
        let more = text(&panel, "moreLabel");
        let footer = (!more.is_empty()).then(|| {
            let step = panel["moreStep"].as_u64().unwrap_or(0);
            div()
                .id("chat-coordinator-threads-more")
                .role(gpui::Role::Button)
                .aria_label(match step {
                    0 => "Show fewer workers".to_string(),
                    1 => "Show recent workers".to_string(),
                    _ => format!("Show {more} workers"),
                })
                .h(px(super::chat_pills::MENU_FOOTER_HEIGHT * s))
                .flex()
                .items_center()
                .px(px(4.0 * s))
                .chat_cursor_pointer()
                .text_size(px(12.0 * s))
                .text_color(p.card_muted)
                .hover(|style| style.text_color(p.foreground))
                .child(more)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(
                        json!({"type":"toggleCoordinatorThreadsMore","step":step}),
                        cx,
                    )
                }))
                .into_any_element()
        });
        Some(super::chat_pills::pill_menu_column(
            "chat-coordinator-threads",
            "Coordinator workers",
            header,
            listed.then(|| rows.into_any_element()),
            footer,
            p,
        ))
    }

    fn coordinator_thread_row(
        &self,
        row: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let needs_approval = row["needsApproval"] == true;
        let marker = if row["working"] == true {
            div()
                .size(px(8.0 * s))
                .rounded_full()
                .bg(rgb(WORKING_COLOR))
        } else {
            div()
                .size(px(8.0 * s))
                .rounded_full()
                .border(px(1.5 * s))
                .border_color(p.muted.opacity(0.7))
        };
        let title = text(row, "title");
        let detail = text(row, "detail");
        let branch = text(row, "branch");
        let tooltip = [title.as_str(), detail.as_str(), branch.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        // A worker on another computer than the coordinator's: its name, and its server, which
        // the click opens it on.
        let computer = text(row, "computer");
        let command = json!({
            "type": "openCoordinatorThread",
            "serverId": row["serverId"],
            "projectId": row["projectId"],
            "sessionId": row["sessionId"],
        });
        div()
            .id(gpui::SharedString::from(format!(
                "chat-coordinator-thread:{}",
                text(row, "key")
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("Open worker {title}"))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .px(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(12.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
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
                div()
                    .flex_shrink_0()
                    .max_w(px(260.0 * s))
                    .truncate()
                    .text_color(p.foreground)
                    .child(title),
            )
            .when(needs_approval, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px(px(5.0 * s))
                        .rounded(px(4.0 * s))
                        .bg(gpui::Hsla::from(rgb(APPROVAL_COLOR)).opacity(0.16))
                        .text_size(px(11.0 * s))
                        .text_color(rgb(APPROVAL_COLOR))
                        .child(
                            ghostex_gx_chat_core::extras::coordinator_threads::NEEDS_APPROVAL_LABEL,
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .child(detail),
            )
            .when(!computer.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(3.0 * s))
                        .max_w(px(120.0 * s))
                        .child(
                            gpui::svg()
                                .path("titlebar/device-desktop.svg")
                                .flex_shrink_0()
                                .size(px(11.0 * s))
                                .text_color(p.muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.0 * s))
                                .text_color(p.muted)
                                .child(computer),
                        ),
                )
            })
            .when(!branch.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(160.0 * s))
                        .truncate()
                        .text_size(px(11.0 * s))
                        .text_color(p.muted)
                        .child(branch),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(command.clone(), cx);
                this.close_pill_menu(cx);
            }))
            .into_any_element()
    }
}
