//! Ghostex Collab in the chat: who is watching a shared chat, who is typing, the owner's "Message
//! from <guest>" cards with Decline / Edit / Send, and on a guest's page their own messages
//! waiting for the owner and what their chat box may do.
//!
//! gxserver puts `collab` on every chat frame (server/src/collab_shim/mod.rs, from the Collab component): `{ shared: false }`,
//! or `{ shared, shareId, mode, owner, ownerTyping, viewers: [{ id, name, typing }], held: [{ id,
//! viewerId, name, text, atMs }] }`. A guest's page says it is one once, with `setCollabGuest`
//! (`{ viewerId, name }`, apps/gpui-web/src/app/collab_guest.rs); every other client is the owner.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1614): a new share starts in "approve each message (the owner gets a "Message from <guest>" card with Decline / Edit / Send)"; the other modes (straight to the agent, view only) are per chat. A guest "can never answer permission prompts" (Q1617), so a guest's chat shows "Waiting for <owner>" where the owner would answer.
//! SEE-ALSO: apps/desktop/src/app/native_chat/collab.rs (desktop and web drawing), server/collab/src/owner.rs (the calls).

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// How often a typing chat box tells gxserver, which shows "is typing" for four seconds.
const TYPING_EVERY_MS: f64 = 2_000.0;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The chat box's picture references (`[Image #1](/path/to/picture.png)`, what a paste writes)
/// taken out of `text`: the words left, and the pictures' paths in order.
fn split_image_references(text: &str) -> (String, Vec<String>) {
    let mut words = String::new();
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[Image #") {
        let after = &rest[start + "[Image #".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let tail = &after[digits..];
        let close = (digits > 0 && tail.starts_with("]("))
            .then(|| tail[2..].find(')'))
            .flatten();
        let Some(close) = close else {
            words.push_str(&rest[..start + 1]);
            rest = &rest[start + 1..];
            continue;
        };
        words.push_str(&rest[..start]);
        paths.push(tail[2..2 + close].to_string());
        rest = &tail[2 + close + 1..];
    }
    words.push_str(rest);
    (words.trim().to_string(), paths)
}

/// A held message's words, without the picture references its pictures replace.
fn held_text(row: &Value) -> String {
    split_image_references(text(row, "text")).0
}

/// A held message's pictures (its attachments, then the ones its words reference), in the
/// transcript's own image source shape, so the renderers draw them with the chat's own thumbnails.
fn held_images(row: &Value) -> Value {
    let referenced = split_image_references(text(row, "text")).1;
    Value::Array(
        row.get("imagePaths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .chain(referenced)
            .map(|path| {
                crate::transcript::images::image_source(&crate::transcript::images::ImageRef {
                    path: Some(path),
                    url: None,
                    alt: None,
                })
            })
            .collect(),
    )
}

fn shared(state: &ChatState) -> Option<&Value> {
    state
        .session
        .collab
        .as_ref()
        .filter(|collab| collab.get("shared").and_then(Value::as_bool) == Some(true))
}

/// The chat owner's name as guests see it.
pub fn owner_name(state: &ChatState) -> String {
    let owner = shared(state)
        .map(|collab| text(collab, "owner"))
        .unwrap_or_default();
    if owner.is_empty() {
        "The owner".to_string()
    } else {
        owner.to_string()
    }
}

/// A held message that carries a form's or page's answers, as the answers card the transcript
/// draws for them (`pageAnswers`), with what else it says (`answersText`). `by` names who answered
/// when it is not the reader.
fn held_answers(state: &ChatState, row: &Value, by: Option<&str>) -> Option<(Value, String)> {
    let (answers, rest) = crate::transcript::page_answers::split_page_answers(text(row, "text"))?;
    let form = state
        .transcript_view
        .question_forms
        .iter()
        .find(|form| form.id == answers.page_id);
    let mut card = crate::transcript::page_answers::project_page_answers(&answers, form);
    if let (Some(by), Some(object)) = (by, card.as_object_mut()) {
        object.insert("by".to_string(), json!(by));
    }
    Some((card, split_image_references(&rest).0))
}

/// On a guest's page, a form whose answers the owner held follows what the owner did with them
/// (the chat frame's `collab.resolved`): sent, they go on to the agent like any answers; declined
/// or taken to edit, the form opens again with the answers picked and says why.
///
/// CDXC:Collab 2026-11-07 WHY: a held answer is not on its way to the agent, so the form read "Sending…" and its echo kept the chat "working" until the owner acted. The form now reads "Waiting for <owner>" with no echo while held.
pub fn settle_held_answers(state: &mut ChatState) {
    let Some(collab) = shared(state) else {
        return;
    };
    let resolved: Vec<(String, String)> = collab
        .get("resolved")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            (
                text(row, "id").to_string(),
                text(row, "outcome").to_string(),
            )
        })
        .collect();
    if resolved.is_empty() {
        return;
    }
    let owner = owner_name(state);
    let mut changed = false;
    for entry in state.questions.forms.values_mut() {
        let Some(held_id) = entry.held_for_owner.as_deref() else {
            continue;
        };
        let Some((_, outcome)) = resolved.iter().find(|(id, _)| id == held_id) else {
            continue;
        };
        entry.held_for_owner = None;
        changed = true;
        if outcome == "sent" {
            continue;
        }
        entry.sent = None;
        entry.confirmation_boundary = None;
        entry.error = Some(if outcome == "edited" {
            format!("{owner} took your answers to edit them before sending.")
        } else {
            format!("{owner} declined your answers.")
        });
    }
    if changed {
        state.messages.new_composition_identity();
        state.core.request_publish();
    }
}

