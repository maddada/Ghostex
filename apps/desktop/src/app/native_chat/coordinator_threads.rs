//! A coordinator's threads, in the floating menu the Workers pill opens: the running ones and those
//! that need the user under their own labels, then one Finished line that opens to the finished
//! ones, each row opening its thread. Order, labels and the steps come from the core
//! (packages/gx-chat-core/src/extras/coordinator_threads.rs, where the user's 2026-11-14 decision
//! on this layout lives); the pill is chat_pills.rs.
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
/// places, its approval red and its question pink.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of the Workers list: "please make the needs approval one red like the dot that's related to it". The row's dot takes the sidebar's approval red (the app's one error red, the same in dark and light), so the header's red dot and the row read as one thing. The phone's AgentPanels.tsx follows.
const WORKING_COLOR: u32 = 0xc68a06;
const APPROVAL_COLOR: u32 = crate::app::native_sidebar::status::APPROVAL_COLOR;
const QUESTION_COLOR: u32 = crate::app::native_sidebar::status::QUESTION_COLOR;

/// The list's pieces, in unscaled pixels: fixed heights, so the floating menu knows its size before
/// it opens (`threads_menu_height`).
const ROW_GAP: f32 = 2.0;
const LIST_MAX: f32 = 420.0;
/// "Running 2" and "Needs you 1" above their rows.
const GROUP_HEIGHT: f32 = 22.0;
/// The Finished line that folds and opens the finished rows.
const FOLD_HEIGHT: f32 = 26.0;
/// The quiet line when nothing is listed.
const EMPTY_HEIGHT: f32 = 28.0;
/// A row: its title line, then up to two lines of its last report, inside its padding.
const TITLE_LINE: f32 = 20.0;
const PREVIEW_LINE: f32 = 18.0;
const ROW_PAD_TOP: f32 = 4.0;
const ROW_PAD_BOTTOM: f32 = 5.0;
/// About how many characters fill one preview line at the menu's width: a longer preview gets its
/// second line. The line clamps, so a guess that runs short ends in an ellipsis instead of growing.
const PREVIEW_LINE_CHARS: usize = 68;

/// One piece of the list, in the order it draws.
enum Entry<'a> {
    Group(&'static str, usize),
    Row(&'a Value),
    Fold,
    Empty,
}

/// The list as the menu draws it: Running and Needs you with their rows, then the Finished line,
/// with the finished rows under it once it is open (`finishedStep` 0).
fn entries(panel: &Value) -> Vec<Entry<'_>> {
    let rows: Vec<&Value> = panel["rows"].as_array().into_iter().flatten().collect();
    let status = |row: &Value| text(row, "status");
    let running: Vec<&Value> = rows
        .iter()
        .copied()
        .filter(|row| status(row) == "working")
        .collect();
    let needs_you: Vec<&Value> = rows
        .iter()
        .copied()
        .filter(|row| matches!(status(row).as_str(), "approval" | "question"))
        .collect();
    // A row from an older core has no status: it reads finished.
    let finished: Vec<&Value> = rows
        .iter()
        .copied()
        .filter(|row| !matches!(status(row).as_str(), "working" | "approval" | "question"))
        .collect();
    let mut list = Vec::new();
    if !running.is_empty() {
        list.push(Entry::Group("Running", running.len()));
        list.extend(running.into_iter().map(Entry::Row));
    }
    if !needs_you.is_empty() {
        list.push(Entry::Group("Needs you", needs_you.len()));
        list.extend(needs_you.into_iter().map(Entry::Row));
    }
    if !text(panel, "finishedLabel").is_empty() {
        list.push(Entry::Fold);
        if panel["finishedStep"].as_u64() == Some(0) {
            list.extend(finished.into_iter().map(Entry::Row));
        }
    } else if list.is_empty() {
        list.extend(finished.into_iter().map(Entry::Row));
    }
    if list.is_empty() && !text(panel, "emptyLabel").is_empty() {
        list.push(Entry::Empty);
    }
    list
}

