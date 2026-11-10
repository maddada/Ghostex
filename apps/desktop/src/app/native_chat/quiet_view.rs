//! Quiet View's lines (gx-chat-core `transcript/quiet_view.rs`, where the user's decision is): the
//! core names, by row, what each quiet line holds (`quietView.stretches`); this draws the line in
//! the first row's place, nothing for the other rows while it is shut, and, opened, every row as the
//! full view draws it on a thin rail under the line. What stays in view of the row that ends a line
//! (its reply, pages, answers, approvals and messages that were not sent) is drawn after it, open or
//! shut.

use super::completed_work_row::{WorkTail, spaced_tail};
use super::quiet_work::{QuietSegment, check, line_icon, loader, styled_segments};
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

/// Where a row stands among the lines.
#[derive(Clone, Copy)]
pub(super) struct QuietPlace {
    /// The line the row is one of.
    line: Option<usize>,
    /// The line of work item lines drawn after the row.
    after: Option<usize>,
}

/// The rail an opened line's rows sit on, as an opened Reduced line's steps do (quiet_work.rs).
fn rail(rows: Vec<AnyElement>, bottom: f32, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .ml(px(6.0 * s))
        .pl(px(16.0 * s))
        .pb(px(bottom * s))
        .gap(px(8.0 * s))
        .border_l(px(s))
        .border_color(p.foreground.opacity(0.1))
        .children(rows)
        .into_any_element()
}

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
        let snapshot = std::sync::Arc::as_ptr(&self.snapshot) as usize;
        let mut cache = self.quiet_lines_cache.borrow_mut();
        if let Some((at, lines)) = cache.as_ref()
            && *at == snapshot
        {
            return Rc::clone(lines);
        }
        let lines = Rc::new(QuietLines::of(&self.snapshot));
        *cache = Some((snapshot, Rc::clone(&lines)));
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
        if self.quiet_open(line) {
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
                if index == line.start {
                    parts.push((self.quiet_view_line(line, open, p, cx), 0.0));
                }
                let tail = line.tail.as_ref().filter(|tail| tail.row == index);
                if open {
                    let mut nested: Vec<AnyElement> = Vec::new();
                    if index == line.start {
                        let cards = self.quiet_cards(line);
                        if let Some(stack) = self.work_card_stack(&cards, p, cx) {
                            nested.push(stack);
                        }
                    }
                    let content = match tail {
                        Some(tail) => {
                            let folded = fold_part(item, tail);
                            self.item_content(&folded, next, true, p, window, cx)
                        }
                        None => self.item_content(item, next, true, p, window, cx),
                    };
                    nested.push(content);
                    // The rail runs on through the row gap to the next row of the line.
                    let bottom = if tail.is_none() && index + 1 < line.end {
                        p.row_gap()
                    } else {
                        0.0
                    };
                    let gap = if index == line.start { 4.0 } else { 0.0 };
                    parts.push((rail(nested, bottom, p), gap));
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
                                let mut part = vec![self.quiet_answers_line(card, open, p, cx)];
                                if open
                                    && let Some(cards) =
                                        self.question_exchange_cards(key, questions, p, cx)
                                {
                                    part.push(
                                        div().pt(px(4.0 * s)).child(cards).into_any_element(),
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
                    } else if open {
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
            parts.push((self.quiet_view_line(line, open, p, cx), 8.0));
            if open {
                let cards = self.quiet_cards(line);
                if let Some(stack) = self.work_card_stack(&cards, p, cx) {
                    parts.push((rail(vec![stack], 0.0, p), 4.0));
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

    /// The work item lines a line holds, as the dock's card JSON.
    fn quiet_cards(&self, line: &QuietStretch) -> Vec<Value> {
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
            .cloned()
            .collect()
    }

    /// The quiet line itself: an icon (the turning loader while the turn runs, the messages icon on
    /// a line reports came in on, else the clock and the green check), its counts, its time, and
    /// the chevron that shows on hover or while it is open.
    fn quiet_view_line(
        &self,
        line: &QuietStretch,
        open: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let icons = if line.running {
            vec![loader(&line.key, p, cx.reduce_motion())]
        } else if !line.time.is_empty() {
            vec![line_icon("titlebar/messages.svg", 13.0, p)]
        } else {
            vec![line_icon("titlebar/clock.svg", 13.0, p), check(p)]
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

    /// The answers card `card` stands for, under Quiet View, when the row `id` is one.
    pub(super) fn quiet_answers(&self, id: &str) -> Option<QuietAnswers> {
        self.quiet_lines().answers.get(id).cloned()
    }

    /// An answers card drawn shut as its own line (gx-chat-core `transcript/quiet_view.rs`
    /// `QuietAnswers` has the user's words): the answered check, "You answered “…”" and its count,
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
            vec![check(p)],
            styled_segments(segments),
            open,
            true,
            Vec::new(),
            p,
            cx,
        )
    }
}
