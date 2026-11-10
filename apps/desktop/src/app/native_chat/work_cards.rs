//! A coordinator's work item cards under its replies (`workCards` in the chat core's document,
//! gx-chat-core extras/work_cards.rs, which holds the user's decision): a card per item the turn
//! changed, with its status, where it stands, who has the ball and how long ago, and a card per
//! ask with its answer buttons; an item's earlier cards and settled asks fold to one quiet line.
//! The phone draws the same cards in apps/mobile/app/src/chat/native/cards/WorkCards.tsx.
//!
//! Titles are the chat's body text (its prose size and Text strength); second lines, folds and the
//! Moving row's names are its muted text at the same size, so the cards read as the chat does and
//! follow Chat zoom and Text strength live.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, on the opened Moving list: "the font size must match the font size of the chat here please
//! and same text strength should apply". Chips and the status and age labels stay small: they
//! are labels, not text to read.
//!
//! The buttons are neutral fills, never the dark primary fill, because the card sits on the chat's
//! glass (the user's standing rule: no black fills on glass).

use super::appearance::ChatFilledButton as _;
use super::chat_text_look::{PROSE_LINE_HEIGHT, PROSE_SIZE};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// A status's glyph and colour, the board's (coordinator_board.rs `group_look`), so a card reads as
/// its row in the Work panel does.
pub(super) fn status_look(status: &str, p: &ChatAppearance) -> (&'static str, gpui::Hsla) {
    let group = match status {
        "waiting" => "hold",
        "finished" => "done",
        other => other,
    };
    if status == "dropped" {
        return ("titlebar/circle-x.svg", p.muted);
    }
    let (icon, color) = super::coordinator_board::group_look(group);
    (icon, color.map_or(p.muted, |color| gpui::rgb(color).into()))
}

/// A chip's glyph by what it says.
/// The dock's cards stop growing here (unscaled) and scroll inside it.
const DOCK_MAX_HEIGHT: f32 = 360.0;
/// A dock row's metrics (unscaled): the rows' inset in the dock, a row's side padding, the gap
/// between its parts, and the glyph with its gap that leads every line.
const DOCK_LIST_PAD: f32 = 4.0;
const DOCK_ROW_PAD_X: f32 = 8.0;
const DOCK_GAP: f32 = 10.0;
const DOCK_LEAD: f32 = 23.0;
/// The rows' width before the dock has painted once (the chat box's own width).
const DOCK_ROW_WIDTH_GUESS: f32 = 736.0;
/// How many lines an opened row's question shows before its "more".
const DOCK_QUESTION_LINES: usize = 4;

/// How wide `label` is in the chat's font at `size` (both unscaled).
fn dock_text_width(label: &str, size: f32, p: &ChatAppearance, cx: &gpui::App) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    let system = gpui::WindowTextSystem::new(cx.text_system().clone());
    let run = gpui::TextRun {
        len: label.len(),
        font: gpui::Font {
            weight: gpui::FontWeight::MEDIUM,
            ..gpui::font(p.font.clone())
        },
        ..Default::default()
    };
    system
        .shape_line(label.to_owned().into(), px(size * p.scale), &[run], None)
        .width
        .as_f32()
        / p.scale
}

/// One answer button's width (unscaled), as `tone_button` draws it: its label, its 11px sides
/// and border, an icon (`icon`) or the star or check (`mark`) with their gaps.
fn dock_button_width(
    label: &str,
    icon: bool,
    mark: bool,
    p: &ChatAppearance,
    cx: &gpui::App,
) -> f32 {
    dock_text_width(label, 13.0, p, cx)
        + 24.0
        + if icon { 20.0 } else { 0.0 }
        + if mark { 16.0 } else { 0.0 }
}

/// A run of answer buttons' width with their 6px gaps.
fn buttons_width(widths: &[f32]) -> f32 {
    widths.iter().sum::<f32>() + 6.0 * widths.len().saturating_sub(1) as f32
}

/// What a row with no answers shows in their place: Open for a page or form, Reply for a
/// question with no preset answers (`work_actions.rs` `work_row_action`).
fn dock_row_action(row: &Value) -> Option<&'static str> {
    if row["open"].is_object() {
        Some("Open")
    } else if row["reply"].is_object() {
        Some("Reply")
    } else {
        None
    }
}

/// The fold key that shows every dock row in place instead of the first `limit`.
const DOCK_ALL_ROWS: &str = "dock-rows:all";
/// The dock's cards fold by themselves while the draft is longer than this.
const DOCK_FOLDS_PAST_CHARS: usize = 280;

/// The opened Moving list draws as a table once the transcript is at least this wide (unscaled):
/// its Item, Who and Age columns plus a "Where it stands" column of at least 260 px, so neither
/// text column is squeezed below a few words a line.
const MOVING_TABLE_MIN_WIDTH: f32 = 640.0;
const MOVING_ITEM_COLUMN: f32 = 200.0;
const MOVING_AGE_COLUMN: f32 = 36.0;

