//! A finished turn: its "Worked for 2m 5s" summary with the turn's counts beside it, the work log
//! behind it with the "N files changed" fold right under the summary while it is open, the
//! artifacts and answered questions it keeps in view, and the final reply. What the summary says is decided in gx-chat-core `work_summary.rs`.
//!
//! At rest the row is one column with a 10px gap. While the
//! turn is folding, or its log is opening or closing, `disclosure_motion.rs` reports a frame and
//! the row is built from the same parts with each gap carried inside the part that owns it, so a
//! part whose height eases to nothing takes its gap with it and the first and last frames match
//! the live rows and the settled row exactly.

use super::cards::{SINGLE_LINE_CARD, card_hover_fill};
use super::disclosure_motion::{FoldFrame, FoldMotionKind, motion_clip};
use super::transcript::text;
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::{ProjectedMessage, TranscriptItem};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

/// Space between the rows of an opened turn's log.
const LOG_GAP: f32 = 8.0;
/// Space between the parts of a finished turn, at Message spacing Default (`part_gap`).
const PART_GAP: f32 = 10.0;

/// Space between the parts of a finished turn: `PART_GAP`, shrunk at Compact like the row gap.
fn part_gap(p: &ChatAppearance) -> f32 {
    p.inner_gap(PART_GAP)
}

/// A part whose height eases between nothing and its natural height; the fold applies its own
/// opacity to the rows, so the clip carries none.
fn eased_part(height: Rc<Cell<f32>>, factor: f32, content: AnyElement) -> AnyElement {
    motion_clip(
        height,
        FoldFrame {
            kind: FoldMotionKind::Fold,
            heading: 1.0,
            heading_offset: 0.0,
            log: factor,
            rows_opacity: 1.0,
        },
        0.0,
        0.0,
        content,
    )
}

/// What a finished turn keeps out of its fold, drawn after it: shaped like the same-named fields
/// of `completed-work`, and of each turn a `grouped-work` line merged.
pub(super) struct WorkTail<'a> {
    /// The turn's `work:` key, which its answered questions' cards are kept under.
    pub key: &'a str,
    pub artifacts: &'a [ProjectedMessage],
    pub questions: &'a [Value],
    pub sent_messages: &'a [Value],
    pub gbot_cards: &'a [Value],
    pub workflows: &'a [Value],
    pub final_message: Option<&'a Value>,
}

/// A finished turn's messages and answers that fold into its row (gx-chat-core `folded_cards.rs`),
/// split from what stays after it: `rows` (the folded `artifacts`), `questions` and `sent` are drawn
/// at the top of the opened row; `kept` and the `tail_` lists are the tail's.
pub(super) struct FoldedCards<'a> {
    pub rows: Vec<&'a ProjectedMessage>,
    pub questions: &'a [Value],
    pub sent: &'a [Value],
    pub kept: Vec<ProjectedMessage>,
    pub tail_questions: &'a [Value],
    pub tail_sent: &'a [Value],
}

