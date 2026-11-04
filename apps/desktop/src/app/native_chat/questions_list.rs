//! The Resources pill's list (chat_pills.rs draws the pill; a click opens it, a second click or
//! Escape closes it, and the ? pill never opens it): every question form, page and chart in the
//! chat, under the All / Html / Q&A / Charts filter (a small dropdown on the header's line), in
//! folding sections (Unanswered, For later, Answered and Withdrawn for questions, then Pages and
//! Charts), each row jumping to its card. The sections, their rows and their order are the core's
//! (`openQuestions.sections`,
//! packages/gx-chat-core/src/questions/open_questions.rs, where the user's decision lives); this
//! folds its sections and draws it in the pills' menu window
//! (`option_menu/pill_menu.rs`), in the Links card's section look (native_sidebar/links_list.rs).
//!
//! CDXC:SessionChat 2026-10-27 SEE-ALSO:
//! The phone draws the same list in apps/mobile/app/src/chat/native/cards/ChatPills.tsx (`QuestionsMenu`). Change them together.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontFeatures, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, svg,
};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The list's id among the pill menus, and its trigger's.
pub(super) const QUESTIONS_MENU: &str = "resources";
pub(super) const QUESTIONS_TRIGGER: &str = "chat-pill-resources";
pub(super) const QUESTIONS_MENU_WIDTH: f32 = 440.0;

/// One section header, one row and the list's cap, unscaled: fixed, so the menu knows its height
/// before it opens.
const SECTION_HEIGHT: f32 = 22.0;
const ROW_HEIGHT: f32 = 26.0;
const LIST_MAX: f32 = 380.0;

/// How often an open list redraws so its ages stay current.
const AGE_REFRESH: std::time::Duration = std::time::Duration::from_secs(30);

/// The groups that start folded: none (the list has two, Unanswered and Earlier).
fn folded_by_default(_section: &str) -> bool {
    false
}

