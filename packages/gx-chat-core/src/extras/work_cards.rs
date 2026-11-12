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
/// A turn that changed more items than this shows one card for them all.
const TURN_CARDS_MAX: usize = 3;

const TURN_KINDS: [&str; 12] = [
    "add", "now", "step", "ask", "wait", "done", "drop", "restore", "status", "edit", "thread",
    "settle",
];

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub(crate) fn at_ms(at: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(at).map(|at| at as i64)
}

/// One turn of the chat: from a message of the user's (or a thread's report) to the next.
pub struct Turn {
    start_ms: i64,
    /// The message starting it.
    id: String,
    /// The last reply of the agent's in it that has words, which its cards sit under.
    anchor: Option<String>,
}

/// The chat's turns, read once for everything a document builds from them: the cards, the dock
/// and every row of the Work panel.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User: "sending messages is the most important part of this app and it's broken", after messages
/// sent from his coordinator's chat reached the agent 7 to 18 seconds late. Every chat runs on one
/// engine thread, which a sample of his app showed busy 40% of the time, more than half of it
/// here: the Work panel was built again on nearly every event (its kept copy ran out with every
/// minute of every one of 231 items, about three times a second), and each of its rows walked the
/// whole chat for its Show in chat, copying every reply's text to ask whether it had words. One
/// walk now serves a whole document, it copies nothing (`prose::has_prose`), and the kept panel
/// stands until a label on it really changes (`coordinator_board.rs` `ages_change_at`).
/// `tests/coordinator_chat_event_cost.rs` pins both.
pub fn turns(state: &ChatState) -> Vec<Turn> {
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
            if crate::transcript::prose::has_prose(&message.blocks) {
                turn.anchor = Some(message.id.clone());
            }
        }
    }
    turns
}

/// What of `turns` a kept Show in chat depends on: how many there are, the first (older turns
/// loaded above it), and the newest with the reply its cards sit under. A turn before the newest
/// keeps its reply, so these four say whether any row's place in the chat moved.
pub fn turns_mark(turns: &[Turn]) -> (usize, String, String, Option<String>) {
    let id = |turn: Option<&Turn>| turn.map(|turn| turn.id.clone()).unwrap_or_default();
    (
        turns.len(),
        id(turns.first()),
        id(turns.last()),
        turns.last().and_then(|turn| turn.anchor.clone()),
    )
}