impl<'a> FoldedCards<'a> {
    pub(super) fn of(
        in_fold: bool,
        artifacts: &'a [ProjectedMessage],
        questions: &'a [Value],
        sent: &'a [Value],
    ) -> Self {
        if !in_fold {
            return Self {
                rows: Vec::new(),
                questions: &[],
                sent: &[],
                kept: artifacts.to_vec(),
                tail_questions: questions,
                tail_sent: sent,
            };
        }
        let (rows, kept): (Vec<&ProjectedMessage>, Vec<&ProjectedMessage>) = artifacts
            .iter()
            .partition(|row| row["foldedIntoWork"] == true);
        Self {
            rows,
            questions,
            sent,
            kept: kept.into_iter().cloned().collect(),
            tail_questions: &[],
            tail_sent: &[],
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.questions.is_empty() && self.sent.is_empty()
    }
}

/// Each tail part with the space above it. The heading is a one-line card unless `above_is_card`
/// says otherwise, so a stack of cards right under it (or under another stack) sits a row gap away
/// (`SINGLE_LINE_CARD.gap_above`); `first_text_gap`, when given, is the space above a first part
/// that is not a card (a quiet line's 2px, quiet_work.rs).
pub(super) fn spaced_tail(
    parts: Vec<(AnyElement, bool)>,
    mut above_is_card: bool,
    first_text_gap: Option<f32>,
    p: &ChatAppearance,
) -> Vec<(AnyElement, f32)> {
    let mut first = true;
    parts
        .into_iter()
        .map(|(part, is_card)| {
            let gap = match first_text_gap {
                Some(gap) if first && !is_card => gap,
                _ => SINGLE_LINE_CARD.gap_above(above_is_card, is_card, part_gap(p), p),
            };
            first = false;
            above_is_card = is_card;
            (part, gap)
        })
        .collect()
}

impl NativeChatView {
    pub(super) fn completed_work_row(
        &mut self,
        item: &TranscriptItem,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Only a finished turn is drawn here (transcript.rs).
        let TranscriptItem::CompletedWork {
            id: turn,
            work,
            questions,
            sent_messages,
            gbot_cards,
            workflows,
            artifacts,
            final_message,
            read_images,
            cards_in_fold,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let id = format!("work:{turn}");
        // The turn's messages between agents and answers, folded into the row (gx-chat-core
        // `folded_cards.rs`, where the user's decision is): drawn at the top of the opened row and
        // never after it.
        let folded = FoldedCards::of(*cards_in_fold, artifacts, questions, sent_messages);
        let expanded = self.is_expanded(&id, p.work_open);
        self.disclosure_motion
            .borrow_mut()
            .arrived(&id, !expanded, cx.reduce_motion());
        let motion = self.disclosure_frame(&id, expanded, cx);
        // The closed card keeps the pictures its rows read in view, under its header (gx-chat-core
        // `read_images.rs`); opened, each row shows its own.
        let card = &*SINGLE_LINE_CARD;
        let card_fill = card.solid_fill(p);
        let pictures = (!expanded && motion.is_none())
            .then(|| {
                let pictures = serde_json::Value::Array(read_images.clone());
                self.read_picture_strip(&format!("{id}:read"), &pictures, 0.0, card_fill, p, cx)
            })
            .flatten();
        let heading = self.work_summary(item, &id, expanded, pictures, p, cx);
        // Every row of a finished turn hides its own writes; they belong to the turn's
        // "N files changed" fold, drawn right under the heading while the turn is open
        // (file_change_card.rs, gx-chat-core `turn_files_fold`). Rows that are still folding keep
        // theirs until they are gone, as they were a moment ago.
        let folding = motion.is_some_and(|frame| frame.kind == FoldMotionKind::Fold);
        let log = if folding {
            let mut rows: Vec<AnyElement> = Vec::new();
            for message in folded.rows.iter().copied() {
                rows.push(self.message_row(message, p, window, cx));
            }
            for message in work {
                rows.push(self.message_row(message, p, window, cx));
            }
            (!rows.is_empty()).then_some(rows)
        } else if expanded || motion.is_some() {
            // The files fold leads the opened turn, collapsed, then the log; both ease in and out
            // together with the turn.
            let mut parts: Vec<AnyElement> = Vec::new();
            // Whether the part right above the log is a stack of one-line cards.
            let mut cards_above = false;
            if let Some(cards) = self.folded_cards(&id, &folded, p, window, cx) {
                parts.push(cards);
                cards_above = true;
            }
            if let Some(files) = self.completed_files_fold(item, true, p, cx) {
                parts.push(files);
                cards_above = false;
            }
            // The log starts at the turn's left edge, under the folded cards and the files fold.
            //
            // CDXC:SessionChat 2026-11-10 DECISION: User, of the opened turn's "Used N tools" card:
            // "why do i see a line next to the used tools card". React hung the log off a rail under
            // a heading whose chevron sat in the marker column (`SessionChatExpansion`); the heading
            // is a card at the left edge now, and the folded messages, answers and files fold above
            // the log stand outside any rail, so the line only set the log apart from the rest of
            // its own turn. The log has no rail or indent; the heading closes the turn.
            self.hide_file_changes = true;
            let mut log: Vec<AnyElement> = Vec::new();
            if let Some(notice) = self.deferred_work_notice(item, p, cx) {
                log.push(notice);
            }
            self.in_work_fold = true;
            // A row that opens on its tool card right under one that closes on its own is a row gap
            // away from it, like any two one-line cards (`SINGLE_LINE_CARD.gap_above`); the log
            // already puts its own 8px between rows.
            let mut card_above = false;
            for message in work {
                let row = self.message_row(message, p, window, cx);
                let card_first = message["toolCard"].is_object()
                    && super::transcript::text(message, "text").trim().is_empty();
                let extra =
                    SINGLE_LINE_CARD.gap_above(card_above, card_first, LOG_GAP, p) - LOG_GAP;
                log.push(if extra > 0.0 {
                    div().mt(px(extra * s)).child(row).into_any_element()
                } else {
                    row
                });
                card_above = message["toolCard"].is_object();
            }
            self.in_work_fold = false;
            // A log that opens on a tool card right under the folded cards is a row gap below
            // them, like any two one-line cards (`SINGLE_LINE_CARD.gap_above`), not a part gap.
            let log_opens_on_card = log.len() == work.len()
                && work.first().is_some_and(|message| {
                    message["toolCard"].is_object()
                        && super::transcript::text(message, "text").trim().is_empty()
                });
            let log_extra =
                SINGLE_LINE_CARD.gap_above(cards_above, log_opens_on_card, part_gap(p), p)
                    - part_gap(p);
            if !log.is_empty() {
                parts.push(
                    div()
                        .when(log_extra > 0.0, |log| log.mt(px(log_extra * s)))
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .gap(px(LOG_GAP * s))
                        .children(log)
                        .into_any_element(),
                );
            }
            (!parts.is_empty()).then_some(parts)
        } else {
            None
        };
        self.hide_file_changes = true;
        let tail = self.work_tail_parts(
            WorkTail {
                key: &id,
                artifacts: &folded.kept,
                questions: folded.tail_questions,
                sent_messages: folded.tail_sent,
                gbot_cards,
                workflows,
                final_message: final_message.as_deref(),
            },
            p,
            window,
            cx,
        );
        let tail = spaced_tail(tail, log.is_none(), None, p);
        self.hide_file_changes = false;

        let Some(frame) = motion else {
            let spaced = |part: AnyElement, gap: f32| div().pt(px(gap * s)).child(part);
            let mut row = div().flex().flex_col().w_full().child(heading);
            if let Some(log) = log {
                row = row.children(log.into_iter().map(|part| spaced(part, part_gap(p))));
            }
            return row
                .children(tail.into_iter().map(|(part, gap)| spaced(part, gap)))
                .into_any_element();
        };
        self.moving_completed_work_row(&id, frame, heading, log, tail, p)
    }

    /// The folded messages and answers at the top of an opened row, one card under another:
    /// the folded rows as they are drawn in the transcript, then the answered questions, then the
    /// messages the work sent (gx-chat-core `folded_cards.rs`).
    pub(super) fn folded_cards(
        &mut self,
        key: &str,
        folded: &FoldedCards<'_>,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if folded.is_empty() {
            return None;
        }
        let s = p.scale;
        let mut parts: Vec<AnyElement> = Vec::new();
        for message in folded.rows.iter().copied() {
            parts.push(self.message_row(message, p, window, cx));
        }
        parts.extend(self.question_exchange_cards(key, folded.questions, p, cx));
        parts.extend(self.sent_agent_message_cards(folded.sent, p, cx));
        let mut first = true;
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .children(parts.into_iter().map(|part| {
                    let gap = if std::mem::take(&mut first) {
                        0.0
                    } else {
                        SINGLE_LINE_CARD.gap_above(true, true, PART_GAP, p)
                    };
                    div().pt(px(gap * s)).child(part)
                }))
                .into_any_element(),
        )
    }

