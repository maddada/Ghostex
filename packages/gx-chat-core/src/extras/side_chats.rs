//! Side chats (server/src/side_chats/): the Side chats pill of a main session's chat and its menu,
//! the "Side chat of" bar of a side chat's own chat, and their Open, Bring back and Delete.
//!
//! CDXC:SideChats 2026-11-02 DECISION:
//! User: side chats show as one of the pills in the pills strip of the chat box (beside Resources, Workers and the links), not as sessions in the sidebar. The pill counts the session's side chats and opens a menu that lists them; a row opens that side chat (in the desktop's side panel, as its own chat screen on the phone), and Bring back and Delete act on it. A side chat's own chat says whose side chat it is, with Bring back and Delete. The pill leads the strip's core pills because it stands for the conversation itself.
//! SEE-ALSO: apps/desktop/src/app/native_chat/side_chats.rs and chat_pills.rs (desktop and web), apps/mobile/app/src/chat/native/cards/SideChats.tsx and ChatPills.tsx (phone).

use serde_json::{json, Value};

use crate::document::ChatPill;
use crate::effect::Effect;
use crate::state::{ChatState, PanelsState};
use crate::wire::ChatRpcMethod;

/// The Side chats pill's id.
pub const SIDE_CHATS_PILL: &str = "sideChats";

/// The side chat call in flight, and the side chat it names (`None`: this chat's own session).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideChatCall {
    BringBack(Option<String>),
    Delete(Option<String>),
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key) == Some(&Value::Bool(true))
}

/// The pill's menu: `{ rows, count, working, waiting, meta }`, oldest side chat first, or `null`
/// while the session has none.
pub fn project(side_chats: Option<&Value>) -> Value {
    let rows = side_chats
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|row| !text(row, "sessionId").is_empty())
        .map(|row| {
            let waiting = if flag(row, "needsApproval") {
                "approval"
            } else if flag(row, "needsAnswer") {
                "question"
            } else {
                ""
            };
            json!({
                "projectId": text(row, "projectId"),
                "sessionId": text(row, "sessionId"),
                "title": match text(row, "title") {
                    "" => "Side chat",
                    title => title,
                },
                "working": flag(row, "working"),
                "waiting": waiting,
                "unread": flag(row, "unread"),
                "sleeping": text(row, "lifecycleState") != "running",
            })
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Value::Null;
    }
    let working = rows.iter().filter(|row| flag(row, "working")).count();
    let waiting = if rows.iter().any(|row| text(row, "waiting") == "approval") {
        "approval"
    } else if rows.iter().any(|row| text(row, "waiting") == "question") {
        "question"
    } else {
        ""
    };
    let count = rows.len();
    let mut meta = format!("{count} side {}", if count == 1 { "chat" } else { "chats" });
    if working > 0 {
        meta.push_str(&format!(" · {working} working"));
    }
    json!({
        "rows": rows,
        "count": count,
        "working": working,
        "waiting": waiting,
        "meta": meta,
    })
}

/// The Side chats pill, while the menu has a row.
pub fn pill(panel: &Value) -> Option<ChatPill> {
    if !panel.is_object() {
        return None;
    }
    let meta = text(panel, "meta").to_string();
    Some(ChatPill {
        id: SIDE_CHATS_PILL.to_string(),
        label: "Side chats".to_string(),
        count: panel
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string(),
        waiting: text(panel, "waiting").to_string(),
        running: panel.get("working").and_then(Value::as_u64).unwrap_or(0) > 0,
        aria_label: format!("Side chats, {meta}"),
        meta,
    })
}

/// A side chat's own bar: `{ parentSessionId, parentTitle }`, or `null` for any other chat.
pub fn bar(side_chat_of: Option<&Value>) -> Value {
    let Some(parent) = side_chat_of.filter(|parent| !text(parent, "sessionId").is_empty()) else {
        return Value::Null;
    };
    json!({
        "parentSessionId": text(parent, "sessionId"),
        "parentTitle": match text(parent, "title") {
            "" => "the main session",
            title => title,
        },
    })
}