/// A list row's title, with a session name's dotted underline when the row opens its card.
fn list_title(
    row: &Value,
    key: &str,
    index: usize,
    clickable: bool,
    body: gpui::Hsla,
    p: &ChatAppearance,
) -> AnyElement {
    div()
        .relative()
        .min_w_0()
        .whitespace_normal()
        .text_color(body)
        .child(text(row, "title"))
        .when(clickable, |title| {
            title.child(super::inter_agent_message::session_name_underline(
                format!("work-moving-underline:{key}:{index}"),
                0.0,
                0.0,
                p.card_muted,
                p.scale,
            ))
        })
        .into_any_element()
}

/// Who has a line of work, as a quiet line under its title: the icon and the name ("Claude ×2",
/// GitHub, You, Coordinator).
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User, of the table's Who column: "The who part doesn't matter in this, just put it under Item's
/// title". The table keeps Item, Where it stands and Age; the rows show the same line.
fn who_line(who: &Value, p: &ChatAppearance) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0 * p.scale))
        .text_color(p.muted)
        .child(who_icon(who, p))
        .child(div().min_w_0().truncate().child(text(who, "label")))
        .into_any_element()
}

/// Who has a line of work, as an icon: the thread's agent logo, GitHub's mark, the user, the
/// coordinator's hat, or a clock for something else outside.
fn who_icon(who: &Value, p: &ChatAppearance) -> AnyElement {
    let agent = text(who, "agentIcon");
    let (path, color) = match text(who, "kind").as_str() {
        "thread" if !agent.is_empty() => (
            format!("agent-icons/{agent}.svg"),
            super::new_session_welcome::brand_logo_color(&agent, p),
        ),
        "thread" => ("titlebar/robot.svg".to_string(), p.muted),
        "github" => ("titlebar/brand-github.svg".to_string(), p.muted),
        "you" => (
            "titlebar/user.svg".to_string(),
            gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into(),
        ),
        "coordinator" => (
            "titlebar/coordinator-captain-hat-small.svg".to_string(),
            p.muted,
        ),
        _ => ("titlebar/clock.svg".to_string(), p.muted),
    };
    svg()
        .path(path)
        .size(px(14.0 * p.scale))
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

fn chip_icon(kind: &str) -> &'static str {
    match kind {
        "thread" => "titlebar/robot.svg",
        // 12 px: the hat's thicker-lined copy for small sizes.
        "coordinator" => "titlebar/coordinator-captain-hat-small.svg",
        "steps" => "titlebar/checklist.svg",
        "item" => "titlebar/stack.svg",
        "question" => "titlebar/help-circle.svg",
        "link" => "titlebar/link.svg",
        "file" => "titlebar/file-text.svg",
        _ => "titlebar/clock.svg",
    }
}

