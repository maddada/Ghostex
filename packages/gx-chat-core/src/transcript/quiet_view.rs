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
//! The user picked on the coordinator quiet page: option B, "one line between two replies": everything that happens between two things the user reads becomes one quiet line, however many turns it took, saying what it holds in counts, and opening on a click to show all of it in order as the full view would. A running turn (option A) is one line that counts up while the agent works ("Working · 3 reports · 1 message sent · 3 tool calls so far"); its cards do not pile up live, and when the turn ends it becomes part of the quiet line. Never hidden (option A, exactly this list): the user's own messages, the agent's replies, Ghostex pages and forms, questions and asks, the user's answers, approval cards waiting on him, errors and "Not sent" cards. Later the same night the user changed that for his answers: an answers card (to a page, a form, or a question or ask) is drawn shut as one small line of its own and opens on a click (`QuietAnswers`, where his words are); his typed messages stay fully shown. The line that says which work items a turn changed folds into the quiet line. Where it applies, his words: "can be applied on all chats from the view menu but it's applied for coordinators by default unless user toggles this off. for other threads they can apply this as Quiet View. Also let's add a toggle in settings to expand the last turn in quiet view. also we need a toggle to let people apply this view by default for regular threads if they want." So a chat's View menu has Quiet View for any chat, and the chat remembers its own choice over the defaults; coordinator chats start in it (`chatQuietViewCoordinators`, on), other chats when `chatQuietViewChats` is on (off), and `chatQuietViewExpandLast` (off) shows the newest stretch (after the last reply, or the running turn) open until a newer reply folds it. Of the pill: "no need for this pill except when the default is something different", so the chat box's row shows a "Quiet view" or "Full view" chip only while the chat's view differs from its default, and a click on it goes back to the default.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! How it sits with the other looks. Summary view folds more than Quiet View does (earlier replies too), so a chat in Summary draws no quiet lines; its View menu still keeps the chat's Quiet View choice. Agent work Default and Reduced draw inside an opened line exactly as they draw without it. Agent work Quiet draws as Reduced while Quiet View is on (`TranscriptViewState::effective_chat_look`): its reply button and its hidden running work would hold the same work a second time, and hiding the running rows would leave the counting line nothing to count. Nothing on the never-hidden list is decided by those looks here: a row is judged on what it holds, so two looks at once never fold one of them away.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! Only a chat with Quiet View settings gets the new shape: the desktop and the web build send their own, and the phone takes the computer's from each chat read (`apply_from_read`; gxserver's `quietView`). A computer that sends none (older than this) leaves the settings unset, so the core keeps Quiet View off and publishes no `quietView`, and the phone keeps its full view there.
//!
//! CDXC:SessionChat 2026-11-11 SEE-ALSO:
//! apps/desktop/src/app/native_chat/quiet_view.rs (desktop and web draw the lines), apps/desktop/src/app/gx_chat/boot.rs (the settings), packages/settings-catalog/src/general/chat.rs (the rows), server/src/session_chat_history.rs (`deferredWork.toolCallCount`, the counts of a turn still on the server), `extras/transcript_search.rs` (Find skips a closed line's rows). apps/mobile/app/src/chat/native/transcript/quietView.tsx (the phone draws the lines), apps/mobile/app/src/chat/native/composer/menus.ts (its View menu row), server/src/session_chat_read.rs (the settings ride every read).

//! CDXC:SessionChat 2026-11-13 DECISION:
//! The user answered the chat view modes page: Q2771 "default on", Quiet View stays its own setting but starts on for every chat (`chatQuietViewChats` on, for new users and anyone who never saved it; Normal, the full view, stays for those who turn it off); Q2776 "on", Expand the last turn in Quiet View starts on (`chatQuietViewExpandLast`). And of an opened line: "if we expand then don't say the numbers for reports, tool calls etc on the expanded header of the quiet view row, since it's visible anyway so it's redundant." So an open line's header says `openText` ("Working", "Worked for 2m 5s", "Previous turn", "Reports"), and a shut line keeps its counts in `text`; the desktop, the web build and the phone draw the same.

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
    /// Every other chat starts in it too (`chatQuietViewChats`, on since 2026-11-13).
    #[serde(default = "on")]
    pub chats: bool,
    /// The newest stretch shows open (`chatQuietViewExpandLast`, on since 2026-11-13).
    #[serde(default = "on")]
    pub expand_last: bool,
}

