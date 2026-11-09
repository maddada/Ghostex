//! A coordinator's work item cards in its chat: under each reply, one card for each item the
//! coordinator changed in that turn (`ghostex coordinator item …`), and one for each thing it asked
//! of the user. gxserver keeps the items (server/src/coordinators/work.rs) and sends them inside
//! `coordinatorThreads` (`work`), each with its recent history; this ties every change to the turn
//! it happened in, by time, and words each card once for every renderer.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, on the work items design page (Q2092 "B. Work items, with live cards under the reply"):
//! the reply is a few sentences of news, and under it a card per item that changed in that turn:
//! its status, where it stands, who has the ball, what it waits on and how long ago; an ask is a
//! card of its own with its answer buttons (or Mark done). Unchanged items get no card. Q2095
//! "fold": when the same item changes again in a later turn, its earlier card folds to one quiet
//! line saying what that turn did; only the newest card shows the item as it is now, and nothing
//! jumps back to an earlier message. Q2094 "hidden": chores never make a card. An ask's card stays
//! open until it is answered, done or withdrawn, then folds to one line too. Open asks count in
//! the chat's ? pill, which steps to their cards like any question.
//! SEE-ALSO: apps/desktop/src/app/native_chat/work_cards.rs (desktop and web),
//! apps/mobile/app/src/chat/native/cards/WorkCards.tsx (the phone).

use std::collections::BTreeMap;

use ghostex_gx_protocol::ChatRole;
use serde_json::{json, Value};

use crate::state::{ChatContext, ChatState};

/// The changes that are the coordinator's own and so make a card in its turn. The user's answer
/// on a card (`answer`) and the move from the board (`moved`) do not.
const TURN_KINDS: [&str; 12] = [
    "add", "now", "step", "ask", "wait", "done", "drop", "restore", "status", "edit", "thread",
    "settle",
];

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn at_ms(at: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(at).map(|at| at as i64)
}

/// One turn of the chat: from a message of the user's (or a thread's report) to the next.
struct Turn {
    start_ms: i64,
    /// The message starting it.
    id: String,
    /// The last reply of the agent's in it that has words, which its cards sit under.
    anchor: Option<String>,
}

fn turns(state: &ChatState) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for message in &state.messages.composed {
        if message.role == ChatRole::User {
            let start_ms = message
                .timestamp
                .or_else(|| turns.last().map(|turn| turn.start_ms))
                .unwrap_or(i64::MIN);
            turns.push(Turn {
                start_ms,
                id: message.id.clone(),
                anchor: None,
            });
        } else if message.role == ChatRole::Assistant {
            let Some(turn) = turns.last_mut() else {
                continue;
            };
            if !crate::transcript::prose::prose_markdown(&message.blocks)
                .trim()
                .is_empty()
            {
                turn.anchor = Some(message.id.clone());
            }
        }
    }
    turns
}

/// The turn a change at `at` happened in: the last one that started at or before it.
fn turn_of(turns: &[Turn], at: i64) -> Option<usize> {
    let after = turns.partition_point(|turn| turn.start_ms <= at);
    after.checked_sub(1)
}

/// The status as the cards word it, and its tone (the renderers' icon and colour).
pub fn status_label(status: &str) -> &'static str {
    match status {
        "needs" => "Needs you",
        "blocked" => "Blocked",
        "moving" => "Moving",
        "waiting" => "Waiting",
        "later" => "Later",
        "finished" => "Finished",
        "dropped" => "Dropped",
        _ => "",
    }
}

/// Who has the ball, in words: "You", "Coordinator", a thread's title, or what it waits on.
pub fn owner_words(item: &Value) -> String {
    let owner = &item["owner"];
    let label = text(owner, "label");
    match text(owner, "kind") {
        "thread" if !label.is_empty() => format!("Thread \u{b7} {label}"),
        "thread" => "Thread".to_string(),
        "item" | "outside" if !label.is_empty() => format!("Waits on {label}"),
        _ => label.to_string(),
    }
}

