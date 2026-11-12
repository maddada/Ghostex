//! Quiet View's lines (gx-chat-core `transcript/quiet_view.rs`, where the user's decision is): the
//! core names, by row, what each quiet line holds (`quietView.stretches`); this draws the line in
//! the first row's place, nothing for the other rows while it is shut, and, opened, every row as the
//! full view draws it on a thin rail under the line. What stays in view of the row that ends a line
//! (its reply, pages, answers, approvals and messages that were not sent) is drawn after it, open or
//! shut.

use super::completed_work_row::{WorkTail, spaced_tail};
use super::quiet_work::{QuietSegment, line_icon, loader, styled_segments};
use super::{appearance::ChatAppearance, state::NativeChatView};
use ghostex_gx_chat_core::transcript::quiet_view::{
    QuietAnswers, QuietStretch, QuietTail, QuietTurnTail,
};
use ghostex_gx_chat_core::{ProjectedMessage, TranscriptItem};
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// The lines of one snapshot, indexed by row.
#[derive(Default)]
pub(super) struct QuietLines {
    lines: Vec<QuietStretch>,
    /// The line each row belongs to.
    rows: HashMap<usize, usize>,
    /// The work item lines' own line drawn at the end of a reply's row.
    after: HashMap<usize, usize>,
    /// The work item lines the lines hold, which the replies no longer draw.
    cards: HashSet<String>,
    /// The answers cards drawn shut as a line of their own, by row id (`<turn id>#questions` for a
    /// turn's answered questions).
    answers: HashMap<String, QuietAnswers>,
}

impl QuietLines {
    fn of(snapshot: &Value) -> Self {
        let lines: Vec<QuietStretch> =
            serde_json::from_value(snapshot["quietView"]["stretches"].clone()).unwrap_or_default();
        let answers: Vec<QuietAnswers> =
            serde_json::from_value(snapshot["quietView"]["answers"].clone()).unwrap_or_default();
        let answers = answers
            .into_iter()
            .map(|card| (card.id.clone(), card))
            .collect();
        let mut rows = HashMap::new();
        let mut after = HashMap::new();
        let mut cards = HashSet::new();
        for (at, line) in lines.iter().enumerate() {
            for row in line.start..line.end {
                rows.insert(row, at);
            }
            if let Some(row) = line.after {
                after.insert(row, at);
            }
            cards.extend(line.cards.iter().cloned());
        }
        Self {
            lines,
            rows,
            after,
            cards,
            answers,
        }
    }
}

/// The space between a shut line and the reply that ends it.
const LINE_TO_REPLY: f32 = 8.0;

/// The empty space under the time in a message's time line: the line is as tall as its 24px ⋯ button
/// and the time's 12px type sits in its middle, so the space a reader sees above a quiet line
/// starts that much higher than the line's own gap.
const TIME_ROW_SLACK: f32 = 6.0;

/// The space above the first letters of a message's text: its line height less its type size,
/// halved.
const REPLY_LEADING: f32 =
    (super::chat_text_look::PROSE_LINE_HEIGHT - super::chat_text_look::PROSE_SIZE) / 2.0;

/// The space above what a row draws first: a reply's or another message's words start their line's
/// leading down; a bubble, a card or a quiet line (whose own leading `balanced_line` accounts for on
/// both sides) starts at its edge.
fn top_leading(item: Option<&TranscriptItem>) -> f32 {
    match item {
        Some(TranscriptItem::Message { message }) if message["role"] == "assistant" => {
            REPLY_LEADING
        }
        _ => 0.0,
    }
}

/// `line` moved `shift` (unscaled; up when negative) without changing any row's height, so the
/// space a reader sees above and below a quiet line comes out the same, from the time line of the
/// row above (`TIME_ROW_SLACK`) to the first letters of what follows (`top_leading`), whatever
/// that is: its reply, your next message, another fold, the end of the chat.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User, of a folded "Previous turn · …" line: "space above is bigger than space below this previous turns line please fix", then "just increase space below and reduce it above pls". The line moves toward the larger gap until both are equal; the space it takes, and so every row's height, stays the same, so nothing else in the chat moves.
fn balanced_line(line: AnyElement, shift: f32, p: &ChatAppearance) -> AnyElement {
    if shift == 0.0 {
        return line;
    }
    div()
        .relative()
        .top(px(shift * p.scale))
        .child(line)
        .into_any_element()
}

/// Where a row stands among the lines.
#[derive(Clone, Copy)]
pub(super) struct QuietPlace {
    /// The line the row is one of.
    line: Option<usize>,
    /// The line of work item lines drawn after the row.
    after: Option<usize>,
}

