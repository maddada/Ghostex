//! Quiet View: a chat that reads as the conversation. Everything between two things the user reads
//! (reports from threads, messages sent to them, tool calls, the line that says which work items a
//! turn changed, turns that ended without a reply) becomes ONE quiet line, however many turns it
//! took, which says what it holds in counts ("3 reports · 1 message sent · 3 tool calls · 1 item
//! updated") and opens on a click onto all of it, in order, as the full view draws it.
//!
//! The line folds the rows the full view already has: the transcript list is not rebuilt, so the
//! "Worked for" rows, Agent work Reduced's lines, Find, the minimap and every row index stay as
//! they are. The document's `quietView.stretches` names, by row index, the rows each line holds
//! (`start..end`) and, for the row that ends one, the parts of it that stay in view (`tail`); the
//! renderers draw the line in the first row's place, nothing for the rows under a closed line, and
//! the stretch's rows on a thin rail under an opened one.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! The user picked on the coordinator quiet page: option B, "one line between two replies": everything that happens between two things the user reads becomes one quiet line, however many turns it took, saying what it holds in counts, and opening on a click to show all of it in order as the full view would. A running turn (option A) is one line that counts up while the agent works ("Working · 3 reports · 1 message sent · 3 tool calls so far"); its cards do not pile up live, and when the turn ends it becomes part of the quiet line. Never hidden (option A, exactly this list): the user's own messages, the agent's replies, Ghostex pages and forms, questions and asks, the user's answers, approval cards waiting on him, errors and "Not sent" cards. The line that says which work items a turn changed folds into the quiet line. Where it applies, his words: "can be applied on all chats from the view menu but it's applied for coordinators by default unless user toggles this off. for other threads they can apply this as Quiet View. Also let's add a toggle in settings to expand the last turn in quiet view. also we need a toggle to let people apply this view by default for regular threads if they want." So a chat's View menu has Quiet View for any chat, and the chat remembers its own choice over the defaults; coordinator chats start in it (`chatQuietViewCoordinators`, on), other chats when `chatQuietViewChats` is on (off), and `chatQuietViewExpandLast` (off) shows the newest stretch (after the last reply, or the running turn) open until a newer reply folds it. Of the pill: "no need for this pill except when the default is something different", so the chat box's row shows a "Quiet view" or "Full view" chip only while the chat's view differs from its default, and a click on it goes back to the default.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! How it sits with the other looks. Summary view folds more than Quiet View does (earlier replies too), so a chat in Summary draws no quiet lines; its View menu still keeps the chat's Quiet View choice. Agent work Default and Reduced draw inside an opened line exactly as they draw without it. Agent work Quiet draws as Reduced while Quiet View is on (`TranscriptViewState::effective_chat_look`): its reply button and its hidden running work would hold the same work a second time, and hiding the running rows would leave the counting line nothing to count. Nothing on the never-hidden list is decided by those looks here: a row is judged on what it holds, so two looks at once never fold one of them away.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! Only a host that sends `quietView` settings gets the new shape (the desktop and the web build). The phone sends none, so its core keeps Quiet View off and publishes no `quietView`: a phone renderer that predates it never meets a row it cannot draw. What the phone needs to draw it is in the module's SEE-ALSO.
//!
//! CDXC:SessionChat 2026-11-11 SEE-ALSO:
//! apps/desktop/src/app/native_chat/quiet_view.rs (desktop and web draw the lines), apps/desktop/src/app/gx_chat/boot.rs (the settings), packages/settings-catalog/src/general/chat.rs (the rows), server/src/session_chat_history.rs (`deferredWork.toolCallCount`, the counts of a turn still on the server), `extras/transcript_search.rs` (Find skips a closed line's rows). The phone (apps/mobile/app/src/chat/) would need: its host to send `quietView` in its chat settings and the `quietView` record in its boot read, `quietView.stretches` drawn in `Transcript.tsx` as these lines, the View menu row and the chip's icon.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::TranscriptItem;
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::chat_look::{asks_in_words, waits_on_approval};

/// The three Quiet View settings (Settings > Chat > Advanced).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietViewSettings {
    /// Coordinator chats start in Quiet View (`chatQuietViewCoordinators`).
    #[serde(default = "on")]
    pub coordinators: bool,
    /// Every other chat starts in it too (`chatQuietViewChats`).
    #[serde(default)]
    pub chats: bool,
    /// The newest stretch shows open (`chatQuietViewExpandLast`).
    #[serde(default)]
    pub expand_last: bool,
}

