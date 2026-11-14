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
/// places, and its approval red, which a row waiting for approval takes for its dot and its "Needs
/// your approval" tag, as the header's dot does (`chat_pills::waiting_color`).
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of the Workers list: "please make the needs approval one red like the dot that's related to it". The tag was amber and the row's dot a hollow ring; both take the sidebar's approval red now (the app's one error red, the same in dark and light), so the header's red dot, the row and its tag read as one thing. The phone's AgentPanels.tsx follows.
const WORKING_COLOR: u32 = 0xc68a06;
const APPROVAL_COLOR: u32 = crate::app::native_sidebar::status::APPROVAL_COLOR;

/// One thread row, the "N more" line and the list's cap, in unscaled pixels: fixed heights, so the
/// floating menu knows its size before it opens (`threads_menu_height`).
const ROW_HEIGHT: f32 = 28.0;
const ROW_GAP: f32 = 2.0;
const LIST_MAX: f32 = 360.0;
/// The second line of a row whose thread is on work items: its pills (`items_line`).
const ITEMS_LINE_HEIGHT: f32 = 22.0;
const ITEM_PILL_HEIGHT: f32 = 19.0;
const ITEM_PILL_MAX: f32 = 230.0;

fn row_height(row: &Value) -> f32 {
    if row["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        ROW_HEIGHT + ITEMS_LINE_HEIGHT
    } else {
        ROW_HEIGHT
    }
}

/// The Workers pill's menu: the header line, the rows and "Show recent" / "N more" / "Show fewer".
pub(super) fn threads_menu_height(panel: &Value) -> f32 {
    let listed = panel["rows"].as_array();
    let rows = listed.map_or(0, Vec::len) + usize::from(!text(panel, "emptyLabel").is_empty());
    let heights: f32 = listed.into_iter().flatten().map(row_height).sum::<f32>()
        + if text(panel, "emptyLabel").is_empty() {
            0.0
        } else {
            ROW_HEIGHT
        };
    let list = (heights + rows.saturating_sub(1) as f32 * ROW_GAP).min(LIST_MAX);
    let footer = if text(panel, "moreLabel").is_empty() {
        0.0
    } else {
        super::chat_pills::MENU_SECTION_GAP + super::chat_pills::MENU_FOOTER_HEIGHT
    };
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list + footer
}

/// A worker's title as its session's pill: the hairline frame around the agent's logo and the
/// title, then the computer's icon for a worker on another computer. The row's own dot left of it
/// is the worker's state, so the pill draws no second one.
///
/// CDXC:SessionChat 2026-11-13 SEE-ALSO: the user's answer that session pills spread to the Workers
/// popover is on `mention_card` in gx-chat-core transcript/session_mentions.rs; the core gives each
/// row its `agentIcon` and `coordinator` (extras/coordinator_threads.rs `project`).
fn worker_pill(row: &Value, title: &str, elsewhere: bool, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let logo = row["agentIcon"].is_string() || row["coordinator"] == true;
    let (icon, icon_color) = super::markdown_links::session_pill_icon(
        row["agentIcon"].as_str().unwrap_or_default(),
        row["coordinator"] == true,
        p,
    );
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .min_w_0()
        .max_w(px(300.0 * s))
        .gap(px(4.0 * s))
        .h(px(20.0 * s))
        .px(px(6.0 * s))
        .rounded_full()
        .border(px(1.0))
        .border_color(p.foreground.opacity(0.14))
        .bg(p.foreground.opacity(0.03))
        .text_color(p.foreground)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_size(px(12.5 * s))
        .when(logo, |pill| {
            pill.child(
                gpui::svg()
                    .path(icon)
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
        })
        .child(div().min_w_0().truncate().child(title.to_string()))
        .when(elsewhere, |pill| {
            pill.child(
                gpui::svg()
                    // That computer's own icon (CDXC:SessionChat 2026-11-14 in session_directory.rs).
                    .path(crate::app::gx_chat::session_directory::computer_icon(
                        row["serverId"].as_str().unwrap_or_default(),
                    ))
                    .size(px(11.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
        })
        .into_any_element()
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
                    .text_size(px(13.0 * s))
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
                .text_size(px(13.0 * s))
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
        } else if needs_approval {
            div()
                .size(px(8.0 * s))
                .rounded_full()
                .bg(rgb(APPROVAL_COLOR))
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
        // A worker on another computer than the coordinator's: its server, which the click opens
        // it on, and its name, which the tooltip says (the pill shows the computer's icon).
        let computer = text(row, "computer");
        let tooltip = [
            title.as_str(),
            detail.as_str(),
            computer.as_str(),
            branch.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let command = json!({
            "type": "openCoordinatorThread",
            "serverId": row["serverId"],
            "projectId": row["projectId"],
            "sessionId": row["sessionId"],
        });
        let key = text(row, "key");
        let items = self.items_line(row, &key, p, cx);
        let line = div()
            .id(gpui::SharedString::from(format!(
                "chat-coordinator-thread-line:{key}"
            )))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
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
            .child(worker_pill(row, &title, !computer.is_empty(), p))
            .when(needs_approval, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px(px(5.0 * s))
                        .rounded(px(4.0 * s))
                        .bg(gpui::Hsla::from(rgb(APPROVAL_COLOR)).opacity(0.16))
                        .text_size(px(13.0 * s))
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
            .when(!branch.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(180.0 * s))
                        .truncate()
                        .text_size(px(13.0 * s))
                        .text_color(p.muted)
                        .child(branch),
                )
            });
        div()
            .id(gpui::SharedString::from(format!(
                "chat-coordinator-thread:{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("Open worker {title}"))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(row_height(row) * s))
            .px(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(13.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(line)
            .children(items)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(command.clone(), cx);
                this.close_pill_menu(cx);
            }))
            .into_any_element()
    }

    /// A row's second line: a pill per work item its thread is on (two, then "+N"), under its
    /// title. A pill's hover is the item's card; its click shows the item in the Work panel
    /// (`jumpToWorkCard`) instead of opening the thread. The pills, cards and order come from the
    /// core (gx-chat-core extras/coordinator_threads.rs `attach_items`, where the user's decision
    /// lives).
    fn items_line(
        &self,
        row: &Value,
        key: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let items = row["items"].as_array().filter(|items| !items.is_empty())?;
        let s = p.scale;
        let tint = super::markdown_links::board_tint(p).unwrap_or(p.muted);
        let pill = |id: String| {
            div()
                .id(gpui::SharedString::from(id))
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(4.0 * s))
                .h(px(ITEM_PILL_HEIGHT * s))
                .px(px(6.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .text_size(px(12.0 * s))
                .whitespace_nowrap()
                .overflow_hidden()
        };
        let mut line = div()
            .flex()
            .items_center()
            .gap(px(4.0 * s))
            .h(px(ITEMS_LINE_HEIGHT * s))
            .pb(px(3.0 * s))
            // Under the title, past the status dot's column.
            .pl(px(21.0 * s))
            .min_w_0()
            .overflow_hidden();
        for item in items {
            let reference = text(item, "ref");
            let card = text(item, "tooltip");
            let command = item["command"].clone();
            line = line.child(
                pill(format!("chat-coordinator-thread-item:{key}:{reference}"))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Show {reference} in Work"))
                    .max_w(px(ITEM_PILL_MAX * s))
                    .when(item["done"] == true, |pill| pill.opacity(0.55))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .tooltip(move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_style_card_tooltip(
                            card.clone(),
                            0.0,
                            0.0,
                            window,
                            cx,
                        )
                    })
                    .child(
                        gpui::svg()
                            .path("titlebar/layout-board-split.svg")
                            .flex_shrink_0()
                            .size(px(12.0 * s))
                            .text_color(tint),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child(reference),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(p.card_muted)
                            .child(text(item, "title")),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                        this.close_pill_menu(cx);
                    })),
            );
        }
        let more = text(row, "moreItems");
        if !more.is_empty() {
            let rest = text(row, "moreItemsTooltip");
            line = line.child(
                pill(format!("chat-coordinator-thread-items-more:{key}"))
                    .text_color(p.muted)
                    .tooltip(move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                            rest.clone(),
                            window,
                            cx,
                        )
                    })
                    .child(more),
            );
        }
        Some(line.into_any_element())
    }
}
