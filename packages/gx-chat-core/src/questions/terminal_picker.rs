//! The `/resume` picker card and the pass-through card: what the chat draws for an agent screen
//! that owns the keyboard, and the answer each key sends to it.
//!
//! CDXC:SessionChat 2026-10-11 DECISION: User: `/resume` must be usable from the chat. Claude Code's and Codex's pickers are MIRRORED as a list card that takes the keyboard (Up/Down move, Enter picks, Esc cancels, typing filters) and sends every key to the agent's real picker; every other agent's input-owning screen (and Codex's own unreadable ones) is PASSED THROUGH: the chat shows that part of the terminal live and the card takes the chat's keyboard, letters, digits, the four arrows, Enter and Esc, even over a composer with text, until the agent closes the screen or the user gives the keyboard back from the card (the user's words are in server/src/session_chat_resume_picker.rs).
//! SEE-ALSO: server/src/session_chat_resume_picker.rs, apps/desktop/src/app/native_chat/terminal_picker.rs, apps/mobile/app/src/chat/native/cards/TerminalPickerCard.tsx.

use serde_json::{json, Map, Value};

use crate::questions::model::{TerminalDialog, TerminalNotice};

/// The keys a picker card hands to the agent, in the order the phone draws them as buttons.
const PICKER_KEYS: [&str; 6] = ["up", "down", "enter", "escape", "pageUp", "pageDown"];
const PASSTHROUGH_KEYS: [&str; 6] = ["up", "down", "left", "right", "enter", "escape"];

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Whether an answer only moves through an open picker or screen. Such answers go out without
/// holding the card: the user presses the next key before gxserver has repainted the last one.
pub(crate) fn is_navigation_answer(answer: &Value) -> bool {
    answer.get("pickerKey").is_some()
        || answer.get("pickerQuery").is_some()
        || answer.get("kind").and_then(Value::as_str) == Some("terminalKey")
}

/// The picker card for a dialog that carries a resume picker, or `None`.
pub fn picker_presentation(dialog: &TerminalDialog) -> Option<Value> {
    let picker = dialog.picker.as_ref()?;
    let answer = |extra: Value| {
        let mut answer = Map::new();
        answer.insert("kind".to_string(), json!("terminalDialog"));
        answer.insert("dialogId".to_string(), json!(dialog.id));
        if let Value::Object(extra) = extra {
            answer.extend(extra);
        }
        Value::Object(answer)
    };
    let mut previous_group: Option<&str> = None;
    let rows: Vec<Value> = picker
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, row)| {
            let title = str_field(row, "title");
            let group = row.get("group").and_then(Value::as_str);
            // A project heading is drawn once, above the first row under it.
            let heading = (group.is_some() && group != previous_group).then_some(group);
            previous_group = group;
            json!({
                "title": title,
                "detail": row.get("detail").and_then(Value::as_str),
                "heading": heading.flatten(),
                "selected": row.get("selected").and_then(Value::as_bool).unwrap_or(false),
                "answer": answer(json!({ "pickerRow": { "index": index, "title": title } })),
            })
        })
        .collect();
    let keys: Map<String, Value> = PICKER_KEYS
        .iter()
        .map(|key| (key.to_string(), answer(json!({ "pickerKey": key }))))
        .collect();
    let fork = dialog.title.starts_with("Fork");
    let agent = str_field(picker, "agent");
    let query = str_field(picker, "query");
    let empty = rows.is_empty();
    Some(json!({
        "title": if fork { "Fork a conversation" } else { "Resume a conversation" },
        "agent": agent,
        "query": query,
        "searchFocused": picker.get("searchFocused").and_then(Value::as_bool).unwrap_or(false),
        "placeholder": "Type to search conversations",
        "position": picker.get("position").and_then(Value::as_str),
        "message": picker.get("message").and_then(Value::as_str).or(empty.then_some({
            if query.is_empty() {
                "No conversations to resume here."
            } else {
                "No conversations match."
            }
        })),
        "hint": if fork {
            "↑ ↓ to move · Enter to fork · Esc to cancel"
        } else {
            "↑ ↓ to move · Enter to resume · Esc to cancel"
        },
        "rows": rows,
        "keys": keys,
        "queryAnswer": answer(json!({ "pickerQuery": "" })),
    }))
}

/// Lines of the agent's screen the pass-through card shows: its picker sits at the bottom.
const PASSTHROUGH_SCREEN_LINES: usize = 16;

/// The bottom of the screen tail, with the wide gaps a terminal uses to right-align a hint closed
/// up, so the card shows the picker without scrolling or wrapping a 200-column row.
fn passthrough_screen(tail: &str) -> String {
    let lines: Vec<&str> = tail.lines().collect();
    let start = lines.len().saturating_sub(PASSTHROUGH_SCREEN_LINES);
    lines[start..]
        .iter()
        .map(|line| {
            let mut out = String::with_capacity(line.len());
            let mut spaces = 0;
            for c in line.trim_end().chars() {
                if c == ' ' {
                    spaces += 1;
                    continue;
                }
                out.extend(std::iter::repeat_n(
                    ' ',
                    if spaces >= 8 { 4 } else { spaces },
                ));
                spaces = 0;
                out.push(c);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/*
CDXC:SessionChat 2026-11-26 DECISION:
User: what stays in the terminal shows only "Open terminal": OMP's card "OMP is waiting for tool
approval" has no keys, no text field and no live screen region.

CDXC:SessionChat 2026-11-26 WHY:
Each renderer draws a `passthrough` object as the pass-through card, with its own key buttons and
the keyboard taken, even when the object carries no keys (apps/desktop native_chat/notice.rs and
terminal_picker.rs). So OMP's tool approval gets no `passthrough` object at all: its card is the
notice's own, with Open terminal (its action) and the screen's tail only behind "Show terminal
output". gxserver still marks the notice `passthrough` (server/src/session_chat_notice/types.rs
`passes_keys_through`), so this core still treats it as owning the input: a hook card's Allow and
Deny stay hidden beside it (`gates::question_card_visible`, extras/relayed_approvals.rs), and a
core that still draws its keys (upstream 56b2016cd) keeps hiding them too; gxserver refuses those
keys. Its place below the Inbox dock stays (`notice_tone::is_error_terminal_notice`).
*/
/// The pass-through card for a notice gxserver cannot read row by row, or `None`, also for OMP's
/// tool approval that the chat leaves to the terminal.
pub fn passthrough_presentation(notice: &TerminalNotice) -> Option<Value> {
    if !notice.passthrough || notice.is_omp_tool_approval() {
        return None;
    }
    let keys: Map<String, Value> = PASSTHROUGH_KEYS
        .iter()
        .map(|key| {
            (
                key.to_string(),
                json!({ "kind": "terminalKey", "key": key }),
            )
        })
        .collect();
    Some(json!({
        "screen": passthrough_screen(notice.screen_tail.as_deref().unwrap_or_default()),
        "hint": "This card has the keyboard: letters, numbers, arrows, Enter and Esc go to the agent's terminal.",
        "releaseLabel": "Return keyboard to chat",
        "keys": keys,
        // The renderer puts the typed character in `text`.
        "textAnswer": { "kind": "terminalKey", "text": "" },
    }))
}
