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
use super::disclosure_body::{DisclosureRail, disclosure_body};
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
/// Space between the parts of a finished turn.
const PART_GAP: f32 = 10.0;

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
                _ => SINGLE_LINE_CARD.gap_above(above_is_card, is_card, PART_GAP, p),
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
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let s = p.scale;
        let id = format!("work:{turn}");
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
            for message in work {
                rows.push(self.message_row(message, p, window, cx));
            }
            (!rows.is_empty()).then_some(rows)
        } else if expanded || motion.is_some() {
            // The files fold leads the opened turn, collapsed, then the log; both ease in and out
            // together with the turn.
            let mut parts: Vec<AnyElement> = Vec::new();
            if let Some(files) = self.completed_files_fold(item, true, p, cx) {
                parts.push(files);
            }
            // React hung the whole log off the rail the "Worked for" heading opened
            // (`SessionChatExpansion`), so the turn's work reads as one indented block.
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
            if !log.is_empty() {
                parts.push(disclosure_body(
                    p,
                    DisclosureRail::Work,
                    LOG_GAP,
                    id.clone(),
                    "Collapse completed work",
                    log,
                    cx,
                ));
            }
            (!parts.is_empty()).then_some(parts)
        } else {
            None
        };
        self.hide_file_changes = true;
        let tail = self.work_tail_parts(
            WorkTail {
                key: &id,
                artifacts,
                questions,
                sent_messages,
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
                row = row.children(log.into_iter().map(|part| spaced(part, PART_GAP)));
            }
            return row
                .children(tail.into_iter().map(|(part, gap)| spaced(part, gap)))
                .into_any_element();
        };
        self.moving_completed_work_row(&id, frame, heading, log, tail, p)
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
    /// CDXC:SessionChat 2026-10-11 DECISION: User: move the count pills to the right side of the "Worked …" row and give them fully rounded (pill) corners. They wrap among themselves, right-aligned, when the row is narrow; the phone's `TranscriptItemRow.tsx` lays them out the same way.
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
        let chip = |kind: &str, label: String| {
            let failed = matches!(kind, "failed" | "recovered");
            let tone = if failed { p.error() } else { p.card_muted };
            let icon = match kind {
                "files" => "titlebar/pencil.svg",
                "images" => "titlebar/photo.svg",
                "failed" | "recovered" => "titlebar/alert-triangle.svg",
                _ => "titlebar/tool.svg",
            };
            card.pill(Some(icon), Some(label), tone, p)
        };
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
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .flex_wrap()
                            .justify_end()
                            .items_center()
                            .gap(px(6.0 * s))
                            .children(stats.into_iter().map(|(kind, label)| chip(&kind, label))),
                    )
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

    /// The row while it folds, opens or closes. Each part carries the gap above it, so a part
    /// eased to nothing leaves no gap behind. Opening or closing, `log` is the files fold and the
    /// work log, which ease together.
    ///
    /// CDXC:SessionChat 2026-11-08 WHY:
    /// Folding rows sit one row gap above the tail, whose first part brings its own `PART_GAP`. At Compact the row gap (8px) is smaller than `PART_GAP`, and the log's clip measures the padding too, so making up the difference with negative padding clipped the bottom of the last row (a tool card's border). The first tail part's own gap gives the difference back instead, eased with the log so it lands on `PART_GAP`.
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
                    *gap -= (PART_GAP - live_row_gap).max(0.0) * frame.log.clamp(0.0, 1.0);
                }
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(live_row_gap * s))
                        .pb(px((live_row_gap - PART_GAP).max(0.0) * s))
                        .opacity(frame.rows_opacity)
                        .children(rows)
                        .into_any_element(),
                ));
            }
        } else {
            row = row.child(top);
            if let Some(log) = log {
                row = row.child(eased_part(
                    self.disclosure_motion.borrow_mut().height(id, "log"),
                    frame.log,
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(PART_GAP * s))
                        .pt(px(PART_GAP * s))
                        .opacity(frame.rows_opacity)
                        .children(log)
                        .into_any_element(),
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