fn on() -> bool {
    true
}

impl Default for QuietViewSettings {
    fn default() -> Self {
        Self {
            coordinators: true,
            chats: false,
            expand_last: false,
        }
    }
}

/// The document's `quietView`: absent where the host sends no settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietViewDocument {
    /// The chat's choice: its own from the View menu, else its default.
    pub on: bool,
    /// What the chat would be in without a choice of its own.
    pub default_on: bool,
    /// The lines, oldest first; empty while Quiet View is off or the chat is in Summary view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stretches: Vec<QuietStretch>,
}

/// One quiet line and the rows it holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietStretch {
    /// The line's disclosure key, `quiet:<first row's id>`.
    pub key: String,
    /// The rows it holds, `start..end` of the transcript list.
    pub start: usize,
    pub end: usize,
    /// A line that holds only the work item lines under a reply (`cards`) and no row: it is drawn
    /// at the end of this row, the reply's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<usize>,
    /// "3 reports · 1 message sent · 3 tool calls · 1 item updated".
    pub text: String,
    /// "1:08 to 1:09 AM" on a line that reports came in on; empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub time: String,
    /// The turn still runs: the line counts up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub running: bool,
    /// The line starts open (`chatQuietViewExpandLast`, the newest stretch).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
    /// The parts of the last row that stay in view after the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<QuietTail>,
    /// The keys of the work item lines (`workCards`, `turnLine`) the line holds, drawn at the top
    /// of the opened line instead of under the reply before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<String>,
}

/// What of a stretch's last row stays in view: a finished turn's (or each merged turn's) reply,
/// its rows that hold a page, a form, a picture, an answer or an approval, its answered questions
/// and the messages it could not send.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietTail {
    pub row: usize,
    pub turns: Vec<QuietTurnTail>,
}

/// One turn's part of [`QuietTail`]: `turn` is its place in a `grouped-work` row's `turns` (0 for
/// a `completed-work` row).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietTurnTail {
    pub turn: usize,
    /// The ids of its `artifacts` that stay in view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Its answered questions stay in view.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub questions: bool,
    /// The places in its `sentMessages` of the messages that were not sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<usize>,
    /// Its reply stays in view.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reply: bool,
}

/// Adopts the host's settings.
pub fn apply_settings(state: &mut ChatState, settings: QuietViewSettings) {
    state.transcript_view.quiet_view_settings = Some(settings);
    sync(state);
}

/// Whether this chat is a coordinator's.
fn coordinator(state: &ChatState) -> bool {
    crate::extras::coordinator_threads::is_coordinator(state.session.coordinator_threads.as_ref())
}

/// What the chat would be in without a choice of its own; `None` where Quiet View is unavailable.
fn default_on(state: &ChatState) -> Option<bool> {
    let settings = state.transcript_view.quiet_view_settings?;
    Some(if coordinator(state) {
        settings.coordinators
    } else {
        settings.chats
    })
}

/// The chat's choice, before the Summary view is taken into account.
fn chosen(state: &ChatState) -> Option<bool> {
    let default = default_on(state)?;
    Some(state.transcript_view.quiet_view_override.unwrap_or(default))
}

/// Sets `quiet_view_on` from the settings, the chat's choice and its view, throwing the cached
/// rows away when that changes how Agent work Quiet projects.
pub fn sync(state: &mut ChatState) {
    let on = chosen(state) == Some(true) && !state.transcript_view.summary_mode;
    let view = &mut state.transcript_view;
    if on != view.quiet_view_on {
        view.quiet_view_on = on;
        if view.chat_look.agent_work == crate::transcript::chat_look::AgentWork::Quiet {
            view.invalidate();
        }
    }
}

/// `setQuietView`: the View menu's choice (`on`), or the chip's way back to the default (`on`
/// null). Lands when its write does (`transcript::settle`).
pub fn set(state: &mut ChatState, on: Option<bool>) -> Vec<Effect> {
    let session_key = state.identity.session_key.clone();
    state.transcript_view.pending_quiet_view_override = Some(on);
    vec![Effect::WriteStorage {
        key: crate::composer::storage::quiet_view_key(&session_key),
        value: on.map(|on| crate::composer::storage::encode_summary(on).to_string()),
        durable: false,
    }]
}