fn on() -> bool {
    true
}

impl Default for QuietViewSettings {
    fn default() -> Self {
        Self {
            coordinators: true,
            chats: true,
            expand_last: true,
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
    /// Shared with the lines a core keeps between documents (`document_kept`), so a document that
    /// changed no line copies none.
    #[serde(default, skip_serializing_if = "shared_is_empty")]
    pub stretches: std::sync::Arc<Vec<QuietStretch>>,
    /// The cards of the user's answers, each drawn shut as its own small line.
    #[serde(default, skip_serializing_if = "shared_is_empty")]
    pub answers: std::sync::Arc<Vec<QuietAnswers>>,
}

fn shared_is_empty<T>(list: &std::sync::Arc<Vec<T>>) -> bool {
    list.is_empty()
}

/// One answers card under Quiet View: a page's or a form's answers row (`id` is the row's), or a
/// finished turn's answered questions (`id` is `<turn id>#questions`), drawn shut as one line of
/// its own, "You answered “Sidebar picks”" and a short count, that opens onto the card.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User, of the full "You answered …" card listing every answer to a page under a quiet line: "let's hide this in quiet mode also fam (under a small section by itself basically, can expand it to see this)". In Quiet View an answers card (to a Ghostex page, a form, or a question or ask) is drawn shut as one small line of its own, never merged into the line of machinery since it is something he said: the answered mark, "You answered “<title>”" and a short count ("7 answers", "7 skipped" when all were skipped, or the one answer when there is only one and it is short); a click opens the card in place and another shuts it. With Expand the last turn on, the newest one shows open. His typed messages stay fully shown. Supersedes, for answers cards only, the 2026-11-11 pick that his answers are never hidden. The full view is unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietAnswers {
    pub id: String,
    /// The line's disclosure key, `quiet-answers:<id>`.
    pub key: String,
    /// "You answered “Sidebar picks”".
    pub text: String,
    /// "7 answers", "7 skipped" or the one short answer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub count: String,
    /// Starts open (`chatQuietViewExpandLast`, the newest answers after the last reply).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
}

/// The longest single answer the shut line says in full.
const SHORT_ANSWER: usize = 40;

fn clip(text: &str, most: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= most {
        text
    } else {
        let cut: String = text.chars().take(most.saturating_sub(1)).collect();
        format!("{}\u{2026}", cut.trim_end())
    }
}

/// The short count after the title: the one short answer, "N skipped" when every one was, else
/// "N answers".
fn answers_count(values: &[String], total: usize) -> String {
    if let Some(only) = values
        .first()
        .filter(|only| total == 1 && !only.is_empty() && only.chars().count() <= SHORT_ANSWER)
    {
        return only.clone();
    }
    if total > 0
        && values.len() == total
        && values
            .iter()
            .all(|value| value == "Skipped" || value == "Not answered")
    {
        return format!("{total} skipped");
    }
    plural(total, "answer", "answers")
}

/// A page's or a form's answers row, unless the user wrote words of his own with it.
fn page_answers(row: &Value) -> Option<QuietAnswers> {
    let card = &row["pageAnswers"];
    if !card.is_object() || card["withText"] == true || row["role"] != "user" {
        return None;
    }
    let id = row["id"].as_str()?.to_string();
    let rows = card["rows"].as_array().map_or(&[][..], Vec::as_slice);
    let values: Vec<String> = rows
        .iter()
        .map(|row| row["value"].as_str().unwrap_or_default().to_string())
        .collect();
    let total = rows.len() + card["more"].as_u64().unwrap_or(0) as usize;
    Some(QuietAnswers {
        key: format!("quiet-answers:{id}"),
        text: format!(
            "You answered \u{201c}{}\u{201d}",
            clip(card["title"].as_str().unwrap_or("the page"), 60)
        ),
        count: answers_count(&values, total),
        id,
        open: false,
    })
}

/// A finished turn's answered questions (`completed-work` `questions`).
fn turn_answers(turn: &str, questions: &[Value]) -> Option<QuietAnswers> {
    let first = questions.first()?;
    let asked: Vec<&Value> = questions
        .iter()
        .flat_map(|exchange| exchange["questions"].as_array().into_iter().flatten())
        .collect();
    let values: Vec<String> = questions
        .iter()
        .flat_map(|exchange| exchange["answers"].as_array().into_iter().flatten())
        .map(|answer| match answer {
            Value::String(text) => text.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            Value::Null => "Skipped".to_string(),
            other => other["text"]
                .as_str()
                .or_else(|| other["label"].as_str())
                .unwrap_or_default()
                .to_string(),
        })
        .collect();
    let title = first["questions"][0]["question"]
        .as_str()
        .or_else(|| first["questions"][0]["header"].as_str())
        .unwrap_or("the question");
    let id = format!("{turn}#questions");
    Some(QuietAnswers {
        key: format!("quiet-answers:{id}"),
        text: format!("You answered \u{201c}{}\u{201d}", clip(title, 60)),
        count: answers_count(&values, asked.len().max(values.len())),
        id,
        open: false,
    })
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
    /// What the line says while it is open, without the counts its rows now show: "Working",
    /// "Worked for 2m 5s", "Previous turn", "Reports" or "Worked".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub open_text: String,
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
    /// How the finished turns the line holds draw once it is open ([`QuietTurnLook`]); empty when
    /// they draw as in the full view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<QuietTurnLook>,
    /// The reads opening the line sends (`{type: 'loadWork', id, work}` each): the rows still on
    /// the server of the turns it draws straight on its rail (`QuietTurnLook::flat`), which have
    /// no card or line of their own to open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<QuietLoad>,
}

/// One read of a turn's rows still on the server: `id` is the owner a finished turn's card reads
/// it by (the turn's id) or, for a Reduced line's turn, its section's prompt (`startId`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuietLoad {
    pub id: String,
    pub work: Value,
}