/// The rail an opened line's rows sit on, as an opened Reduced line's steps do (quiet_work.rs).
fn rail(
    key: &str,
    rows: Vec<AnyElement>,
    bottom: f32,
    p: &ChatAppearance,
    cx: &Context<NativeChatView>,
) -> AnyElement {
    super::disclosure_body::closing_rail(
        p,
        key.to_string(),
        6.0,
        23.0,
        RAIL_ROW_GAP,
        bottom,
        rows,
        cx,
    )
}

/// The space between the rows on an opened line's rail, between two parts of one row (its work
/// cards and the row) and between one row and the next alike.
///
/// CDXC:SessionChat 2026-11-12 DECISION:
/// User, of an opened "1 report · 1 tool call · 1 item updated" line whose message row sat a row gap above the next row but 8px below the work card: "gap is different, please fix (make it like the small one, not the larger one)". Every gap on the rail is this 8px, on the desktop and the phone (`QuietRow` in apps/mobile/app/src/chat/native/transcript/quietView.tsx).
/// CDXC:SessionChat 2026-11-12 DECISION: User, of an opened "3 items updated" line whose "Work item finished" cards sat a row gap apart while an opened "1 report · 1 tool call" line's cards sat close: the gap between stacked cards in an opened line is "ALWAYS small". The work item cards on the rail are this 8px apart too, not the row gap one-line cards keep elsewhere (`work_card_stack`; the phone's `WorkCardStack` gets the same `gap`).
const RAIL_ROW_GAP: f32 = 8.0;

/// `item` without what of it stays in view after its line.
fn fold_part(item: &TranscriptItem, tail: &QuietTail) -> TranscriptItem {
    let mut item = item.clone();
    let trim = |turn: &QuietTurnTail,
                artifacts: &mut Vec<ProjectedMessage>,
                questions: &mut Vec<Value>,
                sent: &mut Vec<Value>,
                reply: &mut Option<ProjectedMessage>| {
        artifacts.retain(|row| {
            !row["id"]
                .as_str()
                .is_some_and(|id| turn.artifacts.iter().any(|kept| kept == id))
        });
        if turn.questions {
            questions.clear();
        }
        let mut at = 0usize;
        sent.retain(|_| {
            let keep = !turn.sends.contains(&at);
            at += 1;
            keep
        });
        if turn.reply {
            *reply = None;
        }
    };
    match &mut item {
        TranscriptItem::CompletedWork {
            artifacts,
            questions,
            sent_messages,
            final_message,
            ..
        } => {
            for turn in &tail.turns {
                trim(turn, artifacts, questions, sent_messages, final_message);
            }
        }
        TranscriptItem::GroupedWork { turns, .. } => {
            for turn in &tail.turns {
                if let Some(grouped) = turns.get_mut(turn.turn) {
                    trim(
                        turn,
                        &mut grouped.artifacts,
                        &mut grouped.questions,
                        &mut grouped.sent_messages,
                        &mut grouped.final_message,
                    );
                }
            }
        }
        _ => {}
    }
    item
}

/// What one turn of `item` keeps in view: (its `work:` key, rows, questions, unsent, reply).
type KeptTurn = (
    String,
    Vec<ProjectedMessage>,
    Vec<Value>,
    Vec<Value>,
    Option<ProjectedMessage>,
);

fn kept_parts(item: &TranscriptItem, tail: &QuietTail) -> Vec<KeptTurn> {
    let take = |turn: &QuietTurnTail,
                id: &str,
                artifacts: &[ProjectedMessage],
                questions: &[Value],
                sent: &[Value],
                reply: &Option<ProjectedMessage>| {
        (
            format!("work:{id}"),
            artifacts
                .iter()
                .filter(|row| {
                    row["id"]
                        .as_str()
                        .is_some_and(|id| turn.artifacts.iter().any(|kept| kept == id))
                })
                .cloned()
                .collect(),
            if turn.questions {
                questions.to_vec()
            } else {
                Vec::new()
            },
            turn.sends
                .iter()
                .filter_map(|at| sent.get(*at).cloned())
                .collect(),
            reply.clone().filter(|_| turn.reply),
        )
    };
    match item {
        TranscriptItem::CompletedWork {
            id,
            artifacts,
            questions,
            sent_messages,
            final_message,
            ..
        } => tail
            .turns
            .iter()
            .map(|turn| take(turn, id, artifacts, questions, sent_messages, final_message))
            .collect(),
        TranscriptItem::GroupedWork { turns, .. } => tail
            .turns
            .iter()
            .filter_map(|turn| {
                let grouped = turns.get(turn.turn)?;
                Some(take(
                    turn,
                    &grouped.id,
                    &grouped.artifacts,
                    &grouped.questions,
                    &grouped.sent_messages,
                    &grouped.final_message,
                ))
            })
            .collect(),
        _ => Vec::new(),
    }
}

