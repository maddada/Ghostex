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

/// An item pill's sides and dot (unscaled).
const PILL_PAD: f32 = 7.0;
const PILL_DOT: f32 = 6.0;

/// How an opened row's item pills fit before its answers: whole (dot and id), dots only, or one
/// "+N" pill.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PillFit {
    Full,
    Dots,
    Plus,
}

/// The item pills' widths (unscaled), each with the 6px gap to the answers after them: whole,
/// dots only, and as one "+N" pill. All 0 with no pills.
pub(super) fn pill_widths(pills: &[Value], p: &ChatAppearance, cx: &gpui::App) -> (f32, f32, f32) {
    if pills.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let gaps = 6.0 * pills.len() as f32;
    let full: f32 = pills
        .iter()
        .map(|pill| {
            PILL_PAD * 2.0 + PILL_DOT + 4.0 + dock_text_width(&text(pill, "id"), PROSE_SIZE, p, cx) + 2.0
        })
        .sum();
    let dots = (PILL_PAD * 2.0 + PILL_DOT + 2.0) * pills.len() as f32;
    let plus = PILL_PAD * 2.0 + dock_text_width(&format!("+{}", pills.len()), PROSE_SIZE, p, cx) + 2.0;
    (full + gaps, dots + gaps, plus + 6.0)
}

/// The fit for `room` (unscaled) left beside the answers.
pub(super) fn pill_fit(widths: (f32, f32, f32), room: f32) -> PillFit {
    if widths.0 <= room {
        PillFit::Full
    } else if widths.1 <= room {
        PillFit::Dots
    } else {
        PillFit::Plus
    }
}

/// The thin divider that sets an opened row's Reply apart from its answers.
fn answers_divider(p: &ChatAppearance) -> AnyElement {
    div()
        .flex_shrink_0()
        .w(px(1.0))
        .h(px(16.0 * p.scale))
        .mx(px(3.0 * p.scale))
        .bg(p.foreground.opacity(0.10))
        .into_any_element()
}

/// A page row's ⋯ menu (`dock_row`): Show in chat, Dismiss and Snooze with its lengths, as the
/// app's dropdown rows (`{label, command}`, a submenu's under `children`).
fn page_row_menu(row: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    if row["showInChat"].is_object() {
        rows.push(json!({ "label": "Show in chat", "command": row["showInChat"] }));
    }
    if row["dismiss"].is_object() {
        rows.push(json!({ "label": "Dismiss", "command": row["dismiss"] }));
    }
    let snooze: Vec<Value> = row["snooze"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|choice| json!({ "label": text(choice, "label"), "command": choice["command"] }))
        .collect();
    if !snooze.is_empty() {
        rows.push(json!({ "label": "Snooze", "children": snooze }));
    }
    rows
}

/// A dock row's mark: a Ghostex page that needs him is a plain page in the need-you pink (`page`
/// on the row, from gx-chat-core); everything else its status's mark (`status_look`). Every row
/// that is a page takes its mark here.
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User: "please change the icon of the ones that need me but are html page to a page icon
/// (without html written there in the icon) but colored pink". A question keeps its circle.
pub(super) fn row_mark(row: &Value, p: &ChatAppearance) -> (&'static str, gpui::Hsla) {
    let status = text(row, "status");
    let (icon, color) = status_look(&status, p);
    if row["page"] == true && status == "needs" {
        return ("titlebar/file.svg", color);
    }
    (icon, color)
}

/// A chip's glyph by what it says.
/// The dock's cards stop growing here (unscaled) and scroll inside it.
const DOCK_MAX_HEIGHT: f32 = 360.0;
/// An opened row's (and an opened Work panel card's) space between its title with its work item
/// line, its description and its answers: a question card's (question.rs `body` gap 12).
pub(super) const OPENED_ROW_GAP: f32 = 12.0;
/// The share of the window's height the dock's rows may take at most, so a short window always
/// keeps the transcript in view above the dock and the chat box (the phone's `dockHeightCap`).
///
/// CDXC:Coordinators 2026-11-12 DECISION:
/// User, of the phone's opened dock that hid the chat: the dock never takes more than a share of
/// the space above the chat box; the desktop keeps the same rule.
const DOCK_WINDOW_SHARE: f32 = 0.4;
/// A dock row's metrics (unscaled): the rows' inset in the dock, a row's side padding, the gap
/// between its parts, and the glyph with its gap that leads every line.
const DOCK_LIST_PAD: f32 = 4.0;
const DOCK_ROW_PAD_X: f32 = 8.0;
const DOCK_GAP: f32 = 10.0;
const DOCK_LEAD: f32 = 23.0;
/// A dock row's age slot is at least the ⋯ that takes its place under the pointer (`row_age_menu`).
const AGE_SLOT_MIN: f32 = 26.0;
/// The rows' width before the dock has painted once (the chat box's own width).
const DOCK_ROW_WIDTH_GUESS: f32 = 736.0;

/// How wide `label` is in the chat's font at `size` (both unscaled).
pub(super) fn dock_text_width(label: &str, size: f32, p: &ChatAppearance, cx: &gpui::App) -> f32 {
    super::fonts::label_width(label, &p.font, gpui::FontWeight::NORMAL, size * p.scale, cx)
        / p.scale
}

/// One answer button's width (unscaled), as `tone_button` draws it: its label, its 11px sides
/// and border, an icon (`icon`) or the star or check (`mark`) with their gaps.
pub(super) fn dock_button_width(
    label: &str,
    icon: bool,
    mark: bool,
    p: &ChatAppearance,
    cx: &gpui::App,
) -> f32 {
    dock_text_width(label, PROSE_SIZE, p, cx)
        + 24.0
        + if icon { 20.0 } else { 0.0 }
        + if mark { 16.0 } else { 0.0 }
}

/// A run of answer buttons' width with their 6px gaps.
pub(super) fn buttons_width(widths: &[f32]) -> f32 {
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

/// A description's Markdown kept compact for a row (`ask_markdown`): every line break kept as a
/// break, a heading as a bold line at the text's size, a table row and a picture as their plain
/// text. Lists, emphasis, code and links stay as written.
fn compact_markdown(words: &str) -> String {
    // Each line, and whether it is code (a fence or inside one), which keeps its plain break.
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut fenced = false;
    for line in words.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            out.push((line.to_string(), true));
            continue;
        }
        if fenced {
            out.push((line.to_string(), true));
            continue;
        }
        let heading = trimmed.trim_start_matches('#');
        let line = if trimmed.starts_with('#') && heading.starts_with(' ') {
            format!("**{}**", heading.trim())
        } else if trimmed.starts_with('|') {
            // A table's row as its words; its rule line goes.
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .collect();
            if cells
                .iter()
                .all(|cell| cell.chars().all(|c| matches!(c, '-' | ':' | ' ')))
            {
                continue;
            }
            cells.join(" \u{b7} ")
        } else {
            plain_pictures(line)
        };
        out.push((line, false));
    }
    // A single line break is kept as a break, not joined into the line before it.
    let mut text = String::new();
    for (index, (line, code)) in out.iter().enumerate() {
        text.push_str(line);
        if let Some((next, _)) = out.get(index + 1) {
            let plain_break = *code || line.trim().is_empty() || next.trim().is_empty();
            text.push_str(if plain_break { "\n" } else { "  \n" });
        }
    }
    text
}