/// On a guest's page, the answers to form `form_id` this guest sent that still wait for the
/// owner (their held messages, matched by name: a reloaded page joins again with a new viewer id).
pub fn own_held_answers(state: &ChatState, form_id: &str) -> Option<Value> {
    let me = guest_name(state)?;
    shared(state)?
        .get("held")
        .and_then(Value::as_array)?
        .iter()
        .filter(|row| text(row, "name") == me)
        .filter_map(|row| crate::transcript::page_answers::split_page_answers(text(row, "text")))
        .find(|(answers, _)| answers.page_id == form_id)
        .map(|(answers, _)| answers.answers)
}

/// This page is a guest's (`setCollabGuest`).
pub fn is_guest(state: &ChatState) -> bool {
    state.session.collab_guest.is_some()
}

fn guest_name(state: &ChatState) -> Option<&str> {
    state
        .session
        .collab_guest
        .as_ref()
        .map(|guest| text(guest, "name"))
}

fn names_line(names: &[String], one: &str, many: &str) -> Option<String> {
    match names {
        [] => None,
        [only] => Some(format!("{only} {one}")),
        [first, second] => Some(format!("{first} and {second} {many}")),
        [first, rest @ ..] => Some(format!("{first} and {} others {many}", rest.len())),
    }
}