/// How many lines of its report a row shows.
fn preview_lines(row: &Value) -> usize {
    match text(row, "preview").chars().count() {
        0 => 0,
        count if count <= PREVIEW_LINE_CHARS => 1,
        _ => 2,
    }
}

fn row_height(row: &Value) -> f32 {
    ROW_PAD_TOP + TITLE_LINE + preview_lines(row) as f32 * PREVIEW_LINE + ROW_PAD_BOTTOM
}

fn entry_height(entry: &Entry<'_>) -> f32 {
    match entry {
        Entry::Group(..) => GROUP_HEIGHT,
        Entry::Row(row) => row_height(row),
        Entry::Fold => FOLD_HEIGHT,
        Entry::Empty => EMPTY_HEIGHT,
    }
}

/// The footer ("1216 more", "Show fewer") shows once the Finished line is open; folded, the line
/// itself is the way in.
fn footer_label(panel: &Value) -> String {
    let more = text(panel, "moreLabel");
    if panel["step"].as_u64().unwrap_or(0) == 0 && !text(panel, "finishedLabel").is_empty() {
        String::new()
    } else {
        more
    }
}

/// The Workers pill's menu: the header line, the list and the footer.
pub(super) fn threads_menu_height(panel: &Value) -> f32 {
    let list = entries(panel);
    let heights: f32 = list.iter().map(entry_height).sum();
    let list = (heights + list.len().saturating_sub(1) as f32 * ROW_GAP).min(LIST_MAX);
    let footer = if footer_label(panel).is_empty() {
        0.0
    } else {
        super::chat_pills::MENU_SECTION_GAP + super::chat_pills::MENU_FOOTER_HEIGHT
    };
    super::chat_pills::MENU_HEADER_HEIGHT + super::chat_pills::MENU_SECTION_GAP + list + footer
}