impl NativeChatView {
    /// The current snapshot's lines, read once per snapshot.
    fn quiet_lines(&self) -> Rc<QuietLines> {
        // CDXC:SessionChat 2026-11-11 WHY: keyed by the snapshot it was read from, held weakly
        // (as message_actions.rs `is_final_reply`): a bare address can be taken by a later
        // snapshot once the old one is freed (off the main thread since 2026-11-10, state.rs
        // `drop_off_main_thread`), and would then hand back the old snapshot's lines.
        let current = std::sync::Arc::as_ptr(&self.snapshot);
        let mut cache = self.quiet_lines_cache.borrow_mut();
        if let Some((built_from, lines)) = cache.as_ref()
            && built_from.as_ptr() == current
        {
            return Rc::clone(lines);
        }
        let lines = Rc::new(QuietLines::of(&self.snapshot));
        *cache = Some((std::sync::Arc::downgrade(&self.snapshot), Rc::clone(&lines)));
        lines
    }

    /// Whether a work item line under a reply moved into a quiet line.
    pub(super) fn quiet_holds_card(&self, card: &Value) -> bool {
        card["key"]
            .as_str()
            .is_some_and(|key| self.quiet_lines().cards.contains(key))
    }

    /// Where row `index` stands among the lines, `None` for a row they leave alone.
    pub(super) fn quiet_row(&self, index: usize) -> Option<QuietPlace> {
        let lines = self.quiet_lines();
        let place = QuietPlace {
            line: lines.rows.get(&index).copied(),
            after: lines.after.get(&index).copied(),
        };
        (place.line.is_some() || place.after.is_some()).then_some(place)
    }

    fn quiet_open(&self, line: &QuietStretch) -> bool {
        self.is_expanded(&line.key, line.open)
    }

    /// The gap row `index` keeps below it when a line changes it (transcript.rs): none on an opened
    /// line's rail, which runs on to the next row; a shut line's own small gap above the reply that
    /// ends it.
    pub(super) fn quiet_row_bottom(&self, index: usize, p: &ChatAppearance) -> Option<f32> {
        let lines = self.quiet_lines();
        let line = &lines.lines[*lines.rows.get(&index)?];
        let tail_row = line.tail.as_ref().map(|tail| tail.row);
        if self.quiet_open(line) || self.disclosure_moving(&line.key) {
            (tail_row != Some(index) && index + 1 < line.end).then_some(0.0)
        } else {
            (index == line.start && tail_row.is_some_and(|row| row > index))
                .then(|| LINE_TO_REPLY * p.scale)
        }
    }