/// The chip at the left of the chat box's pill strip while the chat's view is not its default:
/// "Quiet view" or "Full view", whose click goes back to the default.
pub fn view_chip(state: &ChatState) -> Option<crate::document::ViewChip> {
    let default = default_on(state)?;
    let on = chosen(state)?;
    if on == default {
        return None;
    }
    let (id, label, back) = if on {
        ("quiet", "Quiet view", "Full view")
    } else {
        ("full", "Full view", "Quiet view")
    };
    Some(crate::document::ViewChip {
        id: id.to_string(),
        label: label.to_string(),
        aria_label: format!("{label}. Go back to {back}"),
        command: serde_json::json!({"type": "setQuietView", "on": null}),
    })
}

/// The document's `quietView`.
pub fn document(
    state: &ChatState,
    context: &ChatContext,
    work_cards: &Value,
    approvals: &Value,
) -> Option<QuietViewDocument> {
    let default_on = default_on(state)?;
    let on = chosen(state)?;
    let stretches = if state.transcript_view.quiet_view_on {
        let items = crate::transcript::rows::rows_shared(state, context);
        stretches(
            items.iter(),
            &Inputs {
                coordinator: coordinator(state),
                working: crate::transcript::foreign::is_working(state),
                expand_last: state
                    .transcript_view
                    .quiet_view_settings
                    .is_some_and(|settings| settings.expand_last),
                approvals: open_approval_anchors(approvals),
                cards: turn_lines(work_cards),
            },
            context,
        )
    } else {
        Vec::new()
    };
    Some(QuietViewDocument {
        on,
        default_on,
        stretches,
    })
}

/// What the lines are built from besides the rows.
struct Inputs {
    coordinator: bool,
    working: bool,
    expand_last: bool,
    /// The rows an open relayed approval sits under (`relayedApprovals` `anchorId`).
    approvals: BTreeSet<String>,
    /// The work item lines under replies: (reply id, card key).
    cards: Vec<(String, String)>,
}

fn open_approval_anchors(approvals: &Value) -> BTreeSet<String> {
    approvals
        .as_array()
        .into_iter()
        .flatten()
        .filter(|card| card["open"] == true)
        .filter_map(|card| card["anchorId"].as_str())
        .filter(|anchor| !anchor.is_empty())
        .map(str::to_string)
        .collect()
}

fn turn_lines(work_cards: &Value) -> Vec<(String, String)> {
    work_cards
        .as_array()
        .into_iter()
        .flatten()
        .filter(|card| card["kind"] == "turnLine")
        .filter_map(|card| {
            Some((
                card["anchorId"].as_str()?.to_string(),
                card["key"].as_str()?.to_string(),
            ))
        })
        .collect()
}

fn has(row: &Value, key: &str) -> bool {
    row[key].as_array().is_some_and(|list| !list.is_empty())
}

/// A message from another session: a thread's report, a subagent's, any agent's.
fn is_report(row: &Value) -> bool {
    row["role"] == "user"
        && (row["interAgentMessage"].is_object() || row["agentMessage"].is_object())
}

/// Whether a row is something the user reads, which never folds: his own messages and answers, the
/// agent's words, pages, forms and questions, pictures, a call waiting on his approval or a relayed
/// approval under it, errors and notices, and a message that was not sent. A kind this does not
/// know stays in view.
fn reads(row: &Value, inputs: &Inputs) -> bool {
    if waits_on_approval(row)
        || row["id"]
            .as_str()
            .is_some_and(|id| inputs.approvals.contains(id))
        || has(row, "questions")
        || has(row, "images")
        || asks_in_words(row)
        || ["apiError", "systemCard", "suppressed", "sideQuestion"]
            .iter()
            .any(|key| row[*key].is_object())
        || not_sent(row).next().is_some()
    {
        return true;
    }
    match row["role"].as_str() {
        Some("user") => !is_report(row),
        Some("assistant") => row["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        Some("reasoning" | "tool") => false,
        _ => true,
    }
}

/// The places of a row's messages that were not sent.
fn not_sent(row: &Value) -> impl Iterator<Item = usize> + '_ {
    failed_sends(
        row["sentMessages"]
            .as_array()
            .map_or(&[][..], Vec::as_slice),
    )
}

fn failed_sends(cards: &[Value]) -> impl Iterator<Item = usize> + '_ {
    cards
        .iter()
        .enumerate()
        .filter(|(_, card)| card["failed"] == true)
        .map(|(at, _)| at)
}

/// The messages a list sent that drew a card and went out.
fn sent_count(cards: &[Value]) -> usize {
    cards
        .iter()
        .filter(|card| card["card"].is_object() && card["failed"] != true)
        .count()
}

