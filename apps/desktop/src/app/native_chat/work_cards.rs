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
use super::pills_strip::STRIP_PAD_X;
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
    if status == "approved" {
        return ("titlebar/shield-check.svg", gpui::rgb(0x2fa865).into());
    }
    if status == "denied" {
        return (
            "titlebar/shield-x.svg",
            gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
        );
    }
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
            PILL_PAD * 2.0
                + PILL_DOT
                + 4.0
                + dock_text_width(&text(pill, "id"), PROSE_SIZE, p, cx)
                + 2.0
        })
        .sum();
    let dots = (PILL_PAD * 2.0 + PILL_DOT + 2.0) * pills.len() as f32;
    let plus =
        PILL_PAD * 2.0 + dock_text_width(&format!("+{}", pills.len()), PROSE_SIZE, p, cx) + 2.0;
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
pub(super) fn answers_divider(p: &ChatAppearance) -> AnyElement {
    div()
        .flex_shrink_0()
        .w(px(1.0))
        .h(px(16.0 * p.scale))
        .mx(px(3.0 * p.scale))
        .bg(p.foreground.opacity(0.10))
        .into_any_element()
}

/// The hairline across the Inbox drawer above a Snoozed or Recent group header (Q4969 C), in
/// the open header's divider colour and reaching the drawer's sides past the rows' inset. Clear
/// (same space) while the group is closed.
fn dock_group_divider(open: bool, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .h(px(1.0))
        .mx(px(-DOCK_LIST_PAD * s))
        .mt(px(4.0 * s))
        .mb(px(2.0 * s))
        .when(open, |line| line.bg(p.composer_border))
        .into_any_element()
}

/// A Snoozed or Recent group header's label (Q4969 C): small, uppercase and medium weight, in
/// the header's muted colour.
fn dock_group_label(label: &str, p: &ChatAppearance) -> gpui::Div {
    div()
        .text_size(px(PROSE_SIZE * GROUP_LABEL_SCALE * p.scale))
        .font_weight(FontWeight::MEDIUM)
        .child(label.to_uppercase())
}

/// A page row's ⋯ menu (`dock_row_look`): Show in chat, Mark answered, Dismiss (or Mark as read) and Snooze with its lengths, as the
/// app's dropdown rows (`{label, command}`, a submenu's under `children`).
fn page_row_menu(row: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    // A snoozed row's Unsnooze (gx-chat-core `dock_with`'s `snoozed`): back under Needs you now.
    if row["unsnooze"].is_object() {
        rows.push(json!({ "label": "Unsnooze", "iconPath": "titlebar/alarm-off.svg", "command": row["unsnooze"] }));
    }
    if row["showInChat"].is_object() {
        rows.push(json!({ "label": "Show in chat", "iconPath": "titlebar/message-share.svg", "command": row["showInChat"] }));
    }
    // Mark answered, beside Dismiss: one answered in the chat box closes by hand
    // (gx-chat-core `questions::later::mark_answered`).
    if row["markAnswered"].is_object() {
        rows.push(json!({ "label": "Mark answered", "iconPath": "titlebar/circle-check.svg", "command": row["markAnswered"] }));
    }
    // Dismiss, or on a row that asks no question gx-chat-core's `dismissLabel`: Seen on a page
    // that only waits to be seen, Not needed on a to-do. None sends the agent anything now; Seen
    // and Not needed show an Undo toast. This menu (and the phone's swipe) is the only place they
    // live: the row has no button of its own for them (gx-chat-core work_cards.rs, Q4917).
    if row["dismiss"].is_object() {
        let label = row["dismissLabel"].as_str().unwrap_or("Dismiss");
        let icon = if super::work_actions::is_clear_label(label) {
            "titlebar/check.svg"
        } else {
            "titlebar/x.svg"
        };
        rows.push(json!({
            "label": label,
            "iconPath": icon,
            "command": row["dismiss"],
        }));
    }
    let snooze: Vec<Value> = row["snooze"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|choice| json!({ "label": text(choice, "label"), "command": choice["command"] }))
        .collect();
    if !snooze.is_empty() {
        rows.push(json!({
            "label": "Snooze",
            "iconPath": "modals/settings/moon.svg",
            "hotkeyAction": "inboxSnooze",
            "children": snooze,
        }));
    }
    rows
}

