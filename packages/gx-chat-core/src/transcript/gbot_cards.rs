//! The "GBot added" card: Chief of Staff added a GBot on its own, and its chat says so in one card
//! with Undo and Open settings.
//!
//! `ghostex gbots create` run by Chief of Staff prints a card line (`ghostex-chat-card: {…}`,
//! packages/gx-protocol/src/chat_card.rs) into the command's output. A message's projection lists
//! the cards its tool calls printed (`gbotCards`, the words that never change, hoisted out of a
//! finished turn's fold like sent messages), and the document's `gbotCardStates` holds what can
//! change, by change id: whether the add was undone, the bot removed or changed since, an Undo in
//! flight or refused. A card with no state yet asks for it once (`gbotCard` with `op: load`), which
//! reads gxserver's change journal (`/api/gbots` `change`), so a reload, another computer or the
//! phone all show the same state.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2103 A, mockup "Researcher added"): the card's title says the bot was added, one line says what it is and what it runs on, and its buttons are Undo and Open settings; after Undo it reads "Researcher removed" and "Undone". Undo is offered only while it would work (gxserver refuses it once the bot changed since), and the reason shows instead.
//! SEE-ALSO: server/src/gbots/card.rs and changes.rs (the card line and Undo), apps/desktop/src/app/native_chat/gbot_card.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/GbotCard.tsx (phone).

use ghostex_gx_protocol::chat_card::{chat_cards_in, GBOT_ADDED, ROUTINE_SET};
use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::transcript::agent_message_card::agent_display_name;
use crate::transcript::tool_fold::ToolPair;
use crate::wire::{ChatRpcMethod, RpcOutcome};

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The "GBot added" cards one tool call printed, with their fixed words.
pub fn gbot_cards(pair: &ToolPair<'_>) -> Vec<Value> {
    let Some(output) = pair.result_output() else {
        return Vec::new();
    };
    // A card line is printed only after its add succeeded, so a command that failed later (several
    // adds in one script, the last refused) still shows the cards of the adds before it.
    if !output.contains("chatCard") && !output.contains("chat-card") {
        return Vec::new();
    }
    chat_cards_in(output)
        .into_iter()
        .filter(|card| {
            matches!(str_of(card, "kind"), GBOT_ADDED | ROUTINE_SET)
                && !str_of(card, "changeId").is_empty()
        })
        .map(|card| {
            if str_of(&card, "kind") == ROUTINE_SET {
                return crate::transcript::routine_cards::routine_card(&card);
            }
            let name = str_of(&card, "name");
            let agent = agent_display_name(str_of(&card, "agentId"), "");
            let role = str_of(&card, "title").trim().trim_end_matches('.');
            let body = match (role.is_empty(), agent.is_empty()) {
                (false, false) => format!("{role}. It runs on {agent}."),
                (false, true) => format!("{role}."),
                (true, false) => format!("It runs on {agent}."),
                (true, true) => String::new(),
            };
            json!({
                "kind": GBOT_ADDED,
                "changeId": str_of(&card, "changeId"),
                "botId": str_of(&card, "botId"),
                "name": name,
                "icon": str_of(&card, "icon"),
                "colorHex": str_of(&card, "colorHex"),
                "title": format!("{name} added"),
                "removedTitle": format!("{name} removed"),
                "body": body,
            })
        })
        .collect()
}