fn list<'a>(row: &'a Value, key: &str) -> &'a [Value] {
    row[key].as_array().map_or(&[][..], Vec::as_slice)
}

/// What a line counts.
#[derive(Default)]
struct Counts {
    reports: usize,
    sent: usize,
    tools: usize,
    files: BTreeSet<String>,
    /// Sections still on the server, counted once each: (prompt id, tool calls).
    held: Vec<(String, usize)>,
    /// The finished turns' first start and latest end, for "Worked for".
    worked: Option<(Option<i64>, Option<i64>)>,
    /// A turn whose time the working card shows: the line says "Previous turn" instead.
    previous_turn: bool,
    first_at: Option<i64>,
    last_at: Option<i64>,
}

impl Counts {
    fn row(&mut self, row: &Value) {
        if is_report(row) {
            self.reports += 1;
        }
        self.sent += sent_count(list(row, "sentMessages"));
        self.tools += list(row, "tools").len();
        for file in list(row, "files") {
            if let Some(path) = file["path"].as_str() {
                self.files.insert(path.to_string());
            }
        }
        self.at(row["timestamp"].as_i64());
    }

    fn at(&mut self, at: Option<i64>) {
        if let Some(at) = at {
            self.first_at = Some(self.first_at.map_or(at, |first| first.min(at)));
            self.last_at = Some(self.last_at.map_or(at, |last| last.max(at)));
        }
    }

    fn turn(&mut self, label: &str, tally: &crate::document::WorkTally) {
        if tally.held.is_empty() {
            self.tools += tally.tool_calls;
        }
        for section in &tally.held {
            if !self.held.iter().any(|(start, _)| *start == section.0) {
                self.held.push(section.clone());
            }
        }
        self.files.extend(tally.changed.iter().cloned());
        self.previous_turn |= label == "Previous turn";
        self.worked = Some(match self.worked {
            None => (tally.started_ms, tally.ended_ms),
            Some((started, ended)) => (started, ended.max(tally.extends_to_ms)),
        });
        self.at(tally.started_ms);
        self.at(tally.ended_ms);
    }

    /// The part of one turn's rows that stays out of the fold: its reports and the messages it
    /// sent count; what of it stays in view is returned.
    fn turn_tail(
        &mut self,
        turn: usize,
        artifacts: &[crate::document::ProjectedMessage],
        questions: &[Value],
        sent: &[Value],
        reply: Option<&crate::document::ProjectedMessage>,
        inputs: &Inputs,
    ) -> Option<QuietTurnTail> {
        let mut kept = Vec::new();
        for row in artifacts {
            if is_report(row) {
                self.reports += 1;
            }
            self.sent += sent_count(list(row, "sentMessages"));
            if reads(row, inputs) {
                if let Some(id) = row["id"].as_str() {
                    kept.push(id.to_string());
                }
            }
        }
        self.sent += sent_count(sent);
        let reply = reply.filter(|reply| reply.is_object());
        if let Some(reply) = reply {
            self.sent += sent_count(list(reply, "sentMessages"));
        }
        let tail = QuietTurnTail {
            turn,
            artifacts: kept,
            questions: !questions.is_empty(),
            sends: failed_sends(sent).collect(),
            reply: reply.is_some(),
        };
        (!tail.artifacts.is_empty() || tail.questions || !tail.sends.is_empty() || tail.reply)
            .then_some(tail)
    }
}

/// How one row of the list takes part.
enum Place {
    /// It is read: it ends the line before it.
    Reads,
    /// It folds into the line.
    Folds,
    /// It folds into the line and ends it, with what of it stays in view after the line.
    Ends(Vec<QuietTurnTail>),
}

fn place(item: &TranscriptItem, counts: &mut Counts, inputs: &Inputs) -> Place {
    match item {
        TranscriptItem::Message { message } => {
            if reads(message, inputs) {
                Place::Reads
            } else {
                counts.row(message);
                Place::Folds
            }
        }
        TranscriptItem::CompletedWork {
            label,
            tally,
            artifacts,
            questions,
            sent_messages,
            final_message,
            ..
        } => {
            counts.turn(label, tally);
            match counts.turn_tail(
                0,
                artifacts,
                questions,
                sent_messages,
                final_message.as_ref(),
                inputs,
            ) {
                Some(tail) => Place::Ends(vec![tail]),
                None => Place::Folds,
            }
        }
        TranscriptItem::GroupedWork {
            label,
            tally,
            turns,
            ..
        } => {
            counts.turn(label, tally);
            let tails: Vec<QuietTurnTail> = turns
                .iter()
                .enumerate()
                .filter_map(|(at, turn)| {
                    counts.turn_tail(
                        at,
                        &turn.artifacts,
                        &turn.questions,
                        &turn.sent_messages,
                        turn.final_message.as_ref(),
                        inputs,
                    )
                })
                .collect();
            if tails.is_empty() {
                Place::Folds
            } else {
                Place::Ends(tails)
            }
        }
        TranscriptItem::Summary { .. } | TranscriptItem::Unknown(_) => Place::Reads,
    }
}