/// The document's `collab`: `null` before gxserver said anything, `{ shared: false, projectId,
/// sessionId }` while the chat is not shared, else what the renderers draw: `{ shared: true,
/// projectId, sessionId, shareId, role, owner, mode, viewers: [name], watching, typing, cards,
/// waiting, guestNote, viewOnly }`.
pub fn project(state: &ChatState) -> Value {
    if !available(state) {
        return Value::Null;
    }
    if !running(state) {
        // Share shows its Install (the popover and the phone's sheet read `installed`).
        let collab = state.session.collab.as_ref();
        return json!({
            "shared": false,
            "available": false,
            "installed": collab.and_then(|collab| collab.get("installed")).cloned().unwrap_or(Value::Bool(false)),
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        });
    }
    let Some(collab) = shared(state) else {
        // The phone's Share sheet makes the share from these.
        return json!({
            "shared": false,
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
        });
    };
    let guest = is_guest(state);
    let me = guest_name(state).unwrap_or_default();
    let owner = text(collab, "owner");
    let owner = if owner.is_empty() { "The owner" } else { owner };
    let mode = text(collab, "mode");
    let viewers: Vec<&Value> = collab
        .get("viewers")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().collect())
        .unwrap_or_default();
    let viewer_names: Vec<String> = viewers
        .iter()
        .map(|viewer| {
            let name = text(viewer, "name");
            if guest && name == me {
                format!("{name} (you)")
            } else {
                name.to_string()
            }
        })
        .collect();
    let mut typing: Vec<String> = viewers
        .iter()
        .filter(|viewer| viewer.get("typing").and_then(Value::as_bool) == Some(true))
        .map(|viewer| text(viewer, "name").to_string())
        .filter(|name| !(guest && name == me))
        .collect();
    if guest && collab.get("ownerTyping").and_then(Value::as_bool) == Some(true) {
        typing.insert(0, owner.to_string());
    }
    let held: Vec<&Value> = collab
        .get("held")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().collect())
        .unwrap_or_default();
    let busy: Vec<&str> = state
        .extras
        .panels
        .collab_requests
        .iter()
        .map(|(_, held_id)| held_id.as_str())
        .collect();
    let cards: Vec<Value> = if guest {
        Vec::new()
    } else {
        held.iter()
            .map(|row| {
                let answers = held_answers(state, row, Some(text(row, "name")));
                json!({
                    "id": text(row, "id"),
                    "title": format!("Message from {}", text(row, "name")),
                    "text": answers.as_ref().map_or_else(|| held_text(row), |(_, rest)| rest.clone()),
                    "answers": answers.map(|(card, _)| card),
                    "images": held_images(row),
                    "busy": busy.contains(&text(row, "id")),
                })
            })
            .collect()
    };
    let waiting: Vec<Value> = if guest {
        held.iter()
            .map(|row| {
                let name = text(row, "name");
                let answers = held_answers(state, row, (name != me).then_some(name));
                json!({
                    "id": text(row, "id"),
                    "label": if name == me {
                        format!("{name} (you) \u{b7} Waiting for {owner}")
                    } else {
                        format!("{name} \u{b7} Waiting for {owner}")
                    },
                    "text": answers.as_ref().map_or_else(|| held_text(row), |(_, rest)| rest.clone()),
                    "answers": answers.map(|(card, _)| card),
                    "images": held_images(row),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let guest_note = guest.then(|| match mode {
        "send" => format!("Message as {me}. It goes straight to the agent."),
        "view" => format!("{owner} shared this chat to watch only."),
        _ => format!("Message as {me}. {owner} approves each message."),
    });
    json!({
        "shared": true,
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "shareId": text(collab, "shareId"),
        "role": if guest { "guest" } else { "owner" },
        "owner": owner,
        "mode": mode,
        "viewers": viewer_names,
        "viewerCount": viewers.len(),
        "watching": names_line(&viewer_names, "is watching", "are watching"),
        "typing": names_line(&typing, "is typing", "are typing"),
        "cards": cards,
        "waiting": waiting,
        "guestNote": guest_note,
        "viewOnly": guest && mode == "view",
        "waitingForOwner": format!("Waiting for {owner}"),
    })
}

/// The pill's id in `chatPills`.
pub const COLLAB_PILL: &str = "collab";

/// Whether Share shows for this chat: gxserver said something about Collab, and either Collab
/// runs here or it doesn't but the owner didn't pick "Don't show Share" (`available: false` with
/// `hideShare`, server/src/collab_shim/mod.rs). Without Collab, Share opens the popover's Install.
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (Q1782): "Show it until you dismiss it": Share shows before Collab is installed, its popover offers Install and "Don't show Share".
fn available(state: &ChatState) -> bool {
    state.session.collab.as_ref().is_some_and(|collab| {
        collab.get("available") != Some(&Value::Bool(false))
            || collab.get("hideShare") != Some(&Value::Bool(true))
    })
}

/// Collab runs on this computer (not just Share showing its Install).
fn running(state: &ChatState) -> bool {
    state
        .session
        .collab
        .as_ref()
        .is_some_and(|collab| collab.get("available") != Some(&Value::Bool(false)))
}

/// The phone's Share pill: "Share", or "Shared" with how many are watching; it opens the phone's
/// Share sheet. Only on a touch composer (`StartConfig::touch_composer`): the desktop and the web
/// build have the header's Share button instead, so a chat never shows both. `None` before
/// gxserver said anything about Collab (an older gxserver) and on a guest's page.
///
/// CDXC:Collab 2026-11-07 DECISION: User (Q1616): the phone is "full (share, approve messages, Stop sharing, see viewers)"; the phone has no chat header button row, so its Share is this pill.
pub fn pill(state: &ChatState) -> Option<crate::document::ChatPill> {
    let touch = state
        .session
        .boot_config
        .as_ref()
        .is_some_and(|config| config.touch_composer);
    if !touch || is_guest(state) || !available(state) {
        return None;
    }
    let collab = state.session.collab.as_ref()?;
    let watching = collab
        .get("viewers")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let shared = collab.get("shared").and_then(Value::as_bool) == Some(true);
    Some(crate::document::ChatPill {
        id: COLLAB_PILL.to_string(),
        label: if shared {
            "Shared".to_string()
        } else {
            "Share".to_string()
        },
        count: if shared && watching > 0 {
            watching.to_string()
        } else {
            String::new()
        },
        aria_label: if shared {
            "Shared through Ghostex Collab: the link, the code, who is watching and Stop sharing"
                .to_string()
        } else {
            "Share this chat through a link".to_string()
        },
        ..crate::document::ChatPill::default()
    })
}

/// `setCollabGuest`: this page is a Ghostex Collab guest's (`{ viewerId, name }`).
pub fn set_guest(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    *state.session.collab_guest = Some(json!({
        "viewerId": action.param("viewerId").cloned().unwrap_or(Value::Null),
        "name": action.param("name").cloned().unwrap_or(Value::Null),
    }));
    state.core.request_publish();
    Vec::new()
}

/// `resolveCollabMessage`: the owner's Send, Edit or Decline on a guest's message.
pub fn resolve(state: &mut ChatState, action: &UserAction, request_id: u64) -> Vec<Effect> {
    if is_guest(state) {
        return Vec::new();
    }
    let held_id = action
        .param("heldId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let choice = action
        .param("action")
        .and_then(Value::as_str)
        .unwrap_or("decline")
        .to_string();
    if held_id.is_empty()
        || state
            .extras
            .panels
            .collab_requests
            .iter()
            .any(|(_, id)| *id == held_id)
    {
        return Vec::new();
    }
    state
        .extras
        .panels
        .collab_requests
        .push((request_id, held_id.clone()));
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::Collab,
        params: Box::new(json!({
            "op": "resolve",
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "heldId": held_id,
            "action": choice,
        })),
    }]
}

/// gxserver's answer to [`resolve`]: Edit puts the guest's words into the chat box; a refusal
/// says why. `None` when `request_id` is not one of these.
///
/// CDXC:Collab 2026-11-08 DECISION:
/// User (#495): sending a guest's message needs the owner's signed approval. Send answers `approvalRequired`; the request shows on the computer and the phone, and gxserver sends the message itself once it is approved, so the card stays until then and the chat only says where to approve.
pub fn settle(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
    code: Option<&str>,
) -> Option<Vec<Effect>> {
    if state.extras.panels.collab_typing_request == Some(request_id) {
        state.extras.panels.collab_typing_request = None;
        return Some(Vec::new());
    }
    let index = state
        .extras
        .panels
        .collab_requests
        .iter()
        .position(|(id, _)| *id == request_id)?;
    state.extras.panels.collab_requests.remove(index);
    state.core.request_publish();
    Some(match answer {
        Ok(result) => match result.get("composerText").and_then(Value::as_str) {
            Some(words) => vec![Effect::SetComposerText {
                content: words.to_string(),
                caret: Some(words.chars().count()),
                from_history: false,
            }],
            None => Vec::new(),
        },
        Err(_) if code == Some("approvalRequired") => vec![Effect::Toast {
            level: "info".to_string(),
            message: "Approve sending it on this computer or your phone.".to_string(),
        }],
        Err(message) => vec![Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                "Couldn't answer that message.".to_string()
            } else {
                message
            },
        }],
    })
}

/// While the chat is shared, a chat box being typed into tells gxserver every couple of seconds,
/// and once more when it is emptied, so everyone sees "<name> is typing".
pub fn typing(state: &mut ChatState, text_now: &str, context: &ChatContext) -> Vec<Effect> {
    if shared(state).is_none() {
        return Vec::new();
    }
    let typing = !text_now.trim().is_empty();
    let panels = &mut state.extras.panels;
    let due = if typing {
        context.now_ms - panels.collab_typing_at >= TYPING_EVERY_MS
    } else {
        panels.collab_typing_on
    };
    if !due {
        return Vec::new();
    }
    panels.collab_typing_at = if typing { context.now_ms } else { 0.0 };
    panels.collab_typing_on = typing;
    let request_id = state.core.allocate_request_id();
    state.extras.panels.collab_typing_request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::CollabTyping,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "typing": typing,
        })),
    }]
}