/// [`turns`] for a chat with work items, and none for any other: only their cards read them.
pub fn turns_for_work(state: &ChatState) -> Vec<Turn> {
    let has_work = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .is_some();
    if has_work {
        turns(state)
    } else {
        Vec::new()
    }
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
        // In progress, never Moving (`work_panel.rs` `WORK_GROUPS` holds the user's words).
        "moving" => "In progress",
        "idle" => "Not being worked on",
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
/// User (Q2205 "A. Quiet links with the agent's logo"): each part of the bottom line is its own
/// quiet link at the chat's text size, the thread with its agent's logo, a GitHub PR or issue with
/// its state, a file, the steps, wrapping to a second line and never cut. "make sure fonts in the
/// card are all same as font of chat, use other stuff to differentiate not font size".
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User (Q2167), of the first cards: "i don't like the tags u have shown at the bottom pls remove
/// the border and make them look better also i don't know wtf is You there, just remove it". The
/// chips are an icon and muted words with no outline or fill (the renderers), and an owner shows
/// only where it says something the card does not: which thread has the work.
pub fn item_chips(item: &Value, now_ms: i64) -> Vec<Value> {
    let mut chips = Vec::new();
    for thread in item["threads"].as_array().into_iter().flatten() {
        let title = text(thread, "title");
        if title.is_empty() {
            continue;
        }
        chips.push(json!({
            "kind": "thread",
            "label": title,
            // Its agent's logo (Q2205 A).
            "agentIcon": crate::transcript::agent_message_card::agent_icon(text(thread, "agent"), ""),
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
        // A snoozed ask waits in Later until its snooze ends, so it is not "for you" meanwhile.
        .filter(|ask| {
            ask["open"] == true
                && !at_ms(text(ask, "snoozedUntil")).is_some_and(|until| until > now_ms)
        })
        .count();
    if open > 0 && !matches!(text(item, "status"), "needs" | "blocked") {
        chips.push(json!({ "kind": "question", "label": format!("{open} for you") }));
    }
    for link in item["links"].as_array().into_iter().flatten() {
        let url = text(link, "url");
        let kind = text(link, "kind");
        // A GitHub PR or issue: GitHub's mark, its number and its state in the state's colour
        // (the board chips' `link_tone`), whichever way the link came (Q2205 A).
        let mut chip = if matches!(kind, "pr" | "issue") {
            json!({
                "kind": "github",
                "label": text(link, "label"),
                "state": text(link, "state"),
                "tone": crate::extras::coordinator_board::link_tone(link),
            })
        } else {
            json!({ "kind": if kind == "link" { "file" } else { "link" }, "label": text(link, "label") })
        };
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

/// The key that locks a Needs you item's Done (its one answer when it asks no open question), or
/// `None` when it asks one or does not need the user. It names the item's last change, so once the
/// coordinator changes the item its Done is offered afresh.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User, of a Needs you item with no question whose card had only Show in chat and Mention: "why
/// don't i see a button to say All good on this task???? please fix for always". Every Needs you
/// item with no open question has Done, in the answers' look, on its Work panel card, Columns card
/// and dock row; a click tells the coordinator "W7: Done", and Reply stays for anything else.
pub(crate) fn done_answer_key(item: &Value) -> Option<String> {
    let asks_open = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|ask| ask["open"] == true);
    (text(item, "kind") == "item" && text(item, "status") == "needs" && !asks_open)
        .then(|| format!("work-done:{}:{}", text(item, "ref"), text(item, "at")))
}

/// A Needs you item's Done button (`done_answer_key`), as an ask's answer buttons are built.
fn done_button(item: &Value, key: &str, picked: bool) -> Value {
    let reference = text(item, "ref");
    json!({
        "label": "Done",
        "recommended": false,
        "picked": picked,
        "command": {
            "type": "coordinatorBoardAnswer",
            "key": key,
            "item": reference,
            "label": "Done",
            "text": format!("{reference}: Done"),
        },
    })
}

/// The words a Reply sent for an ask from here: this client's pick when it is none of the ask's
/// own answers, "" otherwise.
fn replied_words(ask: &Value, picked: Option<&str>) -> String {
    let Some(picked) = picked else {
        return String::new();
    };
    let preset = ask["answers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|answer| text(answer, "label") == picked);
    if preset || (picked == "Done" && ask["answers"].as_array().is_none_or(Vec::is_empty)) {
        String::new()
    } else {
        picked.to_string()
    }
}

/// The other lines of work that wait on an ask's answer (`item wait W8 --on Q12`), as
/// `[{title, ref, command}]` where the command shows that item in the Work panel.
fn waited_by(items: &[&Value], own: &Value, tag: &str) -> Value {
    if tag.is_empty() {
        return json!([]);
    }
    Value::Array(
        items
            .iter()
            .filter(|other| text(other, "ref") != text(own, "ref"))
            .filter(|other| !matches!(text(other, "status"), "finished" | "dropped"))
            .filter(|other| {
                other["waits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|wait| text(wait, "on").eq_ignore_ascii_case(tag) && wait["open"] != false)
            })
            .map(|other| {
                json!({
                    "title": text(other, "title"),
                    "ref": text(other, "ref"),
                    "command": { "type": "jumpToWorkCard", "key": text(other, "key") },
                })
            })
            .collect(),
    )
}

/// The work items an ask belongs to or holds up, as the small pills that lead an opened row's
/// answers: its own item in its own state, then each item that waits on its answer in the blocked
/// state (`waited_by`). Each pill shows a dot in its state's colour and a short id (the `#579` an
/// item's title starts with, else its `W7`); `tip` is what hovering (a long press on the phone)
/// reads, and `command` opens the item in the Work panel.
///
/// CDXC:Coordinators 2026-11-12 DECISION:
/// User, picking "B. At the start of the answers" for the line under an opened row's title: "has
/// to be pills looking, and small, only show id (hover to read which work item), and show dot
/// color for each one before the id in the pill. for example red dot for waiting on answer (like
/// it's blocked on it)".
pub(crate) fn item_pills(items: &[&Value], item: &Value, tag: &str) -> Value {
    let pill = |other: &Value, state: &str, tip: String| {
        json!({
            "id": short_item_id(other),
            "title": text(other, "title"),
            "state": state,
            "tip": tip,
            "command": { "type": "jumpToWorkCard", "key": text(other, "key") },
        })
    };
    let own_state = text(item, "status");
    let own_words = status_label(own_state);
    let mut pills = vec![pill(
        item,
        own_state,
        if own_words.is_empty() {
            text(item, "title").to_string()
        } else {
            format!("{} \u{b7} {own_words}", text(item, "title"))
        },
    )];
    for other in waited_by(items, item, tag).as_array().into_iter().flatten() {
        let Some(found) = items
            .iter()
            .find(|candidate| text(candidate, "ref") == text(other, "ref"))
        else {
            continue;
        };
        pills.push(pill(
            found,
            "blocked",
            format!("{} \u{b7} Waits on this answer", text(found, "title")),
        ));
    }
    Value::Array(pills)
}

/// An item's id as its pill shows it: the `#579` its title starts with, else its `W7`.
fn short_item_id(item: &Value) -> String {
    let title = text(item, "title");
    let digits: String = title
        .strip_prefix('#')
        .map(|rest| rest.chars().take_while(char::is_ascii_digit).collect())
        .unwrap_or_default();
    if digits.is_empty() {
        text(item, "ref").to_string()
    } else {
        format!("#{digits}")
    }
}

/// Whether an open ask is snoozed now: gxserver's `snoozedUntil`, or a snooze sent from here.
fn snoozed(ask: &Value, key: &str, state: &ChatState, now_ms: i64) -> bool {
    at_ms(text(ask, "snoozedUntil")).is_some_and(|until| until > now_ms)
        || state
            .extras
            .panels
            .work_snoozed
            .get(key)
            .is_some_and(|until| *until > now_ms)
}

/// Snooze for 1, 6 or 24 hours, as the dock's cards offer it (`command` per hours).
fn snooze_choices(snooze: impl Fn(u64) -> Value) -> Value {
    Value::Array(
        [(1, "1 hour"), (6, "6 hours"), (24, "24 hours")]
            .into_iter()
            .map(|(hours, label)| json!({ "label": label, "command": snooze(hours) }))
            .collect(),
    )
}

/// An ask's card: its question, what waits on it, the work it belongs to, and its buttons while
/// open; folded once settled (its answer whole in `foldOutcome`).
fn ask_card(
    item: &Value,
    ask: &Value,
    picked: Option<&str>,
    at: i64,
    now_ms: i64,
    context: &ChatContext,
) -> Value {
    let reference = text(item, "ref");
    let tag = text(ask, "tag");
    let key = format!("work-ask:{reference}:{tag}");
    let open = ask["open"] == true && picked.is_none();
    let answered: String = text(ask, "answered").chars().take(200).collect();
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
    let snooze = snooze_choices(
        |hours| json!({ "type": "snoozeWorkAsk", "key": key, "item": reference, "ask": tag, "hours": hours }),
    );
    let cancel = json!({
        "type": "coordinatorBoardAnswer",
        "op": "cancel",
        "key": key,
        "item": reference,
        "ask": tag,
        "label": "Dismissed",
        // Dismiss, one word for taking it off for good (the user's "Dismiss is the same thing as
        // Cancel").
        "text": format!("{reference} {tag}: I dismissed this; no need to ask it again."),
    });
    json!({
        "snooze": if open { snooze } else { Value::Null },
        "cancel": if open { cancel } else { Value::Null },
        "key": key,
        "kind": "ask",
        "folded": ask["open"] != true,
        "ref": reference,
        "title": text(ask, "text"),
        // Its longer explanation under the short title (`item ask --detail`), "" for none.
        "body": crate::transcript::native_markdown::ask_markdown(text(ask, "detail")),
        "status": ask_status,
        "statusLabel": status_label(ask_status),
        "tag": tag,
        "holds": ask["holds"] == true,
        "line": line,
        "chips": ask_chips,
        "buttons": buttons,
        "busy": picked.is_some() && ask["open"] == true,
        // A settled ask's line: a check once answered or done.
        "foldStatus": if ask["open"] == true { ask_status } else { "finished" },
        "answered": answered,
        "foldText": format!("{tag} \u{b7} {}", text(ask, "text")),
        "foldOutcome": if answered.is_empty() { "Answered".to_string() } else { answered.clone() },
        "age": crate::transcript::message_time::age_label(now_ms, at),
        "time": clock(at, context),
    })
}

/// The dock above a coordinator's chat box: `{cards, line}`. `cards` is every open ask, newest
/// first, as its full card with its buttons; `line` is the one status line under them
/// (`{parts: [{text, tone, group}]}`, "1 needs you · 5 in progress · 1 waiting"), counted from the
/// Work panel's own groups (`work_panel::work_board`), so the line and the panel never disagree.
/// `null` when the chat is not a coordinator's or it has no work.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User (placement page, "C. Asks docked, the rest one line", with "Asks, blocked, done card,
/// should be shown there. also please let's show those inline htmls from the agent replies there
/// docked above while not answered. also for any ask i should be able to click 'Snooze' and snooze
/// it for 1h 6h 24h or cancel it"): what needs him docks above the chat box as its full card, and
/// everything else is one status line under it that opens the Work panel. An open ask is drawn
/// only there, never also under its reply; it leaves the dock once settled.
pub fn dock(state: &ChatState, context: &ChatContext) -> Value {
    dock_with(state, context, &turns_for_work(state))
}

/// [`dock`], over turns the caller already read (`turns_for_work`).
pub fn dock_with(state: &ChatState, context: &ChatContext, turns: &[Turn]) -> Value {
    let now_ms = context.now_millis();
    // (sinks, at, row): a row that needs him is newest first, but one older than a day sinks below
    // the newer ones, so long-lived tasks do not squat in the first rows (they stay reachable under
    // "N more need you" and in the Work panel).
    let mut needs: Vec<(bool, i64, Value)> = question_rows(state, context, now_ms)
        .into_iter()
        .map(|(at, row)| (now_ms - at > DAY_MS, at, row))
        .collect();
    let work = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"));
    let items: Vec<&Value> = work
        .and_then(|work| work["items"].as_array())
        .into_iter()
        .flatten()
        // Lines of work and threads on no item, as the Work panel lists them.
        .filter(|item| text(item, "status") != "dropped")
        .collect();
    let answers = &state.extras.panels.board_answers;
    let mut later: Vec<(i64, Value)> = Vec::new();
    for item in &items {
        for ask in item["asks"].as_array().into_iter().flatten() {
            if ask["open"] != true {
                continue;
            }
            let key = format!("work-ask:{}:{}", text(item, "ref"), text(ask, "tag"));
            // A snoozed ask waits out of the dock until its end.
            if snoozed(ask, &key, state, now_ms) {
                continue;
            }
            let at = at_ms(text(ask, "askedAt")).unwrap_or(now_ms);
            let card = ask_card(
                item,
                ask,
                answers.get(&key).map(String::as_str),
                at,
                now_ms,
                context,
            );
            let asked_in = item["history"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|event| {
                    text(event, "kind") == "ask"
                        && text(event, "text").split(':').next() == Some(text(ask, "tag"))
                })
                .filter_map(|event| at_ms(text(event, "at")))
                .next_back()
                .unwrap_or(at);
            let mut row = json!({
                "key": key,
                "kind": "ask",
                "status": card["status"],
                "title": card["title"],
                // Its title's links (`#562`, a Markdown link) as runs the renderers draw, or null.
                "titleRuns": crate::questions::form_links::word_runs(text(ask, "text")),
                "body": card["body"],
                "age": card["age"],
                // The line under it names only its own item (`chips`); whether it holds that item
                // up is the row's mark. Other items that wait on its answer follow (`waitedBy`).
                //
                // CDXC:Coordinators 2026-11-10 DECISION:
                // User, picking on the dock item line page (footer-page.html) after a line read
                // "<item> · <item> waits on this": "C. Only the name", other items that wait "B.
                // Name it when it is one, count when it is more", and "A. One line: the name is
                // cut" when it does not fit.
                "detail": [],
                "waitedBy": waited_by(&items, item, text(ask, "tag")),
                "itemPills": item_pills(&items, item, text(ask, "tag")),
                "buttons": card["buttons"],
                "busy": card["busy"],
                // The words sent from here as its Reply, which its Reply button shows until gxserver
                // settles it (an answer picked from its buttons is marked on that button instead).
                "replied": replied_words(ask, answers.get(&key).map(String::as_str)),
                "reply": { "item": text(item, "ref"), "key": key, "tag": text(ask, "tag") },
                "mention": mention_of(item),
                "dismiss": card["cancel"],
                "snooze": card["snooze"],
                "chips": card["chips"],
                "needs": true,
            });
            row["showInChat"] = show_in_chat(&turns, asked_in, &key);
            needs.push((now_ms - at > DAY_MS, at, row));
        }
    }
    // Every Needs you line of work with no open question, with its Done and Reply.
    for item in &items {
        let Some(key) = done_answer_key(item) else {
            continue;
        };
        let picked =
            item["saidDone"] == true || answers.get(&key).is_some_and(|label| label == "Done");
        let at = at_ms(text(item, "at")).unwrap_or(now_ms);
        let card = dock_item_card(item, now_ms, None);
        let changed_at = item["history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|event| at_ms(text(event, "at")))
            .max()
            .unwrap_or(at);
        let mut row = json!({
            "key": key,
            "kind": "item",
            "status": "needs",
            "title": card["title"],
            "age": card["age"],
            "detail": [card["line"]],
            "buttons": [done_button(item, &key, picked)],
            "busy": picked && item["saidDone"] != true,
            "reply": { "item": text(item, "ref"), "key": key, "tag": "" },
            "mention": mention_of(item),
            "chips": card["chips"],
            "needs": true,
        });
        row["showInChat"] = show_in_chat(&turns, changed_at, &key);
        needs.push((now_ms - at > DAY_MS, at, row));
    }
    // Then every blocked line of work no holding ask already stands for (a stuck thread, or the
    // coordinator's word), and what the newest turn finished: a finished row leaves after the next
    // turn, or with its x.
    for item in items
        .iter()
        .filter(|item| text(item, "status") == "blocked")
    {
        if item["asks"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|ask| ask["open"] == true && ask["holds"] == true)
        {
            continue;
        }
        let at = at_ms(text(item, "at")).unwrap_or(0);
        later.push((at, work_row(item, &turns, now_ms, None)));
    }
    let newest_turn = turns.len().checked_sub(1);
    let dismissed = &state.extras.panels.work_dock_dismissed;
    for item in items
        .iter()
        .filter(|item| text(item, "status") == "finished")
    {
        let Some(done_at) = item["history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| text(event, "kind") == "done")
            .filter_map(|event| at_ms(text(event, "at")))
            .max()
        else {
            continue;
        };
        let turn = turn_of(&turns, done_at);
        if turn.is_none() || turn != newest_turn {
            continue;
        }
        let key = format!(
            "work-dock:{}:{}:done",
            text(item, "ref"),
            turns[turn.unwrap_or(0)].id
        );
        if !dismissed.contains(&key) {
            later.push((done_at, work_row(item, &turns, now_ms, Some(key))));
        }
    }
    needs.sort_by(|left, right| left.0.cmp(&right.0).then(right.1.cmp(&left.1)));
    later.sort_by(|left, right| right.0.cmp(&left.0));
    let rows: Vec<Value> = needs
        .into_iter()
        .map(|(_, _, row)| row)
        .chain(later.into_iter().map(|(_, row)| row))
        .collect();
    if rows.is_empty() {
        return Value::Null;
    }
    // Hidden by the reader: it stays hidden until shown again or a row that was not there when it
    // was hidden arrives, since that is news he has not seen. The phone hides the whole dock with
    // its x and its Work pill brings it back; the computer draws `hidden` as the dock collapsed to
    // its header, whose chevron opens it again (native_chat/work_cards.rs).
    //
    // CDXC:Coordinators 2026-11-10 DECISION:
    // User: "please let's allow fully collapsing the needs u row". One click hides the dock
    // completely; the Work pill keeps its "N need you" and brings it back. On the computer the
    // user later picked a header that collapses the dock to one bar instead (its renderer has the
    // picks), and a collapsed dock opens by itself when something new needs him.
    let hidden = state
        .extras
        .panels
        .work_dock_hidden
        .as_ref()
        .is_some_and(|keys| rows.iter().all(|row| keys.contains(text(row, "key"))));
    // At most four rows show (three on the phone); the rest are "N more need you", which opens the
    // Work panel at Needs you. The counts are the Work pill's (`coordinator_board::pill`).
    json!({
        "rows": rows,
        "hidden": hidden,
        "limit": 4,
        "phoneLimit": 3,
        "more": { "type": "showCoordinatorBoardGroup", "group": "needs" },
    })
}

/// A Work panel row's actions for its opened card, the same as the dock's opened row: an ask's
/// Reply, Show in chat, Dismiss and Snooze; any other line of work's Show in chat. `null` for none.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User: "WE MUST HAVE REPLY BUTTON FOR NEEDS YOU STUFF", for every needs-you item wherever it is
/// drawn: the dock, the Work panel and Columns open to the same actions.
pub fn panel_actions(state: &ChatState, context: &ChatContext, row: &Value) -> Value {
    panel_actions_with(state, context, row, &turns_for_work(state))
}

/// [`panel_actions`], over turns the caller already read: the Work panel asks this of every row
/// it builds (`coordinator_board.rs`), and a walk of the chat per row was most of its cost.
pub fn panel_actions_with(
    state: &ChatState,
    context: &ChatContext,
    row: &Value,
    turns: &[Turn],
) -> Value {
    let Some(items) = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
    else {
        return Value::Null;
    };
    let reference = text(row, "ref");
    let Some(item) = items.iter().find(|item| text(item, "ref") == reference) else {
        return Value::Null;
    };
    let now_ms = context.now_millis();
    if row["askRow"] == true {
        let tag = text(row, "ask");
        let Some(ask) = item["asks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|ask| text(ask, "tag") == tag && ask["open"] == true)
        else {
            return Value::Null;
        };
        let key = format!("work-ask:{reference}:{tag}");
        let at = at_ms(text(ask, "askedAt")).unwrap_or(now_ms);
        let picked = state
            .extras
            .panels
            .board_answers
            .get(&key)
            .map(String::as_str);
        let card = ask_card(item, ask, picked, at, now_ms, context);
        let asked_in = item["history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| {
                text(event, "kind") == "ask" && text(event, "text").split(':').next() == Some(tag)
            })
            .filter_map(|event| at_ms(text(event, "at")))
            .next_back()
            .unwrap_or(at);
        return json!({
            "reply": { "item": reference, "key": key, "tag": tag },
            "dismiss": card["cancel"],
            "snooze": card["snooze"],
            "showInChat": show_in_chat(&turns, asked_in, &key),
        });
    }
    let changed_at = item["history"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|event| at_ms(text(event, "at")))
        .max()
        .unwrap_or(0);
    let show = show_in_chat(&turns, changed_at, text(row, "key"));
    // A Needs you item with no open question: Reply too, for anything besides its Done.
    if let Some(key) = done_answer_key(item) {
        return json!({
            "reply": { "item": reference, "key": key, "tag": "" },
            "showInChat": show,
        });
    }
    if show.is_null() {
        Value::Null
    } else {
        json!({ "showInChat": show })
    }
}

/// A form's or page's actions for its opened Work panel card, the dock's row's.
pub fn question_actions(state: &ChatState, context: &ChatContext, id: &str) -> Value {
    let key = format!("dock-questions:{id}");
    question_rows(state, context, context.now_millis())
        .into_iter()
        .map(|(_, row)| row)
        .find(|row| row["key"] == key.as_str())
        .map_or(Value::Null, |row| {
            json!({
                "reply": row["reply"],
                "open": row["open"],
                "dismiss": row["dismiss"],
                "snooze": row["snooze"],
                "showInChat": row["showInChat"],
            })
        })
}

/// A day, in ms: a needs-you row older than this sinks below the newer ones (`dock`).
const DAY_MS: i64 = 86_400_000;

/// The mention a work item's Mention button puts in the chat box: `[title](board:B12)`, the
/// reference a + pick inserts.
fn mention_of(item: &Value) -> String {
    let reference = text(item, "ref").replacen('W', "B", 1);
    if reference.is_empty() || text(item, "kind") != "item" {
        return String::new();
    }
    format!(
        "[{}](board:{reference})",
        text(item, "title").replace(['[', ']'], "")
    )
}

/// "Show in chat": the reply of the turn holding `at`, or `null` when no loaded turn does. It
/// scrolls only when pressed (`showInChat`), the one deliberate exception to "nothing scrolls".
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User: "i think we should have jump to reference to jump to the place it was asked if i want to
/// see more from the agent chat there", then "do this one for all of them so i can go to the turn
/// that gave me this q cuz maybe agent mentioned something in that turn end about it". Every row's
/// expanded card has it: an ask's goes to the reply of the turn that asked it, a form's or page's
/// to the reply that sent it, any other line of work's to the latest turn that changed it.
fn show_in_chat(turns: &[Turn], at: i64, target: &str) -> Value {
    match turn_of(turns, at).and_then(|turn| turns[turn].anchor.clone()) {
        Some(anchor) => json!({ "type": "showInChat", "messageId": anchor, "target": target }),
        None => Value::Null,
    }
}

/// A blocked or just-finished line of work's dock row (`done_key`: the finished row's, with the x
/// that dismisses it).
fn work_row(item: &Value, turns: &[Turn], now_ms: i64, done_key: Option<String>) -> Value {
    let card = dock_item_card(item, now_ms, done_key.clone());
    let changed_at = item["history"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|event| at_ms(text(event, "at")))
        .max()
        .unwrap_or(0);
    let key = text(&card, "key").to_string();
    json!({
        "key": key,
        "kind": if done_key.is_some() { "finished" } else { "blocked" },
        "status": card["status"],
        "title": card["title"],
        "age": card["age"],
        "detail": [card["line"]],
        "buttons": [],
        "mention": mention_of(item),
        "dismiss": card["dismiss"],
        "chips": card["chips"],
        "showInChat": show_in_chat(turns, changed_at, &key),
        "needs": false,
    })
}

/// The forms, pages and one-click questions in the chat's replies that still wait on the reader,
/// as dock rows with when they were asked. They are the ? pill's own unanswered list
/// (`questions::open_questions::unanswered`), so For later ones stay out and nothing is counted
/// twice; one leaves the dock once every question on it is answered, skipped or withdrawn.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User: "also please let's show those inline htmls from the agent replies there docked above while
/// not answered", then, of questions asked inside a reply: "i need to scroll back to see that
/// question ... should always be shown as card ... this can be smaller just for 1 off quick
/// question". A one-click question is one compact row with its answers on it (the recommended one
/// starred) and Reply; a form of several questions, a text question or a page keeps "N questions
/// open" and Open (the answer popup, or the page). Any agent chat, not only a coordinator's.
fn question_rows(state: &ChatState, context: &ChatContext, now_ms: i64) -> Vec<(i64, Value)> {
    let _ = context;
    crate::questions::open_questions::unanswered(state)
        .into_iter()
        .map(|asker| {
            let at = asker.at.unwrap_or(now_ms);
            let form = matches!(asker.kind, crate::transcript::open_questions::AskerKind::Form);
            let open = asker.waiting(None);
            let noun = if open == 1 { "question" } else { "questions" };
            let snooze = snooze_choices(
                |hours| json!({ "type": "snoozeQuestion", "id": asker.id, "hours": hours }),
            );
            let base = json!({
                "key": format!("dock-questions:{}", asker.id),
                "status": "needs",
                "title": if asker.title.is_empty() { "Questions" } else { asker.title.as_str() },
                "age": crate::transcript::message_time::age_label(now_ms, at),
                "dismiss": { "type": "cancelQuestion", "id": asker.id },
                "snooze": snooze,
                "showInChat": { "type": "showInChat", "messageId": asker.message_id, "target": asker.id },
                "mention": "",
                "needs": true,
            });
            let mut row = base;
            // A one-click question: its answers on the row and Reply, no Open step.
            if let Some(question) = form.then(|| one_click(state, &asker.message_id, &asker.id)).flatten() {
                row["kind"] = json!("question");
                row["title"] = json!(question.question);
                row["buttons"] = json!(question
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(index, choice)| json!({
                        "label": choice.label,
                        "recommended": choice.recommended,
                        "command": { "type": "inlineAnswer", "form": asker.id, "choice": index },
                    }))
                    .collect::<Vec<_>>());
                row["reply"] = json!({ "form": asker.id, "tag": question.tag });
                row["detail"] = json!([]);
            } else {
                row["kind"] = json!("form");
                // A Ghostex page (not a form): its mark is a page (desktop `row_mark`), a click
                // opens it, and its name carries how much it still asks (`page_count`).
                row["page"] = json!(!form);
                if !form {
                    row["pageId"] = json!(asker.id);
                    row["count"] = json!(page_count(state, &asker));
                }
                row["buttons"] = json!([]);
                // A page that asks nothing waits to be marked seen (`Asker::seen_only`).
                let detail = if asker.seen_only {
                    "Page, not seen yet".to_string()
                } else {
                    format!("{open} {noun} open \u{b7} {}", if form { "Form" } else { "Page" })
                };
                row["detail"] = json!([detail]);
                row["open"] = json!({ "type": "openResource", "id": asker.id });
            }
            (at, row)
        })
        .collect()
}

/// `{type: "loadPageQuestionCounts", pageId}`: asks the page's gxserver how many of its questions
/// the reader's draft answers (`readVisualPageSource`), unless a read is already out.
pub fn load_page_counts(state: &mut ChatState, page_id: &str) -> Vec<crate::effect::Effect> {
    let page_id = page_id.to_ascii_lowercase();
    let panels = &state.extras.panels;
    if page_id.is_empty()
        || panels
            .page_count_requests
            .values()
            .any(|asked| *asked == page_id)
    {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state
        .extras
        .panels
        .page_count_requests
        .insert(request_id, page_id.clone());
    vec![crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::ReadVisualPageSource,
        params: Box::new(json!({ "pageId": page_id })),
    }]
}

/// Takes the answer to [`load_page_counts`]: the counts, kept by page; nothing for a refused read
/// (an older gxserver), whose rows keep their count from the page's own markup.
pub fn settle_page_counts(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<crate::effect::Effect>> {
    let page_id = state
        .extras
        .panels
        .page_count_requests
        .remove(&request_id)?;
    state.core.claim_refusal();
    if let Ok(result) = answer {
        if let (Some(answered), Some(total)) = (
            result["questionsAnswered"].as_u64(),
            result["questionsTotal"].as_u64(),
        ) {
            state
                .extras
                .panels
                .page_question_counts
                .insert(page_id, (answered, total));
        }
    }
    Some(Vec::new())
}

/// How much a page still asks, after its name in the quieter colour: "(14 questions)" while none
/// is answered, "(10/16 answered)" once some are (a pick or words in the page's saved draft
/// counts, sent or not), "(not seen yet)" for a page that asks nothing and waits to be marked
/// seen. Before its gxserver has said, the questions its block lists, none answered.
///
/// CDXC:Coordinators 2026-11-11 DECISION:
/// User, of a page row in the dock: "also show (10/16 answered) or (14 questions) next to the
/// page name to know how many i need to answer".
pub(crate) fn page_count(
    state: &ChatState,
    asker: &crate::transcript::open_questions::Asker,
) -> String {
    if asker.seen_only {
        return "(not seen yet)".to_string();
    }
    let (answered, total) = state
        .extras
        .panels
        .page_question_counts
        .get(&asker.id.to_ascii_lowercase())
        .copied()
        .unwrap_or((0, asker.tags.len() as u64));
    match (answered, total) {
        (_, 0) => String::new(),
        (0, 1) => "(1 question)".to_string(),
        (0, total) => format!("({total} questions)"),
        (answered, total) => format!("({answered}/{total} answered)"),
    }
}

/// The one-click question (a reply's answer links) a form id names, when the form is one.
fn one_click(
    state: &ChatState,
    message_id: &str,
    form_id: &str,
) -> Option<crate::transcript::inline_answers::InlineQuestion> {
    let message = state
        .messages
        .composed
        .iter()
        .find(|message| message.id == message_id)?;
    let prose = crate::transcript::prose::prose_markdown(&message.blocks);
    crate::transcript::inline_answers::inline_questions(&prose)
        .into_iter()
        .find(|question| question.form().id == form_id)
}

/// The Work panel row key of the open ask a question tag names (`work-ask:W3:Q12`), if a work item
/// asks it.
pub fn ask_key_of_tag(state: &ChatState, tag: &str) -> Option<String> {
    let items = state
        .session
        .coordinator_threads
        .as_ref()?
        .get("work")?
        .get("items")?
        .as_array()?;
    items.iter().find_map(|item| {
        item["asks"]
            .as_array()?
            .iter()
            .find(|ask| text(ask, "tag").eq_ignore_ascii_case(tag))
            .map(|ask| format!("work-ask:{}:{}", text(item, "ref"), text(ask, "tag")))
    })
}

/// A line of work's card in the dock: blocked, or finished in the latest turn (`done_key`, with
/// the command that dismisses it).
fn dock_item_card(item: &Value, now_ms: i64, done_key: Option<String>) -> Value {
    let status = text(item, "status");
    let at = at_ms(text(item, "at")).unwrap_or(now_ms);
    let mut line = text(item, "now").to_string();
    if line.is_empty() {
        if let Some(why) = item["owner"]["why"].as_str() {
            line = why.to_string();
        }
    }
    let key = done_key
        .clone()
        .unwrap_or_else(|| format!("work-dock:{}:{status}", text(item, "ref")));
    let mut card = json!({
        "key": key,
        "kind": "item",
        "ref": text(item, "ref"),
        "title": text(item, "title"),
        "status": status,
        "statusLabel": status_label(status),
        "line": line,
        "chips": item_chips(item, now_ms),
        "age": crate::transcript::message_time::age_label(now_ms, at),
        "at": at,
    });
    if let Some(key) = done_key {
        card["dismiss"] = json!({ "type": "dismissWorkDockCard", "key": key });
    }
    card
}

/// The turn whose line joins the "Worked for" line above the chat box: the newest, once it has
/// finished and that line shows its time.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User (the target page): under a reply "one quiet line ... Worked for 40s · Started <item>",
/// not the turn's line and "Worked for" stacked as two.
fn merged_turn(state: &ChatState, turns: &[Turn]) -> Option<usize> {
    let shows = state.transcript_view.worked_line.is_some()
        && !state.transcript_view.chat_look.status_in_transcript;
    shows.then(|| turns.len().checked_sub(1)).flatten()
}

/// The newest finished turn's line, for the end of its "Worked for" line
/// (`extras::working_card`): "Pairing page started · 1 ask".
pub fn turn_summary(state: &ChatState) -> Option<String> {
    // Asked on every document: no walk of the chat unless its line shows on a chat with work.
    let shows = state.transcript_view.worked_line.is_some()
        && !state.transcript_view.chat_look.status_in_transcript;
    if !shows {
        return None;
    }
    let turns = turns_for_work(state);
    let newest = merged_turn(state, &turns)?;
    let turn_id = turns.get(newest)?.id.clone();
    let mut lines: BTreeMap<usize, TurnLine> = BTreeMap::new();
    let items = state
        .session
        .coordinator_threads
        .as_ref()?
        .get("work")?
        .get("items")?
        .as_array()?;
    for item in items.iter().filter(|item| item["kind"] == "item") {
        let events: Vec<&Value> = item["history"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| TURN_KINDS.contains(&text(event, "kind")))
            .filter(|event| {
                at_ms(text(event, "at")).and_then(|at| turn_of(&turns, at)) == Some(newest)
            })
            .collect();
        if events.is_empty() {
            continue;
        }
        let summary = lines.entry(newest).or_default();
        let kinds: Vec<&str> = events.iter().map(|event| text(event, "kind")).collect();
        summary.asks += kinds.iter().filter(|kind| **kind == "ask").count();
        if kinds.iter().all(|kind| *kind == "ask") {
            continue;
        }
        let verb = if kinds.contains(&"done") {
            "finished"
        } else if kinds.contains(&"drop") {
            "dropped"
        } else if kinds.contains(&"add") {
            "started"
        } else if kinds.contains(&"wait") {
            "waiting"
        } else {
            "updated"
        };
        summary
            .parts
            .push(format!("{} {verb}", text(item, "title")));
    }
    let _ = turn_id;
    let summary = lines.remove(&newest)?;
    let mut words: Vec<String> = summary.parts.iter().take(3).cloned().collect();
    if summary.parts.len() > 3 {
        words.push(format!("{} more", summary.parts.len() - 3));
    }
    match summary.asks {
        0 => {}
        1 => words.push("1 ask".to_string()),
        asks => words.push(format!("{asks} asks")),
    }
    (!words.is_empty()).then(|| words.join(" \u{b7} "))
}

/// What one turn changed, for its quiet line (`project`).
#[derive(Default)]
struct TurnLine {
    parts: Vec<String>,
    asks: usize,
    finished: bool,
    at: i64,
    item_key: String,
    /// What the turn did to each item it touched, in the order it touched them (`turn_change`).
    changes: Vec<TurnChange>,
}

/// One item a turn changed, said plainly for its card: "Work item updated", the item's title set
/// apart, the one line that changed, and the item's state after it as the card's pill.
#[derive(Default)]
struct TurnChange {
    /// "Work item updated", "added", "finished", "dropped", "needs you", "asked you", "now waiting".
    lead: String,
    title: String,
    /// The line that changed: its new status line, the question asked, what it waits on.
    detail: String,
    /// The state the pill names (`status_label`), empty for none.
    status: String,
    key: String,
    at: i64,
}

/// What a turn's events did to one item, for its card (`TurnChange`). The strongest change names
/// it (finished, dropped, a question still open, added, waiting, then any other change), and its
/// own event's words are the line that changed. The pill is the item's state now when this turn
/// changed it last, else the state that change left it in.
fn turn_change(item: &Value, events: &[&Value], latest: bool) -> TurnChange {
    let last_text = |kinds: &[&str]| {
        events
            .iter()
            .rev()
            .find(|event| kinds.contains(&text(event, "kind")))
            .map(|event| text(event, "text").to_string())
            .unwrap_or_default()
    };
    let has = |kind: &str| events.iter().any(|event| text(event, "kind") == kind);
    // A question asked in this turn that is still open.
    let open_ask = events
        .iter()
        .rev()
        .filter(|event| text(event, "kind") == "ask")
        .find(|event| {
            let tag = text(event, "text").split(':').next().unwrap_or_default();
            item["asks"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|ask| text(ask, "tag") == tag && ask["open"] == true)
        })
        .map(|event| text(event, "text").to_string());
    let (lead, detail, status) = if has("done") {
        ("finished", last_text(&["done"]), "finished")
    } else if has("drop") {
        ("dropped", String::new(), "dropped")
    } else if let Some(question) = open_ask {
        ("needs you", question, "needs")
    } else if has("add") {
        ("added", last_text(&["now", "step", "status"]), "")
    } else if has("wait") {
        ("now waiting", last_text(&["wait"]), "waiting")
    } else if has("ask") {
        ("asked you", last_text(&["ask"]), "")
    } else {
        (
            "updated",
            last_text(&[
                "now", "step", "status", "edit", "thread", "settle", "restore",
            ]),
            "",
        )
    };
    let current = text(item, "status");
    let status = if latest && !current.is_empty() {
        current.to_string()
    } else {
        status.to_string()
    };
    TurnChange {
        lead: format!("Work item {lead}"),
        title: text(item, "title").to_string(),
        detail,
        status,
        key: text(item, "key").to_string(),
        at: events
            .iter()
            .filter_map(|event| at_ms(text(event, "at")))
            .max()
            .unwrap_or_default(),
    }
}

/// The cards, in transcript order: `[{ key, kind ("item" | "ask"), anchorId, turnId, folded,
/// ref, title, status, statusLabel, line, chips: [{kind, label}], age, at, foldText, time, tag,
/// holds, buttons: [{label, recommended, picked, command}], answered, busy }]`, and the open asks
/// the ? pill steps through: `[{ key, anchorId, at }]`.
pub fn project(state: &ChatState, context: &ChatContext) -> (Value, Vec<OpenAsk>) {
    project_with(state, context, &turns_for_work(state))
}

/// [`project`], over turns the caller already read (`turns_for_work`).
pub fn project_with(
    state: &ChatState,
    context: &ChatContext,
    turns: &[Turn],
) -> (Value, Vec<OpenAsk>) {
    let Some(items) = state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
    else {
        return (json!([]), Vec::new());
    };
    if turns.is_empty() {
        return (json!([]), Vec::new());
    }
    let now_ms = context.now_millis();
    // Each card with the moment it orders by.
    // Each card with where it goes: its turn, then needs-you first (`priority`), then its moment.
    let mut cards: Vec<(i64, u8, i64, Value)> = Vec::new();
    let mut open_asks = Vec::new();
    let mut lines: BTreeMap<usize, TurnLine> = BTreeMap::new();
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
        for (turn_index, events) in &by_turn {
            let turn = &turns[*turn_index];
            if turn.anchor.is_none() {
                continue;
            }
            let first_at = at_ms(text(events[0], "at")).unwrap_or(turn.start_ms);
            let summary = lines.entry(*turn_index).or_default();
            // Asks made in this turn: counted on its line; an open one waits in the dock, and the
            // ? pill counts it.
            for event in events.iter().filter(|event| text(event, "kind") == "ask") {
                summary.asks += 1;
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
                let at = at_ms(text(event, "at")).unwrap_or(first_at);
                if ask["open"] == true && !snoozed(ask, &key, state, now_ms) {
                    open_asks.push(OpenAsk {
                        key,
                        anchor: turn.anchor.clone().unwrap_or_default(),
                        at,
                    });
                }
            }
            let last_at = events
                .iter()
                .filter_map(|event| at_ms(text(event, "at")))
                .max()
                .unwrap_or(first_at);
            summary.at = summary.at.max(last_at);
            // What the turn did to the item, for its own card (`turn_change`).
            let latest = by_turn.keys().next_back() == Some(turn_index);
            summary.changes.push(turn_change(item, events, latest));
            // What the turn did to the item, in a word, unless it only asked.
            let kinds: Vec<&str> = events.iter().map(|event| text(event, "kind")).collect();
            if kinds.iter().all(|kind| *kind == "ask") {
                continue;
            }
            let verb = if kinds.contains(&"done") {
                summary.finished = true;
                "finished"
            } else if kinds.contains(&"drop") {
                "dropped"
            } else if kinds.contains(&"add") {
                "started"
            } else if kinds.contains(&"wait") {
                "waiting"
            } else {
                "updated"
            };
            summary
                .parts
                .push(format!("{} {verb}", text(item, "title")));
            if summary.item_key.is_empty() {
                summary.item_key = text(item, "key").to_string();
            }
        }
    }
    // One quiet line per turn under its reply: what the turn changed, its asks and when. It opens
    // the Work panel at the first line of work it names; the cards themselves live in the dock
    // (`dock`) and the Work panel.
    //
    // CDXC:Coordinators 2026-11-10 DECISION:
    // User (placement page, C): "The transcript keeps one quiet line per turn". The cards under a
    // reply and the In progress row there are gone: what needs the user docks above the chat box,
    // the In progress list opens from the status line, and the Work panel keeps the rest.
    let merged = merged_turn(state, &turns);
    for (turn_index, summary) in lines {
        let turn = &turns[turn_index];
        let Some(anchor) = turn.anchor.as_deref() else {
            continue;
        };
        // The newest finished turn's line rides on its "Worked for" line instead (`turn_summary`).
        if Some(turn_index) == merged {
            continue;
        }
        // One card per item the turn changed, said plainly: "Work item updated", the item's
        // title, the line that changed and its state now as the pill. A turn that changed more
        // than `TURN_CARDS_MAX` items is one card naming how many, its titles as the line.
        //
        // CDXC:Coordinators 2026-11-11 DECISION:
        // User, of the thin line "Usage meters: a click must not make the main window flicker
        // updated" above a "Message from" card: "make this one look like the other card please"
        // and "please make it clearer, i dont get what this card means". The title with "updated"
        // on its end read as part of the title. Each card now leads with what happened to a work
        // item, sets the item's title apart, and opens it in the Work panel (`jumpToWorkCard`).
        // Supersedes the one line per turn joining "<title> updated" parts.
        let base = |key: String, status: &str, at: i64| {
            json!({
                "key": key,
                "kind": "turnLine",
                "anchorId": anchor,
                "turnId": turn.id,
                "folded": true,
                "foldStatus": if status.is_empty() { "moving" } else { status },
                "time": clock(at, context),
            })
        };
        if summary.changes.len() > TURN_CARDS_MAX {
            let titles: Vec<&str> = summary
                .changes
                .iter()
                .map(|change| change.title.as_str())
                .collect();
            let lead = format!("{} work items changed", titles.len());
            let mut line = base(format!("work-turn:{}", turn.id), "moving", summary.at);
            line["lead"] = json!(lead);
            line["title"] = json!("");
            line["detail"] = json!(titles.join(" \u{b7} "));
            line["foldText"] = json!(format!("{lead}: {}", titles.join(" \u{b7} ")));
            line["foldOutcome"] = json!("");
            if let Some(first) = summary
                .changes
                .first()
                .filter(|change| !change.key.is_empty())
            {
                line["command"] = json!({ "type": "jumpToWorkCard", "key": first.key });
            }
            cards.push((turn.start_ms, 5, summary.at, line));
            continue;
        }
        for (index, change) in summary.changes.iter().enumerate() {
            let mut line = base(
                format!("work-turn:{}:{index}", turn.id),
                &change.status,
                change.at.max(turn.start_ms),
            );
            line["lead"] = json!(change.lead);
            line["title"] = json!(change.title);
            line["detail"] = json!(change.detail);
            line["status"] = json!(change.status);
            line["statusLabel"] = json!(if change.status.is_empty() {
                ""
            } else {
                status_label(&change.status)
            });
            // The words the phone's line shows until it draws the card's parts.
            line["foldText"] = json!(format!("{}: {}", change.lead, change.title));
            line["foldOutcome"] = json!("");
            if !change.key.is_empty() {
                line["command"] = json!({ "type": "jumpToWorkCard", "key": change.key });
            }
            cards.push((turn.start_ms, 5, change.at, line));
        }
    }
    cards.sort_by_key(|(turn, priority, at, _)| (*turn, *priority, *at));
    open_asks.sort_by_key(|ask| ask.at);
    (
        Value::Array(cards.into_iter().map(|(_, _, _, card)| card).collect()),
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

/// When the cards next read differently with nothing but the clock moving: the first age on a
/// live card to change (`age_label_changes_at`), or the first snooze to end, which brings its ask
/// back.
pub fn ages_change_at(state: &ChatState, cards: &Value, now_ms: i64) -> Option<i64> {
    let ages = cards
        .as_array()
        .into_iter()
        .flatten()
        .filter(|card| card["folded"] != true)
        .filter_map(|card| card["at"].as_i64())
        .map(|at| crate::transcript::message_time::age_label_changes_at(now_ms, at));
    ages.chain(snoozes_end_at(state, now_ms)).min()
}

/// When the first snooze still running at `now_ms` ends: gxserver's `snoozedUntil` on an open ask,
/// or one sent from here. Its ask is back under Needs you from then, in the dock and the panel.
pub fn snoozes_end_at(state: &ChatState, now_ms: i64) -> Option<i64> {
    let sent = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("work"))
        .and_then(|work| work.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| item["asks"].as_array().into_iter().flatten())
        .filter_map(|ask| at_ms(text(ask, "snoozedUntil")));
    let local = state.extras.panels.work_snoozed.values().copied();
    sent.chain(local).filter(|until| *until > now_ms).min()
}

/// Everything [`project`] reads besides the clock, kept beside the cards it built
/// (`document::AssembleCache`), so an event that moves nothing here (a streamed token of a reply
/// that already has words) does not walk the transcript again.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkInputs {
    /// The stamp of the coordinator threads `work` was read from, and those threads themselves
    /// (shared, `document::SharedValue`), which hold the `work` the cards were built from.
    threads: u64,
    held_threads: Option<crate::document::SharedValue>,
    messages: usize,
    last_id: String,
    last_has_words: bool,
    /// The stamp of the panels `answers` was read from.
    panels: u64,
    answers: BTreeMap<String, String>,
    /// The newest turn's line rides on "Worked for" while that shows (`merged_turn`).
    worked: bool,
    /// Snoozes sent from here (`snoozed`).
    snoozed: BTreeMap<String, i64>,
}

/// Whether the chat's last message is a reply with words in it.
fn last_has_words(state: &ChatState) -> bool {
    state.messages.composed.last().is_some_and(|message| {
        message.role == ChatRole::Assistant && crate::transcript::prose::has_prose(&message.blocks)
    })
}

/// Whether the newest turn's line rides on "Worked for" (`merged_turn`).
fn worked(state: &ChatState) -> bool {
    state.transcript_view.worked_line.is_some()
        && !state.transcript_view.chat_look.status_in_transcript
}

fn work_of(state: &ChatState) -> Option<&Value> {
    state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("work"))
}

impl WorkInputs {
    fn of(state: &ChatState) -> Self {
        let last = state.messages.composed.last();
        Self {
            threads: state.session.coordinator_threads.stamp(),
            held_threads: (*state.session.coordinator_threads).clone(),
            messages: state.messages.composed.len(),
            last_id: last.map(|message| message.id.clone()).unwrap_or_default(),
            last_has_words: last_has_words(state),
            panels: state.extras.panels.stamp(),
            answers: state.extras.panels.board_answers.clone(),
            worked: worked(state),
            snoozed: state.extras.panels.work_snoozed.clone(),
        }
    }

    /// Whether `state` holds these inputs: the threads and panels by stamp, and by value only
    /// when their stamp moved, without copies.
    fn matches(&mut self, state: &ChatState) -> bool {
        // CDXC:Coordinators 2026-11-11 WHY: every input is named (no `..`), so one added to this
        // struct fails the build here until it is compared below too: a kept part checked on fewer
        // inputs than it was built from would serve a stale value.
        let Self {
            threads,
            held_threads,
            messages,
            last_id,
            last_has_words: had_words,
            panels,
            answers,
            worked: was_worked,
            snoozed,
        } = self;
        if *messages != state.messages.composed.len()
            || last_id
                != state
                    .messages
                    .composed
                    .last()
                    .map_or("", |message| message.id.as_str())
            || *had_words != last_has_words(state)
            || *was_worked != worked(state)
        {
            return false;
        }
        let threads_now = state.session.coordinator_threads.stamp();
        if *threads != threads_now {
            let held_work = held_threads
                .as_deref()
                .and_then(|threads| threads.get("work"));
            if held_work != work_of(state) {
                return false;
            }
            *threads = threads_now;
            // The threads held now, so the ones they replaced are not kept alive here.
            *held_threads = (*state.session.coordinator_threads).clone();
        }
        let panels_now = state.extras.panels.stamp();
        if *panels != panels_now {
            if *answers != state.extras.panels.board_answers
                || *snoozed != state.extras.panels.work_snoozed
            {
                return false;
            }
            *panels = panels_now;
        }
        true
    }
}

/// Kept cards: their inputs, the clock they were built at, the moment their first age changes,
/// and the cards with the open asks.
pub type KeptWork = (
    WorkInputs,
    i64,
    Option<i64>,
    (crate::document::SharedValue, Vec<OpenAsk>),
);

/// [`project`], reused while its inputs hold and no age on it moved.
pub fn project_kept(
    state: &ChatState,
    context: &ChatContext,
    kept: &mut Option<KeptWork>,
    turns: &[Turn],
) -> (crate::document::SharedValue, Vec<OpenAsk>) {
    // Not a coordinator, or a gxserver that sends no work: nothing to build or keep.
    if state
        .session
        .coordinator_threads
        .as_deref()
        .and_then(|threads| threads.get("work"))
        .is_none()
    {
        *kept = None;
        return (json!([]).into(), Vec::new());
    }
    let now_ms = context.now_millis();
    if let Some((was, built_at, changes_at, value)) = kept.as_mut() {
        if now_ms >= *built_at && changes_at.is_none_or(|at| now_ms < at) && was.matches(state) {
            return value.clone();
        }
    }
    let (cards, asks) = project_with(state, context, turns);
    let changes_at = ages_change_at(state, &cards, now_ms);
    let value = (crate::document::SharedValue::from(cards), asks);
    *kept = Some((WorkInputs::of(state), now_ms, changes_at, value.clone()));
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
        .as_deref()
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
