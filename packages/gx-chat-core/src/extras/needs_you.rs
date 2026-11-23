//! A session's Needs you to-dos (`ghostex needs-you add`, server/src/session_needs_you/) as rows of
//! the dock above its chat box: the title, Done (or the agent's one-click answers) and Reply, and
//! the command to run as a chip that copies it.
//!
//! They are a coordinator ask's Done row in every way the renderers see (`kind: "ask"`), and their
//! buttons send the ask's own `coordinatorBoardAnswer`, naming the to-do's tag as `item`: gxserver
//! records the answer once (`session_needs_you::route_board_answer`) and the chat then sends
//! "Q61: Done" to the agent, queued after a working turn like any message. So Done, Reply,
//! Dismiss, the one-click lock and the retry after a restart are the ask's, not a copy of them.
//!
//! CDXC:SessionChat 2026-11-16 DECISION:
//! User (Q59 "B. A to-do any session can add", Q64 "phone OTA"): the to-do shows as a row in that
//! session's dock above the chat box with Done and Reply, and a command as a chip the user clicks
//! to copy; the phone gets the same row in the same change (copy also in the row's long-press
//! menu), over the air.
//! SEE-ALSO: packages/gx-chat-core/src/extras/work_cards.rs (`dock_with` adds these rows),
//! apps/desktop/src/app/native_chat/work_cards.rs (`dock_row`'s command chip),
//! apps/mobile/app/src/chat/native/cards/WorkCards.tsx (`DockRow`'s command chip).

use serde_json::{json, Value};