/// "Waiting on iPhone voice calls and Phone reconnect", from the item's open waits.
pub fn waits_line(item: &Value) -> String {
    let open: Vec<String> = item["waits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|wait| wait["open"] == true)
        .map(|wait| {
            let until = text(wait, "until");
            if until.is_empty() {
                text(wait, "label").to_string()
            } else {
                format!("{} (by {until})", text(wait, "label"))
            }
        })
        .collect();
    match open.len() {
        0 => String::new(),
        1 => format!("Waiting on {}", open[0]),
        _ => format!(
            "Waiting on {} and {}",
            open[..open.len() - 1].join(", "),
            open[open.len() - 1]
        ),
    }
}

/// The chips under an item card: who has the ball, its steps, what it asks of the user.
pub fn item_chips(item: &Value) -> Vec<Value> {
    let mut chips = Vec::new();
    let owner = owner_words(item);
    // A waiting item's line already says what it waits on.
    let says_wait = matches!(text(&item["owner"], "kind"), "item" | "outside")
        && text(item, "status") == "waiting"
        && !waits_line(item).is_empty();
    if !owner.is_empty() && !says_wait {
        let kind = match text(&item["owner"], "kind") {
            "user" => "you",
            "thread" => "thread",
            "coordinator" => "coordinator",
            _ => "wait",
        };
        chips.push(json!({ "kind": kind, "label": owner }));
    }
    let total = item["steps"]["total"].as_u64().unwrap_or(0);
    if total > 0 {
        chips.push(json!({
            "kind": "steps",
            "label": format!("{} of {total} steps", item["steps"]["done"].as_u64().unwrap_or(0)),
        }));
    }
    let open = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|ask| ask["open"] == true)
        .count();
    if open > 0 && !matches!(text(&item["owner"], "kind"), "user") {
        chips.push(json!({ "kind": "you", "label": format!("{open} for you") }));
    }
    chips
}

/// What a change said, for an earlier card's one line.
fn event_words(event: &Value) -> String {
    let words = text(event, "text");
    match text(event, "kind") {
        "add" => "Started".to_string(),
        "drop" => "Dropped".to_string(),
        "restore" => "Back on the list".to_string(),
        "done" if words.is_empty() => "Finished".to_string(),
        "done" => format!("Finished: {words}"),
        _ => words.to_string(),
    }
}

