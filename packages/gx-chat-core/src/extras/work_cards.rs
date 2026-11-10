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

/// The chips under an item card: the threads on it, its steps, what it asks of the user, its
/// links. Never "You" or "Coordinator": the card's status already says whose turn it is.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User (Q2167), of the first cards: "i don't like the tags u have shown at the bottom pls remove
/// the border and make them look better also i don't know wtf is You there, just remove it". The
/// chips are an icon and muted words with no outline or fill (the renderers), and an owner shows
/// only where it says something the card does not: which thread has the work.
pub fn item_chips(item: &Value) -> Vec<Value> {
    let mut chips = Vec::new();
    for thread in item["threads"].as_array().into_iter().flatten() {
        let title = text(thread, "title");
        if title.is_empty() {
            continue;
        }
        chips.push(json!({
            "kind": "thread",
            "label": title,
            "command": {
                "type": "openCoordinatorThread",
                "serverId": text(thread, "serverId"),
                "projectId": text(thread, "projectId"),
                "sessionId": text(thread, "sessionId"),
            },
        }));
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
    if open > 0 && !matches!(text(item, "status"), "needs" | "blocked") {
        chips.push(json!({ "kind": "question", "label": format!("{open} for you") }));
    }
    for link in item["links"].as_array().into_iter().flatten() {
        let url = text(link, "url");
        let label = match text(link, "kind") {
            "pr" => format!("PR {}", text(link, "label")),
            "issue" => format!("Issue {}", text(link, "label")),
            _ => text(link, "label").to_string(),
        };
        let mut chip = json!({ "kind": if text(link, "kind") == "link" { "file" } else { "link" }, "label": label });
        if !url.is_empty() {
            chip["command"] = json!({ "type": "openMarkdownLink", "href": url });
        }
        chips.push(chip);
    }
    chips
}

/// A file or link an ask's steps are in, as a chip that opens it.
fn steps_chip(steps: &str) -> Value {
    let name = steps
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(steps);
    json!({
        "kind": "file",
        "label": name,
        "command": { "type": "openMarkdownLink", "href": steps },
    })
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
    // Each card with where it goes: its turn, then needs-you first (`priority`), then its moment.
    let mut cards: Vec<(i64, u8, i64, Value)> = Vec::new();
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
        // The newest turn that drew the item's own card (a turn that only asked draws an ask card).
        let newest = by_turn
            .iter()
            .filter(|(_, events)| events.iter().any(|event| text(event, "kind") != "ask"))
            .map(|(turn, _)| *turn)
            .next_back();
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
                let line = if ask["holds"] == true {
                    format!("{} waits on this", text(item, "title"))
                } else {
                    "Nothing waits on it".to_string()
                };
                let mut ask_chips = vec![json!({ "kind": "item", "label": text(item, "title") })];
                if !text(ask, "steps").is_empty() {
                    ask_chips.push(steps_chip(text(ask, "steps")));
                }
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
                    turn.start_ms,
                    if ask["open"] == true { 0 } else { 5 },
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
                        "chips": ask_chips,
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
            // A finished item with nothing more to read is its one line, so a turn that changed a
            // dozen things stays short (Q2166 A).
            let folded = !live || (status == "finished" && line.chars().count() <= 120);
            let priority = if folded {
                5
            } else {
                match status {
                    "needs" | "blocked" => 0,
                    "moving" => 1,
                    "waiting" => 2,
                    "later" => 3,
                    _ => 4,
                }
            };
            let fold_text = [text(item, "title"), words.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" \u{b7} ");
            cards.push((
                turn.start_ms,
                priority,
                first_at,
                json!({
                    "key": format!("work:{reference}:{}", turn.id),
                    "kind": "item",
                    "anchorId": anchor,
                    "turnId": turn.id,
                    "folded": folded,
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
    // Everything still running, under the newest reply: one quiet row (Q2166 A).
    if let Some(row) = moving_row(items, &turns, &cards, now_ms) {
        cards.push(row);
    }
    cards.sort_by_key(|(turn, priority, at, _)| (*turn, *priority, *at));
    open_asks.sort_by_key(|ask| ask.at);
    (
        Value::Array(cards.into_iter().map(|(_, _, _, card)| card).collect()),
        open_asks,
    )
}

/// The "Moving · 7" row under the newest reply: every line of work still running (Moving or
/// Waiting) that has no card of its own there, which opens to a list; a row of the list goes to
/// the item's newest card.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User (Q2166 "A. A card per item that changed, news on top"): the reply is a few sentences of
/// news, then a card per line of work the turn touched, needs-you first; a settled ask folds to its
/// line; "everything still running is one 'Moving · 7' row that opens to a list"; finished items
/// with nothing to read draw as the one-line fold, so a turn with twelve changes stays short.
fn moving_row(
    items: &[Value],
    turns: &[Turn],
    cards: &[(i64, u8, i64, Value)],
    now_ms: i64,
) -> Option<(i64, u8, i64, Value)> {
    let turn = turns.iter().rev().find(|turn| turn.anchor.is_some())?;
    let anchor = turn.anchor.clone()?;
    let shown: Vec<&str> = cards
        .iter()
        .filter(|(_, _, _, card)| card["anchorId"] == anchor.as_str() && card["folded"] != true)
        .map(|(_, _, _, card)| text(card, "ref"))
        .collect();
    let mut rows: Vec<(i64, Value)> = Vec::new();
    for item in items.iter().filter(|item| item["kind"] == "item") {
        let status = text(item, "status");
        let reference = text(item, "ref");
        if !matches!(status, "moving" | "waiting") || shown.contains(&reference) {
            continue;
        }
        let at = at_ms(text(item, "at")).unwrap_or(0);
        // Its newest card, which a click on its row goes to.
        let target = cards
            .iter()
            .filter(|(_, _, _, card)| card["kind"] == "item" && card["ref"] == reference)
            .max_by_key(|(turn, _, _, _)| *turn)
            .map(|(_, _, _, card)| card);
        let mut row = json!({
            "ref": reference,
            "title": text(item, "title"),
            "status": status,
            "line": if status == "waiting" { waits_line(item) } else { text(item, "now").to_string() },
            "age": crate::transcript::message_time::age_label(now_ms, at),
        });
        if let Some(target) = target {
            row["command"] = json!({
                "type": "jumpToWorkCard",
                "anchorId": target["anchorId"],
                "key": target["key"],
            });
        }
        rows.push((at, row));
    }
    if rows.is_empty() {
        return None;
    }
    rows.sort_by(|left, right| right.0.cmp(&left.0));
    let waiting = rows
        .iter()
        .filter(|(_, row)| row["status"] == "waiting")
        .count();
    let titles: Vec<&str> = rows.iter().map(|(_, row)| text(row, "title")).collect();
    Some((
        turn.start_ms,
        4,
        i64::MAX,
        json!({
            "key": format!("work-moving:{anchor}"),
            "kind": "moving",
            "anchorId": anchor,
            "turnId": turn.id,
            "title": format!("Moving \u{b7} {}", rows.len()),
            "summary": titles.join(", "),
            "waiting": if waiting > 0 { format!("{waiting} waiting") } else { String::new() },
            "rows": rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
        }),
    ))
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

/// Every question tag a work item asks (open or settled), uppercased. A form or one-click
/// question in a reply whose tags are all among them is that ask, already drawn as its card.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User, after answering the same question twice (Q2169, Q2171): the coordinator had asked it as
/// one-click links in its reply and again with `item ask --tag`, so the pill under the reply and
/// the ask card below it both waited on him. A question is drawn once, as the ask card: a form or
/// one-click question whose tags all belong to item asks is not drawn and does not count in the
/// ? pill, and answering either settles both (gxserver settles the ask from the chat's answers;
/// the core treats the form as answered once its ask is).
pub fn card_tags(state: &ChatState) -> std::collections::BTreeSet<String> {
    state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| item["asks"].as_array().into_iter().flatten())
        .map(|ask| text(ask, "tag").to_ascii_uppercase())
        .filter(|tag| !tag.is_empty())
        .collect()
}

/// Whether a form or question page asking `tags` is covered by item asks ([`card_tags`]).
pub fn covered_by_cards(tags: &[String], cards: &std::collections::BTreeSet<String>) -> bool {
    !tags.is_empty()
        && tags
            .iter()
            .all(|tag| cards.contains(&tag.to_ascii_uppercase()))
}