    /// Row `index` under Quiet View, or `None` when a shut line draws nothing for it.
    pub(super) fn quiet_row_content(
        &mut self,
        items: &[TranscriptItem],
        index: usize,
        place: QuietPlace,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let lines = self.quiet_lines();
        let item = &items[index];
        let next = items.get(index + 1);
        let mut parts: Vec<(AnyElement, f32)> = Vec::new();
        match place.line.map(|at| &lines.lines[at]) {
            None => parts.push((self.item_content(item, next, true, p, window, cx), 0.0)),
            Some(line) => {
                let open = self.quiet_open(line);
                // CDXC:SessionChat 2026-11-12 DECISION: User, of an opened Quiet View line and the
                // "2 tool calls" card under it: "make expand / collapse of this and all similar
                // ones animate please". The line's rows ease open and shut with the chat's one
                // fold motion (disclosure_motion.rs), each row's part clipped to the same frame.
                let frame = self.disclosure_frame(&line.key, open, cx);
                let shown = open || frame.is_some();
                if index == line.start {
                    // Midway between the time line of the row above and what follows the line: its
                    // reply, or the next row (`balanced_line`).
                    let below = if line.tail.is_some() {
                        LINE_TO_REPLY + REPLY_LEADING
                    } else {
                        p.row_gap() + top_leading(items.get(line.end))
                    };
                    let shift = (below - (TIME_ROW_SLACK + p.row_gap())) / 2.0;
                    let drawn = self.quiet_view_line(line, open, p, cx);
                    parts.push((balanced_line(drawn, shift, p), 0.0));
                }
                let tail = line.tail.as_ref().filter(|tail| tail.row == index);
                if shown {
                    let mut nested: Vec<AnyElement> = Vec::new();
                    if index == line.start {
                        let cards = self.quiet_cards(line);
                        if let Some(stack) =
                            self.work_card_stack(&cards, Some(RAIL_ROW_GAP * s), p, cx)
                        {
                            nested.push(stack);
                        }
                    }
                    self.quiet_boxed = true;
                    let content = match tail {
                        Some(tail) => {
                            let folded = fold_part(item, tail);
                            self.item_content(&folded, next, true, p, window, cx)
                        }
                        None => self.item_content(item, next, true, p, window, cx),
                    };
                    self.quiet_boxed = false;
                    nested.push(content);
                    // The rail runs on to the next row of the line, as far below as its parts sit
                    // apart.
                    let bottom = if tail.is_none() && index + 1 < line.end {
                        RAIL_ROW_GAP
                    } else {
                        0.0
                    };
                    let gap = if index == line.start { 4.0 } else { 0.0 };
                    let drawn = rail(&line.key, nested, bottom, p, cx);
                    let ends = tail.is_some() || index + 1 >= line.end;
                    let drawn = self.disclosure_part_motion(
                        &line.key,
                        &index.to_string(),
                        frame,
                        gap * s,
                        drawn,
                        ends,
                    );
                    parts.push((drawn, gap));
                }
                if let Some(tail) = tail {
                    let kept = kept_parts(item, tail);
                    let mut drawn: Vec<(AnyElement, bool)> = Vec::new();
                    self.hide_file_changes = true;
                    for (key, artifacts, questions, sent, reply) in &kept {
                        // The turn's answered questions are drawn shut as a line of their own.
                        let turn = key.strip_prefix("work:").unwrap_or(key);
                        let answers = self
                            .quiet_lines()
                            .answers
                            .get(&format!("{turn}#questions"))
                            .cloned();
                        let questions: &[Value] = match &answers {
                            Some(card) if !questions.is_empty() => {
                                let open = self.is_expanded(&card.key, card.open);
                                let frame = self.disclosure_frame(&card.key, open, cx);
                                let mut part = vec![self.quiet_answers_line(card, open, p, cx)];
                                if (open || frame.is_some())
                                    && let Some(cards) =
                                        self.question_exchange_cards(key, questions, p, cx)
                                {
                                    let cards =
                                        div().pt(px(4.0 * s)).child(cards).into_any_element();
                                    part.push(
                                        self.disclosure_body_motion(&card.key, frame, 0.0, cards),
                                    );
                                }
                                drawn.push((
                                    div()
                                        .flex()
                                        .flex_col()
                                        .w_full()
                                        .children(part)
                                        .into_any_element(),
                                    false,
                                ));
                                &[]
                            }
                            _ => questions,
                        };
                        drawn.extend(self.work_tail_parts(
                            WorkTail {
                                key,
                                artifacts,
                                questions,
                                sent_messages: sent,
                                gbot_cards: &[],
                                workflows: &[],
                                final_message: reply.as_deref(),
                            },
                            p,
                            window,
                            cx,
                        ));
                    }
                    self.hide_file_changes = false;
                    let gap = if parts.is_empty() {
                        0.0
                    } else if shown {
                        p.row_gap()
                    } else {
                        LINE_TO_REPLY
                    };
                    let mut first = true;
                    for (part, above) in spaced_tail(drawn, false, None, p) {
                        let above = if std::mem::take(&mut first) {
                            gap
                        } else {
                            above
                        };
                        parts.push((part, above));
                    }
                }
            }
        }
        if let Some(line) = place.after.map(|at| &lines.lines[at]) {
            let open = self.quiet_open(line);
            // Under its reply's time line, 8px below it and a row gap above the next row: midway
            // between the two.
            let drawn = self.quiet_view_line(line, open, p, cx);
            let below = p.row_gap() + top_leading(items.get(index + 1));
            let shift = (below - (TIME_ROW_SLACK + LINE_TO_REPLY)) / 2.0;
            parts.push((balanced_line(drawn, shift, p), LINE_TO_REPLY));
            let frame = self.disclosure_frame(&line.key, open, cx);
            if open || frame.is_some() {
                let cards = self.quiet_cards(line);
                if let Some(stack) = self.work_card_stack(&cards, Some(RAIL_ROW_GAP * s), p, cx) {
                    let drawn = rail(&line.key, vec![stack], 0.0, p, cx);
                    let drawn = self.disclosure_part_motion(
                        &line.key,
                        "after",
                        frame,
                        4.0 * s,
                        drawn,
                        true,
                    );
                    parts.push((drawn, 4.0));
                }
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .children(
                    parts
                        .into_iter()
                        .map(|(part, gap)| div().w_full().min_w_0().pt(px(gap * s)).child(part)),
                )
                .into_any_element(),
        )
    }

    /// Whether the tool calls drawn now sit in the "Message to" card's shell: in an opened Quiet
    /// View line's rows, but not in a finished turn's log under its "Worked for" card (already a
    /// card) nor as quiet steps.
    pub(super) fn quiet_boxed_here(&self) -> bool {
        self.quiet_boxed && !self.in_work_fold && !self.quiet_steps
    }

    /// The work item lines a line holds, as the dock's card JSON.
    /// Borrowed from the snapshot, as every work card stack takes its cards (work_cards.rs).
    fn quiet_cards(&self, line: &QuietStretch) -> Vec<&Value> {
        if line.cards.is_empty() {
            return Vec::new();
        }
        self.snapshot["workCards"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|card| {
                card["key"]
                    .as_str()
                    .is_some_and(|key| line.cards.iter().any(|held| held == key))
            })
            .collect()
    }

    /// The quiet line itself: an icon (the messages icon on a line reports came in on, else the
    /// clock; the turning loader alone while the turn runs), its counts, its time,
    /// and the chevron that shows on hover or while it is open.
    fn quiet_view_line(
        &self,
        line: &QuietStretch,
        open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        // One mark: a finished line its own 13px icon, a running line its turning loader.
        //
        // CDXC:SessionChat 2026-11-12 DECISION: User, of "Working · 1 message sent · 3 tool calls so
        // far": "Show just the spinner not spinner + icon."; then, of finished lines ("1 item
        // updated", "You answered …", "Worked for 19s · …"): "remove check mark from all these no
        // need". Quiet View lines never fail, so no mark is lost. The phone does the same
        // (quietView.tsx).
        let lead = if !line.time.is_empty() {
            "titlebar/messages.svg"
        } else {
            "titlebar/clock.svg"
        };
        let icons = if line.running {
            vec![loader(&line.key, p, cx.reduce_motion())]
        } else {
            vec![line_icon(lead, 13.0, p)]
        };
        let mut segments: Vec<QuietSegment> = vec![(line.text.clone(), None)];
        if !line.time.is_empty() {
            segments.push((format!("\u{2002}{}", line.time), None));
        }
        let spoken = segments
            .iter()
            .map(|(piece, _)| piece.as_str())
            .collect::<String>();
        self.quiet_line(
            line.key.clone(),
            spoken,
            icons,
            styled_segments(segments),
            open,
            true,
            Vec::new(),
            p,
            cx,
        )
    }

    /// One quiet line of the Summary view's open "Active work" (gx-chat-core
    /// `quiet_view::summary_work_lines`, where the user's decision is): the line, and while it is
    /// open the rows it holds on the rail an opened Quiet View line draws them on.
    pub(super) fn summary_quiet_run(
        &mut self,
        line: &QuietStretch,
        rows: &[&std::sync::Arc<Value>],
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let open = self.quiet_open(line);
        let frame = self.disclosure_frame(&line.key, open, cx);
        let mut column = div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.quiet_view_line(line, open, p, cx));
        if open || frame.is_some() {
            let drawn: Vec<AnyElement> = rows
                .iter()
                .map(|row| self.message_row(row, p, window, cx))
                .collect();
            let body = rail(&line.key, drawn, 0.0, p, cx);
            column = column.child(self.disclosure_body_motion(&line.key, frame, 4.0 * s, body));
        }
        column.into_any_element()
    }

    /// The answers card `card` stands for, under Quiet View, when the row `id` is one.
    pub(super) fn quiet_answers(&self, id: &str) -> Option<QuietAnswers> {
        self.quiet_lines().answers.get(id).cloned()
    }

    /// An answers card drawn shut as its own line (gx-chat-core `transcript/quiet_view.rs`
    /// `QuietAnswers` has the user's words): the question card's icon, "You answered “…”" and its count,
    /// and the chevron; opened, the card follows it.
    pub(super) fn quiet_answers_line(
        &self,
        card: &QuietAnswers,
        open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut segments: Vec<QuietSegment> = vec![(card.text.clone(), None)];
        if !card.count.is_empty() {
            segments.push((format!(" \u{b7} {}", card.count), None));
        }
        let spoken = segments
            .iter()
            .map(|(piece, _)| piece.as_str())
            .collect::<String>();
        self.quiet_line(
            card.key.clone(),
            spoken,
            // The question card's own icon alone, as every finished quiet line starts.
            vec![line_icon("titlebar/chat-card-checklist.svg", 13.0, p)],
            styled_segments(segments),
            open,
            true,
            Vec::new(),
            p,
            cx,
        )
    }
}