/// The id a row is known by, for its line's key.
fn row_id(item: &TranscriptItem) -> String {
    crate::transcript::presentation::message_id(item)
}

/// The ids of the replies a row draws, which work item lines sit under.
fn reply_ids(item: &TranscriptItem) -> Vec<String> {
    let id = |row: &crate::document::ProjectedMessage| row["id"].as_str().map(str::to_string);
    match item {
        TranscriptItem::Message { message } => id(message).into_iter().collect(),
        TranscriptItem::CompletedWork { final_message, .. } => {
            final_message.iter().filter_map(id).collect()
        }
        TranscriptItem::GroupedWork { turns, .. } => turns
            .iter()
            .filter_map(|turn| turn.final_message.as_ref())
            .filter_map(id)
            .collect(),
        _ => Vec::new(),
    }
}

/// A line being gathered.
struct Open {
    start: usize,
    key: String,
    counts: Counts,
}

/// The lines over `items` (see the module).
fn stretches<'a>(
    items: impl Iterator<Item = &'a TranscriptItem>,
    inputs: &Inputs,
    context: &ChatContext,
) -> Vec<QuietStretch> {
    let mut built: Vec<(QuietStretch, Counts)> = Vec::new();
    let mut open: Option<Open> = None;
    // The row each reply id is drawn in.
    let mut replies: Vec<(String, usize)> = Vec::new();
    let mut length = 0usize;
    for (at, item) in items.enumerate() {
        length = at + 1;
        for id in reply_ids(item) {
            replies.push((id, at));
        }
        // A row read changes no count, so the counts below are the open line's either way.
        let mut counts = open
            .as_mut()
            .map(|open| std::mem::take(&mut open.counts))
            .unwrap_or_default();
        let placed = place(item, &mut counts, inputs);
        match placed {
            Place::Reads => {
                if let Some(Open { start, key, .. }) = open.take() {
                    built.push((stretch(key, start, at, None), counts));
                }
            }
            Place::Folds => {
                let (start, key) = open.take().map_or_else(
                    || (at, format!("quiet:{}", row_id(item))),
                    |open| (open.start, open.key),
                );
                open = Some(Open { start, key, counts });
            }
            Place::Ends(turns) => {
                let (start, key) = open.take().map_or_else(
                    || (at, format!("quiet:{}", row_id(item))),
                    |open| (open.start, open.key),
                );
                built.push((
                    stretch(key, start, at + 1, Some(QuietTail { row: at, turns })),
                    counts,
                ));
            }
        }
    }
    // The stretch after the last thing read: the running turn while the agent works.
    let mut trailing: Option<usize> = None;
    if let Some(Open { start, key, counts }) = open.take() {
        let mut line = stretch(key, start, length, None);
        line.running = inputs.working;
        built.push((line, counts));
        trailing = Some(built.len() - 1);
    }
    // The work item lines under a reply join the line right after it, or make one of their own.
    for (anchor, card) in &inputs.cards {
        let Some((_, row)) = replies.iter().rev().find(|(id, _)| id == anchor) else {
            continue;
        };
        let after = row + 1;
        match built
            .iter_mut()
            .find(|(line, _)| line.start == after && line.after.is_none() && line.end > line.start)
        {
            Some((line, _)) => line.cards.push(card.clone()),
            None => match built.iter_mut().find(|(line, _)| line.after == Some(*row)) {
                Some((line, _)) => line.cards.push(card.clone()),
                None => {
                    let mut line = stretch(format!("quiet-cards:{anchor}"), after, after, None);
                    line.after = Some(*row);
                    line.cards.push(card.clone());
                    built.push((line, Counts::default()));
                }
            },
        }
    }
    built.sort_by_key(|(line, _)| (line.start, line.after.is_none()));
    // The newest stretch, which Expand the last turn shows open: the running or trailing one, or
    // the work item lines under the newest reply.
    let newest = match trailing {
        Some(_) => built
            .iter()
            .rposition(|(line, _)| line.end == length && line.tail.is_none()),
        None => built
            .iter()
            .rposition(|(line, _)| line.after.is_some_and(|row| row + 1 == length)),
    };
    built
        .into_iter()
        .enumerate()
        .map(|(at, (mut line, counts))| {
            line.open = inputs.expand_last && Some(at) == newest;
            words(&mut line, counts, inputs, context);
            line
        })
        .collect()
}