/// What a dock row's Copy as Markdown copies, "" for a row that has none: a work item ask's or a
/// one-click question's title as a heading, its detail, and its answers line ("Answers: A, B
/// (recommended: A)"); a form's title and each question it still asks (`questionForms`, its
/// waiting tags from `reply.tag`), each with its detail and answers; a page's title (a link to
/// it when its mention carries one) and the tags it still asks. The phone's `rowMarkdown`
/// (apps/mobile/app/src/chat/native/cards/workItemParts.tsx) copies the same.
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// User: a work item ask in the dock needs "Copy as Markdown" in its ⋯ and right-click menu,
/// copying its title and full detail as Markdown plus its answers; form and page rows copy their
/// title and each open question.
fn row_markdown(row: &Value, forms: &Value) -> String {
    let answers = |choices: &[Value]| {
        let labels: Vec<&str> = choices
            .iter()
            .filter_map(|choice| choice["label"].as_str())
            .filter(|label| !label.is_empty())
            .collect();
        if labels.is_empty() {
            return String::new();
        }
        let recommended: Vec<&str> = choices
            .iter()
            .filter(|choice| choice["recommended"] == true)
            .filter_map(|choice| choice["label"].as_str())
            .collect();
        let mut line = format!("Answers: {}", labels.join(", "));
        if !recommended.is_empty() {
            line.push_str(&format!(" (recommended: {})", recommended.join(", ")));
        }
        line
    };
    let join = |parts: Vec<String>| {
        parts
            .into_iter()
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let str_of = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
    let title = str_of(row, "title");
    let title = title.trim();
    let buttons = row["buttons"].as_array().cloned().unwrap_or_default();
    match str_of(row, "kind").as_str() {
        "ask" | "question" => join(vec![
            format!("## {title}"),
            text(row, "body").to_string(),
            answers(&buttons),
        ]),
        "form" if row["page"] == true => {
            // Its `titleLink` is `[Name](url) (Q12 to Q14)` (gx-chat-core `questions_mention`), or
            // the name in quotes when the page has no address: the heading is the link, else the
            // title.
            let tags = str_of(&row["reply"], "tag");
            let mention = str_of(row, "titleLink");
            let link = match mention.rfind(" (") {
                Some(at) if !tags.is_empty() && mention.ends_with(')') => &mention[..at],
                _ => mention.as_str(),
            }
            .trim_end();
            let heading = if link.starts_with('[') && link.ends_with(')') {
                link
            } else {
                title
            };
            join(vec![
                format!("## {heading}"),
                if tags.is_empty() {
                    String::new()
                } else {
                    format!("Open questions: {tags}")
                },
            ])
        }
        "form" => {
            let form = &forms[str_of(&row["reply"], "questions").as_str()];
            let tags = str_of(&row["reply"], "tag");
            let waiting: Vec<&str> = tags.split_whitespace().collect();
            let mut parts = vec![format!("## {title}")];
            for question in form["questions"].as_array().into_iter().flatten() {
                let tag = str_of(question, "tag");
                if !waiting.is_empty() && !waiting.contains(&tag.as_str()) {
                    continue;
                }
                let name = str_of(question, "title");
                let name = name.trim();
                let heading = if tag.is_empty() {
                    name.to_string()
                } else {
                    format!("{tag} {name}")
                };
                let choices = question["choices"].as_array().cloned().unwrap_or_default();
                parts.push(join(vec![
                    format!("### {heading}"),
                    text(question, "detail").to_string(),
                    answers(&choices),
                ]));
            }
            join(parts)
        }
        _ => String::new(),
    }
}

/// A dock row's mark: a Ghostex page that needs him is a plain page in the need-you pink (`page`
/// on the row, from gx-chat-core), a question form that needs him the checklist in that pink;
/// everything else its status's mark (`status_look`). Every row
/// that is a page takes its mark here.
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User: "please change the icon of the ones that need me but are html page to a page icon
/// (without html written there in the icon) but colored pink". A question keeps its circle.
pub(super) fn row_mark(row: &Value, p: &ChatAppearance) -> (&'static str, gpui::Hsla) {
    let status = text(row, "status");
    let kind = text(row, "kind");
    let (icon, color) = status_look(&status, p);
    if kind == "threadWait" {
        return match text(row, "waitKind").as_str() {
            "approval" => (
                "titlebar/shield-alert.svg",
                gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
            ),
            "signIn" | "usageLimit" => (
                "titlebar/alert-triangle.svg",
                gpui::rgb(crate::app::native_sidebar::status::WORKING_COLOR).into(),
            ),
            _ if row["needsApproval"] == true => (
                "titlebar/shield-alert.svg",
                gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
            ),
            _ => (icon, color),
        };
    }
    if kind == "approval" {
        return (
            "titlebar/shield-alert.svg",
            gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
        );
    }
    if kind == "approvalSettled" {
        return if status == "denied" || status == "dropped" {
            (
                "titlebar/shield-x.svg",
                gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
            )
        } else {
            ("titlebar/shield-check.svg", gpui::rgb(0x2fa865).into())
        };
    }
    if kind == "pageBundle" {
        return (
            "titlebar/stack.svg",
            gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into(),
        );
    }
    if row["page"] == true && status == "needs" {
        return ("titlebar/file.svg", color);
    }
    // A question form that needs him: the visuals list's checklist (questions_list.rs
    // `kind_icon`), in the same pink.
    //
    // CDXC:Coordinators 2026-11-12 DECISION:
    // User, of a form's row that showed the question ring: "use this icon for this one when it's
    // a question form (keep pink color)". Work item asks and pages keep theirs.
    if kind == "form" && row["page"] != true && status == "needs" {
        return ("titlebar/list-check.svg", color);
    }
    // Q4761 all-literal left type icons: to-dos get a checkbox, questions get a help-circle.
    if status == "needs" && (row["needsYou"] == true || text(row, "key").starts_with("needs-you:"))
    {
        return ("titlebar/checkbox.svg", color);
    }
    if status == "needs" {
        return ("titlebar/help-circle.svg", color);
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

/// How a dock row is drawn (`dock_row_look`): the dock's own (the default), or a Work panel Needs
/// you card's (coordinator_board.rs `board_dock_card`).
#[derive(Default)]
pub(super) struct DockRowLook {
    /// The row's width (unscaled) with its sides; `None` for the dock's.
    pub width: Option<f32>,
    /// The title wraps over as many lines as it needs, with the answers on the line under it.
    pub wrap: bool,
    /// Leads the row's ids, so the same row drawn in the dock and in the panel stays apart.
    pub scope: &'static str,
    /// What takes the age's place (the panel card's age and ⋯); `None` for the dock's age and
    /// ⋯ with its right-click menu.
    pub age: Option<AnyElement>,
    /// What a click on the row opens in place of the dock's (its modal, or the page).
    pub click: Option<Value>,
    /// What shows in the answers' place (a thread's relayed approval buttons).
    pub buttons: Option<AnyElement>,
    /// What an ⌥-click puts in the chat box (a Work panel card's `reference`), "" for none.
    pub reference: String,
    /// Keyboard-focused row in Session Inbox triage mode (Q4687).
    pub focused: bool,
    /// A row under a Snoozed or Recent group header: its title one step smaller and dimmer
    /// (`GROUP_ROW_TITLE_SCALE`), in the same columns as the pings above (Q4969 D).
    pub quiet: bool,
}

/// A Snoozed or Recent group row's title size against a ping's (Q4969 D: one step quieter).
const GROUP_ROW_TITLE_SCALE: f32 = 0.93;
/// A Snoozed or Recent group row's title alpha over the row's own dimming (works in light and
/// dark, where the muted colours differ).
const GROUP_ROW_TITLE_ALPHA: f32 = 0.82;
/// A group header's label size against a ping's title (Q4969 C: a small uppercase label).
const GROUP_LABEL_SCALE: f32 = 0.80;

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

/// What a row shows in its answers' place: Details for a question with details to read first
/// (its answers wait in the panel, gx-chat-core `details_fields`), Answer (or Open for a page that
/// asks nothing) for a page or form, Reply for a question with no preset answers
/// (`work_actions.rs` `work_row_action`).
fn dock_row_action(row: &Value) -> Option<&'static str> {
    if row["details"] == true {
        Some("Details")
    } else if row["open"].is_object() {
        Some(if text(row, "openLabel") == "Answer" {
            "Answer"
        } else {
            "Open"
        })
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
/// `NativeChatView::expanded`'s key for the dock's Snoozed row opened (`work_dock_snoozed`).
const DOCK_SNOOZED_ROWS: &str = "dock-rows:snoozed";
/// `NativeChatView::expanded`'s key for the dock's Recent drawer opened (`work_dock_recent`).
const DOCK_RECENT_ROWS: &str = "dock-rows:recent";
/// Legacy keys from before Done and Handled merged into Recent; migrated on read and toggle.
const DOCK_LEGACY_DONE_ROWS: &str = "dock-rows:done";
const DOCK_LEGACY_ANSWERED_ROWS: &str = "dock-rows:answered";
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
        "usageLimit" => "titlebar/clock.svg",
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
    /// Which card the approval row in the dock stands for (`"notice"` or `"prompt"`, gx-chat-core
    /// `approvalInDock`), or "" when the card itself is drawn: also while the chat box is
    /// maximized, which draws no dock, so an approval is never waiting without being drawn.
    pub(super) fn approval_in_dock(&self) -> String {
        if self.maximized_window.is_some() {
            String::new()
        } else {
            text(&self.snapshot, "approvalInDock")
        }
    }

    pub(super) fn render_work_dock(
        &mut self,
        p: &ChatAppearance,
        window_height: f32,
        error_shown: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.board.dock_top.set(None);
        if !error_shown {
            self.board.dock_peek_over_error = false;
        }
        self.board.dock_error_active = error_shown;
        let dock = &self.snapshot["workDock"];
        let board = &self.snapshot["coordinatorBoard"];
        let rows: Vec<Value> = dock["rows"].as_array().cloned().unwrap_or_default();
        self.board.approval_seen.borrow_mut().retain(|key, _| {
            rows.iter()
                .any(|row| row["key"].as_str() == Some(key.as_str()))
        });
        // The snoozed asks, forms, pages and to-dos, folded under one Snoozed row (`work_dock_snoozed`).
        let snoozed: Vec<Value> = dock["snoozed"].as_array().cloned().unwrap_or_default();
        // Combined Recent drawer (recently handled asks/approvals/to-dos + finished work items),
        // falling back to answered + done for older chat cores.
        let recent: Vec<Value> = if let Some(list) = dock["recent"].as_array() {
            list.clone()
        } else {
            let mut merged = dock["answered"].as_array().cloned().unwrap_or_default();
            merged.extend(dock["done"].as_array().cloned().unwrap_or_default());
            merged
        };
        let has_dock_content = !rows.is_empty() || !snoozed.is_empty() || !recent.is_empty();
        if !has_dock_content && !board.is_object() {
            return None;
        }
        // An ordinary session's bar stays away until it has an item (the user's "Nothing in
        // progress" bar is a coordinator's).
        if !has_dock_content && board["own"] == true && board["empty"] == true {
            return None;
        }
        let long_draft = self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).value().len() > DOCK_FOLDS_PAST_CHARS);
        // An approval the agent waits on (or one just answered) keeps the dock up: it is drawn
        // nowhere else (gx-chat-core extras/dock_approvals.rs).
        let stays = rows.iter().any(|row| row["stays"] == true);
        if !stays && (long_draft || (self.board.open && self.board.docked)) {
            return None;
        }
        let s = p.scale;
        // The core's `hidden` (hideWorkDock) is the collapsed bar here; it opens by itself when a
        // row that was not there when it collapsed arrives. While an error banner sits above the
        // dock, fold to the header row until the error clears or the user expands it manually.
        let collapsible = has_dock_content;
        let hidden_in_core = dock["hidden"] == true;
        let open =
            collapsible && !hidden_in_core && (!error_shown || self.board.dock_peek_over_error);
        // Collapsing or opening, the rows ease their height and fade as the chat's other folds do
        // (disclosure_motion.rs, its timing and the reduced motion setting), the chevron turning
        // with them; the bar and the chat box under the dock follow its height.
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User, of the dock header's collapse: "please make this animate when expanded /
        // collapsed".
        let motion = self.disclosure_frame(DOCK_MOTION_KEY, open, cx);
        let turned = motion.map_or(if open { 1.0 } else { 0.0 }, |frame| frame.log);
        let header = self.work_dock_header(
            &rows,
            collapsible,
            open,
            hidden_in_core,
            error_shown,
            turned,
            p,
            cx,
        );
        let mut surface = div()
            .relative()
            .w_full()
            .min_w_0()
            .rounded(px(12.0 * s))
            .border_1()
            .border_color(p.composer_border)
            // CDXC:SessionChat 2026-11-25 DECISION: User, of the Inbox card over the chat box: "the
            // bg here vs here look diff to me ... i want both like chat's bg if there's actually a
            // diff. same for the bg of the top bar for each one". The card takes the chat box's
            // own fill (a theme's Chat box colour, not its Cards colour, which the default theme
            // sets 2% lighter), and its header bar the pills bar's tint over it (`STRIP_TINT`).
            // Its outline and the open header's divider take the box's outline and the pills
            // bar's hairline (`composer_border`), not the inputs' border.
            .bg(p.composer_background)
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s))
            .when_some(self.board.inbox_focus_handle.as_ref(), |surface, handle| {
                surface.track_focus(handle)
            })
            .child(header);
        if open || motion.is_some() {
            let limit = dock["limit"].as_u64().unwrap_or(4) as usize;
            let hidden = rows.len().saturating_sub(limit);
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
            for (idx, row) in rows
                .iter()
                .take(if all { rows.len() } else { limit })
                .enumerate()
            {
                let focused = self.board.inbox_focus == Some(idx);
                list = list.child(self.dock_row_at(row, focused, p, cx));
            }
            // Clear Inbox lives in the header's ⋯ menu (`work_dock_header_menu`), and its Undo, like
            // a row's Seen or Not needed, is a toast (`sync_inbox_undo_toast`).
            if hidden > 0 {
                list = list.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0 * s))
                        .px(px(DOCK_ROW_PAD_X * s))
                        .pt(px(4.0 * s))
                        .pb(px(5.0 * s))
                        .child(
                            div()
                                .id("work-dock-more")
                                .role(gpui::Role::Button)
                                .pl(px(23.0 * s))
                                .text_color(p.muted)
                                .chat_cursor_pointer()
                                .hover(|style| style.text_color(p.foreground))
                                .aria_expanded(all)
                                .child(if all {
                                    "Show fewer".to_string()
                                } else {
                                    ghostex_gx_chat_core::extras::work_cards::dock_more_label(
                                        &rows[limit.min(rows.len())..],
                                    )
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.expanded.remove(DOCK_ALL_ROWS) {
                                        this.expanded.insert(DOCK_ALL_ROWS.to_string());
                                    }
                                    cx.notify();
                                })),
                        ),
                );
            }
            // CDXC:Coordinators 2026-11-24 DECISION:
            // User: combine Handled and Done into one Recent drawer ordered after Snoozed,
            // newest first, with the same row left inset for all drawers. Since 2026-11-25 it has no
            // Archive all: it clears itself (gx-chat-core work_cards.rs, Q4915).
            // CDXC:Coordinators 2026-11-25 DECISION: User, of the Snoozed and Recent groups: "i
            // wish there was a way to tell that the items under snoozed and recent are part of
            // that group? i wanna still keep the alignment between all tho". They picked Q4969 C
            // and D: a hairline across the drawer above each group header (when something sits
            // above it) with the header as a small uppercase label, and the group's rows a step
            // quieter (`DockRowLook::quiet`). Icons and titles keep the pings' columns.
            // Then, of the lines: "let's please not show the lines when it's collapsed, show only
            // when expanded". A group's line shows only while that group is open; a closed group
            // keeps the line's space with a clear line, so opening or closing never moves a header.
            let shown_above = rows.len().min(if all { rows.len() } else { limit }) > 0;
            let snoozed_open = self.expanded.contains(DOCK_SNOOZED_ROWS);
            let recent_open = self.expanded.contains(DOCK_RECENT_ROWS)
                || self.expanded.contains(DOCK_LEGACY_DONE_ROWS)
                || self.expanded.contains(DOCK_LEGACY_ANSWERED_ROWS);
            if !snoozed.is_empty() {
                if shown_above {
                    list = list.child(dock_group_divider(snoozed_open, p));
                }
                list = list.children(self.work_dock_snoozed(&snoozed, p, cx));
            }
            if !recent.is_empty() {
                if shown_above || !snoozed.is_empty() {
                    list = list.child(dock_group_divider(recent_open, p));
                }
                list = list.children(self.work_dock_recent(&recent, p, cx));
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

    /// The dock's Snoozed row under the others (Q4686 A): "Snoozed" and its count badge with a
    /// moon icon and chevron, folded until clicked; open, snoozed asks, forms, pages and to-dos
    /// follow dimmed.
    fn work_dock_snoozed(
        &self,
        snoozed: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let open = self.expanded.contains(DOCK_SNOOZED_ROWS);
        let line = div()
            .id("work-dock-snoozed")
            .role(gpui::Role::Button)
            .aria_label(format!("Snoozed, {}", snoozed.len()))
            .aria_expanded(open)
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(DOCK_GAP * s))
            .px(px(DOCK_ROW_PAD_X * s))
            .py(px(5.0 * s))
            .rounded(px(8.0 * s))
            .text_color(p.muted)
            .chat_cursor_pointer()
            .hover(|style| {
                style
                    .bg(p.foreground.opacity(0.04))
                    .text_color(p.foreground)
            })
            .child(
                svg()
                    .path("modals/settings/moon.svg")
                    .flex_shrink_0()
                    .size(px(13.0 * s))
                    .text_color(p.card_muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .child(dock_group_label("Snoozed", p))
                    .child(crate::app::element::count_badge(
                        snoozed.len().to_string(),
                        p.foreground,
                        s,
                    )),
            )
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .flex_shrink_0()
                    .size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .when(open, |icon| {
                        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            std::f32::consts::PI,
                        )))
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if !this.expanded.remove(DOCK_SNOOZED_ROWS) {
                    this.expanded.insert(DOCK_SNOOZED_ROWS.to_string());
                }
                cx.notify();
            }));
        let mut out = vec![line.into_any_element()];
        if open {
            out.extend(snoozed.iter().map(|row| {
                div()
                    .w_full()
                    .opacity(0.72)
                    .child(self.dock_group_row(row, p, cx))
                    .into_any_element()
            }));
        }
        out
    }

    /// The collapsible Recent drawer below Snoozed: combines recently handled asks/approvals/to-dos
    /// and work finished in the latest turn, newest first, with the Done check icon, "Recent",
    /// count badge and chevron. Collapsed by default; migrates legacy Done/Handled open state on
    /// read and toggle.
    fn work_dock_recent(
        &self,
        recent: &[Value],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        let open = self.expanded.contains(DOCK_RECENT_ROWS)
            || self.expanded.contains(DOCK_LEGACY_DONE_ROWS)
            || self.expanded.contains(DOCK_LEGACY_ANSWERED_ROWS);
        let (done_icon, done_color) = status_look("finished", p);
        let green: gpui::Hsla = gpui::rgb(0x2fa865).into();
        let red: gpui::Hsla = gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into();
        let line = div()
            .id("work-dock-recent")
            .role(gpui::Role::Button)
            .aria_label(format!("Recent, {}", recent.len()))
            .aria_expanded(open)
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(DOCK_GAP * s))
            .px(px(DOCK_ROW_PAD_X * s))
            .py(px(5.0 * s))
            .rounded(px(8.0 * s))
            .text_color(p.muted)
            .chat_cursor_pointer()
            .hover(|style| {
                style
                    .bg(p.foreground.opacity(0.04))
                    .text_color(p.foreground)
            })
            .child(
                svg()
                    .path(done_icon)
                    .flex_shrink_0()
                    .size(px(13.0 * s))
                    .text_color(done_color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .child(dock_group_label("Recent", p))
                    .child(crate::app::element::count_badge(
                        recent.len().to_string(),
                        p.foreground,
                        s,
                    )),
            )
            .child(
                svg()
                    .path("titlebar/chevron-down.svg")
                    .flex_shrink_0()
                    .size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .when(open, |icon| {
                        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            std::f32::consts::PI,
                        )))
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                let was_open = this.expanded.remove(DOCK_RECENT_ROWS)
                    | this.expanded.remove(DOCK_LEGACY_DONE_ROWS)
                    | this.expanded.remove(DOCK_LEGACY_ANSWERED_ROWS);
                if !was_open {
                    this.expanded.insert(DOCK_RECENT_ROWS.to_string());
                }
                cx.notify();
            }));
        let mut out = vec![line.into_any_element()];
        if open {
            for (idx, row) in recent.iter().enumerate() {
                if text(row, "kind") == "finished" {
                    out.push(
                        div()
                            .w_full()
                            .opacity(0.72)
                            .child(self.dock_group_row(row, p, cx))
                            .into_any_element(),
                    );
                    continue;
                }
                let key = text(row, "key");
                let title = text(row, "title");
                let outcome = text(row, "outcome");
                let age = text(row, "age");
                let show_in_chat = row["showInChat"].clone();
                let clickable = show_in_chat.is_object();
                let (icon_path, icon_color) = match text(row, "status").as_str() {
                    "approved" => ("titlebar/shield-check.svg", green),
                    "denied" | "dropped" => ("titlebar/shield-x.svg", red),
                    _ => ("titlebar/message-circle-check.svg", green.opacity(0.85)),
                };
                out.push(
                    div()
                        .id(gpui::SharedString::from(format!("dock-recent:{key}:{idx}")))
                        .w_full()
                        .min_w_0()
                        .opacity(0.72)
                        .px(px(DOCK_ROW_PAD_X * s))
                        .py(px(5.0 * s))
                        .rounded(px(8.0 * s))
                        .flex()
                        .items_center()
                        .gap(px(DOCK_GAP * s))
                        .when(clickable, |el| {
                            el.role(gpui::Role::Button)
                                .chat_cursor_pointer()
                                .hover(|style| style.bg(p.foreground.opacity(0.04)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.invoke(show_in_chat.clone(), cx);
                                }))
                        })
                        .child(
                            svg()
                                .path(icon_path)
                                .flex_shrink_0()
                                .size(px(13.0 * s))
                                .text_color(icon_color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(PROSE_SIZE * GROUP_ROW_TITLE_SCALE * s))
                                .text_color(p.card_muted.opacity(GROUP_ROW_TITLE_ALPHA))
                                .child(title),
                        )
                        .when(
                            !outcome.is_empty()
                                && !outcome.eq_ignore_ascii_case("answered")
                                && !outcome.eq_ignore_ascii_case("handled")
                                && !outcome.eq_ignore_ascii_case("done"),
                            |el| {
                                el.child(
                                    div()
                                        .flex_shrink_0()
                                        .max_w(px(180.0 * s))
                                        .truncate()
                                        .px(px(6.0 * s))
                                        .py(px(1.0 * s))
                                        .rounded(px(5.0 * s))
                                        .bg(p.foreground.opacity(0.05))
                                        .text_size(px(PROSE_SIZE * 0.88 * s))
                                        .text_color(p.muted)
                                        .child(outcome),
                                )
                            },
                        )
                        .when(!age.is_empty(), |el| {
                            el.child(div().flex_shrink_0().text_color(p.muted).child(age))
                        })
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// The Inbox header (Q4680 B + E): `[inbox icon] Inbox [ghost count chips]` and chevron.
    /// Count chips sit clean at rest (no dotted underline); hovering a count chip lights up only
    /// that chip (pill background, subtle border, brighter label) and never lights up the collapse
    /// bar behind it.
    #[allow(clippy::too_many_arguments)]
    fn work_dock_header(
        &self,
        rows: &[Value],
        collapsible: bool,
        open: bool,
        hidden_in_core: bool,
        error_shown: bool,
        turned: f32,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ghostex_gx_chat_core::extras::work_panel::{
            ASKS_GROUP_NAME, ASKS_GROUP_PLURAL, ASKS_GROUP_SINGULAR, INBOX_FULL_NAME, INBOX_NAME,
        };
        let s = p.scale;
        let board = &self.snapshot["coordinatorBoard"];
        let counts: Vec<(&'static str, u64)> = if board.is_object() {
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
            let needs = rows
                .iter()
                .filter(|row| row["needs"] == true && row["opened"] != true)
                .count() as u64;
            let blocked = rows.iter().filter(|row| row["kind"] == "blocked").count() as u64;
            vec![("needs", needs), ("blocked", blocked)]
        };
        let counts: Vec<(&'static str, u64)> =
            counts.into_iter().filter(|(_, count)| *count > 0).collect();
        let name_of = |group: &str, count: u64| match group {
            "needs" if count == 1 => ASKS_GROUP_SINGULAR,
            "needs" => ASKS_GROUP_PLURAL,
            "blocked" => "blocked",
            _ => "in progress",
        };
        let room = match self.board.dock_width.get() {
            width if width > 0.0 => width,
            _ => DOCK_ROW_WIDTH_GUESS,
        } - 12.0
            - 5.0
            - 8.0
            - if collapsible { 24.0 + 6.0 } else { 0.0 }
            - if self.snapshot["workDock"]["clearInbox"].is_object() {
                crate::app::window::page_band_geometry::CONTROL_SIZE + 6.0
            } else {
                0.0
            };
        let bar_font = crate::assets::chat_working::VISUAL.font_size;
        let inbox_label_width = 14.0 + 5.0 + dock_text_width(INBOX_NAME, bar_font, p, cx) + 10.0;
        let named_width: f32 = counts
            .iter()
            .map(|(group, count)| {
                12.0 + 13.0
                    + 5.0
                    + dock_text_width(&count.to_string(), bar_font, p, cx)
                    + 4.0
                    + dock_text_width(name_of(group, *count), bar_font, p, cx)
            })
            .sum::<f32>()
            + 6.0 * counts.len().saturating_sub(1) as f32;
        let status_card = self.dock_status_card();
        let status_width = status_card.as_ref().map_or(0.0, |card| {
            24.0 + dock_text_width(card["label"].as_str().unwrap_or_default(), bar_font, p, cx)
                + 12.0
        });
        let show_inbox_text =
            status_card.is_none() || (inbox_label_width + named_width + status_width <= room);
        let named = (if show_inbox_text {
            inbox_label_width
        } else {
            24.0
        }) + named_width
            + status_width
            <= room;
        let chip_hover_bg = p.foreground.opacity(if p.light { 0.075 } else { 0.105 });
        let chip_active_bg = p.foreground.opacity(if p.light { 0.13 } else { 0.16 });
        let chip_hover_border = p.foreground.opacity(0.10);
        let marks: Vec<AnyElement> = counts
            .into_iter()
            .map(|(group, count)| {
                let (icon, color) = super::coordinator_board::group_look(group);
                let color = color.map_or(p.muted, |color| gpui::rgb(color).into());
                let group_title = match group {
                    "needs" => ASKS_GROUP_NAME,
                    "blocked" => "Blocked",
                    _ => "In progress",
                };
                let tip = format!("Open {group_title} in Work");
                let opens = board.is_object();
                let is_hovered = self.board.dock_count_hovered == Some(group);
                let count_label = if named {
                    format!("{count} {}", name_of(group, count))
                } else {
                    count.to_string()
                };
                div()
                    .id(gpui::SharedString::from(format!("work-dock-count:{group}")))
                    .flex()
                    .items_center()
                    .gap(px(5.0 * s))
                    .px(px(6.0 * s))
                    .py(px(2.0 * s))
                    .rounded(px(6.0 * s))
                    .border_1()
                    .border_color(if is_hovered && opens {
                        chip_hover_border
                    } else {
                        gpui::transparent_black()
                    })
                    .when(is_hovered && opens, |chip| chip.bg(chip_hover_bg))
                    .flex_shrink_0()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .when(opens, |mark| {
                        mark.role(gpui::Role::Button)
                            .aria_label(format!("Open {group_title} in Work"))
                            .chat_cursor_pointer()
                            .active(move |style| {
                                style.bg(chip_active_bg).border_color(chip_hover_border)
                            })
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.board.dock_count_hovered = Some(group);
                                } else if this.board.dock_count_hovered == Some(group) {
                                    this.board.dock_count_hovered = None;
                                }
                                cx.notify();
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.invoke(
                                    json!({"type": "showCoordinatorBoardGroup", "group": group}),
                                    cx,
                                );
                            }))
                    })
                    .child(
                        svg()
                            .path(icon)
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(color),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0 * s))
                            .child(
                                div()
                                    .text_color(p.foreground)
                                    .font_features(gpui::FontFeatures(
                                        vec![("tnum".into(), 1)].into(),
                                    ))
                                    .child(count.to_string()),
                            )
                            .when(named, |wrap| {
                                wrap.child(
                                    div()
                                        .text_color(if is_hovered {
                                            p.foreground
                                        } else {
                                            p.card_muted
                                        })
                                        .child(name_of(group, count)),
                                )
                            }),
                    )
                    .when(false, |mark| mark.child(count_label))
                    .into_any_element()
            })
            .collect();
        let quiet = marks.is_empty();
        let inbox_badge = div()
            .id("work-dock-inbox-label")
            .flex()
            .items_center()
            .gap(px(5.0 * s))
            .when(!quiet || (!collapsible && status_card.is_none()), |badge| {
                badge.mr(px(6.0 * s))
            })
            .flex_shrink_0()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new(INBOX_FULL_NAME)
                    .keys(super::composer::hotkey_chord(Some("focusSessionInbox")))
                    .build(window, cx)
            })
            .child(
                svg()
                    .path("titlebar/inbox.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.foreground.opacity(0.85)),
            )
            .when(show_inbox_text, |badge| {
                badge.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.foreground)
                        .child(INBOX_NAME),
                )
            });
        let chip_hovered = self.board.dock_count_hovered.is_some();
        let mut head = div()
            .id("work-dock-head")
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .pl(px(12.0 * s))
            // CDXC:Coordinators 2026-11-25 DECISION:
            // User, of the Inbox label sitting almost on the bar's right edge, above the chat box
            // whose last pill (Boost) sits further in: "please make it shifted like the pill below
            // (have same space from the right edge)". The bar's last content ends as far from its
            // border as the pills strip's last pill does (`STRIP_PAD_X`). With the chevron button
            // (24px box, 14px glyph) its own 5px inset makes up the rest of that gap.
            .pr(px((STRIP_PAD_X
                - if collapsible {
                    (24.0 - 14.0) / 2.0
                } else {
                    0.0
                })
                * s))
            .text_size(px(bar_font * s))
            .line_height(px(19.0 * s))
            .whitespace_nowrap()
            .text_color(p.card_muted)
            .map(|head| {
                if open {
                    head.h(px(34.0 * s + 1.0))
                        .border_b_1()
                        .border_color(p.composer_border)
                        .bg(p.foreground.opacity(super::pills_strip::STRIP_TINT))
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
                    .gap(px(4.0 * s))
                    .overflow_hidden()
                    .child(inbox_badge)
                    .children(marks)
                    .when(quiet && !collapsible && status_card.is_none(), |summary| {
                        summary.child(div().text_color(p.muted).child("Nothing in progress"))
                    }),
            )
            .when(status_card.is_none(), |head| {
                head.child(div().flex_1().min_w(px(8.0 * s)))
            })
            .children(self.work_dock_header_menu(p, cx));
        if collapsible {
            head = head
                .role(gpui::Role::Button)
                .aria_label(if open {
                    "Collapse Inbox"
                } else {
                    "Expand Inbox"
                })
                .aria_expanded(open)
                .chat_cursor_pointer()
                .when(!chip_hovered, |head| {
                    head.hover(|style| style.bg(p.foreground.opacity(0.055)))
                })
                .child(
                    div()
                        .size(px(24.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .rounded(px(6.0 * s))
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
                    if open {
                        this.board.dock_peek_over_error = false;
                        this.invoke(json!({ "type": "hideWorkDock" }), cx);
                        return;
                    }
                    if error_shown {
                        this.board.dock_peek_over_error = true;
                    }
                    if hidden_in_core {
                        this.invoke(json!({ "type": "showWorkDock" }), cx);
                    }
                    cx.notify();
                }));
        }
        head.into_any_element()
    }

    /// The Inbox header's ⋯: the round control the Work item pop-up and the page window wear
    /// (page_controls.rs `control_button`), with one item, Clear Inbox, and the line under it saying
    /// what it marks and that nothing is deleted (gx-chat-core `clear_inbox_hint`). It shows only
    /// while some row asks nothing. Its Undo is the toast (`sync_inbox_undo_toast`).
    ///
    /// CDXC:Coordinators 2026-11-25 DECISION:
    /// User (Q4932): Archive all moves to a new ⋯ in the Inbox header; "pls name it correct so
    /// people understand wtf is archive and that it wont get rid of ones that are seen and it should
    /// have undo button also when i click this to undo all the ones archived pls". The item is
    /// "Clear Inbox", its line names what it marks and that seen pages stay in Resources.
    fn work_dock_header_menu(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let clear = self.snapshot["workDock"]["clearInbox"].clone();
        if !clear.is_object() {
            return None;
        }
        let controls = crate::app::window::page_controls::ControlGlass::for_theme(p.light);
        let button = crate::app::window::page_controls::control_button(
            "work-dock-head-more",
            "titlebar/dots.svg",
            "More Inbox actions",
            controls,
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                let rows = vec![json!({
                    "label": text(&clear, "label"),
                    "description": text(&clear, "hint"),
                    "wrap": true,
                    "iconPath": "titlebar/check.svg",
                    "command": clear["command"],
                })];
                let in_main = this
                    .open_main_window(cx)
                    .is_none_or(|main| main == window.window_handle());
                if in_main {
                    this.show_chat_menu_at(rows, event.position(), 300.0, window, cx);
                } else {
                    this.show_chat_menu_in_window_at(rows, event.position(), 300.0, window, cx);
                }
            }),
        );
        Some(div().flex_shrink_0().child(button).into_any_element())
    }

    /// Raises the app's toast with Undo (window/toast.rs `GpuiAppToastAction::ChatUndo`) once for
    /// each new press of Seen, Not needed or Clear Inbox (gx-chat-core `workDock.undoToast`), for
    /// as long as the core keeps the undo. The host finds this view by its session and makes the
    /// toast's Undo run the core's command here, so it restores exactly what that press hid and
    /// takes back the note for the agent.
    ///
    /// CDXC:Coordinators 2026-11-25 DECISION:
    /// User (Q4931): undo with a toast; of the full-width "Archived N · Undo" row: "i dont want to
    /// see that undo item when i archive something please it's too big ... the placement of that
    /// undo list item is bad too". The row is gone; the small toast at the bottom of the window
    /// says what was marked, with Undo, for a few seconds.
    pub(super) fn sync_inbox_undo_toast(&self, before: &Value, cx: &mut Context<Self>) {
        let undo = &self.snapshot["workDock"]["undoToast"];
        let id = &undo["id"];
        if !undo.is_object() || id.is_null() || *id == before["workDock"]["undoToast"]["id"] {
            return;
        }
        let shown_ms = undo["remainingMs"].as_u64().unwrap_or(0);
        if shown_ms < 500 {
            return;
        }
        cx.emit(super::state::NativeChatEvent::Host(json!({
            "type": "toast",
            // One id: the core keeps one undo, so a newer press's toast takes the older's place.
            "toastId": "inbox-undo",
            "level": "success",
            "title": text(undo, "label"),
            "durationMs": shown_ms,
            "chatUndo": undo["command"],
        })));
    }

    /// One dock row: its mark, its question on one line, its answers and Reply beside it (folded
    /// into one Choose an answer dropdown when they do not all fit), and its age with the ⋯ over it
    /// under the pointer. A click on it shows the whole ask in its modal (dock_ask_modal.rs); a
    /// Ghostex page's row opens the page.
    ///
    /// CDXC:Coordinators 2026-11-14 DECISION:
    /// User: "i want when i click on a thing like this one to open modal instead of expanding like
    /// accordion because accordion is annoying pls fix". A row never opens in place any more: its
    /// title, description, answers and actions are a modal over the chat. Supersedes the opened
    /// row card (2026-11-11 and 2026-11-12 decisions, now on the modal in dock_ask_modal.rs) and
    /// its scrolling into view past the dock's height cap.
    ///
    /// A row under the Snoozed or Recent header: drawn quiet (`DockRowLook::quiet`, Q4969 D).
    fn dock_group_row(
        &self,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.dock_row_in(row, false, true, p, cx)
    }

    fn dock_row_at(
        &self,
        row: &Value,
        focused: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.dock_row_in(row, focused, false, p, cx)
    }

    fn dock_row_in(
        &self,
        row: &Value,
        focused: bool,
        quiet: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if text(row, "kind") == "pageBundle" {
            return self.dock_page_bundle_row(row, focused, p, cx);
        }
        if text(row, "kind") == "collab" {
            self.board
                .approval_seen
                .borrow_mut()
                .entry(text(row, "key"))
                .or_insert_with(web_time::Instant::now);
        }
        if text(row, "kind") == "approval" {
            self.board
                .approval_seen
                .borrow_mut()
                .entry(text(row, "key"))
                .or_insert_with(web_time::Instant::now);
            return self.dock_row_look(
                row,
                DockRowLook {
                    wrap: true,
                    focused,
                    quiet,
                    ..DockRowLook::default()
                },
                p,
                cx,
            );
        }
        self.dock_row_look(
            row,
            DockRowLook {
                focused,
                quiet,
                ..DockRowLook::default()
            },
            p,
            cx,
        )
    }

    /// A bundled same-turn page burst row (Q4683 A): stack icon, "N pages from last turn", Expand,
    /// the age with its hover ⋯ (All seen), and indented sub-rows for each page when expanded.
    fn dock_page_bundle_row(
        &self,
        row: &Value,
        focused: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let title = text(row, "title");
        let age = text(row, "age");
        let unread = row["unread"] == true;
        let bundle_key = format!("dock-bundle:{key}");
        let open = self.expanded.contains(&bundle_key);
        let hover = gpui::SharedString::from(format!("dock-row-hover:{key}"));
        let pages: Vec<Value> = row["bundlePages"].as_array().cloned().unwrap_or_default();
        let pink: gpui::Hsla = gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into();
        let toggle_key_btn = bundle_key.clone();
        let toggle_key_row = bundle_key.clone();
        let line = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(DOCK_GAP * s))
            .child(
                div()
                    .h(px(PROSE_LINE_HEIGHT * s))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .child(
                        svg()
                            .path("titlebar/stack.svg")
                            .size(px(13.0 * s))
                            .text_color(pink),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if unread { p.foreground } else { p.card_muted })
                    .child(title),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "dock-bundle-expand:{key}"
                            )))
                            .role(gpui::Role::Button)
                            .px(px(8.0 * s))
                            .py(px(2.0 * s))
                            .rounded(px(6.0 * s))
                            .border_1()
                            .border_color(p.foreground.opacity(0.10))
                            .bg(p.foreground.opacity(0.04))
                            .text_size(px(PROSE_SIZE * 0.90 * s))
                            .text_color(p.foreground)
                            .chat_cursor_pointer()
                            .hover(|style| style.bg(p.foreground.opacity(0.09)))
                            .child(if open { "Collapse" } else { "Expand" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if !this.expanded.remove(&toggle_key_btn) {
                                    this.expanded.insert(toggle_key_btn.clone());
                                }
                                cx.notify();
                            })),
                    ),
            )
            // All seen is in the ⋯ that takes the age's place on hover, as on every other row
            // (`row_age_menu`, gx-chat-core work_cards.rs Q4917).
            .child(self.row_age_menu(&key, &age, &hover, row, p, cx));
        let press = Self::dock_row_press(&key, line, p)
            .group(hover.clone())
            .when(focused, |el| {
                el.bg(pink.opacity(if p.light { 0.10 } else { 0.14 }))
                    .border_l_2()
                    .border_color(pink)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.expanded.remove(&toggle_key_row) {
                    this.expanded.insert(toggle_key_row.clone());
                }
                cx.notify();
            }));
        let mut wrap = div().w_full().flex().flex_col().child(press);
        if open {
            let mut sub = div()
                .w_full()
                .pl(px(22.0 * s))
                .pr(px(4.0 * s))
                .pb(px(4.0 * s))
                .flex()
                .flex_col()
                .gap(px(2.0 * s));
            for (idx, page) in pages.iter().enumerate() {
                let page_key = text(page, "key");
                let page_title = text(page, "title");
                let page_age = text(page, "age");
                let opened = page["opened"] == true;
                let page_open = page["open"].clone();
                sub = sub.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "dock-bundle-page:{page_key}:{idx}"
                        )))
                        .role(gpui::Role::Button)
                        .w_full()
                        .min_w_0()
                        .pl(px(10.0 * s))
                        .pr(px(DOCK_ROW_PAD_X * s))
                        .py(px(4.0 * s))
                        .rounded(px(6.0 * s))
                        .border_l_2()
                        .border_color(p.foreground.opacity(0.10))
                        .flex()
                        .items_center()
                        .gap(px(8.0 * s))
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.04)))
                        .child(
                            svg()
                                .path("titlebar/file.svg")
                                .flex_shrink_0()
                                .size(px(12.0 * s))
                                .text_color(pink),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(if opened { p.card_muted } else { p.foreground })
                                .child(page_title),
                        )
                        .when(!page_age.is_empty(), |el| {
                            el.child(div().flex_shrink_0().text_color(p.muted).child(page_age))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            if page_open.is_object() {
                                this.open_work_target(page_open.clone(), window, cx);
                            }
                        })),
                );
            }
            wrap = wrap.child(sub);
        }
        wrap.into_any_element()
    }

    /// A dock row drawn as `look` says: in the dock (one line), or as a Work panel Needs you card
    /// (coordinator_board.rs `board_dock_card`): the same mark, title, answers and Open, with the
    /// title wrapping over as many lines as it needs and the answers on the line under it.
    pub(super) fn dock_row_look(
        &self,
        row: &Value,
        look: DockRowLook,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(row, "key");
        let wrap = look.wrap;
        // A Ghostex page's row never opens to a second line: a click opens the page, and its other
        // actions are the ⋯ menu that takes the age's place under the pointer (`page_row_menu`).
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User: "for pages that need me, lets show 3 dots on hover (appears in place of the time
        // basically) where i can click to see show in chat/dismiss/snooze. don't make it expand
        // on click since it's useless when it expands".
        let page_row = row["page"] == true && row["open"].is_object();
        // A form's or thread-wait row opens its target at once, as a page's opens the page: no
        // empty intermediate modal (gx-chat-core `details_fields`, Q4477).
        let opens_directly = page_row
            || (matches!(text(row, "kind").as_str(), "form" | "thread" | "threadWait")
                && row["open"].is_object());
        let (icon, color) = row_mark(row, p);
        // A thread (only a Work panel card, `BoardRow::dock`) is marked by its agent's logo, as
        // the panel's agent button and the Moving list's Who are.
        let agent = text(row, "agentIcon");
        let (icon, color): (gpui::SharedString, gpui::Hsla) =
            if text(row, "kind") == "thread" && !agent.is_empty() {
                (
                    format!("agent-icons/{agent}.svg").into(),
                    super::new_session_welcome::brand_logo_color(&agent, p),
                )
            } else {
                (icon.into(), color)
            };
        // An answered approval's row says so quietly for its few seconds (gx-chat-core
        // extras/dock_approvals.rs).
        let settled = row["settled"] == true;
        let body = if settled {
            p.card_muted
        } else {
            p.strengthened(p.prose)
        };
        let busy = row["busy"] == true;
        let title = text(row, "title");
        let age = text(row, "age");
        // The answers, the look every place shares (work_actions.rs); a row with none shows what
        // it asks for in their place (Open or Reply).
        let answers: Vec<Value> = row["buttons"].as_array().cloned().unwrap_or_default();
        // CDXC:Coordinators 2026-11-24 DECISION (ask-details-page, Layout C + "Details"):
        // Keep the answer buttons visible on the folded row and show a quiet neutral "Details"
        // button beside them to open the full floating panel. Live pages and forms (`row["open"]`)
        // still use their direct icon action when they have no inline answers.
        //
        // CDXC:Coordinators 2026-11-25 DECISION:
        // User's "please only show the title line for all these work items in inbox" removed the
        // Inbox row's second line, which was the only place a short detail (`details` false,
        // gx-chat-core `details_fields`) showed. So an Inbox row (`look.age` is `None`) with any
        // detail (`hasDetail`) gets Details too, its answers staying beside it. A Work panel
        // Needs you card still draws that short detail as its preview line, so it keeps its
        // buttons.
        let has_details_button = (row["details"] == true
            || (look.age.is_none() && row["hasDetail"] == true))
            && !row["open"].is_object()
            && !settled
            && look.buttons.is_none();
        let action = if answers.is_empty() && row["needs"] == true && !has_details_button {
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
        let reply_answer = ((!answers.is_empty()
            || has_details_button
            || matches!(action, Some("Open" | "Answer" | "Details")))
            && row["reply"].is_object())
        .then(|| Self::reply_answer(&row["reply"], &text(row, "replied"), busy));
        // What fits: the answers may take the first line while the question keeps half the row.
        let row_width = match look.width {
            Some(width) => width,
            None => {
                (match self.board.dock_width.get() {
                    width if width > 0.0 => width,
                    _ => DOCK_ROW_WIDTH_GUESS,
                }) - 2.0 * DOCK_LIST_PAD
            }
        } - 2.0 * DOCK_ROW_PAD_X;
        // The age's slot, which the ⋯ takes under the pointer (`row_age_menu`): never narrower
        // than the ⋯, so the two swap without anything moving.
        let age_width = dock_text_width(&age, PROSE_SIZE, p, cx).max(AGE_SLOT_MIN) + DOCK_GAP;
        // Wrapped, the answers have their own line under the title.
        let beside_room = if wrap {
            row_width - DOCK_LEAD
        } else {
            row_width - DOCK_LEAD - age_width - row_width / 2.0
        };
        // Open and Reply are icon buttons, as wide as they are tall.
        let icon_width = Self::tone_button_height();
        let widths: Vec<f32> = match &action {
            Some(_) => vec![icon_width],
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
        let reply_width = reply_answer.as_ref().map_or(0.0, |_| icon_width + 6.0);
        // Quiet "Details" button beside the answers when the question has longer details.
        let details_width = if has_details_button {
            dock_button_width("Details", false, false, p, cx) + 6.0
        } else {
            0.0
        };
        let reply_width = reply_width + details_width;
        // Jump to context before Reply, on a row whose asking message is in another chat (gx-chat-core
        // `work_cards::context_of`).
        let context_button = if look.buttons.is_some() || settled {
            None
        } else {
            self.dock_context_icon(&key, row, p, cx)
        };
        let reply_width = reply_width + context_button.as_ref().map_or(0.0, |_| icon_width + 6.0);
        let all_width = buttons_width(&widths) + reply_width;
        // The answers and Reply are one line, never wrapped: when they do not all fit beside the
        // title, the answers fold into one Choose an answer dropdown in their place. Measured, with
        // the dock's width at its last paint, so a resize decides again. Answers no wider than the
        // dropdown stay as they are: folding them would gain nothing.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of an opened row whose Reply wrapped under four answers: "i dont like the choices
        // wrapping, please make them never wrap… should just show a dropdown". Supersedes the
        // closed row's "+N" chip.
        let choose_width =
            dock_button_width(super::work_actions::CHOOSE_ANSWER, true, false, p, cx);
        // An approval's answers never fold into a dropdown: they wrap onto more lines, each in the
        // agent's own words (gx-chat-core extras/dock_approvals.rs).
        let approval = row["commandLines"] == true;
        let fold = action.is_none()
            && !approval
            && !answers.is_empty()
            && buttons_width(&widths) > choose_width
            && all_width > beside_room;
        let mut buttons: Vec<AnyElement> = Vec::new();
        buttons.extend(match &action {
            _ if look.buttons.is_some() => look.buttons.into_iter().collect::<Vec<_>>(),
            Some(_) => self
                .dock_row_icon_action(&key, row, p, cx)
                .into_iter()
                .collect(),
            None if fold => vec![self.answers_dropdown(&key, &answers, busy, p, cx)],
            None => self.work_answer_buttons(&key, &answers, busy, p, cx),
        });
        if has_details_button {
            buttons.push(self.dock_details_button(&key, p, cx));
        }
        // Reply is always the rightmost button, whatever stands before it (answers, Details, Jump
        // to context), so it never shifts between rows.
        //
        // CDXC:Coordinators 2026-11-20 DECISION:
        // User, of a thread to-do whose Jump to context sat right of Reply: "switch it so Reply is
        // always on the most right instead of shifting around".
        buttons.extend(context_button);
        if let Some(answer) = &reply_answer {
            buttons.push(self.dock_reply_icon(&format!("{key}:reply"), answer, busy, p, cx));
        }
        // A page or form's answered/total ("0/1"), quiet, at the row's right end just before
        // its buttons and age.
        //
        // CDXC:Coordinators 2026-11-20 DECISION:
        // User, of "Title 0/1" in a dock row: the count goes on the right side of the row, not
        // after the title. Supersedes the 2026-11-11 placement after the page's name.
        let count = text(row, "count");
        let mut count = (opens_directly && !count.is_empty()).then(|| {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(px(PROSE_SIZE * s))
                .line_height(px(PROSE_LINE_HEIGHT * s))
                .text_color(p.card_muted)
                .child(count.to_string())
        });
        let in_dock = look.age.is_none();
        let unread = in_dock && row["unread"] == true;
        let duplicates: Vec<Value> = if in_dock {
            row["duplicates"].as_array().cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        let dupes_key = format!("dock-dupes:{key}");
        let dupes_open = !duplicates.is_empty() && self.expanded.contains(&dupes_key);
        let sources_chip = (!duplicates.is_empty()).then(|| {
            let sources_count = row["sourcesCount"]
                .as_u64()
                .unwrap_or((duplicates.len() + 1) as u64);
            let toggle_key = dupes_key.clone();
            div()
                .id(gpui::SharedString::from(format!("dock-row-sources:{key}")))
                .role(gpui::Role::Button)
                .aria_expanded(dupes_open)
                .flex_shrink_0()
                .px(px(6.0 * s))
                .py(px(1.0 * s))
                .rounded_full()
                .border_1()
                .border_color(p.foreground.opacity(0.10))
                .bg(p.foreground.opacity(0.05))
                .flex()
                .items_center()
                .gap(px(3.0 * s))
                .text_size(px(PROSE_SIZE * 0.84 * s))
                .line_height(px(16.0 * s))
                .text_color(p.card_muted)
                .chat_cursor_pointer()
                .hover(|style| {
                    style
                        .bg(p.foreground.opacity(0.10))
                        .text_color(p.foreground)
                })
                .child(format!("{sources_count} sources"))
                .child(
                    svg()
                        .path("titlebar/chevron-down.svg")
                        .size(px(10.0 * s))
                        .text_color(p.muted)
                        .when(dupes_open, |chevron| {
                            chevron.with_transformation(gpui::Transformation::rotate(
                                gpui::radians(std::f32::consts::PI),
                            ))
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !this.expanded.remove(&toggle_key) {
                        this.expanded.insert(toggle_key.clone());
                    }
                    cx.notify();
                }))
        });
        let title_color = if look.quiet {
            p.card_muted.opacity(GROUP_ROW_TITLE_ALPHA)
        } else if settled {
            p.card_muted
        } else if in_dock {
            if unread { p.foreground } else { p.card_muted }
        } else {
            body
        };
        let answered_by = text(row, "answeredBy");
        let answered_by_chip = (!answered_by.is_empty()).then(|| {
            let green: gpui::Hsla = gpui::rgb(if p.light { 0x168049 } else { 0x34d399 }).into();
            let open_page = row["open"].clone();
            let has_open = open_page.is_object();
            div()
                .id(gpui::SharedString::from(format!(
                    "dock-row-answered-by:{key}"
                )))
                .flex_shrink_0()
                .px(px(7.0 * s))
                .py(px(1.5 * s))
                .rounded_full()
                .bg(green.opacity(if p.light { 0.12 } else { 0.16 }))
                .border_1()
                .border_color(green.opacity(0.32))
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .text_size(px(PROSE_SIZE * 0.84 * s))
                .line_height(px(16.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .text_color(green)
                .when(has_open, |chip| {
                    chip.role(gpui::Role::Button)
                        .chat_cursor_pointer()
                        .hover(move |style| {
                            style
                                .bg(green.opacity(if p.light { 0.20 } else { 0.26 }))
                                .border_color(green.opacity(0.55))
                        })
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new(
                                "Open Live page to preview their answers",
                            )
                            .build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_work_target(open_page.clone(), window, cx);
                        }))
                })
                .child(
                    svg()
                        .path("titlebar/user.svg")
                        .size(px(11.0 * s))
                        .text_color(green),
                )
                .child(answered_by)
        });
        let question = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .child(div().flex_1().min_w_0().child(self.ask_title_weighted(
                &key,
                &title,
                &row["titleRuns"],
                !wrap,
                FontWeight::NORMAL,
                title_color,
                if look.quiet {
                    PROSE_SIZE * GROUP_ROW_TITLE_SCALE
                } else {
                    PROSE_SIZE
                },
                PROSE_LINE_HEIGHT,
                p,
                cx,
            )))
            .children(answered_by_chip)
            .children(sources_chip)
            .into_any_element();
        let pink_dot: gpui::Hsla =
            gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into();
        let mut head = div()
            .w_full()
            .min_w_0()
            .flex()
            .when(wrap, |head| head.items_start())
            .when(!wrap, |head| head.items_center())
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
        // Wrapped, the answers and Open take the line under the title, after the mark's room.
        let mut under = None;
        if wrap && !buttons.is_empty() {
            under = Some(
                div()
                    .min_w_0()
                    .pl(px(DOCK_LEAD * s))
                    .pt(px(6.0 * s))
                    .flex()
                    .items_center()
                    .when(approval, |line| line.flex_wrap())
                    .gap(px(6.0 * s))
                    .children(buttons),
            );
        } else if !buttons.is_empty() || count.is_some() {
            head = head.child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .children(count.take().map(|count| count.mr(px(4.0 * s))))
                    .children(buttons),
            );
        }
        // Wrapped, the buttons are under the title: the count ends the title's line, before the age.
        if let Some(count) = count {
            head = head.child(count);
        }
        // The row's other actions (Show in chat, Dismiss, Snooze, Mention) are one ⋯ menu that
        // takes the age's place while the pointer is over the row.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of the ⋯ at the right end of an opened row's answers: "please move this button to
        // be over the time in the card but only when i hover over the card (whether collapsed or
        // expanded)". The answers and Reply get its room.
        let hover = gpui::SharedString::from(format!("{}dock-row-hover:{key}", look.scope));
        // A Work panel card keeps its own age, ⋯ and right-click menu (`look.age`).
        let age_menu = match look.age {
            Some(age) => age,
            None => self.row_age_menu(&key, &age, &hover, row, p, cx),
        };
        let right_row = row.clone();
        let line = head.child(age_menu);
        // CDXC:Coordinators 2026-11-25 DECISION:
        // User, of Inbox rows above the composer drawing a second, dimmer preview line under the
        // title: "sir showing 2 lines here is NOT A THING. please only show the title line for all
        // these work items in inbox!!!!" Supersedes the 2026-11-20 and 2026-11-24 Inbox preview
        // line: Inbox rows (`in_dock`) show only the title line with their buttons and time, while
        // the full detail text stays reachable through Details / the row modal. Work panel Needs
        // you cards (`!in_dock`) keep their one-line preview under the title.
        let preview = text(row, "preview");
        let preview = (!in_dock && !preview.is_empty()).then(|| {
            div()
                .min_w_0()
                .pl(px(DOCK_LEAD * s))
                .truncate()
                .text_size(px(PROSE_SIZE * 0.93 * s))
                .text_color(p.card_muted)
                .child(preview.to_string())
        });
        // An approval's answers go under its command, so what they approve is read first.
        let after_command =
            if row["commandLines"] == true && !text(row, "command").trim().is_empty() {
                under.take()
            } else {
                None
            };
        // Why the agent flagged an approval ("Dangerous rm operation …"), on the row itself:
        // amber for a warning, quiet for a note (gx-chat-core extras/dock_approvals.rs).
        let note = text(&row["note"], "text");
        let note = (!note.is_empty()).then(|| {
            div()
                .min_w_0()
                .pl(px(DOCK_LEAD * s))
                .pt(px(2.0 * s))
                .whitespace_normal()
                .text_size(px(PROSE_SIZE * 0.93 * s))
                .text_color(if row["note"]["tone"] == "warning" {
                    gpui::Hsla::from(gpui::rgb(if p.light { 0xb45309 } else { 0xf5a524 }))
                } else {
                    p.card_muted
                })
                .child(note.to_string())
        });
        // Why an approval's answer did not land, under its title.
        let error = text(row, "error");
        let error = (!error.is_empty()).then(|| {
            div()
                .min_w_0()
                .pl(px(DOCK_LEAD * s))
                .pt(px(2.0 * s))
                .whitespace_normal()
                .text_size(px(PROSE_SIZE * 0.93 * s))
                .text_color(gpui::rgb(0xec5357))
                .child(error.to_string())
        });
        let line = if under.is_some() || preview.is_some() || error.is_some() || note.is_some() {
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .child(line)
                .children(preview)
                .children(note)
                .children(error)
                .children(under)
        } else {
            line
        };
        let press = Self::dock_row_press(&format!("{}{key}", look.scope), line, p)
            // Named after its row, so accessibility clients (cua-driver, VoiceOver) can find it.
            .aria_label(text(row, "title"))
            .group(hover.clone())
            .when(look.focused, |press| {
                press
                    .bg(pink_dot.opacity(if p.light { 0.10 } else { 0.14 }))
                    .border_l_2()
                    .border_color(pink_dot)
            })
            .when(in_dock, |press| {
                press.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let rows = this.row_menu(&right_row, cx);
                        this.show_dock_row_menu(rows, event.position, window, cx);
                    }),
                )
            });
        let dupe_subrows = (dupes_open && !duplicates.is_empty()).then(|| {
            let mut sub = div()
                .w_full()
                .pl(px(22.0 * s))
                .pr(px(4.0 * s))
                .pb(px(4.0 * s))
                .flex()
                .flex_col()
                .gap(px(2.0 * s));
            for (idx, dupe) in duplicates.iter().enumerate() {
                let dupe_key = text(dupe, "key");
                let (d_icon, d_color) = row_mark(dupe, p);
                let source_label = text(dupe, "sourceLabel");
                let label = if source_label.is_empty() {
                    text(dupe, "title")
                } else {
                    source_label
                };
                let d_age = text(dupe, "age");
                let d_open = dupe["open"].clone();
                let d_modal = dupe_key.clone();
                sub = sub.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "dock-dupe-sub:{dupe_key}:{idx}"
                        )))
                        .role(gpui::Role::Button)
                        .w_full()
                        .min_w_0()
                        .pl(px(10.0 * s))
                        .pr(px(DOCK_ROW_PAD_X * s))
                        .py(px(4.0 * s))
                        .rounded(px(6.0 * s))
                        .border_l_2()
                        .border_color(p.foreground.opacity(0.10))
                        .flex()
                        .items_center()
                        .gap(px(8.0 * s))
                        .text_size(px(PROSE_SIZE * 0.92 * s))
                        .text_color(p.card_muted)
                        .chat_cursor_pointer()
                        .hover(|style| {
                            style
                                .bg(p.foreground.opacity(0.04))
                                .text_color(p.foreground)
                        })
                        .child(
                            svg()
                                .path(d_icon)
                                .flex_shrink_0()
                                .size(px(12.0 * s))
                                .text_color(d_color),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(label))
                        .when(!d_age.is_empty(), |el| {
                            el.child(div().flex_shrink_0().text_color(p.muted).child(d_age))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            if d_open.is_object() {
                                this.open_work_target(d_open.clone(), window, cx);
                            } else if !d_modal.is_empty() {
                                this.open_dock_ask_modal(&d_modal, window, cx);
                            }
                        })),
                );
            }
            sub
        });
        // A Work panel card opens what it points to (`BoardRow::go`); ⌥-click mentions it, as
        // every board card does. A drag of the card, or a drag that selects, opens nothing.
        if let Some(go) = look.click {
            let reference = look.reference.clone();
            return div()
                .w_full()
                .child(press.on_click(cx.listener(
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        if !super::row_click::acts_on_row(event, window, cx) {
                            return;
                        }
                        this.close_board_card_menu(cx);
                        if event.modifiers().alt && !reference.is_empty() {
                            this.insert_board_reference(&reference, cx);
                        } else {
                            this.open_work_target(go.clone(), window, cx);
                        }
                    },
                )))
                .children(self.dock_row_command(&key, row, p, cx))
                .children(after_command.map(|answers| {
                    answers
                        .pt_0()
                        .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                        .pr(px(DOCK_ROW_PAD_X * s))
                        .pb(px(6.0 * s))
                }))
                .into_any_element();
        }
        if opens_directly {
            let page = row["open"].clone();
            if let Some(page_id) = row["pageId"].as_str() {
                self.refresh_page_counts(page_id, cx);
            }
            return div()
                .w_full()
                .child(press.on_click(cx.listener(move |this, _, window, cx| {
                    // Straight to the page (`open_work_target`, CDXC:Coordinators 2026-11-18).
                    this.open_work_target(page.clone(), window, cx);
                })))
                .children(dupe_subrows)
                .into_any_element();
        }
        if settled {
            return div()
                .w_full()
                .child(press)
                .children(dupe_subrows)
                .into_any_element();
        }
        let shown = key.clone();
        div()
            .w_full()
            .child(press.on_click(cx.listener(move |this, _, window, cx| {
                this.open_dock_ask_modal(&shown, window, cx);
            })))
            .children(self.dock_row_command(&key, row, p, cx))
            .children(after_command.map(|answers| {
                answers
                    .pt_0()
                    .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                    .pr(px(DOCK_ROW_PAD_X * s))
                    .pb(px(6.0 * s))
            }))
            .children(dupe_subrows)
            .into_any_element()
    }

    /// The command under a row's title: an approval's as a block of its first lines with "Show N
    /// more lines" (`dock_command_block`), anything else's as one copy chip.
    fn dock_row_command(
        &self,
        key: &str,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if row["commandLines"] == true {
            return self.dock_command_block(key, row, p, cx);
        }
        self.dock_command_strip(key, row, p, cx)
    }

    /// An approval's command (gx-chat-core extras/dock_approvals.rs): its first lines, wrapped, never
    /// cut without saying so; "Show N more lines" opens the rest in place and "Show fewer" folds it,
    /// and the lines gxserver itself left out say so. A click on it copies the whole command.
    ///
    /// CDXC:SessionChat 2026-11-20 WHY:
    /// A security review of the dock approval rows found the row showed one visual line of a command that was several (a Codex `echo ok &&` hid the `curl … | sh` after it), so the approval read as less than it grants.
    fn dock_command_block(
        &self,
        key: &str,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let command = text(row, "command");
        if command.trim().is_empty() {
            return None;
        }
        let s = p.scale;
        let more = row["commandMore"].as_u64().unwrap_or(0);
        let fold_key = format!("dock-command:{key}");
        let open = more > 0 && self.expanded.contains(&fold_key);
        let shown = if more == 0 || open {
            command.clone()
        } else {
            text(row, "commandPreview")
        };
        let cut = text(row, "commandCut");
        let copied = command.clone();
        // A Collab message's words are drawn in the chat's own type and copied as a message.
        let plain = row["commandPlain"] == true;
        let copy_label = if plain {
            "Copy message"
        } else {
            "Copy command"
        };
        let block = div()
            .id(gpui::SharedString::from(format!("dock-row-command:{key}")))
            .role(gpui::Role::Button)
            .aria_label(copy_label)
            .chat_cursor_pointer()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .px(px(8.0 * s))
            .py(px(4.0 * s))
            .rounded(px(6.0 * s))
            .bg(p.foreground.opacity(0.05))
            .hover(|style| style.bg(p.foreground.opacity(0.09)))
            .when(!plain, |block| block.font_family(super::fonts::CHAT_MONO))
            .text_size(px(PROSE_SIZE * if plain { 1.0 } else { 0.9 } * s))
            .text_color(p.strengthened(p.prose))
            .children(shown.lines().map(|line| {
                div()
                    .min_w_0()
                    .whitespace_normal()
                    .child(if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.to_string()
                    })
            }))
            // Cut whole lines say so with "…" (a line cut short already ends with one).
            .when(
                more > 0 && !open && !shown.trim_end().ends_with('\u{2026}'),
                |block| block.child(div().text_color(p.card_muted).child("\u{2026}")),
            )
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(copy_label).build(window, cx)
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(copied.clone()),
                    cx,
                );
            }));
        let toggle = (more > 0).then(|| {
            let fold_key = fold_key.clone();
            div()
                .id(gpui::SharedString::from(format!(
                    "dock-row-command-more:{key}"
                )))
                .role(gpui::Role::Button)
                .chat_cursor_pointer()
                .text_size(px(PROSE_SIZE * 0.9 * s))
                .text_color(p.card_muted)
                .hover(|style| style.text_color(p.foreground))
                .child(if open {
                    "Show fewer".to_string()
                } else if more == 1 {
                    "Show 1 more line".to_string()
                } else {
                    format!("Show {more} more lines")
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !this.expanded.remove(&fold_key) {
                        this.expanded.insert(fold_key.clone());
                    }
                    cx.notify();
                }))
        });
        Some(
            div()
                .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                .pr(px(DOCK_ROW_PAD_X * s))
                .pb(px(6.0 * s))
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.0 * s))
                .child(block)
                .children(toggle)
                .when(!cut.is_empty(), |column| {
                    column.child(
                        div()
                            .text_size(px(PROSE_SIZE * 0.9 * s))
                            .text_color(p.card_muted)
                            .child(cut),
                    )
                })
                .into_any_element(),
        )
    }

    /// A Needs you to-do's command strip on a dock row: copy-only chip for non-runnable commands,
    /// or the neutral command strip with Copy + "Run it" (and live status while running).
    fn dock_command_strip(
        &self,
        key: &str,
        row: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let strip = self.needs_you_command_strip(key, row, true, p, cx)?;
        let s = p.scale;
        Some(
            div()
                .pl(px((DOCK_ROW_PAD_X + DOCK_LEAD) * s))
                .pr(px(DOCK_ROW_PAD_X * s))
                .pb(px(6.0 * s))
                .w_full()
                .min_w_0()
                .child(strip)
                .into_any_element(),
        )
    }

    /// Layout A ("Command strip with button") for a Needs you to-do or coordinator ask.
    ///
    /// Renders either the quiet Copy-only chip (when the command is outside the Ghostex allowlist),
    /// the ready command strip with Copy + "Run it", or live status (running, waiting for Touch ID /
    /// password / sign-in / approval, done automatically, or failed/stopped with Retry).
    pub(super) fn needs_you_command_strip(
        &self,
        key: &str,
        row: &Value,
        compact: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let raw_command = text(row, "command");
        let trimmed = raw_command.trim();
        if trimmed.is_empty() {
            return None;
        }
        let run_action = row["runCommand"].clone();
        let stop_action = row["stopCommand"].clone();
        let run = &row["commandRun"];
        let run_state = text(run, "state");
        let can_run = run_action.is_object();
        let s = p.scale;

        // Non-allowlisted commands stay as the quiet Copy-only chip.
        if !can_run && run_state.is_empty() {
            return self.dock_command_chip(key, trimmed, p, cx);
        }

        let display_cmd = ghostex_gx_chat_core::extras::needs_you_command::display_command(trimmed);
        let run_cmd = {
            let from_run = text(run, "command");
            if from_run.trim().is_empty() {
                display_cmd.clone()
            } else {
                from_run
            }
        };
        let copied = display_cmd.clone();
        let prefix = if compact { "dock-cmd" } else { "modal-cmd" };

        let quiet_btn = |id: String, label: String, icon: Option<&'static str>| {
            div()
                .id(gpui::SharedString::from(id))
                .role(gpui::Role::Button)
                .chat_cursor_pointer()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .px(px(if compact { 6.0 } else { 7.0 } * s))
                .py(px(if compact { 3.0 } else { 4.0 } * s))
                .rounded(px(6.0 * s))
                .text_size(px(12.0 * s))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(p.card_muted)
                .hover(|style| {
                    style
                        .bg(p.foreground.opacity(0.07))
                        .text_color(p.foreground)
                })
                .when_some(icon, |btn, path| {
                    btn.child(
                        svg()
                            .path(path)
                            .flex_shrink_0()
                            .size(px(11.0 * s))
                            .text_color(p.card_muted),
                    )
                })
                .child(label)
        };

        let neutral_btn = |id: String, label: String, icon: &'static str| {
            div()
                .id(gpui::SharedString::from(id))
                .role(gpui::Role::Button)
                .chat_cursor_pointer()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(5.0 * s))
                .px(px(if compact { 8.0 } else { 10.0 } * s))
                .py(px(if compact { 3.0 } else { 4.0 } * s))
                .rounded(px(6.0 * s))
                .bg(p.foreground.opacity(0.08))
                .border_1()
                .border_color(p.foreground.opacity(0.14))
                .text_size(px(12.0 * s))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(p.strengthened(p.prose))
                .hover(|style| style.bg(p.foreground.opacity(0.14)))
                .child(
                    svg()
                        .path(icon)
                        .flex_shrink_0()
                        .size(px(10.0 * s))
                        .text_color(p.strengthened(p.prose)),
                )
                .child(label)
        };

        // Ready state: quiet rounded bar with monospace command, Copy, and neutral "Run it".
        if run_state.is_empty() {
            let copy_text = copied.clone();
            let copy_btn = quiet_btn(
                format!("{prefix}-copy:{key}"),
                "Copy".to_string(),
                Some("titlebar/copy.svg"),
            )
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Copy command").build(window, cx)
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(copy_text.clone()),
                    cx,
                );
            }));

            let run_btn = neutral_btn(
                format!("{prefix}-run:{key}"),
                "Run it".to_string(),
                "titlebar/player-play.svg",
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.invoke(run_action.clone(), cx);
            }));

            return Some(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0 * s))
                    .px(px(if compact { 8.0 } else { 10.0 } * s))
                    .py(px(if compact { 5.0 } else { 7.0 } * s))
                    .rounded(px(if compact { 8.0 } else { 10.0 } * s))
                    .bg(p.foreground.opacity(0.05))
                    .border_1()
                    .border_color(p.foreground.opacity(0.09))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(compact, |el| el.truncate())
                            .when(!compact, |el| el.whitespace_normal())
                            .font_family(super::fonts::CHAT_MONO)
                            .text_size(px(PROSE_SIZE * 0.9 * s))
                            .text_color(p.strengthened(p.prose))
                            .child(display_cmd),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(4.0 * s))
                            .child(copy_btn)
                            .child(run_btn),
                    )
                    .into_any_element(),
            );
        }

        // Live / finished states: running, waiting (Touch ID / password / sign-in), succeeded, failed/stopped.
        let output = text(run, "output");
        let hint_title = text(run, "hintTitle");
        let hint_body = text(run, "hint");
        let error_text = text(run, "error");
        let out_key = format!("command-run-output:{key}");
        let output_open = !output.trim().is_empty() && self.expanded.contains(&out_key);

        let (dot_color, status_label, default_hint, show_stop, show_retry) = match run_state
            .as_str()
        {
            "running" => (
                p.card_muted.opacity(0.8),
                format!("Running {run_cmd}\u{2026}"),
                "Started on this computer. Output stays tucked away unless you open it."
                    .to_string(),
                true,
                false,
            ),
            "waiting" => (
                gpui::Hsla::from(gpui::rgb(0xe5a00d)),
                if hint_title.is_empty() {
                    "Waiting for Touch ID or your password".to_string()
                } else {
                    hint_title.clone()
                },
                if hint_body.is_empty() {
                    "Use Touch ID or enter your password on this computer to approve. Ghostex will finish automatically once you approve.".to_string()
                } else {
                    hint_body.clone()
                },
                true,
                false,
            ),
            "succeeded" => (
                gpui::Hsla::from(gpui::rgb(0x30a46c)),
                "Done automatically".to_string(),
                "The command succeeded and your agent was told Done.".to_string(),
                false,
                false,
            ),
            "stopped" => (
                p.card_muted,
                if error_text.is_empty() {
                    "Stopped before it finished.".to_string()
                } else {
                    error_text.clone()
                },
                String::new(),
                false,
                can_run,
            ),
            _ => (
                gpui::Hsla::from(gpui::rgb(0xe5484d)),
                if error_text.is_empty() {
                    "Command did not finish cleanly.".to_string()
                } else {
                    error_text.clone()
                },
                String::new(),
                false,
                can_run,
            ),
        };

        let mut actions: Vec<AnyElement> = Vec::new();
        if !output.trim().is_empty() {
            let toggle_label = if output_open {
                "Hide output".to_string()
            } else if run_state == "succeeded" {
                "View output".to_string()
            } else {
                "Show output".to_string()
            };
            let toggle_key = out_key.clone();
            actions.push(
                quiet_btn(format!("{prefix}-out:{key}"), toggle_label, None)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.expanded.remove(&toggle_key) {
                            this.expanded.insert(toggle_key.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if show_stop && stop_action.is_object() {
            actions.push(
                quiet_btn(format!("{prefix}-stop:{key}"), "Stop".to_string(), None)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(stop_action.clone(), cx);
                    }))
                    .into_any_element(),
            );
        }
        if show_retry {
            let copy_text = copied.clone();
            actions.push(
                quiet_btn(
                    format!("{prefix}-copy-err:{key}"),
                    "Copy".to_string(),
                    Some("titlebar/copy.svg"),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    crate::app::helpers::gpui_copy_to_clipboard(
                        gpui::ClipboardItem::new_string(copy_text.clone()),
                        cx,
                    );
                }))
                .into_any_element(),
            );
            actions.push(
                neutral_btn(
                    format!("{prefix}-retry:{key}"),
                    "Retry".to_string(),
                    "titlebar/refresh.svg",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(run_action.clone(), cx);
                }))
                .into_any_element(),
            );
        }

        let status_indicator = if run_state == "succeeded" {
            svg()
                .path("titlebar/check.svg")
                .flex_shrink_0()
                .size(px(12.0 * s))
                .text_color(dot_color)
                .into_any_element()
        } else {
            div()
                .flex_shrink_0()
                .size(px(8.0 * s))
                .rounded_full()
                .bg(dot_color)
                .into_any_element()
        };

        let show_hint = !default_hint.is_empty() && (!compact || run_state == "waiting");

        Some(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(5.0 * s))
                .px(px(if compact { 8.0 } else { 11.0 } * s))
                .py(px(if compact { 6.0 } else { 8.0 } * s))
                .rounded(px(if compact { 8.0 } else { 10.0 } * s))
                .bg(p.foreground.opacity(0.05))
                .border_1()
                .border_color(p.foreground.opacity(0.09))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0 * s))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(7.0 * s))
                                .child(status_indicator)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .when(compact, |el| el.truncate())
                                        .when(!compact, |el| el.whitespace_normal())
                                        .text_size(px(12.5 * s))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(p.strengthened(p.prose))
                                        .child(status_label),
                                ),
                        )
                        .when(!actions.is_empty(), |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0 * s))
                                    .children(actions),
                            )
                        }),
                )
                .when(show_hint, |box_el| {
                    box_el.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(12.0 * s))
                            .line_height(px(17.0 * s))
                            .text_color(p.card_muted)
                            .child(default_hint),
                    )
                })
                .when(output_open, |box_el| {
                    box_el.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .max_h(px(160.0 * s))
                            .overflow_hidden()
                            .px(px(8.0 * s))
                            .py(px(6.0 * s))
                            .rounded(px(7.0 * s))
                            .bg(p.foreground.opacity(0.04))
                            .border_1()
                            .border_color(p.foreground.opacity(0.08))
                            .font_family(super::fonts::CHAT_MONO)
                            .text_size(px(11.5 * s))
                            .line_height(px(16.0 * s))
                            .text_color(p.card_muted)
                            .flex()
                            .flex_col()
                            .children(
                                output
                                    .trim_end()
                                    .lines()
                                    .rev()
                                    .take(12)
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .rev()
                                    .map(|line| {
                                        div().min_w_0().whitespace_normal().child(
                                            if line.is_empty() {
                                                " ".to_string()
                                            } else {
                                                line.to_string()
                                            },
                                        )
                                    }),
                            ),
                    )
                })
                .into_any_element(),
        )
    }

    /// A Needs you to-do's command (`ghostex needs-you add --command`) under its title, as one
    /// quiet chip in the chat's monospace that copies it on a click; `None` for a row with none.
    /// Its own click target beside the row's, never over it (gx-chat-core `extras::needs_you`).
    fn dock_command_chip(
        &self,
        key: &str,
        command: &str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let command = command.trim();
        if command.is_empty() {
            return None;
        }
        let s = p.scale;
        let copied = command.to_string();
        Some(
            div()
                .flex()
                .min_w_0()
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("dock-row-command:{key}")))
                        .role(gpui::Role::Button)
                        .aria_label("Copy command")
                        .chat_cursor_pointer()
                        .min_w_0()
                        .max_w_full()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .px(px(8.0 * s))
                        .py(px(3.0 * s))
                        .rounded(px(6.0 * s))
                        .bg(p.foreground.opacity(0.05))
                        .hover(|style| style.bg(p.foreground.opacity(0.09)))
                        .child(
                            svg()
                                .path("titlebar/copy.svg")
                                .flex_shrink_0()
                                .size(px(12.0 * s))
                                .text_color(p.card_muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(super::fonts::CHAT_MONO)
                                .text_size(px(PROSE_SIZE * 0.9 * s))
                                .text_color(p.strengthened(p.prose))
                                .child(copied.clone()),
                        )
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Copy command").build(window, cx)
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            crate::app::helpers::gpui_copy_to_clipboard(
                                gpui::ClipboardItem::new_string(copied.clone()),
                                cx,
                            );
                        })),
                )
                .into_any_element(),
        )
    }

    /// A dock row's clickable surface: its one line.
    fn dock_row_press(key: &str, line: gpui::Div, p: &ChatAppearance) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(gpui::SharedString::from(format!("dock-row-head:{key}")))
            .role(gpui::Role::Button)
            .w_full()
            .min_w_0()
            .px(px(DOCK_ROW_PAD_X * s))
            .py(px(6.0 * s))
            .rounded(px(8.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .child(line)
    }

    /// A row's ⋯ (and right-click) menu: Show in chat, Dismiss and Snooze (`page_row_menu`), Copy
    /// as Markdown for an ask, a question, a form or a page (`row_markdown`), then Mention in chat
    /// box, which puts the item's, form's or page's reference in the chat box where the caret is
    /// now.
    pub(super) fn row_menu(&self, row: &Value, cx: &Context<Self>) -> Vec<Value> {
        let mut rows = page_row_menu(row);
        // A Needs you to-do's command, the chip's copy in the menu too.
        let command = text(row, "command");
        if !command.trim().is_empty() {
            rows.insert(
                0,
                json!({
                    "label": "Copy command",
                    "iconPath": "titlebar/copy.svg",
                    "command": { "type": "copyText", "text": command.trim() },
                }),
            );
        }
        let markdown = row_markdown(row, &self.snapshot["questionForms"]);
        if !markdown.is_empty() {
            rows.push(json!({
                "label": "Copy as Markdown",
                "iconPath": "titlebar/markdown.svg",
                "command": { "type": "copyText", "text": markdown },
            }));
        }
        // Copy reference and Reference in chat (gx-chat-core `item_references.rs`): the row's
        // question, page or work item as a link named by its words; Copy reference's keeps the
        // chat's session in a question's link, for pasting into another chat.
        let mention = text(row, "mention");
        let reference = Some(text(row, "reference"))
            .filter(|reference| !reference.is_empty())
            .unwrap_or_else(|| mention.clone());
        if !reference.is_empty() {
            rows.push(json!({
                "label": "Copy reference",
                "iconPath": "titlebar/hash.svg",
                "command": { "type": "copyText", "text": reference },
            }));
        }
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
                "label": "Reference in chat",
                "iconPath": "titlebar/quote.svg",
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
    pub(super) fn item_line(
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
        self.ask_title_weighted(
            key,
            title,
            runs,
            one_line,
            FontWeight::NORMAL,
            color,
            size,
            line_height,
            p,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask_title_weighted(
        &self,
        key: &str,
        title: &str,
        runs: &Value,
        one_line: bool,
        weight: FontWeight,
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
                .font_weight(weight)
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
                weight,
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
                quiet_links: false,
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
        let (button_id, label, icon) = (
            format!("dock-row-menu:{key}"),
            "More actions",
            "titlebar/dots.svg",
        );
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
                    .id(gpui::SharedString::from(button_id))
                    .role(gpui::Role::Button)
                    .aria_label(label)
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
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                    })
                    .child(svg().path(icon).size(px(14.0 * s)).text_color(p.card_muted))
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
    pub(super) fn show_dock_row_menu(
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
    pub(super) fn work_thread_logo(
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

    pub(super) fn work_card_part(
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
        let state = text(chip, "state");
        let state_color = super::coordinator_board::tone_color(&text(chip, "tone"))
            .map_or(p.muted, |color| gpui::rgb(color).into());
        let (icon, icon_color) = match kind.as_str() {
            "thread" if !agent.is_empty() => (
                format!("agent-icons/{agent}.svg"),
                super::new_session_welcome::brand_logo_color(&agent, p),
            ),
            "github" => ("titlebar/brand-github.svg".to_string(), p.muted),
            "usageLimit" => (chip_icon(&kind).to_string(), state_color),
            _ => (chip_icon(&kind).to_string(), p.muted),
        };
        let label_color = if kind == "usageLimit" {
            state_color
        } else {
            p.card_muted
        };
        let tip = text(chip, "title");
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
            .text_color(label_color)
            .when(!tip.is_empty(), |part| {
                let tip = tip.clone();
                part.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            })
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
        // The item's title is cut, never collapsed: on a tight row the time steps aside first
        // (cards.rs `title_fit`, the user's Q3300 A).
        let room_key = format!("work-turn-card:{}", text(card, "key"));
        let time_width =
            super::fonts::label_width(&time, &p.font, FontWeight::NORMAL, 12.0 * s, cx) / s;
        let fit = if title.is_empty() {
            super::cards::TitleFit {
                show_time: true,
                show_marks: true,
            }
        } else {
            let lead_width = super::fonts::label_width(
                &lead,
                &p.font,
                FontWeight(shape.title_weight),
                shape.title_size * s,
                cx,
            ) / s
                + 6.0;
            shape.title_fit(
                self.title_room.borrow().get(&room_key).copied(),
                lead_width,
                time_width,
                0.0,
            )
        };
        let show_time = !time.is_empty() && fit.show_time;
        let time_room = if show_time {
            time_width + super::cards::TITLE_ROW_GAP
        } else {
            0.0
        };
        let row = div()
            .h(px(shape.header_height(s)))
            .px(px(shape.padding_x * s))
            .flex()
            .items_center()
            .gap(px(super::cards::TITLE_ROW_GAP * s))
            .min_w_0()
            .child(
                shape
                    .tile("titlebar/layout-board-split.svg", p)
                    .child(shape.mark_badge(mark, color, p)),
            )
            .child(
                shape
                    .title(div().relative().flex_1().min_w_0().flex().items_center(), p)
                    .gap(px(6.0 * s))
                    .overflow_hidden()
                    .child(super::cards::title_room_probe(
                        self.title_room.clone(),
                        room_key,
                        time_room,
                        s,
                        cx.entity_id(),
                    ))
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
                                .min_w(px(shape.name_min() * s))
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
            .when(show_time, |row| {
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