/// Whether this chat is a side chat, which cannot start another one.
pub fn is_side_chat(side_chat_of: Option<&Value>) -> bool {
    !bar(side_chat_of).is_null()
}

/// A row was picked: the host opens that side chat (the desktop's side panel, the phone's chat
/// screen). A sleeping one wakes when its chat opens, like any session.
pub fn open(project_id: &str, session_id: &str) -> Option<Effect> {
    let (project_id, session_id) = (project_id.trim(), session_id.trim());
    if project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    Some(Effect::HostAction {
        action: "openSideChat".to_string(),
        params: Box::new(json!({ "projectId": project_id, "sessionId": session_id })),
    })
}

/// Bring back or Delete of `session_id` (from the main chat's menu), or of this chat itself when
/// it is the side chat (`None`).
pub fn call(panels: &mut PanelsState, call: SideChatCall, request_id: u64) -> Effect {
    let (method, target) = match &call {
        SideChatCall::BringBack(target) => (ChatRpcMethod::BringBackSideChat, target.clone()),
        SideChatCall::Delete(target) => (ChatRpcMethod::DeleteSideChat, target.clone()),
    };
    panels.side_chat_request = Some((request_id, call));
    let mut params = json!({});
    if let Some(target) = target.filter(|target| !target.trim().is_empty()) {
        params["sideChatSessionId"] = json!(target.trim());
    }
    Effect::SendRpc {
        request_id,
        method,
        params: Box::new(params),
    }
}

/// The session's side chat list as this chat last knew it, changed at once by a start or a delete
/// it made: a main session that sleeps streams no frames, so its pill would wait for the next read.
fn edit_list(state: &mut ChatState, edit: impl FnOnce(&mut Vec<Value>)) {
    let mut rows = state
        .session
        .side_chats
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    edit(&mut rows);
    state.session.side_chats = Some(Value::Array(rows));
}

/// A side chat this chat just started joins its list.
pub fn note_started(state: &mut ChatState, result: &Value) {
    let Some(session) = result.pointer("/fork/session") else {
        return;
    };
    let session_id = text(session, "sessionId").to_string();
    if session_id.is_empty() {
        return;
    }
    let row = json!({
        "projectId": text(session, "projectId"),
        "sessionId": session_id,
        "title": text(session, "title"),
        "working": true,
        "lifecycleState": "running",
    });
    edit_list(state, |rows| {
        if !rows.iter().any(|row| text(row, "sessionId") == session_id) {
            rows.push(row);
        }
    });
}

/// Takes the answer to [`call`]: a toast for Bring back, the host closing the side chat's tab for
/// Delete. `None` when `request_id` is not that call.
pub fn settle(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    let panels = &mut state.extras.panels;
    if panels.side_chat_request.as_ref().map(|(id, _)| *id) != Some(request_id) {
        return None;
    }
    let (_, call) = panels.side_chat_request.take()?;
    if let (SideChatCall::Delete(_), Ok(result)) = (&call, &answer) {
        let deleted = text(result, "sessionId").to_string();
        edit_list(state, |rows| {
            rows.retain(|row| text(row, "sessionId") != deleted)
        });
    }
    let effect = match (call, answer) {
        (SideChatCall::BringBack(_), Ok(_)) => Effect::Toast {
            level: "info".to_string(),
            message: "Brought back to the main session.".to_string(),
        },
        (SideChatCall::Delete(_), Ok(result)) => Effect::HostAction {
            action: "sideChatDeleted".to_string(),
            params: Box::new(json!({
                "projectId": text(result, "projectId"),
                "sessionId": text(result, "sessionId"),
                "parentSessionId": text(result, "parentSessionId"),
            })),
        },
        (call, Err(message)) => Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                match call {
                    SideChatCall::BringBack(_) => "Could not bring back that side chat.",
                    SideChatCall::Delete(_) => "Could not delete that side chat.",
                }
                .to_string()
            } else {
                message
            },
        },
    };
    Some(vec![effect])
}

/// The answer to a Side chat send (`startSideChat`): the host opens the new side chat.
pub fn opened_by_start(result: &Value) -> Option<Effect> {
    let session = result.pointer("/fork/session")?;
    open(text(session, "projectId"), text(session, "sessionId"))
}