/// A row's hover tooltip: its title and the full date and time it was asked. No Q numbers
/// anywhere in the list (user, 2026-10-28: "lets also not show the Q numbers in the question
/// tooltip when hovering on them").
fn row_tooltip(row: &Value) -> String {
    let thread = text(row, "thread");
    let posted_in = if thread.is_empty() {
        String::new()
    } else {
        format!("Posted by {thread}")
    };
    [text(row, "title"), posted_in, text(row, "timeTitle")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// How long ago a row was asked, worded as of `now_ms` from its `atMs`; the document's own `age`
/// when the row carries no stamp (it was worded when the document was built).
fn row_age(row: &Value, now_ms: i64) -> String {
    match row["atMs"].as_i64() {
        Some(at) => ghostex_gx_chat_core::transcript::message_time::age_label(now_ms, at),
        None => text(row, "age"),
    }
}

/// The button a row offers: "For later" on a form or page still waiting, "Back to open" on one
/// put off. It sends the action the question cards' own button sends (`questionForLater`), so
/// the cards and the list agree. `status` is the row's (gx-chat-core `QuestionListRow::status`);
/// other rows (answered, withdrawn, pages and charts that ask nothing) offer none.
fn later_action(status: &str, kind: &str, id: &str) -> Option<(&'static str, Value)> {
    if kind == "chart" {
        return None;
    }
    match status {
        "waiting" => Some((
            "For later",
            json!({"type": "questionForLater", "id": id, "later": true}),
        )),
        "later" => Some((
            "Back to open",
            json!({"type": "questionForLater", "id": id, "later": false}),
        )),
        _ => None,
    }
}

/// The word a row carries before its age: "For later" on a put-off form or page, "Canceled" on
/// one the reader removed without answering.
fn status_label(status: &str) -> Option<&'static str> {
    match status {
        "later" => Some("For later"),
        "canceled" => Some("Canceled"),
        _ => None,
    }
}

/// The second button of a For later row: removes the question without answering it
/// (`questionRemove`, gx-chat-core questions/later.rs). Only a put-off form or page gets it.
fn remove_action(status: &str, kind: &str, id: &str) -> Option<(&'static str, Value)> {
    (status == "later" && kind != "chart").then(|| {
        (
            "Remove without answering",
            json!({"type": "questionRemove", "id": id}),
        )
    })
}

/// A row's hover button is this wide, with this gap between two of them.
const BUTTON_SIZE: f32 = 20.0;
const BUTTON_GAP: f32 = 2.0;

/// The list's folding, per chat.
#[derive(Default)]
pub(super) struct QuestionList {
    /// The sections the reader folded or opened against their default.
    toggled: HashSet<String>,
    /// "Show more" was clicked, so scrolling to the bottom adds the next page by itself; cleared
    /// when the list closes or its filter changes.
    auto_more: std::cell::Cell<bool>,
    /// The filter the list was last drawn under, which `auto_more` belongs to.
    auto_filter: std::cell::RefCell<String>,
    /// The `remaining` an automatic page was last asked at, so one stay at the bottom asks once.
    auto_asked_at: std::cell::Cell<Option<u64>>,
    /// The list's scroll position, which the bottom check reads.
    scroll: gpui::ScrollHandle,
    /// Redraws the open list every 30 s so the ages on its rows keep up (dropped with the list).
    tick: Option<gpui::Task<()>>,
    /// The filter dropdown's trigger, in screen space, which its pick list opens below.
    filter_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
}

impl QuestionList {
    fn folded(&self, section: &str) -> bool {
        folded_by_default(section) ^ self.toggled.contains(section)
    }
}

/// The list's content height (unscaled) for `open` (the document's
/// `openQuestions`), or `None` when the chat has nothing to list.
pub(super) fn questions_menu_height(open: &Value, list: &QuestionList) -> Option<f32> {
    if open["total"].as_u64().unwrap_or(0) == 0 {
        return None;
    }
    let sections = open["sections"].as_array().cloned().unwrap_or_default();
    if sections.is_empty() {
        // The picked filter has nothing: one line says so.
        return Some(
            super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP
                + ROW_HEIGHT,
        );
    }
    // Each group's own "Show more" row sits under its rows while the core cut some, and a folded
    // group shows none.
    let rows: f32 = sections
        .iter()
        .map(|section| {
            let shown = if list.folded(&text(section, "id")) {
                0
            } else {
                section["rows"].as_array().map_or(0, Vec::len)
            };
            // A group with no title (Earlier, alone) has no header line.
            let header = if text(section, "title").is_empty() {
                0.0
            } else {
                SECTION_HEIGHT
            };
            let more = if !list.folded(&text(section, "id"))
                && section["remaining"].as_u64().unwrap_or(0) > 0
            {
                ROW_HEIGHT
            } else {
                0.0
            };
            more + header + shown as f32 * ROW_HEIGHT
        })
        .sum::<f32>();
    Some(
        super::chat_pills::MENU_HEADER_HEIGHT
            + super::chat_pills::MENU_SECTION_GAP
            + rows.min(LIST_MAX),
    )
}

/// The glyph of a row's For later button: a clock to put it off, the way back once it is.
fn later_icon(status: &str) -> &'static str {
    if status == "later" {
        "titlebar/arrow-back-up.svg"
    } else {
        "titlebar/clock.svg"
    }
}

/// What a row is, by its icon: an HTML page, a Q&A form or a chart.
fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "page" => "titlebar/file-type-html.svg",
        "chart" => "titlebar/chart-bar.svg",
        _ => "titlebar/list-check.svg",
    }
}