/// How one finished turn's row (`completed-work`, or a Reduced `grouped-work` line) draws on an
/// opened line's rail.
///
/// CDXC:SessionChat 2026-11-14 DECISION:
/// The user, of an opened Quiet View line "Worked for 48s" whose only row was the turn's own "Worked for 48s · 4 tool calls" card: "I don't like seeing worked for 48 twice". From the options page (docs/2026-11-14/worked-for-twice/page.html) he picked Q16 C, "the card inside drops its time": when the line names the turn's time (one finished turn, no reports), the turn's card inside is titled by its calls ("4 tool calls", `title`) and that count leaves its pills; a Reduced line inside says "4 tool calls · …" the same way. And Q17 B, "all calls on the rail, a small mark between turns": when a line holds two or more finished turns, no turn draws its card or Reduced line; their calls sit straight on the line's rail, with a small "Then · 22s" mark (`marks`) where the agent picked up again. The full view is unchanged. The desktop, the web build and the phone draw the same.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietTurnLook {
    /// The row, an index into the transcript list.
    pub row: usize,
    /// The card's (or Reduced line's) title in place of "Worked for 48s": "4 tool calls".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The `kind` of the stat `title` names, which the pills then leave out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title_kind: String,
    /// The row draws its calls straight on the rail, with no card or line of its own; opening the
    /// Quiet View line reads back its rows still on the server.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flat: bool,
    /// The marks before each turn that is not the line's first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<QuietTurnMark>,
}