    /// The parts of a finished turn after its fold, in order: its artifacts, its answered
    /// questions, the messages its work sent other agents, its Workflow cards and its reply, each
    /// with whether it is a stack of one-line cards. A Reduced line draws each merged turn's the
    /// same way (quiet_work.rs).
    pub(super) fn work_tail_parts(
        &mut self,
        tail: WorkTail<'_>,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(AnyElement, bool)> {
        let mut parts: Vec<(AnyElement, bool)> = Vec::new();
        for message in tail.artifacts {
            parts.push((self.message_row(message, p, window, cx), false));
        }
        // The turn's answered questions, lifted out of the fold so an exchange the reader took
        // part in is never buried by a collapsed "Worked for Xs" section.
        if let Some(cards) = self.question_exchange_cards(tail.key, tail.questions, p, cx) {
            parts.push((cards, true));
        }
        if let Some(cards) = self.sent_agent_message_cards(tail.sent_messages, p, cx) {
            parts.push((cards, true));
        }
        if let Some(cards) = self.gbot_cards(tail.gbot_cards, p, cx) {
            parts.push((cards, true));
        }
        if let Some(cards) = self.workflow_cards(tail.workflows, p, cx) {
            parts.push((cards, false));
        }
        if let Some(reply) = tail.final_message.filter(|reply| reply.is_object()) {
            parts.push((self.message_row(reply, p, window, cx), false));
        }
        parts
    }