#[cfg(test)]
mod held_answers_tests {
    use super::*;

    const ANSWERS: &str = "<ghostex-page-answers page=\"0123456789abcdef0123456789abcdef\" title=\"Receipt layout\">\n{\n  \"Which layout?\": \"compact\"\n}\n</ghostex-page-answers>";

    /// Held answers wait as the answers card, not as the message that carries them.
    #[test]
    fn held_answers_show_as_an_answers_card() {
        let mut state = ChatState::default();
        *state.session.collab_guest = Some(json!({"viewerId": "v1", "name": "Sam"}));
        *state.session.collab = Some(json!({
            "shared": true, "owner": "Mohamad", "mode": "approve", "viewers": [],
            "held": [
                {"id": "held_1", "viewerId": "v1", "name": "Sam", "text": ANSWERS},
                {"id": "held_2", "viewerId": "v2", "name": "Dana", "text": "Hello"},
            ],
        }));
        let waiting = project(&state)["waiting"].clone();
        assert_eq!(waiting[0]["answers"]["title"], "Receipt layout");
        assert!(
            waiting[0]["answers"].get("by").is_none(),
            "the reader's own answers"
        );
        assert_eq!(waiting[0]["text"], "");
        assert!(waiting[1]["answers"].is_null());
        assert_eq!(waiting[1]["text"], "Hello");

        *state.session.collab_guest = None;
        let cards = project(&state)["cards"].clone();
        assert_eq!(cards[0]["answers"]["by"], "Sam");
    }
}