/// "Then · 22s" before the turn whose rows start at `at` in the row's `work` (0 for a
/// `completed-work` row).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuietTurnMark {
    pub at: usize,
    pub text: String,
}

/// A finished turn row the lines may hold: its index, the title its calls give it, and each turn
/// it holds as (where its rows start in `work`, its own label).
struct TurnRow {
    row: usize,
    title: Option<(String, String)>,
    turns: Vec<(usize, String)>,
    /// Its rows still on the server, as opening its card or line would read them.
    loads: Vec<QuietLoad>,
}

impl TurnRow {
    fn of(row: usize, item: &TranscriptItem) -> Option<Self> {
        let section = |deferred: &Option<Value>| deferred.clone().filter(Value::is_object);
        let (stats, turns, loads) = match item {
            TranscriptItem::CompletedWork {
                id,
                label,
                stats,
                deferred,
                ..
            } => (
                stats,
                vec![(0, label.clone())],
                section(deferred)
                    .map(|work| QuietLoad {
                        id: id.clone(),
                        work,
                    })
                    .into_iter()
                    .collect(),
            ),
            TranscriptItem::GroupedWork { stats, turns, .. } => (
                stats,
                turns
                    .iter()
                    .map(|turn| (turn.work_start, turn.label.clone()))
                    .collect(),
                turns
                    .iter()
                    .filter_map(|turn| {
                        let work = section(&turn.deferred)?;
                        let id = work["startId"].as_str().unwrap_or(&turn.id).to_string();
                        Some(QuietLoad { id, work })
                    })
                    .collect(),
            ),
            _ => return None,
        };
        let kind = |stat: &Value| stat["kind"].as_str().unwrap_or_default().to_string();
        // The tool calls name the turn; failing those, the first count that is not the files
        // (a Reduced line says its files after the counts, with their line counts).
        let title = stats
            .iter()
            .find(|stat| stat["kind"] == "tools")
            .or_else(|| stats.iter().find(|stat| stat["kind"] != "files"))
            .and_then(|stat| Some((stat["label"].as_str()?.to_string(), kind(stat))));
        Some(Self {
            row,
            title,
            turns,
            loads,
        })
    }
}