/// `![alt](src)` as its alt words, so a description never draws a picture.
fn plain_pictures(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(start) = rest.find("![") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("](").and_then(|close| {
            after[close + 2..]
                .find(')')
                .map(|end| (close, close + 2 + end))
        }) {
            Some((close, end)) => {
                out.push_str(&after[..close]);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str("![");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// How often a drawn page row asks its gxserver for its counts (`refresh_page_counts`).
const PAGE_COUNTS_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// The dock's rows' fold, as the chat's disclosure motion knows it.
const DOCK_MOTION_KEY: &str = "work-dock:rows";

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
        "coordinator" => ("titlebar/coordinator-crown-points.svg".to_string(), p.muted),
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
        // The coordinator's crown (native_sidebar/threads.rs `coordinator_icon`).
        "coordinator" => "titlebar/coordinator-crown-points.svg",
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
    pub(super) fn work_cards_under(&self, message_id: &str) -> Vec<&Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["anchorId"].as_str() == Some(message_id))
            .collect()
    }

    /// The cards of a summary-mode turn (`turnId`), drawn under the turn, outside its folds.
    pub(super) fn work_cards_of_turn(&self, turn_id: &str) -> Vec<&Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["turnId"].as_str() == Some(turn_id))
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
        window_height: f32,
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
        // Collapsing or opening, the rows ease their height and fade as the chat's other folds do
        // (disclosure_motion.rs, its timing and the reduced motion setting), the chevron turning
        // with them; the bar and the chat box under the dock follow its height.
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User, of the dock header's collapse: "please make this animate when expanded /
        // collapsed".
        let motion = self.disclosure_frame(DOCK_MOTION_KEY, open, cx);
        let turned = motion.map_or(if open { 1.0 } else { 0.0 }, |frame| frame.log);
        let header = self.work_dock_header(&rows, collapsible, open, turned, p, cx);
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
        if open || motion.is_some() {
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
                .max_h(px(
                    (DOCK_MAX_HEIGHT * s).min(window_height * DOCK_WINDOW_SHARE)
                ))
                .overflow_y_scroll()
                .track_scroll(&self.board.dock_scroll)
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
                            // "1 more needs you", "3 more need you".
                            if hidden == 1 {
                                "1 more needs you".to_string()
                            } else {
                                format!("{hidden} more need you")
                            }
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.expanded.remove(DOCK_ALL_ROWS) {
                                this.expanded.insert(DOCK_ALL_ROWS.to_string());
                            }
                            cx.notify();
                        })),
                );
            }
            surface = surface.child(self.disclosure_body_motion(
                DOCK_MOTION_KEY,
                motion,
                0.0,
                list.into_any_element(),
            ));
        }
        // Its top edge, which the work items list opens above (chat_pills.rs `toggle_pill_menu`),
        // so the list never covers the button that opened it.
        let top = self.board.dock_top.clone();
        // Its rows' width, which decides what fits beside a question (`dock_row`); a change draws
        // the rows again at the new width.
        let width = self.board.dock_width.clone();
        let chat = cx.weak_entity();
        Some(
            surface
                .child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            top.set(Some(bounds.top()));
                            let inner = (bounds.size.width.as_f32() - 2.0) / s;
                            if (width.get() - inner).abs() > 0.5 {
                                width.set(inner);
                                // The chat's render is cached: tell it to lay the rows out again.
                                let chat = chat.clone();
                                cx.defer(move |cx| {
                                    let _ = chat.update(cx, |_, cx| cx.notify());
                                });
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
        turned: f32,
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
        let counts: Vec<(&str, u64)> = counts.into_iter().filter(|(_, count)| *count > 0).collect();
        // Its words agree with its count: "1 needs you", "2 need you".
        let name_of = |group: &str, count: u64| match group {
            "needs" if count == 1 => "needs you",
            "needs" => "need you",
            "blocked" => "blocked",
            _ => "in progress",
        };
        // Each mark takes its name after its count when the header has room for all of them, and
        // none does when it has not; the tooltips stay either way.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of the header's marks: "make the icons on the left show name of each when there's
        // space and not show name when no space". All switch together, never one named alone.
        let room = match self.board.dock_width.get() {
            width if width > 0.0 => width,
            _ => DOCK_ROW_WIDTH_GUESS,
        } - 12.0
            - 5.0
            - 8.0
            - if board.is_object() { 26.0 + 6.0 } else { 0.0 }
            - if collapsible { 24.0 + 6.0 } else { 0.0 };
        let named_width: f32 = counts
            .iter()
            .map(|(group, count)| {
                14.0 + 5.0
                    + dock_text_width(&count.to_string(), PROSE_SIZE, p, cx)
                    + 4.0
                    + dock_text_width(name_of(group, *count), PROSE_SIZE, p, cx)
            })
            .sum::<f32>()
            + 12.0 * counts.len().saturating_sub(1) as f32;
        // The agent's status rides at the bar's left (`dock_status_card`): the marks give up their
        // names before it is cut, and it is cut at its end before anything overlaps.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, picking on the chat box bars page: "A. The status joins the dock's quiet line",
        // the status first at the left and the count at the right ("B"), and of the drawing where
        // it dropped under the rows: "it should always be on same row ok? not sometimes below in
        // separate bar like u show here". So it is always on this bar while the chat has a dock.
        let status_card = self.dock_status_card();
        let status_width = status_card.as_ref().map_or(0.0, |card| {
            24.0 + dock_text_width(card["label"].as_str().unwrap_or_default(), PROSE_SIZE, p, cx) + 12.0
        });
        let named = named_width + status_width <= room;
        let marks: Vec<AnyElement> = counts
            .into_iter()
            .map(|(group, count)| {
                let (icon, color) = super::coordinator_board::group_look(group);
                let color = color.map_or(p.muted, |color| gpui::rgb(color).into());
                let tip = match group {
                    "needs" if count == 1 => "Needs you",
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
                            .text_color(p.foreground)
                            .font_features(gpui::FontFeatures(vec![("tnum".into(), 1)].into()))
                            .child(count.to_string()),
                    )
                    .when(named, |mark| {
                        mark.child(div().ml(px(-1.0 * s)).child(name_of(group, count)))
                    })
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
            .text_size(px(PROSE_SIZE * s))
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
            .when_some(status_card.as_ref(), |head, card| {
                head.child(
                    div()
                        .flex_shrink(1.0)
                        .min_w_0()
                        .overflow_hidden()
                        .child(self.working_line_in_bar(card, p, cx)),
                )
            })
            .when(status_card.is_some(), |head| {
                head.child(div().flex_1().min_w(px(8.0 * s)))
            })
            .child(
                div()
                    .min_w_0()
                    .when(status_card.is_some(), |summary| summary.flex_shrink_0())
                    .flex()
                    .items_center()
                    .gap(px(12.0 * s))
                    .overflow_hidden()
                    .children(marks)
                    // Nothing counted and nothing to show: one quiet word, so the bar is not blank.
                    .when(quiet && !collapsible && status_card.is_none(), |summary| {
                        summary.child(div().text_color(p.muted).child("Nothing in progress"))
                    }),
            )
            .when(status_card.is_none(), |head| {
                head.child(div().flex_1().min_w(px(8.0 * s)))
            });
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
                        // Down while open, up while collapsed, turning between them.
                        .child(
                            svg()
                                .path("titlebar/chevron-down.svg")
                                .size(px(14.0 * s))
                                .flex_shrink_0()
                                .text_color(p.muted)
                                .with_transformation(gpui::Transformation::rotate(
                                    gpui::percentage(0.5 * (1.0 - turned.clamp(0.0, 1.0))),
                                )),
                        ),
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
            // A plain button, as the chevron beside it: no fill at rest, the usual fills on hover
            // and press, and pressed while the list it opens is shown.
            //
            // CDXC:Coordinators 2026-11-11 DECISION:
            // User: "make the button on the right Work items not have bg behind it when not
            // hovered so it's plain button".
            .hover(move |style| style.bg(fills.hover))
            .active(move |style| style.bg(fills.pressed))
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

    /// One dock row. Its first line is its status glyph, its question, the answers beside it when
    /// they fit and its age, which stays at the top right open or closed, with the row's ⋯ menu
    /// (Show in chat, Dismiss, Snooze, Mention) over it under the pointer. Closed it stays one
    /// line. Opened, it is its own card (one open at a time): the title, the work it belongs to as
    /// a small label under it, the whole description; the answers stay beside the title when they
    /// fit and go on a line under it when they do not, Reply after a divider. The answers and
    /// Reply are always one line: past its room the answers fold into one Choose an answer
    /// dropdown (work_actions.rs `answers_dropdown`).
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, picking on the dock rows page (rows-page.html) after a long question was squeezed
    /// into a narrow column beside five answers: an opened row's answers sit "beside the question
    /// when they fit, under it when they do not", where fit means they leave the question at least
    /// half the row (B); a closed row whose answers do not fit shows "the answers that fit, then
    /// +3" and stays one line (B); the icon buttons go "at the right end of the row's last line"
    /// (A, superseded by the ⋯ menu); a very long question shows "four lines, then more" (B,
    /// superseded: an opened row shows it whole). Also as drawn: the age stays
    /// at the top right, and the work item's name and "Nothing waits on it" share one line.
    ///
    /// Opening or shutting, the row eases between the two as the chat's folds do
    /// (disclosure_motion.rs, its timing, curve and the reduced motion setting): its height
    /// moves from the one line's to the card's while the line fades out and the card fades in
    /// over it, so the rows under it slide instead of jumping. Shutting does the same backwards,
    /// and a press part of the way through turns it around from where it is.
    ///
    /// CDXC:Coordinators 2026-11-12 DECISION:
    /// User: opening and closing each docked card animated, quickly, the way the chat's folds
    /// ease open and shut.
    fn dock_row(&self, row: &Value, p: &ChatAppearance, cx: &mut Context<Self>) -> AnyElement {
        let key = text(row, "key");
        let open_key = format!("dock-row:{key}");
        let drawn = self.dock_row_moving(row, p, cx);
        self.dock_row_reveal(&open_key, drawn)
    }

    /// A row the reader just opened, past the dock's height cap: as it grows, the dock's rows
    /// scroll just far enough that its end shows, never so far that its title leaves the top,
    /// with the transcript's own arithmetic for a fold opened by a reader scrolled up
    /// (disclosure_reveal.rs `reveal_distance`, the row's title being inside what is measured).
    /// It follows the row while it moves and lands once on its settled height; a row that fits
    /// moves nothing, and a row opened more than 1.5 seconds ago is left where the reader puts it.
    ///
    /// CDXC:Coordinators 2026-11-12 DECISION:
    /// User: when a docked row opens while the dock is at its height cap, scroll it into view, as
    /// the chat's folds do.
    fn dock_row_reveal(&self, open_key: &str, drawn: AnyElement) -> AnyElement {
        let Some((_, pressed)) = self
            .board
            .dock_reveal
            .as_ref()
            .filter(|(key, pressed)| key == open_key && pressed.elapsed().as_millis() <= 1500)
        else {
            return drawn;
        };
        let pressed = *pressed;
        let scroll = self.board.dock_scroll.clone();
        div()
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .child(drawn)
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        let view = scroll.bounds();
                        let distance = super::disclosure_reveal::reveal_distance(
                            view.top().as_f32(),
                            view.bottom().as_f32(),
                            bounds.top().as_f32(),
                            bounds.bottom().as_f32(),
                            0.0,
                        );
                        let offset = scroll.offset();
                        let max = scroll.max_offset().y.as_f32();
                        let target = (offset.y.as_f32() - distance).max(-max);
                        if distance > 0.5 && target < offset.y.as_f32() - 0.5 {
                            scroll.set_offset(gpui::point(offset.x, px(target)));
                            // The row's height and the list's room are a frame behind: look
                            // again until nothing is left to show.
                            if pressed.elapsed().as_millis() <= 1500 {
                                window.refresh();
                            }
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// One dock row at rest, or easing between its line and its card.
    fn dock_row_moving(
        &self,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = text(row, "key");
        let open_key = format!("dock-row:{key}");
        let page_row = row["page"] == true && row["open"].is_object();
        let open = self.expanded.contains(&open_key) && !page_row;
        // The shut line measures its height whenever it is drawn, kept between motions, so an
        // opening starts from exactly the line on screen.
        let shut_height = self.disclosure_floor(&open_key);
        let shut_id = gpui::SharedString::from(format!("dock-row-shut:{key}"));
        let open_id = gpui::SharedString::from(format!("dock-row-open:{key}"));
        let Some(frame) = self.disclosure_frame(&open_key, open, cx) else {
            let layer = self.dock_row_layout(row, open, p, cx);
            return if open {
                div().id(open_id).w_full().child(layer).into_any_element()
            } else {
                div()
                    .id(shut_id)
                    .w_full()
                    .child(super::disclosure_motion::measured(shut_height, layer))
                    .into_any_element()
            };
        };
        let factor = frame.log.clamp(0.0, 1.0);
        let card_height = self
            .disclosure_motion
            .borrow_mut()
            .height(&open_key, "card");
        let (shut, card) = (shut_height.get(), card_height.get());
        // Both layers keep the ids they have at rest, so nothing they hold is lost when the
        // motion starts or ends. The one the row is turning into lies on top and takes presses.
        let shut_layer = div()
            .id(shut_id)
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .opacity((1.0 - factor / 0.75).clamp(0.0, 1.0))
            .child(super::disclosure_motion::measured(
                shut_height,
                self.dock_row_layout(row, false, p, cx),
            ))
            .into_any_element();
        let card_layer = div()
            .id(open_id)
            .w_full()
            .flex_shrink_0()
            .opacity(((factor - 0.25) / 0.75).clamp(0.0, 1.0))
            .child(super::disclosure_motion::measured(
                card_height,
                self.dock_row_layout(row, true, p, cx),
            ))
            .into_any_element();
        let layers = if open {
            [shut_layer, card_layer]
        } else {
            [card_layer, shut_layer]
        };
        div()
            .relative()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .overflow_hidden()
            .map(|holder| {
                if shut > 0.0 && card > 0.0 {
                    holder.h(px(shut + (card - shut) * factor))
                } else if shut > 0.0 && factor < 0.5 {
                    holder.h(px(shut))
                } else {
                    holder
                }
            })
            .children(layers)
            .into_any_element()
    }

    /// One dock row drawn shut (`open` false, one line) or opened (its card), at rest
    /// (`dock_row` eases between the two).
    fn dock_row_layout(
        &self,
        row: &Value,
        open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let open_key = format!("dock-row:{key}");
        // A Ghostex page's row never opens to a second line: a click opens the page, and its other
        // actions are the ⋯ menu that takes the age's place under the pointer (`page_row_menu`).
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User: "for pages that need me, lets show 3 dots on hover (appears in place of the time
        // basically) where i can click to see show in chat/dismiss/snooze. don't make it expand
        // on click since it's useless when it expands".
        let page_row = row["page"] == true && row["open"].is_object();
        let open = open && !page_row;
        let (icon, color) = row_mark(row, p);
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
        // An ask with answers has Reply as its last answer (work_actions.rs `reply_answer`), and a
        // form or page has it after its Open: the words go to the agent tagged with the questions
        // it still asks (gx-chat-core `questionsReply`).
        //
        // CDXC:Coordinators 2026-11-12 DECISION:
        // User, of a form's row with only Open: "i can't reply to this or mention it :> pls fix".
        let reply_answer = ((!answers.is_empty() || action == Some("Open"))
            && row["reply"].is_object())
        .then(|| Self::reply_answer(&row["reply"], &text(row, "replied"), busy));
        // What fits: the answers may take the first line while the question keeps half the row.
        let row_width = match self.board.dock_width.get() {
            width if width > 0.0 => width,
            _ => DOCK_ROW_WIDTH_GUESS,
        } - 2.0 * DOCK_LIST_PAD
            - 2.0 * DOCK_ROW_PAD_X;
        // The age's slot, which the ⋯ takes under the pointer (`row_age_menu`): never narrower
        // than the ⋯, so the two swap without anything moving.
        let age_width = dock_text_width(&age, PROSE_SIZE, p, cx).max(AGE_SLOT_MIN) + DOCK_GAP;
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
        // Reply always shows: it is the one answer that takes anything, so a row keeps it one
        // click away.
        let reply_width = reply_answer.as_ref().map_or(0.0, |answer| {
            dock_button_width(&text(answer, "label"), true, false, p, cx)
                + 6.0
                + if open { 13.0 } else { 0.0 }
        });
        let all_width = buttons_width(&widths) + reply_width;
        let fit = all_width <= beside_room;
        // The answers, Reply and its divider are one line, never wrapped: when they do not all fit
        // (beside the title on a closed row, on their own line under an opened one's text), the
        // answers fold into one Choose an answer dropdown in their place. Measured, with the
        // dock's width at its last paint, so a resize decides again.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of an opened row whose Reply wrapped under four answers: "i dont like the choices
        // wrapping, please make them never wrap… should just show a dropdown". Supersedes the
        // closed row's "+N" chip.
        // Answers no wider than the dropdown stay as they are: folding them would gain nothing.
        let line_room = row_width - DOCK_LEAD;
        let choose_width =
            dock_button_width(super::work_actions::CHOOSE_ANSWER, true, false, p, cx);
        // An opened row's item pills lead its answers (`item_pills`): the answers fold first,
        // then the pills drop their ids, then they become one "+N" pill.
        let pills: Vec<Value> = if open {
            row["itemPills"].as_array().cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        let pill_room = pill_widths(&pills, p, cx);
        let fold = action.is_none()
            && !answers.is_empty()
            && buttons_width(&widths) > choose_width
            && !(if open {
                all_width + pill_room.0 <= line_room
            } else {
                fit
            });
        let answers_used = if fold {
            choose_width + reply_width
        } else {
            all_width
        };
        let pill_fit = pill_fit(pill_room, line_room - answers_used);
        let mut buttons: Vec<AnyElement> = match &action {
            Some(_) => self.work_row_action(&key, row, p, cx).into_iter().collect(),
            None if fold => vec![self.answers_dropdown(&key, &answers, busy, p, cx)],
            None => self.work_answer_buttons(&key, &answers, busy, p, cx),
        };
        let toggle = open_key.clone();
        if let Some(answer) = &reply_answer {
            // Opened, Reply stands apart from the answers after a thin divider.
            if open {
                buttons.push(answers_divider(p));
            }
            buttons.extend(self.work_answer_buttons(
                &format!("{key}:reply"),
                std::slice::from_ref(answer),
                busy,
                p,
                cx,
            ));
        }
        // Opened, the answers always take their own line under the text, so the title is the
        // only thing that shares its width with the age and the ⋯.
        let beside = !open;
        let detail = text(row, "body");
        let title_runs = &row["titleRuns"];
        let count = text(row, "count");
        let question = if page_row && !count.is_empty() {
            // The page's name, then how much it still asks; the name gives way first.
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .gap(px(6.0 * s))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(body)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(p.card_muted)
                        .child(count.to_string()),
                )
                .into_any_element()
        } else if !open {
            div()
                .flex_1()
                .min_w_0()
                .child(self.ask_title(
                    &key,
                    &title,
                    title_runs,
                    true,
                    body,
                    PROSE_SIZE,
                    PROSE_LINE_HEIGHT,
                    p,
                    cx,
                ))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .child(self.ask_title(
                    &key,
                    &title,
                    title_runs,
                    false,
                    body,
                    PROSE_SIZE,
                    PROSE_LINE_HEIGHT,
                    p,
                    cx,
                ))
                .into_any_element()
        };
        // Opened: the work it belongs to as a small label under the title, then the whole
        // description (or where a line of work stands). Nothing is cut; the row's own open and
        // shut hides it. It sits under the title line, from the title's left edge to the card's
        // right padding, not in a column beside the age and the ⋯.
        //
        // CDXC:Coordinators 2026-11-12 DECISION:
        // User, of an opened row whose description stopped where the age and the ⋯ sit: "It
        // should take up the full width of this card when I expand".
        //
        // CDXC:Coordinators 2026-11-12 DECISION:
        // User, of the thread line between an opened row's title and text ("looks out of place
        // in this case. and the spacing above it is smaller than below it"), picking the logo
        // alone: each thread of a line of work is its agent's logo at the left of the Done /
        // Reply line, its name in a tooltip, a click opening the thread. The title, the text and
        // the answers are then 6 px apart (two lines of text) and 12 px before the answers.
        let (thread_chips, other_chips): (Vec<Value>, Vec<Value>) = if open
            && !row["waitedBy"].is_array()
        {
            row["chips"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .partition(|chip| chip["kind"] == "thread")
        } else {
            (Vec::new(), Vec::new())
        };
        let thread_logos: Vec<AnyElement> = thread_chips
            .iter()
            .enumerate()
            .map(|(index, chip)| self.work_thread_logo(chip, &format!("dock:{key}:{index}"), p, cx))
            .collect();
        let opened_detail = open.then(|| {
            let mut column = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0 * s))
                .pl(px(DOCK_LEAD * s));
            if row["waitedBy"].is_array() {
                // An ask's own item, then the other items that wait on its answer: one line. With
                // item pills on the answers row, only its other parts (the steps) are left here.
                let parts = self.item_line(&key, row, p, cx);
                if !parts.is_empty() {
                    column = column.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_x(px(6.0 * s))
                            .overflow_hidden()
                            .text_size(px(PROSE_SIZE * s))
                            .line_height(px(20.0 * s))
                            .children(parts),
                    );
                }
            } else {
                // A line of work's threads and links, wrapping, with no separator between them.
                let chips: Vec<AnyElement> = other_chips
                    .iter()
                    .enumerate()
                    .map(|(index, chip)| {
                        self.work_card_part(chip, &format!("dock:{key}:{index}"), p, cx)
                    })
                    .collect();
                if !chips.is_empty() {
                    column = column.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_x(px(14.0 * s))
                            .gap_y(px(2.0 * s))
                            .children(chips),
                    );
                }
            }
            // The description stands apart from the title and its work item line, as a question
            // card's text stands under its head: `OPENED_ROW_GAP` with the column's own 2.
            //
            // CDXC:Coordinators 2026-11-12 DECISION:
            // User, of an opened row: "please add space between the title/description/choice
            // buttons they're too close together now". The gaps are the question card's
            // (question.rs: 12 from the head to the text, 12 from the text to the options).
            let mut first_body = true;
            // Two lines of text are 6 px apart (the title's 2 under it and 4), with a work item
            // line or a link between them the question card's 12.
            let has_line = row["waitedBy"].is_array() || !other_chips.is_empty();
            let body_lead = if has_line { OPENED_ROW_GAP - 2.0 } else { 4.0 };
            if !detail.is_empty() {
                first_body = false;
                column = column.child(div().mt(px(body_lead * s)).child(
                    self.ask_markdown(
                        &format!("detail:{key}"),
                        &detail,
                        p.card_muted,
                        PROSE_SIZE,
                        PROSE_LINE_HEIGHT,
                        p,
                        cx,
                    ),
                ));
            }
            // Where a line of work stands, on lines of its own: no dot can be left at a line's end.
            for line in row["detail"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|line| !line.is_empty())
            {
                column = column.child(
                    div()
                        .min_w_0()
                        .when(first_body, |line| line.mt(px(body_lead * s)))
                        .whitespace_normal()
                        .text_color(p.card_muted)
                        .child(line.to_string()),
                );
                first_body = false;
            }
            column
        });
        let mut head = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap(px(DOCK_GAP * s))
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
                        .items_center()
                        .gap(px(6.0 * s))
                        .children(buttons),
                );
            }
        } else {
            // The item pills at the left, the answers and Reply at the right edge.
            //
            // CDXC:Coordinators 2026-11-12 DECISION:
            // User: "move the answer buttons to bottom right. keep work item tag bottom left".
            under_answers = self.item_pill_elements(&key, &pills, pill_fit, p, cx);
            under_answers.extend(thread_logos);
            under_answers.push(div().flex_1().min_w_0().into_any_element());
            under_answers.extend(buttons);
        }
        // The row's other actions (Show in chat, Dismiss, Snooze, Mention) are one ⋯ menu that
        // takes the age's place while the pointer is over the row, closed or opened.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of the ⋯ at the right end of an opened row's answers: "please move this button to
        // be over the time in the card but only when i hover over the card (whether collapsed or
        // expanded)". The answers and Reply get its room.
        let hover = gpui::SharedString::from(format!("dock-row-hover:{key}"));
        let age_menu = self.row_age_menu(&key, &age, &hover, row, p, cx);
        if page_row {
            let page = row["open"].clone();
            let right_row = row.clone();
            let head = Self::dock_row_press(&key, open, head.child(age_menu), p)
                .group(hover.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(page.clone(), cx);
                }))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let rows = this.row_menu(&right_row, cx);
                        this.show_dock_row_menu(rows, event.position, window, cx);
                    }),
                );
            if let Some(page_id) = row["pageId"].as_str() {
                self.refresh_page_counts(page_id, cx);
            }
            return div().w_full().child(head).into_any_element();
        }
        let press = Self::dock_row_press(&key, open, head.child(age_menu), p).on_click(
            cx.listener(move |this, _, _, cx| {
                // One row open at a time: opening this one shuts the one that was open.
                if this.expanded.remove(&toggle) {
                    this.board.dock_reveal = None;
                } else {
                    this.expanded.retain(|key| !key.starts_with("dock-row:"));
                    this.expanded.insert(toggle.clone());
                    // Past the dock's height cap it scrolls into view as it opens.
                    this.board.dock_reveal = Some((toggle.clone(), web_time::Instant::now()));
                }
                cx.notify();
            }),
        );
        // Opened, only the title line opens and shuts the row: what sits under it takes a press
        // as text, so a drag selects, a double-click picks a word and Cmd+C copies.
        //
        // CDXC:Coordinators 2026-11-12 DECISION:
        // User, of an opened row whose command he could not select: "let's make collapsing
        // require clicking on the header of the card not the inner part please".
        let head = match opened_detail {
            Some(detail) => div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .child(press)
                .child(
                    // `pl` again, the title's indent with the row's own padding: `px` would
                    // replace the indent the column carries.
                    detail
                        .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                        .pr(px(DOCK_ROW_PAD_X * s))
                        // The answers' line sits `OPENED_ROW_GAP` under the text.
                        .pb(px(if under_answers.is_empty() {
                            6.0
                        } else {
                            OPENED_ROW_GAP
                        } * s)),
                )
                .into_any_element(),
            None => press.into_any_element(),
        };
        if !open {
            return div().w_full().group(hover).child(head).into_any_element();
        }
        // Opened: its own card, a little lighter than the dock, with the answers on a line under
        // its text.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, picking on the opened row page (opened-row-page.html): "C. Answers and one menu"
        // (only the answers stay buttons; Show in chat, Dismiss, Snooze and Mention are one ⋯ menu
        // at the right end of the answers), Show more removed ("remove show more, they can
        // collapse this item if they dont want to read it"), the work item's name as a small label
        // under the title, Reply after a divider apart from the answers, and an opened row as its
        // own card with only one open at a time.
        let mut card = div()
            .w_full()
            .group(hover)
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.foreground.opacity(if p.light { 0.10 } else { 0.10 }))
            .bg(if p.light {
                gpui::white().opacity(0.6)
            } else {
                p.foreground.opacity(0.055)
            })
            .child(head);
        if !under_answers.is_empty() {
            card = card.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                    .pr(px(DOCK_ROW_PAD_X * s))
                    .pb(px(8.0 * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .children(under_answers),
                    ),
            );
        }
        // Its spacing above and below is padding, not margin, so the height `dock_row` measures
        // while it moves is the whole room it takes.
        div()
            .w_full()
            .py(px(4.0 * s))
            .child(card)
            .into_any_element()
    }

    /// A dock row's clickable surface: its title line (opened, only that line; what sits under it
    /// is the row's text, `dock_row`).
    fn dock_row_press(
        key: &str,
        open: bool,
        line: gpui::Div,
        p: &ChatAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(gpui::SharedString::from(format!("dock-row-head:{key}")))
            .w_full()
            .min_w_0()
            .px(px(DOCK_ROW_PAD_X * s))
            .pt(px(6.0 * s))
            .pb(px(if open { 2.0 } else { 6.0 } * s))
            .rounded(px(8.0 * s))
            .chat_cursor_pointer()
            // Opened too: the title line is what shuts the row, so it lights up under the pointer
            // as the rows do; what sits under it takes no hover (it is for selecting text).
            //
            // CDXC:Coordinators 2026-11-12 DECISION:
            // User, of an opened row's title: "show bg hover effect change when i hover the title
            // of this so i know clicking collapses it please".
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .child(line)
    }

    /// A row's ⋯ (and a page row's right-click) menu: Show in chat, Dismiss and Snooze
    /// (`page_row_menu`), then Mention in chat box, which puts the item's, form's or page's
    /// reference in the chat box where the caret is now.
    fn row_menu(&self, row: &Value, cx: &Context<Self>) -> Vec<Value> {
        let mut rows = page_row_menu(row);
        let mention = text(row, "mention");
        if !mention.is_empty() {
            let selection = self
                .input
                .as_ref()
                .map(|input| input.read(cx).selected_range())
                .unwrap_or(self.draft.len()..self.draft.len());
            let utf16 = |at: usize| {
                self.draft[..at.min(self.draft.len())]
                    .encode_utf16()
                    .count()
            };
            rows.push(json!({
                "label": "Mention in chat box",
                "command": {
                    "type": "insertBoardReference",
                    "reference": mention,
                    "text": self.draft,
                    "start": utf16(selection.start),
                    "end": utf16(selection.end),
                },
            }));
        }
        rows
    }

    /// An opened row's item pills (gx-chat-core `item_pills`): a dot in the item's state colour
    /// (an item that waits on the answer in the blocked red) and its short id, its whole name and
    /// state in the tooltip, a click opening it in the Work panel. `PillFit::Dots` drops the ids,
    /// `PillFit::Plus` draws one "+N" pill whose click lists them.
    pub(super) fn item_pill_elements(
        &self,
        key: &str,
        pills: &[Value],
        fit: PillFit,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        if pills.is_empty() {
            return Vec::new();
        }
        let shell = |id: String| {
            div()
                .id(gpui::SharedString::from(id))
                .flex_shrink_0()
                .h(px(20.0 * s))
                .px(px(PILL_PAD * s))
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .rounded_full()
                .border_1()
                .border_color(p.foreground.opacity(0.10))
                .bg(p.foreground.opacity(0.04))
                .text_size(px(PROSE_SIZE * s))
                .line_height(px(18.0 * s))
                .whitespace_nowrap()
                .text_color(p.card_muted)
                .chat_cursor_pointer()
                .hover(|style| {
                    style
                        .bg(p.foreground.opacity(0.08))
                        .text_color(p.foreground)
                })
        };
        if fit == PillFit::Plus {
            let tips: Vec<String> = pills.iter().map(|pill| text(pill, "tip")).collect();
            let rows: Vec<Value> = pills
                .iter()
                .map(|pill| json!({ "label": text(pill, "tip"), "command": pill["command"] }))
                .collect();
            let tip = tips.join("\n");
            return vec![
                shell(format!("item-pills:{key}"))
                    .child(format!("+{}", pills.len()))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            let at = event.position();
                            let in_main = this
                                .open_main_window(cx)
                                .is_none_or(|main| main == window.window_handle());
                            if in_main {
                                this.show_chat_menu_at(rows.clone(), at, 260.0, window, cx);
                            } else {
                                this.show_chat_menu_in_window_at(
                                    rows.clone(),
                                    at,
                                    260.0,
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
                    .into_any_element(),
            ];
        }
        pills
            .iter()
            .enumerate()
            .map(|(index, pill)| {
                let (_, color) = status_look(&text(pill, "state"), p);
                let tip = text(pill, "tip");
                let command = pill["command"].clone();
                shell(format!("item-pill:{key}:{index}"))
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(PILL_DOT * s))
                            .rounded_full()
                            .bg(color),
                    )
                    .when(fit == PillFit::Full, |pill_div| {
                        pill_div.child(text(pill, "id"))
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// An opened ask row's line: its own work item's name, then, when one other item waits on
    /// its answer, "· <that item> waits on this", or "· N others wait" that opens the list of
    /// them. It stays one line: a name too long is cut with an ellipsis and whole in its tooltip,
    /// the other names giving way before the row's own item does, and the words and dots never
    /// do. Its other parts (the steps to follow) come after.
    ///
    /// CDXC:Coordinators 2026-11-10 DECISION:
    /// User, of a line that read "<item> · <item> waits on this" with its dot left hanging: only
    /// the item's name (the row's mark says whether it holds that item up); one other item that
    /// waits is named, more are counted; and the line is cut, never wrapped.
    fn item_line(
        &self,
        key: &str,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let chips: Vec<Value> = row["chips"].as_array().cloned().unwrap_or_default();
        let dot = || {
            div()
                .flex_shrink_0()
                .text_color(p.muted)
                .opacity(0.6)
                .child("\u{b7}")
                .into_any_element()
        };
        // A name with its stack icon, cut at the end when it must give way (`shrink`, against
        // the others' weights, down to `floor`) and whole in its tooltip; `command` makes it a
        // link. Another item's name gives way first but keeps a few words.
        let name = |id: String, label: String, shrink: f32, floor: f32, command: Value| {
            let tip = label.clone();
            div()
                .id(gpui::SharedString::from(id))
                .flex_shrink(shrink)
                .min_w(px(floor * s))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .text_color(p.card_muted)
                .child(
                    svg()
                        .path("titlebar/stack.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(div().min_w_0().truncate().child(label))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
                .when(command.is_object(), |part| {
                    part.chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.invoke(command.clone(), cx);
                        }))
                })
                .into_any_element()
        };
        let mut parts = Vec::new();
        // The item pills on the answers row carry the item and what waits on it.
        let pilled = row["itemPills"]
            .as_array()
            .is_some_and(|pills| !pills.is_empty());
        for chip in chips
            .iter()
            .filter(|chip| text(chip, "kind") == "item" && !pilled)
        {
            parts.push(name(
                format!("dock-item:{key}"),
                text(chip, "label"),
                1.0,
                60.0,
                chip["command"].clone(),
            ));
        }
        let waited: Vec<Value> = if pilled {
            Vec::new()
        } else {
            row["waitedBy"].as_array().cloned().unwrap_or_default()
        };
        match waited.as_slice() {
            [] => {}
            [other] => {
                if !parts.is_empty() {
                    parts.push(dot());
                }
                parts.push(name(
                    format!("dock-waits:{key}"),
                    text(other, "title"),
                    8.0,
                    110.0,
                    other["command"].clone(),
                ));
                parts.push(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(p.card_muted)
                        .child("waits on this")
                        .into_any_element(),
                );
            }
            several => {
                if !parts.is_empty() {
                    parts.push(dot());
                }
                let rows: Vec<Value> = several
                    .iter()
                    .map(|other| {
                        json!({ "label": text(other, "title"), "command": other["command"] })
                    })
                    .collect();
                parts.push(
                    div()
                        .id(gpui::SharedString::from(format!("dock-waits:{key}")))
                        .role(gpui::Role::Button)
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(p.card_muted)
                        .underline()
                        .chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .child(format!("{} others wait", several.len()))
                        .on_click(
                            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                cx.stop_propagation();
                                let at = event.position();
                                let in_main = this
                                    .open_main_window(cx)
                                    .is_none_or(|main| main == window.window_handle());
                                if in_main {
                                    this.show_chat_menu_at(rows.clone(), at, 240.0, window, cx);
                                } else {
                                    this.show_chat_menu_in_window_at(
                                        rows.clone(),
                                        at,
                                        240.0,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                        .into_any_element(),
                );
            }
        }
        // The steps to follow, and any other part, after the names.
        for (index, chip) in chips
            .iter()
            .enumerate()
            .filter(|(_, chip)| text(chip, "kind") != "item")
        {
            parts.push(
                div()
                    .flex_shrink_0()
                    .child(self.work_card_part(chip, &format!("dock:{key}:{index}"), p, cx))
                    .into_any_element(),
            );
        }
        parts
    }

    /// A work item ask's title: one plain line cut at its end (`one_line`, a closed row or card),
    /// or wrapping (opened), with its references (`#562`, a Markdown link) clickable the way a
    /// question's are (`link_words`, from the core's `titleRuns`); a click on a link opens it
    /// only, never the row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask_title(
        &self,
        key: &str,
        title: &str,
        runs: &Value,
        one_line: bool,
        color: gpui::Hsla,
        size: f32,
        line_height: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        if !runs.is_array() {
            return div()
                .min_w_0()
                .text_size(px(size * s))
                .line_height(px(line_height * s))
                .text_color(color)
                .when(one_line, |title| title.truncate())
                .when(!one_line, |title| title.whitespace_normal())
                .child(title.to_string())
                .into_any_element();
        }
        let words = self.link_words(
            &format!("ask-title:{key}"),
            super::link_words::LinkWords {
                plain: title.to_string(),
                runs,
                size,
                line_height,
                weight: FontWeight::NORMAL,
                color,
            },
            p,
            cx,
        );
        if !one_line {
            return words;
        }
        // One line: the words past the row's width wrap out of sight below it.
        div()
            .min_w_0()
            .h(px(line_height * s))
            .overflow_hidden()
            .child(words)
            .into_any_element()
    }

    /// A work item ask's description as the chat renders a message's text (`text_view_look`, the
    /// same Markdown view: its line breaks and paragraphs, bold, italic, code, links, lists, and
    /// `#562` linking to the project's GitHub), in the quieter colour at `size`, whole. Headings
    /// are bold lines at the text's size, and a table or picture shows as its plain text
    /// (`compact_markdown`).
    ///
    /// CDXC:Coordinators 2026-11-11 DECISION:
    /// User, of an ask's description drawn as one flat run: "no new lines or markdown rendering
    /// here which is wrong pls fix. also please make links like #562 clickable pls like we did in
    /// regular chat (we have code for this already)".
    ///
    /// CDXC:Coordinators 2026-11-11 DECISION:
    /// User, of the four-line cut and its Show more: "remove show more, they can collapse this
    /// item if they dont want to read it". An opened row or card shows its whole description; its
    /// own open and shut hides it. Supersedes the four lines then "more" rule.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask_markdown(
        &self,
        id: &str,
        words: &str,
        color: gpui::Hsla,
        size: f32,
        line_height: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.text_view_look(
            format!("ask-markdown:{id}"),
            compact_markdown(words),
            &Value::Null,
            p,
            super::rich_markdown::TextLook {
                color,
                semibold: false,
                size,
                line_height,
                // An opened row's text keeps the arrow, links the hand.
                //
                // CDXC:Coordinators 2026-11-12 DECISION:
                // User: "make cursor shown on the text inside the card not show caret please".
                arrow: true,
            },
            cx,
        )
    }

    /// A dock row's right end: its age, and over it while the row (`hover`, closed or opened) is
    /// hovered the ⋯ that opens its menu (`row_menu`), as a board card's age and ⋯
    /// (coordinator_board.rs `age_slot`). The ⋯ stays while its menu is open. The slot keeps one
    /// width and height either way, so nothing moves when they swap.
    fn row_age_menu(
        &self,
        key: &str,
        age: &str,
        hover: &gpui::SharedString,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let pinned = self
            .board
            .dock_menu_row
            .as_ref()
            .is_some_and(|(row, menu)| {
                row == key
                    && self
                        .option_menu
                        .as_ref()
                        .is_some_and(|open| open.entity_id() == *menu)
            });
        let row = row.clone();
        let menu_key = key.to_owned();
        div()
            .relative()
            .flex_shrink_0()
            .min_w(px(AGE_SLOT_MIN * s))
            .h(px(PROSE_LINE_HEIGHT * s))
            .flex()
            .items_center()
            .justify_end()
            .child(
                div()
                    .text_color(p.muted)
                    .when(pinned, |age| age.opacity(0.0))
                    .group_hover(hover.clone(), |style| style.opacity(0.0))
                    .child(age.to_string()),
            )
            .child(
                div()
                    .id(gpui::SharedString::from(format!("dock-row-menu:{key}")))
                    .role(gpui::Role::Button)
                    .aria_label("More actions")
                    .absolute()
                    .right(px(-2.0 * s))
                    .top(px((PROSE_LINE_HEIGHT - 22.0) / 2.0 * s))
                    .w(px(AGE_SLOT_MIN * s))
                    .h(px(22.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0 * s))
                    .when(!pinned, |dots| dots.opacity(0.0))
                    .when(pinned, |dots| dots.bg(p.foreground.opacity(0.08)))
                    .group_hover(hover.clone(), |style| style.opacity(1.0))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("More actions").build(window, cx)
                    })
                    .child(
                        svg()
                            .path("titlebar/dots.svg")
                            .size(px(14.0 * s))
                            .text_color(p.card_muted),
                    )
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            let rows = this.row_menu(&row, cx);
                            this.show_dock_row_menu(rows, event.position(), window, cx);
                            this.board.dock_menu_row = this
                                .option_menu
                                .as_ref()
                                .map(|menu| (menu_key.clone(), menu.entity_id()));
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    /// Opens a dock row's menu at `at` with the app's dropdown, inside the window it was pressed
    /// in (the chat's, or the floating Work card's).
    fn show_dock_row_menu(
        &mut self,
        rows: Vec<Value>,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            return;
        }
        let in_main = self
            .open_main_window(cx)
            .is_none_or(|main| main == window.window_handle());
        if in_main {
            self.show_chat_menu_at(rows, at, 180.0, window, cx);
        } else {
            self.show_chat_menu_in_window_at(rows, at, 180.0, window, cx);
        }
    }

    /// Asks the page's gxserver how much of it is answered, at most every few seconds while its
    /// row is drawn, so a pick made in the page shows in the row's count.
    pub(super) fn refresh_page_counts(&self, page_id: &str, cx: &mut Context<Self>) {
        let now = web_time::Instant::now();
        let due = self
            .board
            .page_counts_asked
            .borrow()
            .get(page_id)
            .is_none_or(|asked| now.duration_since(*asked) >= PAGE_COUNTS_EVERY);
        if !due {
            return;
        }
        self.board
            .page_counts_asked
            .borrow_mut()
            .insert(page_id.to_string(), now);
        let action = json!({ "type": "loadPageQuestionCounts", "pageId": page_id });
        let chat = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = chat.update(cx, |this, cx| this.invoke(action, cx));
        });
    }

    /// A column of cards, or `None` when there are none. `gap` spaces them when given (an opened
    /// Quiet View line's rail, quiet_view.rs), already scaled.
    pub(super) fn work_card_stack(
        &self,
        cards: &[&Value],
        gap: Option<f32>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        // A stack of work item update cards is spaced as the chat's other one-line cards are.
        let gap = gap.unwrap_or_else(|| {
            if cards.iter().all(|card| card["lead"].is_string()) {
                super::cards::SINGLE_LINE_CARD.stack_gap(p)
            } else {
                6.0 * p.scale
            }
        });
        (!cards.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap(px(gap))
                .children(cards.iter().map(|card| {
                    let drawn = if card["kind"] == "moving" {
                        self.work_moving_row(card, p, cx)
                    } else if card["lead"].is_string() {
                        self.work_turn_card(card, p, cx)
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
    /// A thread of an opened dock row as its agent's logo alone, at the left of the answers' line:
    /// its name (and state) is the tooltip, and a click opens the thread as the line's name did.
    fn work_thread_logo(
        &self,
        chip: &Value,
        id: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let label = text(chip, "label");
        let state = text(chip, "state");
        let command = chip["command"].clone();
        let agent = text(chip, "agentIcon");
        let (icon, icon_color) = if agent.is_empty() {
            (chip_icon("thread").to_string(), p.muted)
        } else {
            (
                format!("agent-icons/{agent}.svg"),
                super::new_session_welcome::brand_logo_color(&agent, p),
            )
        };
        let tip = if state.is_empty() {
            label.clone()
        } else {
            format!("{label}, {state}")
        };
        let tip_text = tip.clone();
        div()
            .id(gpui::SharedString::from(format!("work-thread-logo:{id}")))
            .size(px(28.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .rounded(px(8.0 * s))
            .aria_label(tip)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tip_text.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .when(command.is_object(), |logo| {
                logo.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
    }

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

    /// What a coordinator's turn did to one of its work items, as the chat's other one-line cards
    /// look (the "Message from" card's shape, cards.rs `SINGLE_LINE_CARD`): the work items icon in
    /// the tile with the item's state mark as its badge, "Work item updated" then the item's title
    /// set apart and the line that changed, the time, and the state it is in as a pill. A click
    /// opens the item in the Work panel (`jumpToWorkCard`).
    fn work_turn_card(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let shape = &*super::cards::SINGLE_LINE_CARD;
        let status = match text(card, "status").as_str() {
            "" => text(card, "foldStatus"),
            status => status.to_string(),
        };
        let (mark, color) = status_look(&status, p);
        let lead = text(card, "lead");
        let title = text(card, "title");
        let detail = text(card, "detail");
        let label = text(card, "statusLabel");
        let time = text(card, "time");
        let command = card["command"].clone();
        // The whole of it on hover, and what a click does.
        let mut said = if title.is_empty() {
            lead.clone()
        } else {
            format!("{lead}: {title}")
        };
        if !detail.is_empty() {
            said = format!("{said}\n{detail}");
        }
        if command.is_object() {
            said = format!("{said}\nClick to open it in the Work panel.");
        }
        let tip = said.clone();
        let row = div()
            .h(px(shape.header_height(s)))
            .px(px(shape.padding_x * s))
            .flex()
            .items_center()
            .gap(px(10.0 * s))
            .min_w_0()
            .child(
                shape
                    .tile("titlebar/layout-board-split.svg", p)
                    .child(shape.mark_badge(mark, color, p)),
            )
            .child(
                shape
                    .title(div().flex_1().min_w_0().flex().items_center(), p)
                    .gap(px(6.0 * s))
                    .overflow_hidden()
                    // The lead-in is quieter than the item's title it names; alone (a turn that
                    // changed many items) it is the title.
                    .child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_color(if title.is_empty() {
                                p.foreground
                            } else {
                                p.card_muted
                            })
                            .child(lead),
                    )
                    .when(!title.is_empty(), |line| {
                        line.child(
                            div()
                                .flex_shrink(1.0)
                                .min_w(px(80.0 * s))
                                .truncate()
                                .text_color(p.foreground)
                                .child(title),
                        )
                    })
                    // The line that changed gives way first.
                    .when(!detail.is_empty(), |line| {
                        line.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::NORMAL)
                                .text_size(px(13.0 * s))
                                .text_color(p.muted)
                                .child(detail.replace('\n', " ")),
                        )
                    }),
            )
            .when(!time.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(time),
                )
            })
            .when(!label.is_empty(), |row| {
                row.child(shape.pill(None, Some(label), color, p))
            });
        let hover = super::cards::card_hover_fill(p);
        shape
            .shell(div(), p)
            .id(gpui::SharedString::from(format!(
                "work-turn-card:{}",
                text(card, "key")
            )))
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .role(gpui::Role::Button)
            .aria_label(said)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
            .child(row)
            .when(command.is_object(), |card| {
                card.chat_cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
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