/// "now", "4m", "2h", "3d": how long ago the thread was last active.
fn age_label(active_at: &str) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(active_at) else {
        return String::new();
    };
    let minutes = chrono::Utc::now()
        .signed_duration_since(at)
        .num_minutes()
        .max(0);
    match minutes {
        0 => "now".to_string(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
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
        // An older core sends no `header`: its `meta` stands in.
        let header_text = match text(&panel, "header") {
            header if header.is_empty() => text(&panel, "meta"),
            header => header,
        };
        let header = super::chat_pills::pill_menu_header(
            "titlebar/users-group.svg",
            "Workers",
            header_text,
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
        let list = entries(&panel);
        let listed = !list.is_empty();
        let mut rows = div()
            .id("chat-coordinator-threads-rows")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(ROW_GAP * s))
            .max_h(px(LIST_MAX * s))
            .overflow_y_scroll();
        for entry in &list {
            rows = rows.child(match entry {
                Entry::Group(label, count) => group_label(label, *count, p),
                Entry::Row(row) => self.coordinator_thread_row(row, p, cx),
                Entry::Fold => self.finished_line(&panel, p, cx),
                Entry::Empty => div()
                    .h(px(EMPTY_HEIGHT * s))
                    .flex()
                    .items_center()
                    .px(px(4.0 * s))
                    .text_size(px(13.0 * s))
                    .text_color(p.card_muted)
                    .child(text(&panel, "emptyLabel"))
                    .into_any_element(),
            });
        }
        let more = footer_label(&panel);
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

    /// "› Finished  27 in the last 2 hours": folded, a click opens the finished rows under it;
    /// open, its chevron points down and a click folds them again.
    fn finished_line(&self, panel: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let step = panel["finishedStep"].as_u64().unwrap_or(0);
        let open = step == 0;
        div()
            .id("chat-coordinator-threads-finished")
            .role(gpui::Role::Button)
            .aria_label(if open {
                "Fold the finished workers"
            } else {
                "Show the finished workers"
            })
            .h(px(FOLD_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .px(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(13.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                gpui::svg()
                    .path(if open {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.card_muted)
                    .child("Finished"),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.muted)
                    .child(text(panel, "finishedLabel")),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(
                    json!({"type":"toggleCoordinatorThreadsMore","step":step}),
                    cx,
                )
            }))
            .into_any_element()
    }

    fn coordinator_thread_row(
        &self,
        row: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let dot = |color: u32| {
            div()
                .size(px(8.0 * s))
                .rounded_full()
                .bg(rgb(color))
                .into_any_element()
        };
        let marker = match text(row, "status").as_str() {
            "working" => dot(WORKING_COLOR),
            "approval" => dot(APPROVAL_COLOR),
            "question" => dot(QUESTION_COLOR),
            _ => gpui::svg()
                .path("titlebar/circle-check.svg")
                .size(px(12.0 * s))
                .text_color(p.muted)
                .into_any_element(),
        };
        let title = text(row, "title");
        let preview = text(row, "preview");
        let branch = text(row, "branch");
        let age = age_label(&text(row, "activeAt"));
        // A worker on another computer than the coordinator's: its server, which the click opens
        // it on, and its name, which the tooltip says (the title shows the computer's icon).
        let computer = text(row, "computer");
        let tooltip = [title.as_str(), computer.as_str(), branch.as_str()]
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
        // The listed session's logo, else the agent gxserver names (`agentLogo`, Q3216); the
        // generic agent icon only when neither is known.
        let logo = match row["agentLogo"].as_str() {
            Some(logo) if !logo.is_empty() => logo.to_string(),
            _ => text(row, "agentIcon"),
        };
        let (icon, icon_color) =
            super::markdown_links::session_pill_icon(&logo, row["coordinator"] == true, p);
        let lines = preview_lines(row);
        let title_line = div()
            .id(gpui::SharedString::from(format!(
                "chat-coordinator-thread-line:{key}"
            )))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .min_w_0()
            .h(px(TITLE_LINE * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(p.foreground)
                    .child(title.clone()),
            )
            .when(!computer.is_empty(), |line| {
                line.child(
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
            .children(self.item_refs(row, &key, p, cx))
            .child(div().flex_1().min_w(px(8.0 * s)))
            .when(!branch.is_empty(), |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(140.0 * s))
                        .truncate()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(branch),
                )
            })
            .when(!age.is_empty(), |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(age),
                )
            });
        div()
            .id(gpui::SharedString::from(format!(
                "chat-coordinator-thread:{key}"
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("Open worker {title}"))
            .flex()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .gap(px(8.0 * s))
            .h(px(row_height(row) * s))
            .pt(px(ROW_PAD_TOP * s))
            .pb(px(ROW_PAD_BOTTOM * s))
            .px(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(13.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .child(
                div()
                    .size(px(13.0 * s))
                    .mt(px(3.5 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(marker),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(title_line)
                    .when(lines > 0, |column| {
                        column.child(
                            div()
                                .min_w_0()
                                .line_height(px(PREVIEW_LINE * s))
                                .max_h(px(lines as f32 * PREVIEW_LINE * s))
                                .overflow_hidden()
                                .line_clamp(lines)
                                .text_ellipsis()
                                .text_color(p.card_muted)
                                .child(preview),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.invoke(command.clone(), cx);
                this.close_pill_menu(cx);
            }))
            .into_any_element()
    }

    /// The work items the thread is on, after its title: the Work panel's board icon and the item's
    /// ref ("W461"), two then "+N". Hovering a ref shows the item's card and a click shows it in the
    /// Work panel (`jumpToWorkCard`) instead of opening the thread, as the pills did. The refs,
    /// cards and order come from the core (gx-chat-core extras/coordinator_threads.rs
    /// `attach_items`).
    fn item_refs(
        &self,
        row: &Value,
        key: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let tint = super::markdown_links::board_tint(p).unwrap_or(p.muted);
        let mut refs = Vec::new();
        for item in row["items"].as_array().into_iter().flatten() {
            let reference = text(item, "ref");
            let card = text(item, "tooltip");
            let command = item["command"].clone();
            refs.push(
                div()
                    .id(gpui::SharedString::from(format!(
                        "chat-coordinator-thread-item:{key}:{reference}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Show {reference} in Work"))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(3.0 * s))
                    .px(px(2.0 * s))
                    .rounded(px(4.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .when(item["done"] == true, |item| item.opacity(0.55))
                    .chat_cursor_pointer()
                    .hover(|style| {
                        style
                            .text_color(p.foreground)
                            .bg(p.foreground.opacity(0.08))
                    })
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
                    .child(reference)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                        this.close_pill_menu(cx);
                    }))
                    .into_any_element(),
            );
        }
        let more = text(row, "moreItems");
        if !more.is_empty() {
            let rest = text(row, "moreItemsTooltip");
            refs.push(
                div()
                    .id(gpui::SharedString::from(format!(
                        "chat-coordinator-thread-items-more:{key}"
                    )))
                    .flex_shrink_0()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .tooltip(move |window, cx| {
                        crate::app::native_sidebar::tooltips::sidebar_style_tooltip(
                            rest.clone(),
                            window,
                            cx,
                        )
                    })
                    .child(more)
                    .into_any_element(),
            );
        }
        refs
    }
}

/// "Running 2" above the running rows, "Needs you 1" above those waiting on the user.
fn group_label(label: &'static str, count: usize, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .h(px(GROUP_HEIGHT * s))
        .flex()
        .items_end()
        .gap(px(6.0 * s))
        .px(px(4.0 * s))
        .pb(px(3.0 * s))
        .text_size(px(12.0 * s))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(p.muted)
        .child(label)
        .child(div().text_color(p.card_muted).child(count.to_string()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Entry, entries, threads_menu_height};
    use serde_json::json;

    fn shape(panel: &serde_json::Value) -> Vec<String> {
        entries(panel)
            .iter()
            .map(|entry| match entry {
                Entry::Group(label, count) => format!("{label} {count}"),
                Entry::Row(row) => row["key"].as_str().unwrap_or_default().to_string(),
                Entry::Fold => "fold".to_string(),
                Entry::Empty => "empty".to_string(),
            })
            .collect()
    }

    /// Running and Needs you list their rows under their labels; the finished rows show only
    /// while the Finished line is open, and the footer only then.
    #[test]
    fn live_rows_first_and_finished_folded() {
        let rows = json!([
            {"key": "a", "status": "working", "preview": "short"},
            {"key": "b", "status": "approval"},
            {"key": "c", "status": "done", "preview": "x".repeat(100)},
        ]);
        let folded = json!({"rows": rows, "step": 0, "finishedLabel": "1 in the last 2 hours",
            "finishedStep": 1, "moreLabel": "Show recent"});
        assert_eq!(
            shape(&folded),
            ["Running 1", "a", "Needs you 1", "b", "fold"]
        );
        let open = json!({"rows": rows, "step": 1, "finishedLabel": "1 in the last 2 hours",
            "finishedStep": 0, "moreLabel": "40 more"});
        assert_eq!(
            shape(&open),
            ["Running 1", "a", "Needs you 1", "b", "fold", "c"]
        );
        // The open list adds the long row (two preview lines) and the footer.
        let grown = threads_menu_height(&open) - threads_menu_height(&folded);
        assert_eq!(grown, 2.0 + (4.0 + 20.0 + 36.0 + 5.0) + 4.0 + 26.0);
        let quiet = json!({"rows": [], "emptyLabel": "No workers working"});
        assert_eq!(shape(&quiet), ["empty"]);
    }
}