    /// The bar a finished turn folds behind: a clock tile with a green check badge, "Worked for
    /// 2m 5s" and the turn's counts as pills, so the closed fold still says what happened; the
    /// whole bar opens and closes it, with no chevron. Pressing it opens the log, reading a turn
    /// whose rows are still on the server first.
    ///
    /// Its height, fill and type are every one-line card's (`SINGLE_LINE_CARD`, cards.rs).
    ///
    /// CDXC:SessionChat 2026-10-11 DECISION: User: move the count pills to the right side of the "Worked …" row and give them fully rounded (pill) corners. How they fit a narrow row is `stat_pills`' (2026-11-10, no longer wrapping).
    fn work_summary(
        &self,
        item: &TranscriptItem,
        id: &str,
        expanded: bool,
        pictures: Option<AnyElement>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let TranscriptItem::CompletedWork {
            id: turn,
            label,
            stats,
            expandable,
            deferred,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let expandable = *expandable;
        let label = label.clone();
        let stats: Vec<(String, String)> = stats
            .iter()
            .map(|stat| (text(stat, "kind"), text(stat, "label")))
            .collect();
        let spoken = std::iter::once(label.clone())
            .chain(stats.iter().map(|(_, label)| label.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        let card = &*SINGLE_LINE_CARD;
        // CDXC:SessionChat 2026-10-25 DECISION: User: "make hovering on this [the Worked for header] show same bg change that we get when i hover on this [the Message from card]". The press and its hover live on the row inside the bordered shell, as the Message from card's do (inter_agent_message.rs `agent_card`), so the fill lies over the shell's fill; on the shell itself it replaced that fill and showed no change.
        let bar = card
            .shell(div(), p)
            .w_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(
                div()
                    .id(SharedString::from(format!("{id}:summary")))
                    .role(gpui::Role::Button)
                    .aria_label(spoken)
                    .aria_expanded(expanded)
                    .flex()
                    .items_center()
                    .w_full()
                    .h(px(card.header_height(s)))
                    .gap(px(10.0 * s))
                    .px(px(card.padding_x * s))
                    .child(
                        card.tile("titlebar/chat-card-clock.svg", p)
                            .child(card.check_badge(p)),
                    )
                    .child(
                        card.title(div().flex_shrink_0(), p)
                            .text_color(p.foreground)
                            .child(label),
                    )
                    .child(self.stat_pills(id, &stats, p, cx))
                    .when(expandable, |row| {
                        let id = id.to_string();
                        let action = deferred
                            .as_ref()
                            .filter(|work| work.is_object())
                            .map(|work| json!({"type":"loadWork","id":turn,"work":work}));
                        row.chat_cursor_pointer()
                            .hover(|style| style.bg(card_hover_fill(p)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.anchor_disclosure_toggle(&id, !expanded);
                                if expanded {
                                    this.expanded.remove(&id);
                                    this.collapsed.insert(id.clone());
                                } else {
                                    this.collapsed.remove(&id);
                                    this.expanded.insert(id.clone());
                                    if let Some(action) = &action {
                                        this.invoke(action.clone(), cx);
                                    }
                                }
                                cx.notify();
                            }))
                    }),
            )
            // The pictures the closed turn read, under its header past a hairline (gx-chat-core
            // `read_images.rs`, where the user's decision is).
            .when_some(pictures, |bar, pictures| {
                bar.child(
                    div()
                        .border_t(px(s))
                        .border_color(card.hairline(p))
                        .px(px(card.padding_x * s))
                        .py(px(8.0 * s))
                        .child(pictures),
                )
            });
        bar.into_any_element()
    }

    /// The count pills right of a "Worked for" title, always on one line: when they don't fit, the
    /// last ones drop their words and show icon and count ("7"), and only if that still doesn't fit
    /// the last ones go into one "+N" pill. The bar's own label still names every count. The room is
    /// measured as painted (`stat_room`), so a new width takes one more frame, as the pills row above
    /// the chat box does (pills_strip.rs `pills_strip_fit`).
    ///
    /// CDXC:SessionChat 2026-11-10 DECISION: User, of a "Worked for" row whose four pills ("7 tool calls", "1 file", "2 messages", "1 answer") wrapped onto two lines and were clipped: "Don't wrap and don't grow the row: follow the rule the pills row above the chat box just got": labels drop from the right, then one "+N" (`plan_strip`). Supersedes the 2026-10-11 rule that the pills wrap among themselves. The phone matches in `statPills.tsx`.
    fn stat_pills(
        &self,
        id: &str,
        stats: &[(String, String)],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        use super::pills_strip::{PILL_GAP, plan_strip};
        let s = p.scale;
        let card = &*SINGLE_LINE_CARD;
        let icon = |kind: &str| match kind {
            "files" => "titlebar/pencil.svg",
            "images" => "titlebar/photo.svg",
            "messages" => "titlebar/messages.svg",
            "answers" => "titlebar/chat-card-question.svg",
            "failed" | "recovered" => "titlebar/alert-triangle.svg",
            _ => "titlebar/tool.svg",
        };
        let tone = |kind: &str| {
            if matches!(kind, "failed" | "recovered") {
                p.error()
            } else {
                p.card_muted
            }
        };
        // Every label opens with its count ("7 tool calls"), which is all a short pill keeps.
        let short = |label: &str| label.split(' ').next().unwrap_or(label).to_string();
        let room = self.stat_room.borrow().get(id).copied();
        let (hidden, collapsed) = match room {
            Some(room) => {
                let size = card.pill_text_size();
                let width = |label: &str, with_icon: bool| {
                    card.pill_width(with_icon, text_width(label, size, p, cx))
                };
                let pills: Vec<(f32, f32)> = stats
                    .iter()
                    .map(|(_, label)| (width(label, true), width(&short(label), true)))
                    .collect();
                let overflow = width(&format!("+{}", stats.len()), false);
                plan_strip(room, &[], &pills, overflow, PILL_GAP)
            }
            // Not painted yet: everything whole, cut at the edge for one frame.
            None => (0, 0),
        };
        let visible = stats.len() - hidden;
        let mut pills: Vec<AnyElement> = stats[..visible]
            .iter()
            .enumerate()
            .map(|(index, (kind, label))| {
                let label = if index >= visible - collapsed {
                    short(label)
                } else {
                    label.clone()
                };
                card.pill(Some(icon(kind)), Some(label), tone(kind), p)
                    .into_any_element()
            })
            .collect();
        if hidden > 0 {
            pills.push(
                card.pill(None, Some(format!("+{hidden}")), p.card_muted, p)
                    .into_any_element(),
            );
        }
        let rooms = self.stat_room.clone();
        let key = id.to_string();
        let chat = cx.entity_id();
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .justify_end()
            .items_center()
            .gap(px(PILL_GAP * s))
            .children(pills)
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let room = f32::from(bounds.size.width) / s;
                        let mut rooms = rooms.borrow_mut();
                        if rooms.get(&key).is_none_or(|was| (was - room).abs() > 0.5) {
                            rooms.insert(key.clone(), room);
                            cx.notify(chat);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    /// The row while it folds, opens or closes. Each part carries the gap above it, so a part
    /// eased to nothing leaves no gap behind. Opening or closing, `log` is the files fold and the
    /// work log, which ease together.
    ///
    /// CDXC:SessionChat 2026-11-08 WHY:
    /// Folding rows sit one row gap above the tail, whose first part brings its own part gap (`part_gap`). When the row gap is smaller than the part gap, the log's clip measures the padding too, so making up the difference with negative padding clipped the bottom of the last row (a tool card's border). The first tail part's own gap gives the difference back instead, eased with the log so it lands on the part gap. Compact halves the part gap with the row gap (CDXC:SessionChat 2026-11-09 DECISION at `inner_gap`, message_spacing.rs), so the row gap is the larger one at every spacing and the log pads its own bottom instead.
    fn moving_completed_work_row(
        &mut self,
        id: &str,
        frame: FoldFrame,
        heading: AnyElement,
        log: Option<Vec<AnyElement>>,
        mut tail: Vec<(AnyElement, f32)>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        // The space between two transcript list items, the gap every row wrapper ends with, which
        // the live rows had between them before they folded.
        let live_row_gap = p.row_gap();
        let part_gap = part_gap(p);
        let mut row = div().flex().flex_col().w_full();
        let top = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .top(px(-frame.heading_offset * s))
                    .opacity(frame.heading)
                    .child(heading),
            )
            .into_any_element();
        if frame.kind == FoldMotionKind::Fold {
            // The heading did not exist while the turn was live, so it grows in; the rows it
            // replaces keep the spacing they had as separate transcript rows until they are gone.
            row = row.child(eased_part(
                self.disclosure_motion.borrow_mut().height(id, "heading"),
                frame.heading,
                top,
            ));
            if let Some(rows) = log {
                if let Some((_, gap)) = tail.first_mut() {
                    *gap -= (part_gap - live_row_gap).max(0.0) * frame.log.clamp(0.0, 1.0);
                }
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    super::row_motion::fixed_opacity_scope(
                        frame.rows_opacity,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(live_row_gap * s))
                            .pb(px((live_row_gap - part_gap).max(0.0) * s))
                            .opacity(frame.rows_opacity)
                            .children(rows)
                            .into_any_element(),
                    ),
                ));
            }
        } else {
            row = row.child(top);
            if let Some(log) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    super::row_motion::fixed_opacity_scope(
                        frame.rows_opacity,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(part_gap * s))
                            .pt(px(part_gap * s))
                            .opacity(frame.rows_opacity)
                            .children(log)
                            .into_any_element(),
                    ),
                ));
            }
        }
        row.children(
            tail.into_iter()
                .map(|(part, gap)| div().pt(px(gap * s)).child(part).into_any_element()),
        )
        .into_any_element()
    }
}

/// How wide `label` is in the chat's font at `size` (both unscaled).
fn text_width(label: &str, size: f32, p: &ChatAppearance, cx: &gpui::App) -> f32 {
    super::fonts::label_width(label, &p.font, gpui::FontWeight::NORMAL, size * p.scale, cx)
        / p.scale
}