fn stretch(key: String, start: usize, end: usize, tail: Option<QuietTail>) -> QuietStretch {
    QuietStretch {
        key,
        start,
        end,
        tail,
        ..QuietStretch::default()
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The line's words and time.
fn words(line: &mut QuietStretch, counts: Counts, inputs: &Inputs, context: &ChatContext) {
    let tools = counts.tools + counts.held.iter().map(|(_, calls)| calls).sum::<usize>();
    let mut parts: Vec<String> = Vec::new();
    if counts.reports > 0 {
        parts.push(if inputs.coordinator {
            plural(counts.reports, "report", "reports")
        } else {
            plural(counts.reports, "message received", "messages received")
        });
    }
    if counts.sent > 0 {
        parts.push(plural(counts.sent, "message sent", "messages sent"));
    }
    if tools > 0 {
        parts.push(plural(tools, "tool call", "tool calls"));
    }
    if !counts.files.is_empty() {
        parts.push(plural(counts.files.len(), "file edited", "files edited"));
    }
    if !line.cards.is_empty() {
        parts.push(plural(line.cards.len(), "item updated", "items updated"));
    }
    let joined = parts.join(" \u{b7} ");
    line.text = if line.running {
        if joined.is_empty() {
            "Working".to_string()
        } else {
            format!("Working \u{b7} {joined} so far")
        }
    } else if counts.reports == 0 && counts.worked.is_some() {
        let lead = if counts.previous_turn {
            "Previous turn".to_string()
        } else {
            let (started, ended) = counts.worked.unwrap_or_default();
            crate::transcript::turns::worked_duration_label(started, ended)
        };
        if joined.is_empty() {
            lead
        } else {
            format!("{lead} \u{b7} {joined}")
        }
    } else if joined.is_empty() {
        "Worked".to_string()
    } else {
        joined
    };
    if counts.reports > 0 && !line.running {
        line.time = time_range(counts.first_at, counts.last_at, context);
    }
}

/// "1:08 AM", or "1:08 to 1:09 AM" when the line spans two times.
fn time_range(first: Option<i64>, last: Option<i64>, context: &ChatContext) -> String {
    let label = |at: i64| {
        crate::transcript::message_time::message_time(Some(at), context)["label"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let (Some(first), Some(last)) = (first, last) else {
        return String::new();
    };
    let (from, to) = (label(first), label(last));
    if from == to || from.is_empty() {
        return to;
    }
    for suffix in [" AM", " PM"] {
        if let (Some(from), true) = (from.strip_suffix(suffix), to.ends_with(suffix)) {
            if !from.contains(' ') {
                return format!("{from} to {to}");
            }
        }
    }
    format!("{from} to {to}")
}

/// The rows Find leaves out: those of each line the reader has not opened, but the row whose
/// reply or pages stay in view after it (`extras::transcript_search` searches only what the reader
/// can see).
pub fn closed_rows(
    state: &ChatState,
    context: &ChatContext,
    folds: &crate::extras::transcript_search::SearchFolds,
) -> Vec<std::ops::Range<usize>> {
    if !state.transcript_view.quiet_view_on {
        return Vec::new();
    }
    let approvals = crate::extras::relayed_approvals::project(state);
    let items = crate::transcript::rows::rows_shared(state, context);
    let inputs = Inputs {
        coordinator: coordinator(state),
        working: crate::transcript::foreign::is_working(state),
        expand_last: state
            .transcript_view
            .quiet_view_settings
            .is_some_and(|settings| settings.expand_last),
        approvals: open_approval_anchors(&approvals),
        cards: Vec::new(),
    };
    stretches(items.iter(), &inputs, context)
        .into_iter()
        .filter(|line| !folds.open(&line.key, line.open))
        .map(|line| {
            let end = match &line.tail {
                Some(tail) => tail.row,
                None => line.end,
            };
            line.start..end
        })
        .filter(|rows| !rows.is_empty())
        .collect()
}
