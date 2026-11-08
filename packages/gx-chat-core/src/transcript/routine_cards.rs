//! The "Routine set" card: a GBot set a routine (`ghostex gbots routine add`), and the chat that
//! asked for it shows it in one card with Pause, Edit and Undo. It rides the "GBot added" card's
//! pipeline (gbot_cards.rs): the same card line, the same per-change state read from gxserver's
//! change journal, the same Undo; this file holds what differs.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 7): the card says the routine was set, what it runs on in plain words ("Set up: every month on the 1st at 09:00") and when it runs next ("Next: Tue 1 Dec, 09:00"), with Pause, Edit and Undo. Pause turns into Resume while it is paused; Edit opens the routine where it can be changed (the Automate view on the desktop, the GBot's Routines on the phone); Undo removes it, and is offered only while it would work.
//! SEE-ALSO: server/src/gbots/routines.rs (the card line, pause, Undo), apps/desktop/src/app/native_chat/gbot_card.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/GbotCard.tsx (phone).

use ghostex_gx_protocol::chat_card::ROUTINE_SET;
use serde_json::{json, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::transcript::gbot_cards::GbotCardEntry;

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// "Every 15 minutes" → "every 15 minutes", keeping a leading name or zone as written.
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if text.starts_with("Every") || text.starts_with("Once") => {
            first.to_lowercase().chain(chars).collect()
        }
        _ => text.to_string(),
    }
}

/// The card's fixed words, from the card line `ghostex gbots routine add` printed.
pub fn routine_card(card: &Value) -> Value {
    let name = str_of(card, "name");
    let mut body = format!("Set up: {}.", lower_first(str_of(card, "scheduleSentence")));
    let check = str_of(card, "precheckSentence");
    if !check.is_empty() {
        body.push_str(&format!(
            " It checks {check} first and runs only when that changes."
        ));
    }
    json!({
        "kind": ROUTINE_SET,
        "changeId": str_of(card, "changeId"),
        "routineId": str_of(card, "routineId"),
        "botId": str_of(card, "botId"),
        "name": name,
        "icon": str_of(card, "icon"),
        "colorHex": str_of(card, "colorHex"),
        "title": format!("Routine set: {name}"),
        "removedTitle": format!("Routine removed: {name}"),
        "body": body,
    })
}

/// What `gbotCardStates` says of a routine card: the GBot card's fields plus `{ routine: true,
/// next, paused, pauseOp, pauseLabel, pauseEnabled, editEnabled }`.
pub fn card_state(entry: &GbotCardEntry) -> Value {
    let change = entry.change.as_ref();
    let undone = change.is_some_and(|change| change.get("undoneAt").is_some_and(Value::is_string));
    let undoable = change.is_some_and(|change| change.get("undoable") == Some(&Value::Bool(true)));
    let routine = change
        .and_then(|change| change.get("routine"))
        .filter(|routine| routine.is_object());
    let paused = routine.is_some_and(|routine| routine.get("paused") == Some(&Value::Bool(true)));
    let note = if undone {
        "Undone. The routine was removed.".to_string()
    } else if change.is_some() && !undoable {
        change
            .and_then(|change| change.get("undoProblem"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };
    let next = match routine {
        None => String::new(),
        Some(_) if undone => String::new(),
        Some(routine) if paused => {
            let held = routine.pointer("/precheck/kind").and_then(Value::as_str) == Some("command")
                && str_of(routine, "createdBy") != "user";
            if held {
                "Paused until you turn it on: it runs a command on its own.".to_string()
            } else {
                "Paused.".to_string()
            }
        }
        Some(routine) => match routine.get("nextLabel").and_then(Value::as_str) {
            Some(next) => format!("Next: {next}"),
            None => "No more runs.".to_string(),
        },
    };
    let failed_check = routine
        .filter(|routine| {
            routine
                .pointer("/check/lastCheckOutcome")
                .and_then(Value::as_str)
                == Some("failed")
        })
        .and_then(|routine| routine.pointer("/check/lastCheckNote"))
        .and_then(Value::as_str)
        .map(|note| format!("Its last check failed: {note}"))
        .unwrap_or_default();
    let live = routine.is_some() && !undone;
    let busy = entry.loading || entry.undoing || entry.pausing;
    json!({
        "routine": true,
        "loading": entry.loading,
        "undone": undone,
        "note": note,
        "next": next,
        "checkNote": failed_check,
        "paused": paused,
        "pauseOp": if paused { "resume" } else { "pause" },
        "pauseLabel": match (entry.pausing, paused) {
            (true, true) => "Resuming…",
            (true, false) => "Pausing…",
            (false, true) => "Resume",
            (false, false) => "Pause",
        },
        "pauseEnabled": live && !busy,
        "editEnabled": live,
        "undoLabel": if entry.undoing { "Undoing…" } else { "Undo" },
        "undoEnabled": undoable && !busy,
        "settingsEnabled": false,
        "error": entry.error,
    })
}

/// Edit: the host opens the routine where it can be changed, on the chat's own computer.
pub fn edit(state: &ChatState, action: &UserAction) -> Vec<Effect> {
    let param = |key: &str| {
        action
            .param(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (bot_id, routine_id) = (param("botId"), param("routineId"));
    if bot_id.is_empty() || routine_id.is_empty() {
        return Vec::new();
    }
    let machine_id = state
        .session
        .computer
        .computer
        .as_ref()
        .filter(|computer| computer.remote)
        .and_then(|computer| computer.machine_id.clone());
    let project_id = state
        .transcript_view
        .gbot_cards
        .get(&param("changeId"))
        .and_then(|entry| entry.change.as_ref())
        .and_then(|change| change.pointer("/routine/projectId"))
        .cloned()
        .unwrap_or(Value::Null);
    vec![Effect::HostAction {
        action: "editGbotRoutine".to_string(),
        params: Box::new(json!({
            "botId": bot_id,
            "routineId": routine_id,
            "projectId": project_id,
            "machineId": machine_id,
        })),
    }]
}