/// What `gbotCardStates` says of one card: `{ loading, undone, note, undoLabel, undoEnabled,
/// settingsEnabled, error }`.
fn card_state(entry: &GbotCardEntry) -> Value {
    let change = entry.change.as_ref();
    if change.is_some_and(|change| str_of(change, "kind").starts_with("routine")) {
        return crate::transcript::routine_cards::card_state(entry);
    }
    let undone = change.is_some_and(|change| change.get("undoneAt").is_some_and(Value::is_string));
    let bot_exists = change.is_some_and(|change| change.get("bot").is_some_and(Value::is_object));
    let undoable = change.is_some_and(|change| change.get("undoable") == Some(&Value::Bool(true)));
    let note = if undone {
        "Undone. Its chats were closed; you can still find them in Previous Sessions.".to_string()
    } else if change.is_some() && !undoable {
        change
            .and_then(|change| change.get("undoProblem"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };
    json!({
        "loading": entry.loading,
        "undone": undone,
        "note": note,
        "undoLabel": if entry.undoing { "Undoing…" } else { "Undo" },
        "undoEnabled": undoable && !entry.undoing && !entry.loading,
        "settingsEnabled": bot_exists && !undone,
        "error": entry.error,
    })
}

/// One card's live state, by change id (`ChatState.transcript_view.gbot_cards`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GbotCardEntry {
    /// The change as gxserver last answered it (`/api/gbots` `change`).
    pub change: Option<Value>,
    pub loading: bool,
    pub undoing: bool,
    /// A routine card's Pause or Resume is on its way.
    pub pausing: bool,
    /// Why the last Undo or read failed.
    pub error: String,
}

/// The document's `gbotCardStates`.
pub fn card_states(state: &ChatState) -> std::collections::BTreeMap<String, Value> {
    state
        .transcript_view
        .gbot_cards
        .iter()
        .map(|(id, entry)| (id.clone(), card_state(entry)))
        .collect()
}

/// `{type: "gbotCard", changeId, op: "load" | "undo" | "openSettings" | "pause" | "resume" |
/// "edit", botId?, routineId?}`; the last three are a "Routine set" card's (routine_cards.rs).
pub fn act(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let change_id = str_of(&Value::Object(action.params.clone()), "changeId")
        .trim()
        .to_string();
    if change_id.is_empty() {
        return Vec::new();
    }
    let op = action
        .param("op")
        .and_then(Value::as_str)
        .unwrap_or("load")
        .to_string();
    if op == "openSettings" {
        let bot_id = action
            .param("botId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if bot_id.is_empty() {
            return Vec::new();
        }
        // The bot lives on the chat's own computer; `machineId` is absent for this one.
        let machine_id = state
            .session
            .computer
            .computer
            .as_ref()
            .filter(|computer| computer.remote)
            .and_then(|computer| computer.machine_id.clone());
        return vec![Effect::HostAction {
            action: "openGbotSettings".to_string(),
            params: Box::new(json!({ "botId": bot_id, "machineId": machine_id })),
        }];
    }
    if op == "edit" {
        return crate::transcript::routine_cards::edit(state, action);
    }
    let entry = state
        .transcript_view
        .gbot_cards
        .entry(change_id.clone())
        .or_default();
    if entry.loading || entry.undoing || entry.pausing {
        return Vec::new();
    }
    let params = match op.as_str() {
        // A card asks once; a later frame never asks again on its own.
        "load" if entry.change.is_none() && entry.error.is_empty() => {
            entry.loading = true;
            json!({ "op": "change", "change": change_id })
        }
        "undo" => {
            entry.undoing = true;
            entry.error.clear();
            json!({ "op": "undo", "change": change_id })
        }
        "pause" | "resume" => {
            let routine_id = action
                .param("routineId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if routine_id.is_empty() {
                return Vec::new();
            }
            entry.pausing = true;
            entry.error.clear();
            json!({
                "op": if op == "pause" { "routinePause" } else { "routineResume" },
                "routine": routine_id,
                "change": change_id,
            })
        }
        _ => return Vec::new(),
    };
    let request_id = state.core.allocate_request_id();
    state
        .transcript_view
        .gbot_card_requests
        .insert(request_id, change_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::Other("gbots".to_string()),
        params: Box::new(params),
    }]
}

/// The answer to an [`act`] call; `false` when `request_id` is not one.
pub fn settle_rpc(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> bool {
    let Some(change_id) = state.transcript_view.gbot_card_requests.remove(&request_id) else {
        return false;
    };
    // The card shows its own refusal; the composer's error line stays clear.
    state.core.claim_refusal();
    let entry = state
        .transcript_view
        .gbot_cards
        .entry(change_id)
        .or_default();
    let undoing = entry.undoing;
    let pausing = entry.pausing;
    entry.loading = false;
    entry.undoing = false;
    entry.pausing = false;
    match outcome {
        RpcOutcome::Ok { result } => {
            if let Some(change) = result.get("change").filter(|change| change.is_object()) {
                entry.change = Some(change.clone());
                entry.error.clear();
            }
        }
        RpcOutcome::Err { code, message, .. } => {
            if undoing && code.as_deref() == Some("alreadyUndone") {
                // Undone elsewhere meanwhile (another computer, the phone): the card says so.
                let mut change = entry.change.clone().unwrap_or_else(|| json!({}));
                change["undoneAt"] = json!("elsewhere");
                change["undoable"] = json!(false);
                entry.change = Some(change);
                entry.error.clear();
                return true;
            }
            entry.error = if message.trim().is_empty() {
                if pausing {
                    "That didn't go through. Try again.".to_string()
                } else if undoing {
                    "Undo didn't go through. Try again.".to_string()
                } else {
                    "Couldn't read this GBot's state.".to_string()
                }
            } else {
                message.clone()
            };
        }
    }
    true
}