/// The looks of the finished turns `line` holds (see [`QuietTurnLook`]) and the reads opening it
/// sends. `names_time`: the line's open header is the turn's own "Worked for 48s" or "Previous
/// turn".
fn turn_looks(line: &mut QuietStretch, rows: &[TurnRow], names_time: bool) {
    let from = rows.partition_point(|turn| turn.row < line.start);
    let to = rows.partition_point(|turn| turn.row < line.end);
    let held = &rows[from..to];
    let count: usize = held.iter().map(|turn| turn.turns.len()).sum();
    if count >= 2 {
        // One read per section: several wake-up turns can share their prompt's section.
        for load in held.iter().flat_map(|turn| &turn.loads) {
            if !line.loads.iter().any(|seen| seen.work == load.work) {
                line.loads.push(load.clone());
            }
        }
        let mut first = true;
        line.turns = held
            .iter()
            .map(|turn| QuietTurnLook {
                row: turn.row,
                flat: true,
                marks: turn
                    .turns
                    .iter()
                    .filter(|_| !std::mem::take(&mut first))
                    .map(|(at, label)| QuietTurnMark {
                        at: *at,
                        text: match crate::transcript::grouped_work::short_time(label) {
                            Some(time) => format!("Then \u{b7} {time}"),
                            None => "Then".to_string(),
                        },
                    })
                    .collect(),
                ..QuietTurnLook::default()
            })
            .collect();
        return;
    }
    line.turns = match held {
        [turn] if names_time => turn
            .title
            .as_ref()
            .map(|(title, kind)| QuietTurnLook {
                row: turn.row,
                title: title.clone(),
                title_kind: kind.clone(),
                ..QuietTurnLook::default()
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
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

/// The host's own settings (the desktop and the web build), which a read's never replace.
pub fn apply_from_host(state: &mut ChatState, settings: QuietViewSettings) {
    state.transcript_view.quiet_view_from_host = true;
    apply_settings(state, settings);
}

/// The computer's settings as a chat read carries them, for a host that sends none of its own
/// (the phone).
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User: "i dont see queit for coordinator chat applied by default like it is on dekstop". The phone draws Quiet View (apps/mobile/app/src/chat/native/transcript/quietView.tsx) and follows the computer's three Quiet View settings, which every chat read carries (`quietView`), so a coordinator's chat opens in Quiet View on the phone as on the desktop. A computer that sends no settings keeps the phone in the full view, as before.
pub fn apply_from_read(
    state: &mut ChatState,
    settings: &ghostex_gx_protocol::ChatQuietViewSettings,
) {
    if state.transcript_view.quiet_view_from_host {
        return;
    }
    let defaults = QuietViewSettings::default();
    let next = QuietViewSettings {
        coordinators: settings.coordinators.unwrap_or(defaults.coordinators),
        chats: settings.chats.unwrap_or(defaults.chats),
        expand_last: settings.expand_last.unwrap_or(defaults.expand_last),
    };
    if state.transcript_view.quiet_view_settings != Some(next) {
        apply_settings(state, next);
        state.core.request_publish();
    }
}

/// Adopts the settings.
pub fn apply_settings(state: &mut ChatState, settings: QuietViewSettings) {
    let expand_moved = expand_last(state) != settings.expand_last;
    state.transcript_view.quiet_view_settings = Some(settings);
    // The Summary view's open "Active work" follows Expand the last turn (`summary_work_lines`).
    if expand_moved && state.transcript_view.summary_mode {
        state.transcript_view.invalidate();
    }
    sync(state);
}

/// Settings > Chat > Expand the last turn in Quiet View.
pub(crate) fn expand_last(state: &ChatState) -> bool {
    state
        .transcript_view
        .quiet_view_settings
        .is_some_and(|settings| settings.expand_last)
}

/// [`coordinator`], for the Summary view's quiet lines.
pub(crate) fn is_coordinator_chat(state: &ChatState) -> bool {
    coordinator(state)
}

/// Whether this chat is a coordinator's.
fn coordinator(state: &ChatState) -> bool {
    crate::extras::coordinator_threads::is_coordinator(state.session.coordinator_threads.as_deref())
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

/// Whether the chat is in Detailed view, which is Quiet View with every line open.
///
/// CDXC:SessionChat 2026-11-13 DECISION:
/// The user answered the chat view modes page, Q2772 "rebuild": Detailed becomes Quiet View with every line open, so switching views only opens or shuts lines. Wherever Quiet View is available (the host sent its settings), a chat in Detailed draws the quiet lines whatever its own Quiet View choice, each starting open (`Inputs::all_open`), over the rows Detailed already projects; a line the reader shuts stays shut. Normal is the chat's Quiet View choice (on by default since Q2771), Summary is unchanged.
fn detailed(state: &ChatState) -> bool {
    crate::transcript::view_level::view_level(&state.transcript_view)
        == crate::transcript::view_level::DETAILED
}

/// Sets `quiet_view_on` from the settings, the chat's choice and its view, throwing the cached
/// rows away when that changes how Agent work Quiet projects.
pub fn sync(state: &mut ChatState) {
    let chosen = chosen(state);
    let on = !state.transcript_view.summary_mode
        && (chosen == Some(true) || (chosen.is_some() && detailed(state)));
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
    document_kept(state, context, work_cards, approvals, &mut None)
}

/// The lines as last built, with everything they were built from: the rows (the projection's own
/// list, which is a new one whenever a row changed), what [`Inputs`] holds, and the day and
/// timezone their times were written in.
#[derive(Clone, Debug)]
pub struct KeptQuiet {
    items: std::sync::Arc<crate::transcript::items::TranscriptItems>,
    inputs: Inputs,
    utc_offset_minutes: i32,
    local_day: i64,
    lines: (
        std::sync::Arc<Vec<QuietStretch>>,
        std::sync::Arc<Vec<QuietAnswers>>,
    ),
}

/// [`document`], reusing the lines `kept` holds while the rows and everything else they read are
/// the ones they were built from.
///
/// CDXC:SessionChat 2026-11-11 WHY:
/// User: "why is the chat engine busy, it used to be so fast". Every document walked every row of
/// the transcript again for these lines, and a core builds two documents an event: a sample of
/// his app showed a fifth of the chat engine's thread here on a long coordinator chat, for events
/// that changed no row (a keystroke, a session's activity). The lines read the rows, a few flags
/// and the day, so they are built again only when one of those moved.
pub fn document_kept(
    state: &ChatState,
    context: &ChatContext,
    work_cards: &Value,
    approvals: &Value,
    kept: &mut Option<KeptQuiet>,
) -> Option<QuietViewDocument> {
    let default_on = default_on(state)?;
    let on = chosen(state)?;
    let (stretches, answers) = if state.transcript_view.quiet_view_on {
        let items = crate::transcript::rows::rows_shared(state, context);
        let inputs = Inputs {
            coordinator: coordinator(state),
            working: crate::transcript::foreign::is_working(state),
            expand_last: state
                .transcript_view
                .quiet_view_settings
                .is_some_and(|settings| settings.expand_last),
            all_open: detailed(state),
            approvals: open_approval_anchors(approvals),
            cards: turn_lines(work_cards),
        };
        let local_day = crate::transcript::message_time::civil(
            context.now_millis(),
            context.utc_offset_minutes,
        )
        .local_day;
        match kept.as_ref().filter(|kept| {
            std::sync::Arc::ptr_eq(&kept.items, &items)
                && kept.inputs == inputs
                && kept.utc_offset_minutes == context.utc_offset_minutes
                && kept.local_day == local_day
        }) {
            Some(kept) => kept.lines.clone(),
            None => {
                let (stretches, answers) = stretches(items.iter(), &inputs, context);
                let lines = (std::sync::Arc::new(stretches), std::sync::Arc::new(answers));
                *kept = Some(KeptQuiet {
                    items,
                    inputs,
                    utc_offset_minutes: context.utc_offset_minutes,
                    local_day,
                    lines: lines.clone(),
                });
                lines
            }
        }
    } else {
        *kept = None;
        Default::default()
    };
    Some(QuietViewDocument {
        on,
        default_on,
        stretches,
        answers,
    })
}

/// What the lines are built from besides the rows.
#[derive(Clone, Debug, PartialEq)]
struct Inputs {
    coordinator: bool,
    working: bool,
    expand_last: bool,
    /// Detailed view: every line and answers card starts open ([`detailed`]).
    all_open: bool,
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

/// A message from another session: a thread's report, a subagent's, any agent's (an agent message
/// card, `systemCard` `agent-message`, included).
fn is_report(row: &Value) -> bool {
    (row["role"] == "user"
        && (row["interAgentMessage"].is_object() || row["agentMessage"].is_object()))
        || row["systemCard"]["kind"] == "agent-message"
}

/// Whether a row's words hold a form, one-click answers or a page: what asks something of the
/// user, unlike a picture.
fn asks_or_pages(row: &Value) -> bool {
    use crate::transcript::inline_answers::ANSWERS_OPEN;
    use crate::transcript::native_markdown::{NATIVE_QUESTIONS_OPEN, NATIVE_VISUAL_OPEN};
    row["text"].as_str().is_some_and(|text| {
        [NATIVE_QUESTIONS_OPEN, ANSWERS_OPEN, NATIVE_VISUAL_OPEN]
            .iter()
            .any(|mark| text.contains(mark))
    })
}

/// Whether a row is something the user reads, which never folds: his own messages and answers, the
/// agent's words, pages, forms and questions, pictures, a call waiting on his approval or a relayed
/// approval under it, errors and notices, and a message that was not sent. A kind this does not
/// know stays in view.
///
/// CDXC:SessionChat 2026-11-11 WHY: a message from another agent folds whatever else it holds, whether it starts a turn or lands in one: a thread's report that linked a picture (the chat draws a link to a local picture as one) was kept in view as if it were the user's, under the line that already counted it. It stays in view only for what waits on the user: a relayed approval under it, a call waiting on an approval, a question, a form or a page.
fn reads(row: &Value, inputs: &Inputs) -> bool {
    let waits = waits_on_approval(row)
        || row["id"]
            .as_str()
            .is_some_and(|id| inputs.approvals.contains(id))
        || has(row, "questions")
        || asks_or_pages(row);
    if is_report(row) {
        return waits;
    }
    if waits
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
        Some("user") => true,
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
///
/// CDXC:SessionChat 2026-11-12 WHY: a call that sent another agent a message (`coordinator start-thread`, `agents send`, `SendMessage`) is drawn as its "Message to …" card, so the line counts it once, as a message sent, and not also as a tool call: a turn that only started a thread read "1 message sent · 1 tool call". The full view's "Worked for" fold still lists the call's row, so its own count keeps it.
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
        self.tools += list(row, "tools")
            .iter()
            .filter(|tool| tool["sentMessage"] != true)
            .count();
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
            self.tools += tally.tool_calls - tally.sent_calls;
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
) -> (Vec<QuietStretch>, Vec<QuietAnswers>) {
    let mut built: Vec<(QuietStretch, Counts)> = Vec::new();
    // The answers cards, with the row each is drawn in, and the newest row with a reply.
    let mut answers: Vec<(QuietAnswers, usize)> = Vec::new();
    let mut last_reply: Option<usize> = None;
    let mut open: Option<Open> = None;
    // The row each reply id is drawn in.
    let mut replies: Vec<(String, usize)> = Vec::new();
    let mut length = 0usize;
    let mut turn_rows: Vec<TurnRow> = Vec::new();
    for (at, item) in items.enumerate() {
        length = at + 1;
        turn_rows.extend(TurnRow::of(at, item));
        for id in reply_ids(item) {
            replies.push((id, at));
        }
        answers.extend(answers_in(item).into_iter().map(|card| (card, at)));
        if replies_in(item) {
            last_reply = Some(at);
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
    let lines: Vec<QuietStretch> = built
        .into_iter()
        .enumerate()
        .map(|(at, (mut line, counts))| {
            line.open = inputs.all_open || (inputs.expand_last && Some(at) == newest);
            let names_time = !line.running && counts.reports == 0 && counts.worked.is_some();
            turn_looks(&mut line, &turn_rows, names_time);
            words(&mut line, counts, inputs, context);
            line
        })
        .collect();
    // Expand the last turn shows the newest answers card open, when no reply came after it.
    let newest_answers = answers
        .iter()
        .rposition(|(_, row)| last_reply.is_none_or(|reply| *row >= reply));
    let answers = answers
        .into_iter()
        .enumerate()
        .map(|(at, (mut card, _))| {
            card.open = inputs.all_open || (inputs.expand_last && Some(at) == newest_answers);
            card
        })
        .collect();
    (lines, answers)
}

/// The quiet lines over a Summary view turn's open "Active work" (`TranscriptItem::Summary`
/// `workLines`), by row index into its `work`: what Quiet View draws for the same rows.
///
/// CDXC:SessionChat 2026-11-12 DECISION: User, on the Summary view page (Q2767), of what shows while the agent works: Active work open, and "show what we show in quiet mode in active work". So the open "Active work" draws the running rows as Quiet View does: what the user reads (the agent's words with their calls folded behind them, pages, forms, questions, pictures, approvals, errors) stays in view, and each run of the rest (tool calls, thinking, reports from threads, messages sent) is one counted line, the newest one "Working · 3 tool calls so far" while the agent works. Expand the last turn in Quiet View opens the newest line, as it does there. Under Agent work Reduced and Quiet the running work keeps its own look (`presentation::turn_items`).
pub(crate) fn summary_work_lines(
    work: &[crate::document::ProjectedMessage],
    working: bool,
    expand_last: bool,
    coordinator: bool,
) -> Vec<QuietStretch> {
    let inputs = Inputs {
        coordinator,
        working,
        expand_last,
        all_open: false,
        approvals: BTreeSet::new(),
        cards: Vec::new(),
    };
    let mut built: Vec<(QuietStretch, Counts)> = Vec::new();
    let mut open: Option<Open> = None;
    for (at, row) in work.iter().enumerate() {
        if reads(row, &inputs) {
            if let Some(Open { start, key, counts }) = open.take() {
                built.push((stretch(key, start, at, None), counts));
            }
            continue;
        }
        let line = open.get_or_insert_with(|| Open {
            start: at,
            key: format!("quiet:{}", row["id"].as_str().unwrap_or_default()),
            counts: Counts::default(),
        });
        line.counts.row(row);
    }
    let trailing = open.take().map(|Open { start, key, counts }| {
        let mut line = stretch(key, start, work.len(), None);
        line.running = working;
        line.open = expand_last;
        (line, counts)
    });
    built.extend(trailing);
    built
        .into_iter()
        .map(|(mut line, counts)| {
            count_words(&mut line, counts, &inputs);
            line
        })
        .collect()
}

/// The answers cards a row draws (`QuietAnswers`).
fn answers_in(item: &TranscriptItem) -> Vec<QuietAnswers> {
    match item {
        TranscriptItem::Message { message } => page_answers(message).into_iter().collect(),
        TranscriptItem::CompletedWork {
            id,
            artifacts,
            questions,
            ..
        } => artifacts
            .iter()
            .filter_map(|row| page_answers(row))
            .chain(turn_answers(id, questions))
            .collect(),
        TranscriptItem::GroupedWork { turns, .. } => turns
            .iter()
            .flat_map(|turn| {
                turn.artifacts
                    .iter()
                    .filter_map(|row| page_answers(row))
                    .chain(turn_answers(&turn.id, &turn.questions))
                    .collect::<Vec<_>>()
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a row draws a reply of the agent's.
fn replies_in(item: &TranscriptItem) -> bool {
    let words = |row: &crate::document::ProjectedMessage| {
        row["role"] == "assistant"
            && row["text"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
    };
    match item {
        TranscriptItem::Message { message } => words(message),
        TranscriptItem::CompletedWork { final_message, .. } => final_message.iter().any(words),
        TranscriptItem::GroupedWork { turns, .. } => turns
            .iter()
            .any(|turn| turn.final_message.iter().any(words)),
        _ => false,
    }
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
    let (first_at, last_at, reports) = (counts.first_at, counts.last_at, counts.reports);
    count_words(line, counts, inputs);
    if reports > 0 && !line.running {
        line.time = time_range(first_at, last_at, context);
    }
}

/// The line's words: what it holds, in counts.
fn count_words(line: &mut QuietStretch, counts: Counts, inputs: &Inputs) {
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
    line.open_text = if line.running {
        "Working".to_string()
    } else if counts.reports == 0 && counts.worked.is_some() {
        if counts.previous_turn {
            "Previous turn".to_string()
        } else {
            let (started, ended) = counts.worked.unwrap_or_default();
            crate::transcript::turns::worked_duration_label(started, ended)
        }
    } else if counts.reports > 0 {
        if inputs.coordinator {
            "Reports".to_string()
        } else {
            "Messages received".to_string()
        }
    } else {
        "Worked".to_string()
    };
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
        all_open: detailed(state),
        approvals: open_approval_anchors(&approvals),
        cards: Vec::new(),
    };
    stretches(items.iter(), &inputs, context)
        .0
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
