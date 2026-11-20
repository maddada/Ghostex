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

/// Whether the session has an open to-do, so the dock reads the chat's turns for it.
pub fn has_items(state: &ChatState) -> bool {
    state
        .session
        .needs_you
        .as_ref()
        .and_then(|value| value["items"].as_array())
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

/// The rows, each with when it was asked (for the dock's order).
pub fn rows(
    state: &ChatState,
    context: &ChatContext,
    turns: &[Turn],
    now_ms: i64,
    show_in_chat: impl Fn(&[Turn], i64, &str) -> Value,
    reply_name: impl Fn(&str) -> String,
) -> Vec<(i64, Value)> {
    let _ = context;
    let Some(items) = state
        .session
        .needs_you
        .as_ref()
        .and_then(|value| value["items"].as_array())
    else {
        return Vec::new();
    };
    let answers = &state.extras.panels.board_answers;
    items
        .iter()
        .filter(|item| !text(item, "tag").is_empty())
        .map(|item| {
            let tag = text(item, "tag");
            let title = text(item, "title");
            let key = format!("needs-you:{tag}");
            let picked = answers.get(&key).map(String::as_str);
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
            let mut row = json!({
                "key": key,
                "kind": "ask",
                "needsYou": true,
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
                // Mark as read: a to-do asks no question, so it is not Dismiss. It sends the
                // agent nothing; gxserver tells it with its next message
                // (server/src/session_chat_dismissed.rs, where the decision lives).
                "dismiss": {
                    "type": "coordinatorBoardAnswer",
                    "op": "cancel",
                    "key": key,
                    "item": tag,
                    "label": "Dismissed",
                },
                "dismissLabel": "Mark as read",
                "snooze": Value::Null,
                "chips": [],
                "needs": true,
            });
            row["showInChat"] = show_in_chat(turns, at, &key);
            (at, row)
        })
        .collect()
}