/// The clock time of an ISO moment, as the transcript words its stamps ("1:48 AM").
fn clock(at: i64, context: &ChatContext) -> String {
    crate::transcript::message_time::message_time(Some(at), context)["label"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// An open ask's buttons: its one-click answers, or Mark done for a task. Each records the answer
/// with gxserver once and then tells the coordinator ("W3 Q2: Now"), as a board card's answer
/// does (`coordinator_board::answer`).
fn ask_buttons(item: &Value, ask: &Value, key: &str, picked: Option<&str>) -> Vec<Value> {
    let reference = text(item, "ref");
    let tag = text(ask, "tag");
    let button = |label: &str, recommended: bool| {
        json!({
            "label": label,
            "recommended": recommended,
            "picked": picked == Some(label),
            "command": {
                "type": "coordinatorBoardAnswer",
                "key": key,
                "item": reference,
                "ask": tag,
                "label": label,
                "text": format!("{reference} {tag}: {label}"),
            },
        })
    };
    let answers = ask["answers"].as_array().cloned().unwrap_or_default();
    if answers.is_empty() {
        return vec![button("Done", false)];
    }
    answers
        .iter()
        .map(|choice| button(text(choice, "label"), choice["recommended"] == true))
        .collect()
}

/// The cards, in transcript order: `[{ key, kind ("item" | "ask"), anchorId, turnId, folded,
/// ref, title, status, statusLabel, line, chips: [{kind, label}], age, at, foldText, time, tag,
/// holds, buttons: [{label, recommended, picked, command}], answered, busy }]`, and the open asks
/// the ? pill steps through: `[{ key, anchorId, at }]`.
pub fn project(state: &ChatState, context: &ChatContext) -> (Value, Vec<OpenAsk>) {
    let Some(items) = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
    else {
        return (json!([]), Vec::new());
    };
    let turns = turns(state);
    if turns.is_empty() {
        return (json!([]), Vec::new());
    }
    let now_ms = context.now_millis();
    let answers = &state.extras.panels.board_answers;
    // Each card with the moment it orders by.
    let mut cards: Vec<(i64, Value)> = Vec::new();
    let mut open_asks = Vec::new();
    for item in items.iter().filter(|item| item["kind"] == "item") {
        let reference = text(item, "ref");
        // The coordinator's changes by turn, oldest first.
        let mut by_turn: BTreeMap<usize, Vec<&Value>> = BTreeMap::new();
        for event in item["history"].as_array().into_iter().flatten() {
            if !TURN_KINDS.contains(&text(event, "kind")) {
                continue;
            }
            let Some(at) = at_ms(text(event, "at")) else {
                continue;
            };
            if let Some(turn) = turn_of(&turns, at) {
                by_turn.entry(turn).or_default().push(event);
            }
        }
        let newest = by_turn.keys().next_back().copied();
        for (turn_index, events) in &by_turn {
            let turn = &turns[*turn_index];
            let Some(anchor) = turn.anchor.as_deref() else {
                continue;
            };
            let first_at = at_ms(text(events[0], "at")).unwrap_or(turn.start_ms);
            let last = events[events.len() - 1];
            let last_at = at_ms(text(last, "at")).unwrap_or(first_at);
            // Asks made in this turn: a card each.
            for event in events.iter().filter(|event| text(event, "kind") == "ask") {
                let tag = text(event, "text").split(':').next().unwrap_or_default();
                let Some(ask) = item["asks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|ask| text(ask, "tag") == tag)
                else {
                    continue;
                };
                let key = format!("work-ask:{reference}:{tag}");
                let picked = answers.get(&key).map(String::as_str);
                let open = ask["open"] == true && picked.is_none();
                let answered = if ask["withdrawn"] == true {
                    "Withdrawn".to_string()
                } else {
                    text(ask, "answered")
                        .to_string()
                        .chars()
                        .take(200)
                        .collect()
                };
                let at = at_ms(text(event, "at")).unwrap_or(first_at);
                let line = if !text(ask, "steps").is_empty() {
                    format!("Steps: {}", text(ask, "steps"))
                } else if ask["holds"] == true {
                    format!("{} waits on this", text(item, "title"))
                } else {
                    "Nothing waits on it".to_string()
                };
                let ask_status = if ask["holds"] == true {
                    "blocked"
                } else {
                    "needs"
                };
                let buttons = if open || (picked.is_some() && ask["open"] == true) {
                    ask_buttons(item, ask, &key, picked)
                } else {
                    Vec::new()
                };
                let ask_fold = format!(
                    "{tag} \u{b7} {} \u{b7} {}",
                    text(ask, "text"),
                    if answered.is_empty() {
                        "answered"
                    } else {
                        answered.as_str()
                    }
                );
                if ask["open"] == true {
                    open_asks.push(OpenAsk {
                        key: key.clone(),
                        anchor: anchor.to_string(),
                        at,
                    });
                }
                cards.push((
                    at,
                    json!({
                        "key": key,
                        "kind": "ask",
                        "anchorId": anchor,
                        "turnId": turn.id,
                        "folded": ask["open"] != true,
                        "ref": reference,
                        "title": text(ask, "text"),
                        "status": ask_status,
                        "statusLabel": status_label(ask_status),
                        "tag": tag,
                        "holds": ask["holds"] == true,
                        "line": line,
                        "chips": [
                            { "kind": "you", "label": "You" },
                            { "kind": "item", "label": text(item, "title") },
                        ],
                        "buttons": buttons,
                        "busy": picked.is_some() && ask["open"] == true,
                        // A settled ask's line: a check once answered or done, a cross once
                        // withdrawn.
                        "foldStatus": if ask["withdrawn"] == true { "dropped" } else if ask["open"] == true { ask_status } else { "finished" },
                        "answered": answered,
                        "foldText": ask_fold,
                        "time": clock(at, context),
                    }),
                ));
            }
            // The item's own card, unless the turn only asked.
            if events.iter().all(|event| text(event, "kind") == "ask") {
                continue;
            }
            let live = Some(*turn_index) == newest;
            let words = events
                .iter()
                .rev()
                // An ask has a card of its own.
                .filter(|event| !matches!(text(event, "kind"), "ask" | "thread"))
                .map(|event| event_words(event))
                .find(|words| !words.is_empty())
                .unwrap_or_default();
            let status = text(item, "status");
            let mut line = text(item, "now").to_string();
            if status == "waiting" {
                let waits = waits_line(item);
                if !waits.is_empty() {
                    line = waits;
                }
            }
            let updated = at_ms(text(item, "at")).unwrap_or(last_at);
            let fold_text = [text(item, "title"), words.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" \u{b7} ");
            cards.push((
                first_at,
                json!({
                    "key": format!("work:{reference}:{}", turn.id),
                    "kind": "item",
                    "anchorId": anchor,
                    "turnId": turn.id,
                    "folded": !live,
                    "ref": reference,
                    "title": text(item, "title"),
                    "status": status,
                    "statusLabel": status_label(status),
                    "line": line,
                    "chips": item_chips(item),
                    "age": crate::transcript::message_time::age_label(now_ms, updated),
                    "at": updated,
                    "foldText": fold_text,
                    "foldStatus": text(last, "status"),
                    "time": clock(last_at, context),
                }),
            ));
        }
    }
    cards.sort_by_key(|(at, _)| *at);
    open_asks.sort_by_key(|ask| ask.at);
    (
        Value::Array(cards.into_iter().map(|(_, card)| card).collect()),
        open_asks,
    )
}

/// An open ask the ? pill counts and steps to: its card's key and the reply it sits under.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenAsk {
    pub key: String,
    pub anchor: String,
    pub at: i64,
}

/// The open asks alone, for the ? pill (`questions/open_questions.rs`).
pub fn open_asks(state: &ChatState, context: &ChatContext) -> Vec<OpenAsk> {
    project(state, context).1
}

/// When the first age on a live card changes after `now_ms`: the next whole minute since its item
/// last changed (`age_label` reads only whole minutes).
pub fn ages_change_at(cards: &Value, now_ms: i64) -> Option<i64> {
    cards
        .as_array()?
        .iter()
        .filter(|card| card["folded"] != true)
        .filter_map(|card| card["at"].as_i64())
        .map(|at| at + ((now_ms - at).max(0) / 60_000 + 1) * 60_000)
        .min()
}

/// Everything [`project`] reads besides the clock, kept beside the cards it built
/// (`document::AssembleCache`), so an event that moves nothing here (a streamed token of a reply
/// that already has words) does not walk the transcript again.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkInputs {
    work: Option<Value>,
    messages: usize,
    last_id: String,
    last_has_words: bool,
    answers: BTreeMap<String, String>,
}

impl WorkInputs {
    fn of(state: &ChatState) -> Self {
        let last = state.messages.composed.last();
        Self {
            work: state
                .session
                .coordinator_threads
                .as_ref()
                .and_then(|threads| threads.get("work"))
                .cloned(),
            messages: state.messages.composed.len(),
            last_id: last.map(|message| message.id.clone()).unwrap_or_default(),
            last_has_words: last.is_some_and(|message| {
                message.role == ChatRole::Assistant
                    && !crate::transcript::prose::prose_markdown(&message.blocks)
                        .trim()
                        .is_empty()
            }),
            answers: state.extras.panels.board_answers.clone(),
        }
    }
}

/// Kept cards: their inputs, the clock they were built at, the moment their first age changes,
/// and the cards with the open asks.
pub type KeptWork = (WorkInputs, i64, Option<i64>, (Value, Vec<OpenAsk>));

/// [`project`], reused while its inputs hold and no age on it moved.
pub fn project_kept(
    state: &ChatState,
    context: &ChatContext,
    kept: &mut Option<KeptWork>,
) -> (Value, Vec<OpenAsk>) {
    // Not a coordinator, or a gxserver that sends no work: nothing to build or keep.
    if state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .is_none()
    {
        *kept = None;
        return (json!([]), Vec::new());
    }
    let now_ms = context.now_millis();
    let inputs = WorkInputs::of(state);
    if let Some((was, built_at, changes_at, value)) = kept.as_ref() {
        if *was == inputs && now_ms >= *built_at && changes_at.is_none_or(|at| now_ms < at) {
            return value.clone();
        }
    }
    let value = project(state, context);
    let changes_at = ages_change_at(&value.0, now_ms);
    *kept = Some((inputs, now_ms, changes_at, value.clone()));
    value
}