use super::work_cards::{at_ms, Turn};
use crate::state::{ChatContext, ChatState};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Whether the session has an open or recently settled to-do, so the dock reads the chat's turns for it.
pub fn has_items(state: &ChatState) -> bool {
    let Some(value) = state.session.needs_you.as_ref() else {
        return false;
    };
    value["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
        || value["settled"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
}

/// The description an opened row shows: the agent's detail, then the command as a code block
/// (which has its own copy button there).
fn body(item: &Value) -> String {
    let detail = text(item, "detail").trim();
    let command = text(item, "command").trim();
    let markdown = match (detail.is_empty(), command.is_empty()) {
        (_, true) => detail.to_string(),
        (true, false) => format!("```sh\n{command}\n```"),
        (false, false) => format!("{detail}\n\n```sh\n{command}\n```"),
    };
    crate::transcript::native_markdown::ask_markdown(&markdown)
}

/// The live and snoozed rows, each with when it was asked (or when its snooze ends).
/// Supports the same 1h / 6h / 24h Snooze menu and Snoozed (N) shelf as work asks (Q4686 A).
pub fn rows_split(
    state: &ChatState,
    context: &ChatContext,
    turns: &[Turn],
    now_ms: i64,
    show_in_chat: impl Fn(&[Turn], i64, &str) -> Value,
    reply_name: impl Fn(&str) -> String,
) -> (Vec<(i64, Value)>, Vec<(i64, Value)>) {
    let _ = context;
    let Some(items) = state
        .session
        .needs_you
        .as_ref()
        .and_then(|value| value["items"].as_array())
    else {
        return (Vec::new(), Vec::new());
    };
    let answers = &state.extras.panels.board_answers;
    let mut live = Vec::new();
    let mut snoozed_rows = Vec::new();
    for item in items {
        let tag = text(item, "tag");
        if tag.is_empty() {
            continue;
        }
        let title = text(item, "title");
        let key = format!("needs-you:{tag}");
        let picked = answers.get(&key).map(String::as_str);
        // Optimistically hide a to-do dismissed by Mark all read until gxserver confirms.
        if picked == Some("Dismissed") {
            continue;
        }
        let at = at_ms(text(item, "askedAt")).unwrap_or(now_ms);
        let button = |label: &str| {
            json!({
                "label": label,
                "recommended": false,
                "picked": picked == Some(label),
                "command": {
                    "type": "coordinatorBoardAnswer",
                    "key": key,
                    "item": tag,
                    "label": label,
                    "text": format!("{tag}: {label}"),
                },
            })
        };
        let offered: Vec<&str> = item["answers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let buttons: Vec<Value> = if offered.is_empty() {
            vec![button("Done")]
        } else {
            offered.iter().map(|label| button(label)).collect()
        };
        // The words of a Reply sent from here, which its Reply button shows until gxserver
        // takes the row away (a pick is marked on its button instead).
        let replied = picked
            .filter(|picked| *picked != "Done" && !offered.contains(picked))
            .unwrap_or_default();
        let is_snoozed = super::work_cards::snoozed(item, &key, state, now_ms);
        let snooze = super::work_cards::snooze_choices(
            |hours| json!({ "type": "snoozeWorkAsk", "key": key, "item": tag, "ask": "", "hours": hours }),
        );
        let mut row = json!({
            "key": key,
            "kind": "ask",
            "needsYou": true,
            "tag": tag,
            "status": "needs",
            "title": title,
            "titleRuns": crate::questions::form_links::word_runs(title),
            "body": body(item),
            // A to-do with a detail shows Details on its row, its answers in the panel
            // (`work_cards::details_fields`); its command chip stays on the row.
            "details": super::work_cards::details_fields(text(item, "detail")).0,
            "preview": super::work_cards::details_fields(text(item, "detail")).1,
            "command": text(item, "command"),
            "age": crate::transcript::message_time::age_label(now_ms, at),
            "detail": [],
            "buttons": buttons,
            "busy": picked.is_some(),
            "replied": replied,
            "reply": { "item": tag, "key": key, "tag": "", "name": reply_name(title) },
            "mention": "",
            // Done: a to-do asks no question, so it is not Dismiss. It sends the
            // agent nothing; gxserver tells it with its next message
            // (server/src/session_chat_dismissed.rs, where the quiet-dismiss decision lives).
            "dismiss": {
                "type": "coordinatorBoardAnswer",
                "op": "cancel",
                "key": key,
                "item": tag,
                "label": "Dismissed",
            },
            "dismissLabel": "Done",
            "snooze": snooze,
            "chips": [],
            "needs": true,
        });
        row["showInChat"] = show_in_chat(turns, at, &key);
        if is_snoozed {
            let back = super::work_cards::snooze_end(item, &key, state);
            row["snooze"] = Value::Null;
            row["age"] = json!(super::work_cards::back_in(back, now_ms));
            row["unsnooze"] = json!({
                "type": "unsnoozeWorkAsk",
                "key": key,
                "item": tag,
                "ask": "",
            });
            snoozed_rows.push((back, row));
        } else {
            live.push((at, row));
        }
    }
    (live, snoozed_rows)
}

/// Recently answered to-dos for the Inbox's "Answered (N)" drawer (Q4684 A).
pub fn answered_rows(
    state: &ChatState,
    turns: &[Turn],
    now_ms: i64,
    kept_ms: i64,
    show_in_chat: impl Fn(&[Turn], i64, &str) -> Value,
) -> Vec<(i64, Value)> {
    let Some(items) = state
        .session
        .needs_you
        .as_ref()
        .and_then(|value| value["settled"].as_array())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| matches!(text(item, "outcome"), "answered" | "done"))
        .filter_map(|item| {
            let at = at_ms(text(item, "at"))?;
            if now_ms - at > kept_ms {
                return None;
            }
            let tag = text(item, "tag");
            let key = format!("answered:needs-you:{tag}");
            let outcome = match text(item, "answer") {
                "" => "Done",
                answer => answer,
            };
            let mut row = json!({
                "key": key,
                "kind": "answered",
                "title": text(item, "title"),
                "outcome": outcome,
                "age": crate::transcript::message_time::age_label(now_ms, at),
            });
            row["showInChat"] = show_in_chat(turns, at, &key);
            Some((at, row))
        })
        .collect()
}