impl NativeChatView {
    /// While the list is open, redraws it every 30 s: the ages on its rows are worded from the
    /// clock, not from a document change.
    pub(super) fn keep_question_ages_current(&mut self, cx: &mut Context<Self>) {
        self.question_list.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AGE_REFRESH).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }

    /// The list went away: its refresh stops.
    pub(super) fn questions_menu_closed(&mut self, cx: &mut Context<Self>) {
        self.question_list.tick = None;
        self.question_list.auto_more.set(false);
        self.question_list.auto_asked_at.set(None);
        // Reopening starts again from one page (the core's `questionListReset`).
        self.invoke(json!({"type": "questionListReset"}), cx);
    }

    fn toggle_question_section(&mut self, section: String, cx: &mut Context<Self>) {
        if !self.question_list.toggled.remove(&section) {
            self.question_list.toggled.insert(section);
        }
        cx.notify();
    }

    /// Where a form or page stands in the list (`unanswered`, `later`, or its group's id): the
    /// question cards read it to offer For later or Back to unanswered.
    pub(super) fn question_section_of(&self, id: &str) -> Option<String> {
        self.snapshot["openQuestions"]["sections"]
            .as_array()?
            .iter()
            .flat_map(|section| {
                section["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(move |row| (section, row))
            })
            .find(|(_, row)| row["id"] == id)
            .map(|(section, row)| match text(row, "status").as_str() {
                "waiting" => "unanswered".to_string(),
                "later" => "later".to_string(),
                "canceled" => "canceled".to_string(),
                _ => text(section, "id"),
            })
    }

    pub(super) fn questions_menu_height(&self) -> Option<f32> {
        questions_menu_height(&self.snapshot["openQuestions"], &self.question_list)
    }

    /// The filter's small dropdown at the header's right end: the current pick and a chevron. It
    /// opens the chat menus' own pick list below it (the one a panel's select opens), with each
    /// filter's count.
    ///
    /// CDXC:SessionChat 2026-10-27 DECISION:
    /// User, of Chat Resources: "remove pill on the right, the icon on the left is enough indicator. and instead of tabs up top just do a small dropdown (should be top right of the pop up aligned with the header there". The All / Html / Q&A / Charts tabs became this dropdown on the header's line, and rows carry only their kind icon (no HTML / Q&A / Chart label). The phone does the same in ChatPills.tsx.
    fn question_list_filter_trigger(
        &self,
        open: &Value,
        p: &ChatAppearance,
        menu_origin: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let filters = open["filters"].as_array().cloned().unwrap_or_default();
        let picked = text(open, "filter");
        let label = filters
            .iter()
            .find(|filter| text(filter, "id") == picked)
            .map(|filter| text(filter, "label"))
            .unwrap_or_else(|| "All".to_string());
        let bounds = self.question_list.filter_bounds.clone();
        let measured = bounds.clone();
        div()
            .id("chat-questions-filter")
            .role(gpui::Role::ComboBox)
            .aria_label(format!("Show: {label}"))
            .flex_shrink_0()
            .relative()
            .flex()
            .items_center()
            .gap(px(4.0 * s))
            .h(px(20.0 * s))
            .px(px(6.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(12.0 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| {
                style
                    .bg(p.foreground.opacity(0.06))
                    .text_color(p.foreground)
            })
            .child(label)
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .size(px(11.0 * s))
                    .text_color(p.card_muted),
            )
            .child(
                gpui::canvas(
                    move |rect: gpui::Bounds<gpui::Pixels>, _, _| {
                        measured.set(gpui::Bounds::new(menu_origin + rect.origin, rect.size))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.open_question_list_filters(cx)
            }))
            .into_any_element()
    }

    /// The filter's pick list, below its trigger with their right edges lined up, so it opens
    /// leftwards inside the popup. Picking keeps the list up.
    fn open_question_list_filters(&mut self, cx: &mut Context<Self>) {
        let open = &self.snapshot["openQuestions"];
        let picked = text(open, "filter");
        let rows: Vec<Value> = open["filters"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|filter| {
                json!({
                    "label": filter["label"],
                    "detail": filter["count"].as_u64().unwrap_or(0).to_string(),
                    "checked": text(filter, "id") == picked,
                    "keepOpen": true,
                    "command": {"type": "questionListFilter", "filter": filter["id"]},
                })
            })
            .collect();
        let anchor = self.question_list.filter_bounds.get();
        if let Some(menu) = self.option_menu.clone() {
            menu.update(cx, |menu, cx| {
                menu.open_select_list_right_aligned(rows, anchor, 160.0, 1, cx)
            });
        }
    }

    /// The list, drawn in the menu's window with this view's listeners.
    pub(super) fn questions_menu_body(
        &self,
        p: &ChatAppearance,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.snapshot["openQuestions"].clone();
        if open["total"].as_u64().unwrap_or(0) == 0 {
            return None;
        }
        let sections = open["sections"].as_array().cloned().unwrap_or_default();
        let s = p.scale;
        let trigger = self.question_list_filter_trigger(&open, p, window.bounds().origin, cx);
        let header = super::chat_pills::pill_menu_header(
            "titlebar/question-mark.svg",
            "Chat Resources",
            text(&open, "summary"),
            Some(trigger),
            p,
        );
        // The rows still to come in the groups that are open: what scrolling to the bottom loads.
        let remaining: u64 = sections
            .iter()
            .filter(|section| !self.question_list.folded(&text(section, "id")))
            .map(|section| section["remaining"].as_u64().unwrap_or(0))
            .sum();
        // A new filter starts again from the top and from one page.
        let filter = text(&open, "filter");
        if *self.question_list.auto_filter.borrow() != filter {
            *self.question_list.auto_filter.borrow_mut() = filter;
            self.question_list.auto_more.set(false);
            self.question_list.auto_asked_at.set(None);
            self.question_list
                .scroll
                .set_offset(gpui::point(px(0.0), px(0.0)));
        }
        if self.question_list.auto_more.get()
            && remaining > 0
            && self.question_list.auto_asked_at.get() != Some(remaining)
            && self.question_list_at_bottom()
        {
            // Scrolled to the bottom after a Show more: the next page comes by itself.
            self.question_list.auto_asked_at.set(Some(remaining));
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(json!({"type": "questionListMore"}), cx)
                });
            })
            .detach();
        }
        let mut list = div()
            .id("chat-questions-list")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .max_h(px(LIST_MAX * s))
            .overflow_y_scroll()
            .track_scroll(&self.question_list.scroll)
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()));
        if sections.is_empty() {
            list = list.child(
                div()
                    .h(px(ROW_HEIGHT * s))
                    .flex()
                    .items_center()
                    .px(px(4.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child("Nothing of this kind in this chat yet."),
            );
        }
        for section in &sections {
            let id = text(section, "id");
            let folded = self.question_list.folded(&id);
            let headed = !text(section, "title").is_empty();
            if headed {
                list = list.child(self.question_section_header(section, folded, p, cx));
            }
            if !folded {
                for row in section["rows"].as_array().into_iter().flatten() {
                    list = list.child(self.question_list_row(headed, row, p, cx));
                }
                // This group's own Show more, under its own rows.
                let left = section["remaining"].as_u64().unwrap_or(0);
                if left > 0 {
                    list = list.child(self.question_show_more_row(&id, left, p, cx));
                }
            }
        }
        Some(
            div()
                .id("chat-questions-menu")
                .w_full()
                .child(super::chat_pills::pill_menu_column(
                    "chat-questions",
                    "Chat Resources",
                    header,
                    Some(list.into_any_element()),
                    None,
                    p,
                ))
                .into_any_element(),
        )
    }

    /// Whether the list is scrolled to its end (and long enough to scroll at all).
    fn question_list_at_bottom(&self) -> bool {
        let scroll = &self.question_list.scroll;
        let max = scroll.max_offset().y;
        max > px(0.0) && scroll.offset().y <= -(max - px(2.0))
    }

    /// The list's last row while the core left rows out: a click adds a page, and from then on
    /// scrolling to the bottom adds the next ones by itself.
    fn question_show_more_row(
        &self,
        group: &str,
        remaining: u64,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(SharedString::from(format!(
                "chat-questions-show-more:{group}"
            )))
            .role(gpui::Role::Button)
            .aria_label(format!("Show more, {remaining} left"))
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(12.0 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| {
                style
                    .bg(p.foreground.opacity(0.06))
                    .text_color(p.foreground)
            })
            .child("Show more")
            .child(
                div()
                    .text_color(p.muted)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .child(remaining.to_string()),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.question_list.auto_more.set(true);
                this.question_list.auto_asked_at.set(None);
                this.invoke(json!({"type": "questionListMore"}), cx);
            }))
            .into_any_element()
    }

    /// A section's header: the chevron, its title and its count, folding the section on a click.
    fn question_section_header(
        &self,
        section: &Value,
        folded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(section, "id");
        let title = text(section, "title");
        let count = section["count"].as_u64().unwrap_or(0);
        let toggled = id.clone();
        div()
            .id(SharedString::from(format!("chat-questions-section:{id}")))
            .role(gpui::Role::Button)
            .aria_label(format!("{title} {count}"))
            .aria_expanded(!folded)
            .flex_shrink_0()
            .h(px(SECTION_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .px(px(4.0 * s))
            .text_size(px(11.0 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| style.text_color(p.foreground))
            .child(
                svg()
                    .path(if folded {
                        "titlebar/chevron-right.svg"
                    } else {
                        "titlebar/chevron-down.svg"
                    })
                    .size(px(11.0 * s))
                    .flex_shrink_0()
                    .text_color(p.card_muted),
            )
            .child(title)
            .child(count.to_string())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_question_section(toggled.clone(), cx)
            }))
            .into_any_element()
    }

    /// One form or page: what it is, its title and when it was asked (its tags are only in the
    /// hover tooltip). A click scrolls the transcript to its card and flashes it.
    /// A row's small icon button (shown while the pointer is on the row) that puts it off For later
    /// or back to open. Its press stops here, so it never also jumps to the question.
    #[allow(clippy::too_many_arguments)]
    fn later_row_button(
        &self,
        id: &str,
        label: &'static str,
        action: Value,
        group: SharedString,
        icon: &'static str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(SharedString::from(format!(
                "chat-questions-later:{id}:{label}"
            )))
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_shrink_0()
            .size(px(BUTTON_SIZE * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0 * s))
            .opacity(0.0)
            .group_hover(group, |this| this.opacity(1.0))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.1)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(svg().path(icon).size(px(13.0 * s)).text_color(p.card_muted))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.invoke(action.clone(), cx);
            }))
            .into_any_element()
    }

    /// A row's right end: "For later" (a put-off row) and its age, right-aligned like every row's.
    /// While the pointer is on the row its buttons take that spot instead, so nothing is reserved
    /// for them at rest.
    #[allow(clippy::too_many_arguments)]
    fn row_trailing(
        &self,
        id: &str,
        status: &str,
        age: &str,
        buttons: Vec<(&'static str, Value, &'static str)>,
        group: SharedString,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let n = buttons.len();
        let slot = if n == 0 {
            0.0
        } else {
            n as f32 * BUTTON_SIZE + (n - 1) as f32 * BUTTON_GAP
        };
        let hide_for_buttons = n > 0;
        div()
            .relative()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0 * s))
            .min_w(px(slot * s))
            .h_full()
            .when_some(status_label(status), |this, label| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * s))
                        .text_color(p.card_muted)
                        .when(hide_for_buttons, |this| {
                            this.group_hover(group.clone(), |style| style.opacity(0.0))
                        })
                        .child(label),
                )
            })
            .when(!age.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * s))
                        .text_color(p.muted)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .when(hide_for_buttons, |this| {
                            this.group_hover(group.clone(), |style| style.opacity(0.0))
                        })
                        .child(age.to_string()),
                )
            })
            .when(n > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .flex()
                        .items_center()
                        .gap(px(BUTTON_GAP * s))
                        .children(buttons.into_iter().map(|(label, action, icon)| {
                            self.later_row_button(id, label, action, group.clone(), icon, p, cx)
                        })),
                )
            })
            .into_any_element()
    }

    fn question_list_row(
        &self,
        headed: bool,
        row: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(row, "id");
        let title = text(row, "title");
        let age = row_age(row, crate::app::gx_chat::now_millis());
        let kind = text(row, "kind");
        let status = text(row, "status");
        let mut buttons: Vec<(&'static str, Value, &'static str)> = Vec::new();
        if let Some((label, action)) = later_action(&status, &kind, &id) {
            buttons.push((label, action, later_icon(&status)));
        }
        if let Some((label, action)) = remove_action(&status, &kind, &id) {
            buttons.push((label, action, "titlebar/x.svg"));
        }
        let icon = kind_icon(&kind);
        let group = SharedString::from(format!("chat-questions-row-group:{id}"));
        // Answered forms and pages are green, withdrawn ones dimmed; the rest are the usual grey.
        let (icon_color, title_color) = match status.as_str() {
            "answered" => (p.success(), p.foreground),
            "withdrawn" | "canceled" => (p.muted.opacity(0.6), p.card_muted),
            _ => (p.card_muted, p.foreground),
        };
        let tooltip = row_tooltip(row);
        let thread = text(row, "thread");
        let jump = id.clone();
        let opens_in_modal = matches!(kind.as_str(), "form" | "chart");
        div()
            .id(SharedString::from(format!("chat-questions-row:{id}")))
            .group(group.clone())
            .role(gpui::Role::Button)
            .aria_label(format!("Go to {title}"))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(ROW_HEIGHT * s))
            .pl(px(if headed { 20.0 } else { 6.0 } * s))
            .pr(px(4.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(12.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.06)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(title_color)
                    .child(title),
            )
            // A page a thread posted names the thread (a coordinator's Thread pages group).
            .when(!thread.is_empty(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(140.0 * s))
                        .truncate()
                        .text_size(px(11.0 * s))
                        .text_color(p.card_muted)
                        .child(thread),
                )
            })
            .child(self.row_trailing(&id, &status, &age, buttons, group, p, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                // A form or chart opens in the floating modal and the chat stays where it is; a
                // page opens as it always did.
                let action = if opens_in_modal {
                    json!({"type": "openResource", "id": jump})
                } else {
                    json!({"type": "jumpToQuestion", "id": jump})
                };
                this.invoke(action, cx);
                this.close_pill_menu(cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_has_the_title_and_full_time_but_no_q_numbers() {
        let row = json!({
            "title": "Run 4, part 3 of 3",
            "tags": ["Q633", "Q634"],
            "time": "1:57 AM",
            "timeTitle": "1:57 AM, 28th October 2026",
        });
        let tooltip = row_tooltip(&row);
        assert_eq!(tooltip, "Run 4, part 3 of 3\n1:57 AM, 28th October 2026");
        assert!(!tooltip.contains("Q633"));
    }

    #[test]
    fn a_worker_pages_tooltip_names_its_worker() {
        let row =
            json!({"title": "Layout notes", "thread": "Fix the sidebar", "timeTitle": "1:57 AM"});
        assert_eq!(
            row_tooltip(&row),
            "Layout notes\nPosted by Fix the sidebar\n1:57 AM"
        );
    }

    #[test]
    fn tooltip_skips_empty_parts() {
        let row = json!({"title": "Page", "tags": [], "time": "", "timeTitle": ""});
        assert_eq!(row_tooltip(&row), "Page");
    }

    #[test]
    fn the_age_is_worded_from_the_stamp_as_the_clock_moves() {
        let row = json!({"atMs": 1_000_000, "age": "stale", "time": "1:57 AM"});
        assert_eq!(row_age(&row, 1_000_000 + 30_000), "now");
        assert_eq!(row_age(&row, 1_000_000 + 4 * 60_000), "4m");
        assert_eq!(row_age(&row, 1_000_000 + 2 * 3_600_000), "2h");
        assert_eq!(row_age(&row, 1_000_000 + 30 * 3_600_000), "yesterday");
        assert_eq!(row_age(&row, 1_000_000 + 3 * 86_400_000), "3d");
        let unstamped = json!({"age": "5m"});
        assert_eq!(row_age(&unstamped, 9_999_999_999), "5m");
        assert_eq!(row_age(&json!({}), 5), "");
    }

    #[test]
    fn the_for_later_button_toggles_by_section() {
        let (label, action) = later_action("waiting", "form", "f1").unwrap();
        assert_eq!(label, "For later");
        assert_eq!(
            action,
            json!({"type": "questionForLater", "id": "f1", "later": true})
        );
        let (label, action) = later_action("later", "page", "p1").unwrap();
        assert_eq!(label, "Back to open");
        assert_eq!(
            action,
            json!({"type": "questionForLater", "id": "p1", "later": false})
        );
        for status in ["answered", "withdrawn", "other"] {
            assert!(later_action(status, "form", "x").is_none(), "{status}");
        }
        assert!(later_action("waiting", "chart", "c").is_none());
        assert_eq!(later_icon("later"), "titlebar/arrow-back-up.svg");
        assert_eq!(later_icon("waiting"), "titlebar/clock.svg");
    }

    #[test]
    fn put_off_and_canceled_rows_are_labelled_and_a_canceled_row_has_no_buttons() {
        assert_eq!(status_label("later"), Some("For later"));
        assert_eq!(status_label("canceled"), Some("Canceled"));
        for status in ["waiting", "answered", "withdrawn", "other"] {
            assert_eq!(status_label(status), None, "{status}");
        }
        assert!(later_action("canceled", "form", "f").is_none());
        assert!(remove_action("canceled", "form", "f").is_none());
    }

    #[test]
    fn only_a_for_later_row_offers_remove() {
        let (label, action) = remove_action("later", "form", "f1").unwrap();
        assert_eq!(label, "Remove without answering");
        assert_eq!(action, json!({"type": "questionRemove", "id": "f1"}));
        for status in ["waiting", "answered", "withdrawn", "other"] {
            assert!(remove_action(status, "form", "x").is_none(), "{status}");
        }
        assert!(remove_action("later", "chart", "c").is_none());
    }

    #[test]
    fn a_groups_show_more_row_counts_in_its_height_only_while_it_is_open() {
        let whole = json!({"id": "pages", "rows": [{"id": "a"}], "count": 30, "remaining": 0});
        let cut = json!({"id": "pages", "rows": [{"id": "a"}], "count": 30, "remaining": 29});
        let other = json!({"id": "threads", "title": "Worker pages", "rows": [{"id": "w"}], "count": 1, "remaining": 0});
        let mut list = QuestionList::default();
        let plain = json!({"total": 31, "sections": [whole, other.clone()]});
        let with_more = json!({"total": 31, "sections": [cut.clone(), other]});
        let base = questions_menu_height(&plain, &list).unwrap();
        assert_eq!(
            questions_menu_height(&with_more, &list).unwrap(),
            base + ROW_HEIGHT
        );
        // Folded: the group's rows and its Show more both go (the 401 under another group).
        list.toggled.insert("pages".to_string());
        let folded = questions_menu_height(&with_more, &list).unwrap();
        let folded_plain = questions_menu_height(&plain, &list).unwrap();
        assert_eq!(folded, folded_plain, "a folded group shows no Show more");
        // A long page is capped at the list's maximum, Show more row or not.
        let rows: Vec<Value> = (0..15).map(|i| json!({"id": i})).collect();
        let long = json!({"total": 90, "sections": [{"id": "answered", "rows": rows, "count": 80, "remaining": 65}]});
        let capped = questions_menu_height(&long, &list).unwrap();
        assert_eq!(
            capped,
            super::super::chat_pills::MENU_HEADER_HEIGHT
                + super::super::chat_pills::MENU_SECTION_GAP
                + LIST_MAX
        );
    }

    #[test]
    fn a_group_without_a_title_has_no_header_line() {
        let list = QuestionList::default();
        let row = json!({"id": "a"});
        let headed = json!({"total": 1, "remaining": 0,
            "sections": [{"id": "earlier", "title": "Earlier", "rows": [row.clone()], "count": 1}]});
        let bare = json!({"total": 1, "remaining": 0,
            "sections": [{"id": "earlier", "title": "", "rows": [row], "count": 1}]});
        assert_eq!(
            questions_menu_height(&headed, &list).unwrap()
                - questions_menu_height(&bare, &list).unwrap(),
            SECTION_HEIGHT
        );
    }
}