impl NativeChatView {
    fn work_cards(&self) -> &[Value] {
        self.snapshot["workCards"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
    }

    /// The cards that sit under the reply `message_id`.
    pub(super) fn work_cards_under(&self, message_id: &str) -> Vec<Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["anchorId"].as_str() == Some(message_id))
            .cloned()
            .collect()
    }

    /// The cards of a summary-mode turn (`turnId`), drawn under the turn, outside its folds.
    pub(super) fn work_cards_of_turn(&self, turn_id: &str) -> Vec<Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["turnId"].as_str() == Some(turn_id))
            .cloned()
            .collect()
    }

    /// The dock directly on the chat box (gx-chat-core work_cards.rs `dock`, which holds the user's
    /// decisions): a header, then one surface of compact rows, what needs him first, then blocked
    /// work and what the newest turn finished; at most `limit` rows, then "N more need you". A row
    /// shows its answer buttons (the recommended one starred); a click on it expands it in place to
    /// its full text, its links and its actions (Reply, Open, Mention, Show in chat, Dismiss,
    /// Snooze), and a click again folds it.
    ///
    /// The header carries the counts (need you, blocked, in progress, as their marks and numbers),
    /// the button that opens the work items list, and the chevron; a click anywhere on it collapses
    /// the dock to that one bar or opens it again. With no rows (work still in progress, nothing to
    /// answer) the bar stays as one quiet line with no chevron. Nothing while a long message is
    /// typed, or while the Work panel is docked beside the chat (it lists the same things).
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, picking on the dock header page: layout "A. Header on the dock, pills row stays
    /// below" (collapsed, the dock is only its header, in the same place); "C. Marks and numbers
    /// only" for the summary, each mark's tooltip saying what it counts; "B. Icon only" for the
    /// list button, its name in its tooltip; "A. The bar stays, one quiet line" when nothing needs
    /// him; and a collapsed dock "opens by itself" when something new needs him. The round x that
    /// hid the dock is gone, replaced by the chevron. The Work pill and the red Blocked pill leave
    /// the desktop's pills row (pills_strip.rs). The phone keeps its own dock and pills.
    pub(super) fn render_work_dock(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.board.dock_top.set(None);
        let dock = &self.snapshot["workDock"];
        let board = &self.snapshot["coordinatorBoard"];
        let rows: Vec<Value> = dock["rows"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() && !board.is_object() {
            return None;
        }
        let long_draft = self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).value().len() > DOCK_FOLDS_PAST_CHARS);
        if long_draft || (self.board.open && self.board.docked) {
            return None;
        }
        let s = p.scale;
        // The core's `hidden` (hideWorkDock) is the collapsed bar here; it opens by itself when a
        // row that was not there when it collapsed arrives.
        let collapsible = !rows.is_empty();
        let open = collapsible && dock["hidden"] != true;
        let header = self.work_dock_header(&rows, collapsible, open, p, cx);
        let mut surface = div()
            .relative()
            .w_full()
            .min_w_0()
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s))
            .child(header);
        if open {
            let limit = dock["limit"].as_u64().unwrap_or(4) as usize;
            let hidden = rows.len().saturating_sub(limit);
            // "N more need you" opens the rest here, under the first rows, and "Show fewer" folds
            // them back; past `DOCK_MAX_HEIGHT` the rows scroll inside the dock. The header's work
            // items button stays the way to the whole list.
            //
            // CDXC:Coordinators 2026-11-10 DECISION:
            // User: the more line opens the rest of the rows in place, inside the dock, instead of
            // the Work panel at Needs you, with a way to fold them back.
            let all = hidden > 0 && self.expanded.contains(DOCK_ALL_ROWS);
            let mut list = div()
                .id("work-dock-rows")
                .w_full()
                .min_w_0()
                .max_h(px(DOCK_MAX_HEIGHT * s))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .p(px(4.0 * s));
            for row in rows.iter().take(if all { rows.len() } else { limit }) {
                list = list.child(self.dock_row(row, p, cx));
            }
            if hidden > 0 {
                list = list.child(
                    div()
                        .id("work-dock-more")
                        .role(gpui::Role::Button)
                        .pl(px(31.0 * s))
                        .pr(px(8.0 * s))
                        .pt(px(2.0 * s))
                        .pb(px(6.0 * s))
                        .text_color(p.muted)
                        .chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .aria_expanded(all)
                        .child(if all {
                            "Show fewer".to_string()
                        } else {
                            format!("{hidden} more need you")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.expanded.remove(DOCK_ALL_ROWS) {
                                this.expanded.insert(DOCK_ALL_ROWS.to_string());
                            }
                            cx.notify();
                        })),
                );
            }
            surface = surface.child(list);
        }
        // Its top edge, which the work items list opens above (chat_pills.rs `toggle_pill_menu`),
        // so the list never covers the button that opened it.
        let top = self.board.dock_top.clone();
        // Its rows' width, which decides what fits beside a question (`dock_row`); a change draws
        // the rows again at the new width.
        let width = self.board.dock_width.clone();
        Some(
            surface
                .child(
                    gpui::canvas(
                        move |bounds, window, _| {
                            top.set(Some(bounds.top()));
                            let inner = (bounds.size.width.as_f32() - 2.0) / s;
                            if (width.get() - inner).abs() > 0.5 {
                                width.set(inner);
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset(px(-1.0)),
                )
                .into_any_element(),
        )
    }

    /// The dock's header: the counts as marks and numbers, then the work items button and, while
    /// there are rows to show, the chevron (down while open, up while collapsed, as the question
    /// card on the chat box). A click anywhere on it collapses or opens the dock.
    fn work_dock_header(
        &self,
        rows: &[Value],
        collapsible: bool,
        open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let board = &self.snapshot["coordinatorBoard"];
        // The Work panel's own counts when the chat has one (the same numbers its groups show),
        // else the rows' own: questions from replies need him, a stuck line of work is blocked.
        let counts: Vec<(&str, u64)> = if board.is_object() {
            let count_of = |id: &str| {
                board["pillCounts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|pair| pair[0] == id)
                    .and_then(|pair| pair[1].as_u64())
                    .unwrap_or(0)
            };
            ["needs", "blocked", "moving"]
                .into_iter()
                .map(|id| (id, count_of(id)))
                .collect()
        } else {
            let needs = rows.iter().filter(|row| row["needs"] == true).count() as u64;
            let blocked = rows.iter().filter(|row| row["kind"] == "blocked").count() as u64;
            vec![("needs", needs), ("blocked", blocked)]
        };
        let marks: Vec<AnyElement> = counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(group, count)| {
                let (icon, color) = super::coordinator_board::group_look(group);
                let color = color.map_or(p.muted, |color| gpui::rgb(color).into());
                let tip = match group {
                    "needs" => "Need you",
                    "blocked" => "Blocked",
                    _ => "In progress",
                };
                div()
                    .id(gpui::SharedString::from(format!("work-dock-count:{group}")))
                    .flex()
                    .items_center()
                    .gap(px(5.0 * s))
                    .flex_shrink_0()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tip).build(window, cx)
                    })
                    .child(
                        svg()
                            .path(icon)
                            .size(px(13.0 * s))
                            .mr(px(s))
                            .flex_shrink_0()
                            .text_color(color),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .font_features(gpui::FontFeatures(vec![("tnum".into(), 1)].into()))
                            .child(count.to_string()),
                    )
                    .into_any_element()
            })
            .collect();
        let quiet = marks.is_empty();
        let mut head = div()
            .id("work-dock-head")
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .pl(px(12.0 * s))
            .pr(px(5.0 * s))
            .text_size(px(13.0 * s))
            .line_height(px(19.0 * s))
            .whitespace_nowrap()
            .text_color(p.card_muted)
            .map(|head| {
                if open {
                    // The open header is the band over the rows, as the pills row on the chat box.
                    head.h(px(34.0 * s + 1.0))
                        .border_b_1()
                        .border_color(p.input_border)
                        .bg(p.foreground.opacity(0.035))
                        .rounded_t(px(11.0 * s))
                } else {
                    head.h(px(34.0 * s)).rounded(px(11.0 * s))
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(12.0 * s))
                    .overflow_hidden()
                    .children(marks)
                    // Nothing counted and nothing to show: one quiet word, so the bar is not blank.
                    .when(quiet && !collapsible, |summary| {
                        summary.child(div().text_color(p.muted).child("Nothing in progress"))
                    }),
            )
            .child(div().flex_1().min_w(px(8.0 * s)));
        if board.is_object() {
            head = head.child(self.work_dock_list_button(p, cx));
        }
        if collapsible {
            head = head
                .role(gpui::Role::Button)
                .aria_label(if open { "Collapse" } else { "Expand" })
                .aria_expanded(open)
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.055)))
                .child(
                    div()
                        .size(px(24.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .rounded(px(6.0 * s))
                        .child(super::ask_card::chevron(open, p)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let kind = if open { "hideWorkDock" } else { "showWorkDock" };
                    this.invoke(json!({ "type": kind }), cx);
                }));
        }
        head.into_any_element()
    }

    /// The header's icon button that opens the work items list, the way the Work pill did: the
    /// docked list or its floating card, or the columns. Its name is its tooltip.
    fn work_dock_list_button(&self, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let s = p.scale;
        let trigger = super::coordinator_board::BOARD_TRIGGER;
        let shown = self.chat_menu_is_open(trigger)
            || self.coordinator_board_docked()
            || self.chat_menu_is_open(super::coordinator_board::BOARD_COLUMNS_TRIGGER);
        let fills = p.neutral_button_fills();
        let bounds = self.pill_bounds.cell(super::coordinator_board::BOARD_MENU);
        let measured = bounds.clone();
        div()
            .id(trigger)
            .relative()
            .role(gpui::Role::Button)
            .aria_label("Work items")
            .aria_expanded(shown)
            .w(px(26.0 * s))
            .h(px(24.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .rounded(px(7.0 * s))
            .chat_cursor_pointer()
            .chat_filled(fills, true)
            .when(shown, |button| button.bg(fills.pressed))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Work items").build(window, cx)
            })
            .child(
                svg()
                    .path("titlebar/layout-board-split.svg")
                    .size(px(13.0 * s))
                    .flex_shrink_0()
                    .text_color(p.card_muted),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_coordinator_board(bounds.get(), window, cx);
            }))
            .child(
                gpui::canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .into_any_element()
    }

    /// One dock row. Its first line is its status glyph, its question, the answers that fit
    /// beside it and its age, which stays at the top right open or closed. Closed it stays one
    /// line: the answers that fit, then "+N" for the rest (a click opens the row). Opened, the
    /// question shows four lines and then "more"; the answers stay beside it when they fit and go
    /// under it when they do not; the last line holds the work item's name and what waits on it,
    /// with the icon buttons at its right end.
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, picking on the dock rows page (rows-page.html) after a long question was squeezed
    /// into a narrow column beside five answers: an opened row's answers sit "beside the question
    /// when they fit, under it when they do not", where fit means they leave the question at least
    /// half the row (B); a closed row whose answers do not fit shows "the answers that fit, then
    /// +3" and stays one line (B); the icon buttons go "at the right end of the row's last line"
    /// (A); a very long question shows "four lines, then more" (B). Also as drawn: the age stays
    /// at the top right, and the work item's name and "Nothing waits on it" share one line.
    fn dock_row(&self, row: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let open_key = format!("dock-row:{key}");
        let open = self.expanded.contains(&open_key);
        let (icon, color) = status_look(&text(row, "status"), p);
        let body = p.strengthened(p.prose);
        let busy = row["busy"] == true;
        let title = text(row, "title");
        let age = text(row, "age");
        // The answers, the look every place shares (work_actions.rs); a row with none shows what
        // it asks for in their place (Open or Reply).
        let answers: Vec<Value> = row["buttons"].as_array().cloned().unwrap_or_default();
        let action = if answers.is_empty() && row["needs"] == true {
            dock_row_action(row)
        } else {
            None
        };
        // What fits: the answers may take the first line while the question keeps half the row.
        let row_width = match self.board.dock_width.get() {
            width if width > 0.0 => width,
            _ => DOCK_ROW_WIDTH_GUESS,
        } - 2.0 * DOCK_LIST_PAD
            - 2.0 * DOCK_ROW_PAD_X;
        let age_width = if age.is_empty() {
            0.0
        } else {
            dock_text_width(&age, PROSE_SIZE, p, cx) + DOCK_GAP
        };
        let beside_room = row_width - DOCK_LEAD - age_width - row_width / 2.0;
        let widths: Vec<f32> = match &action {
            Some(label) => vec![dock_button_width(label, true, false, p, cx)],
            None => answers
                .iter()
                .map(|answer| {
                    dock_button_width(
                        &text(answer, "label"),
                        false,
                        answer["recommended"] == true || answer["picked"] == true,
                        p,
                        cx,
                    )
                })
                .collect(),
        };
        let all_width = buttons_width(&widths);
        let fit = all_width <= beside_room;
        // Closed: the answers that fit, then "+N"; opened, all of them beside or under.
        let shown = if open || fit {
            widths.len()
        } else {
            let mut shown = widths.len();
            while shown > 0 {
                let rest = widths.len() - shown;
                let chip = if rest > 0 {
                    dock_button_width(&format!("+{rest}"), false, false, p, cx) + 6.0
                } else {
                    0.0
                };
                if buttons_width(&widths[..shown]) + chip <= beside_room {
                    break;
                }
                shown -= 1;
            }
            shown
        };
        let mut buttons: Vec<AnyElement> = match &action {
            Some(_) => self.work_row_action(&key, row, p, cx).into_iter().collect(),
            None => self.work_answer_buttons(&key, &answers[..shown], busy, p, cx),
        };
        let rest = widths.len().saturating_sub(shown);
        let toggle = open_key.clone();
        if !open && rest > 0 {
            let toggle = toggle.clone();
            buttons.push(
                self.tone_button(
                    gpui::SharedString::from(format!("dock-row-rest:{key}")),
                    &format!("+{rest}"),
                    None,
                    super::tone_card::ToneButton::Secondary,
                    None,
                    false,
                    p,
                )
                .aria_label(format!("{rest} more answers"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.expanded.insert(toggle.clone());
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        let beside = !open || fit;
        // The question: one cut line closed; opened, up to four lines and then "more".
        let more_key = format!("dock-row-more:{key}");
        let title_width =
            row_width - DOCK_LEAD - age_width - if beside { all_width + DOCK_GAP } else { 0.0 };
        let long = open
            && dock_text_width(&title, PROSE_SIZE, p, cx)
                > title_width.max(80.0) * (DOCK_QUESTION_LINES as f32 - 0.25);
        let whole = long && self.expanded.contains(&more_key);
        let question = div()
            .flex_1()
            .min_w_0()
            .text_color(body)
            .when(!open, |title| title.truncate())
            .when(open, |title| title.whitespace_normal())
            .when(long && !whole, |title| {
                title.line_clamp(DOCK_QUESTION_LINES).text_ellipsis()
            })
            .child(title.to_string());
        let question = if long {
            let more_key = more_key.clone();
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(question)
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("dock-row-more:{key}")))
                        .role(gpui::Role::Button)
                        .aria_expanded(whole)
                        .text_color(p.muted)
                        .chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .child(if whole { "less" } else { "more" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !this.expanded.remove(&more_key) {
                                this.expanded.insert(more_key.clone());
                            }
                            cx.notify();
                        })),
                )
        } else {
            question
        };
        let mut head = div()
            .id(gpui::SharedString::from(format!("dock-row-head:{key}")))
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap(px(DOCK_GAP * s))
            .px(px(DOCK_ROW_PAD_X * s))
            .py(px(6.0 * s))
            .rounded(px(8.0 * s))
            .chat_cursor_pointer()
            .when(!open, |head| {
                head.hover(|style| style.bg(p.foreground.opacity(0.04)))
            })
            .child(
                div()
                    .h(px(PROSE_LINE_HEIGHT * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .child(svg().path(icon).size(px(13.0 * s)).text_color(color)),
            )
            .child(question);
        let mut under_answers = Vec::new();
        if beside {
            if !buttons.is_empty() {
                head = head.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .gap(px(6.0 * s))
                        .children(buttons),
                );
            }
        } else {
            under_answers = buttons;
        }
        head = head
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .child(age.to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.expanded.remove(&toggle) {
                    this.expanded.insert(toggle.clone());
                }
                cx.notify();
            }));
        if !open {
            return div().w_full().child(head).into_any_element();
        }
        // Opened: the answers that did not fit, then one last line with the work it belongs to and
        // what waits on it, the icon buttons at its right end.
        let mut under = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
            .pr(px(DOCK_ROW_PAD_X * s))
            .pb(px(8.0 * s));
        if !under_answers.is_empty() {
            under = under.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0 * s))
                    .children(under_answers),
            );
        }
        let mut parts: Vec<AnyElement> = row["chips"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, chip)| self.work_card_part(chip, &format!("dock:{key}:{index}"), p, cx))
            .collect();
        let said: Vec<String> = row["detail"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        if !said.is_empty() {
            if !parts.is_empty() {
                parts.push(
                    div()
                        .text_color(p.muted)
                        .opacity(0.6)
                        .child("\u{b7}")
                        .into_any_element(),
                );
            }
            parts.push(
                div()
                    .min_w_0()
                    .whitespace_normal()
                    .text_color(p.card_muted)
                    .child(said.join(" \u{b7} "))
                    .into_any_element(),
            );
        }
        let actions = json!({
            "reply": row["reply"],
            "open": row["open"],
            "showInChat": row["showInChat"],
            "dismiss": row["dismiss"],
            "snooze": row["snooze"],
        });
        let icons = self.work_action_icons(&key, &actions, &text(row, "mention"), false, p, cx);
        if !parts.is_empty() || icons.is_some() {
            under = under.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_end()
                    .gap(px(8.0 * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h(px(28.0 * s))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_x(px(8.0 * s))
                            .gap_y(px(4.0 * s))
                            .children(parts),
                    )
                    .children(icons.map(|icons| div().flex_shrink_0().child(icons))),
            );
        }
        div()
            .w_full()
            .rounded(px(8.0 * s))
            .bg(p.foreground.opacity(0.03))
            .child(head)
            .child(under)
            .into_any_element()
    }

    /// A column of cards, or `None` when there are none.
    pub(super) fn work_card_stack(
        &self,
        cards: &[Value],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        (!cards.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap(px(6.0 * p.scale))
                .children(cards.iter().map(|card| {
                    let drawn = if card["kind"] == "moving" {
                        self.work_moving_row(card, p, cx)
                    } else if card["folded"] == true {
                        self.work_fold_line(card, p, cx)
                    } else {
                        self.work_card(card, p, cx)
                    };
                    // The ? pill's jump lands on an ask's card and flashes it (open_questions.rs).
                    let key = text(card, "key");
                    self.open_question_frame(Some(key.as_str()), drawn, p)
                }))
                .into_any_element()
        })
    }

    fn work_card(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let (icon, color) = status_look(&text(card, "status"), p);
        let status = text(card, "statusLabel");
        let age = text(card, "age");
        let line = text(card, "line");
        let body = p.strengthened(p.prose);
        // On a narrow chat the status and age wrap under the title rather than squeeze it.
        let mut top = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .min_h(px(PROSE_LINE_HEIGHT * s))
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(140.0 * s))
                    .whitespace_normal()
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(body)
                    .child(text(card, "title")),
            );
        let right = [status.as_str(), age.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        if !right.is_empty() {
            top = top.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .text_color(p.muted)
                    .child(right),
            );
        }
        // A finished card in the dock goes away with its × (gx-chat-core `dock`).
        let dismiss = card["dismiss"].clone();
        if dismiss.is_object() {
            top = top.child(
                div()
                    .id(gpui::SharedString::from(format!("work-dismiss:{key}")))
                    .role(gpui::Role::Button)
                    .aria_label("Dismiss")
                    .flex_shrink_0()
                    .size(px(20.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0 * s))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Dismiss").build(window, cx)
                    })
                    .child(
                        svg()
                            .path("titlebar/x.svg")
                            .size(px(12.0 * s))
                            .text_color(p.muted),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(dismiss.clone(), cx);
                    })),
            );
        }
        let mut column = div()
            .id(gpui::SharedString::from(format!("work-card:{key}")))
            .role(gpui::Role::Group)
            .aria_label(format!("{}, {status}", text(card, "title")))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .px(px(12.0 * s))
            .py(px(10.0 * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .child(top);
        if !line.is_empty() {
            column = column.child(
                div()
                    .pl(px(22.0 * s))
                    .min_w_0()
                    .whitespace_normal()
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .text_color(p.card_muted)
                    .child(line),
            );
        }
        // Each part a quiet link at the chat's size: its icon (the thread's agent logo, GitHub's
        // mark) and muted words, brightening and underlined on hover; a GitHub PR or issue says its
        // state in that state's colour. Parts wrap to another line, never cut (Q2205 A).
        let chips: Vec<AnyElement> = card["chips"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, chip)| self.work_card_part(chip, &format!("{key}:{index}"), p, cx))
            .collect();
        let busy = card["busy"] == true;
        // The dock's answers, the look every place shares (work_actions.rs).
        let answers: Vec<Value> = card["buttons"].as_array().cloned().unwrap_or_default();
        let buttons = self.work_answer_buttons(&key, &answers, busy, p, cx);
        if !chips.is_empty() || !buttons.is_empty() {
            column = column.child(
                div()
                    .pl(px(22.0 * s))
                    .pt(px(2.0 * s))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(14.0 * s))
                    .gap_y(px(4.0 * s))
                    .children(chips)
                    .when(!buttons.is_empty(), |row| {
                        row.child(
                            div()
                                .ml_auto()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.0 * s))
                                .children(buttons),
                        )
                    }),
            );
        }
        // Dismiss and Snooze as the icon buttons every place shares (work_actions.rs).
        let actions = json!({ "dismiss": card["cancel"], "snooze": card["snooze"] });
        if let Some(icons) = self.work_action_icons(&key, &actions, "", false, p, cx) {
            column = column.child(div().pl(px(22.0 * s)).child(icons));
        }
        column.into_any_element()
    }

    /// One part of a card's bottom line (`chips` in gx-chat-core work_cards.rs `item_chips`).
    fn work_card_part(
        &self,
        chip: &Value,
        id: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let kind = text(chip, "kind");
        let label = text(chip, "label");
        let command = chip["command"].clone();
        let agent = text(chip, "agentIcon");
        let (icon, icon_color) = match kind.as_str() {
            "thread" if !agent.is_empty() => (
                format!("agent-icons/{agent}.svg"),
                super::new_session_welcome::brand_logo_color(&agent, p),
            ),
            "github" => ("titlebar/brand-github.svg".to_string(), p.muted),
            _ => (chip_icon(&kind).to_string(), p.muted),
        };
        let state = text(chip, "state");
        let state_color = super::coordinator_board::tone_color(&text(chip, "tone"))
            .map_or(p.muted, |color| gpui::rgb(color).into());
        // A part that opens something wears the dotted underline a session name has on a message
        // card, faint at rest and full under the pointer (the user's "hovering on these kind of
        // links should put dotted underline below them so it's clear they are clickable").
        let words = div()
            .relative()
            .min_w_0()
            .whitespace_normal()
            .child(label.clone())
            .when(command.is_object(), |words| {
                words.hover(|style| style.text_color(p.foreground)).child(
                    super::inter_agent_message::session_name_underline(
                        format!("work-chip-underline:{id}"),
                        0.0,
                        0.0,
                        p.card_muted,
                        s,
                    ),
                )
            });
        div()
            .id(gpui::SharedString::from(format!("work-chip:{id}")))
            .flex()
            .max_w_full()
            .min_w_0()
            .items_center()
            .gap(px(6.0 * s))
            .min_h(px(PROSE_LINE_HEIGHT * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s))
            .text_color(p.card_muted)
            .aria_label(if state.is_empty() {
                label.clone()
            } else {
                format!("{label}, {state}")
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .child(words)
            .when(!state.is_empty(), |part| {
                part.child(div().flex_shrink_0().text_color(state_color).child(state))
            })
            .when(command.is_object(), |part| {
                part.chat_cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }

    /// Everything still running, as one quiet row ("Moving · 7" and their names) that opens to a
    /// list; a row of the list goes to that item's newest card (gx-chat-core work_cards.rs
    /// `moving_row`, which holds the user's Q2166).
    fn work_moving_row(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let open = self.expanded.contains(&key);
        let (icon, color) = status_look("moving", p);
        let toggle_key = key.clone();
        let body = p.strengthened(p.prose);
        let head = div()
            .id(gpui::SharedString::from(format!("work-moving:{key}")))
            .role(gpui::Role::Button)
            .aria_label(format!(
                "{}, {}",
                text(card, "title"),
                if open { "open" } else { "closed" }
            ))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(34.0 * s))
            .px(px(12.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .child(
                svg()
                    .path(if open {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            // The same 14 px as the list's who-has-it icons under it (the user's "make this icon
            // bigger for the ones inside the list").
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(body)
                    .child(text(card, "title")),
            )
            // Closed, it says who has what ("1 with you · 1 on GitHub · 5 threads"), what is
            // with the user in the question colour.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .when(!open, |summary| {
                        summary
                            .children(
                                card["summaryParts"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .enumerate()
                                    .flat_map(|(index, part)| {
                                        let tone = super::coordinator_board::tone_color(&text(
                                            part, "tone",
                                        ))
                                        .map_or(p.card_muted, |color| gpui::rgb(color).into());
                                        [
                                            (index > 0)
                                                .then(|| " \u{b7} ".to_string().into_any_element()),
                                            Some(
                                                gpui::div()
                                                    .flex_shrink_0()
                                                    .text_color(tone)
                                                    .child(text(part, "text"))
                                                    .into_any_element(),
                                            ),
                                        ]
                                    })
                                    .flatten(),
                            )
                            .flex()
                            .gap(px(0.0))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !this.expanded.remove(&toggle_key) {
                    this.expanded.insert(toggle_key.clone());
                }
                cx.notify();
            }));
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            // The work item cards' own background, border and radius.
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .overflow_hidden()
            .child(head);
        if open {
            // The pane, capped at the transcript column. Not `transcript_text_width`: it asks the
            // transcript list for the minimap's gutter while that list is drawing this row.
            let width = self
                .bounds
                .get()
                .size
                .width
                .as_f32()
                .min(self.transcript_column_cap(p));
            let wide = width / s >= MOVING_TABLE_MIN_WIDTH;
            column = column.child(self.work_moving_list(card, wide, p, cx));
        }
        column.into_any_element()
    }

    /// The opened Moving list, grouped by who has each line of work (gx-chat-core `who_has`, which
    /// holds the user's Q2207): on a wide chat the compact table (Item, Where it stands, Who,
    /// Age), else rows with the full text; quiet group labels, no hairlines between rows.
    fn work_moving_list(
        &self,
        card: &Value,
        wide: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let body = p.strengthened(p.prose);
        let mut list = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .px(px(12.0 * s))
            .pb(px(8.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s));
        if wide {
            list = list.child(
                div()
                    .flex()
                    .gap(px(16.0 * s))
                    .pt(px(2.0 * s))
                    .text_color(p.muted)
                    .child(
                        div()
                            .w(px(MOVING_ITEM_COLUMN * s))
                            .flex_shrink_0()
                            .child("Item"),
                    )
                    .child(div().flex_1().min_w_0().child("Where it stands"))
                    .child(
                        div()
                            .w(px(MOVING_AGE_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .child("Age"),
                    ),
            );
        }
        let groups: Vec<&Value> = card["groups"].as_array().into_iter().flatten().collect();
        let mut current = String::new();
        for (index, row) in card["rows"].as_array().into_iter().flatten().enumerate() {
            let group = text(row, "group");
            if group != current {
                current = group.clone();
                let count = groups
                    .iter()
                    .find(|entry| entry["key"] == group.as_str())
                    .and_then(|entry| entry["count"].as_u64())
                    .unwrap_or(0);
                list = list.child(
                    div()
                        .pt(px(8.0 * s))
                        .pb(px(2.0 * s))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(format!("{} \u{b7} {count}", text(row, "groupLabel"))),
                );
            }
            let command = row["command"].clone();
            let clickable = command.is_object();
            let line = text(row, "line");
            let who = &row["who"];
            let mut drawn = div()
                .id(gpui::SharedString::from(format!(
                    "work-moving-row:{key}:{index}"
                )))
                .role(gpui::Role::Button)
                .aria_label(format!("{}, {}", text(row, "title"), text(who, "label")))
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .py(px(4.0 * s))
                .rounded(px(6.0 * s));
            drawn = if wide {
                drawn
                    .gap(px(16.0 * s))
                    .child(
                        div()
                            .w(px(MOVING_ITEM_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .child(list_title(row, &key, index, clickable, body, p)),
                            )
                            .child(who_line(who, p)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .text_color(p.card_muted)
                            .child(line),
                    )
                    .child(
                        div()
                            .w(px(MOVING_AGE_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .text_color(p.muted)
                            .child(text(row, "age")),
                    )
            } else {
                drawn
                    .gap(px(8.0 * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .child(list_title(row, &key, index, clickable, body, p)),
                            )
                            .child(who_line(who, p))
                            .when(!line.is_empty(), |cell| {
                                cell.child(
                                    div()
                                        .whitespace_normal()
                                        .text_color(p.card_muted)
                                        .child(line.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.muted)
                            .child(text(row, "age")),
                    )
            };
            list = list.child(drawn.when(command.is_object(), |row| {
                row.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.04)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            }));
        }
        list.into_any_element()
    }

    /// An item's earlier card, or a settled ask: one quiet line, its status's glyph, what that turn
    /// did, and when.
    fn work_fold_line(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let status = match text(card, "foldStatus").as_str() {
            "" => text(card, "status"),
            status => status.to_string(),
        };
        let (icon, color) = status_look(&status, p);
        let words = text(card, "foldText");
        let outcome = text(card, "foldOutcome");
        let command = card["command"].clone();
        let tooltip = if outcome.is_empty() {
            words.clone()
        } else {
            format!("{words} \u{b7} {outcome}")
        };
        div()
            .id(gpui::SharedString::from(format!(
                "work-fold:{}",
                text(card, "key")
            )))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(34.0 * s))
            .px(px(12.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.input_border.opacity(0.7))
            .text_size(px(PROSE_SIZE * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(color.opacity(0.8)),
            )
            // The question or title gives way; the outcome and time stay whole.
            .child(
                div()
                    .flex_1()
                    .min_w(px(48.0 * s))
                    .truncate()
                    .text_color(p.card_muted)
                    .child(words),
            )
            .when(!outcome.is_empty(), |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(320.0 * s))
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.card_muted)
                        .child(outcome),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .child(text(card, "time")),
            )
            // A turn's line opens the Work panel at the first line of work it names.
            .when(command.is_object(), |line| {
                line.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.04)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }
}
