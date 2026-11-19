//! The cards that used to stack between the dock and the chat box and are rows of the dock now:
//! the session's computer being offline (Retry) and messages held behind a failed model change
//! (Pick model). Added to `workDock` after the document's other families ran, since the held
//! messages' card is the menus family's (`menus/picker/hold_card.rs`).
//!
//! CDXC:SessionChat 2026-11-20 DECISION:
//! User: "pls let's not show multiple bars like this above the dock chat box; the dock/dock bar should be the only one there", then, through their coordinator, move the other bars that stack above the chat box into the dock as the design page's table proposed, keeping in place only what must block typing. The offline computer's card (CDXC:RemoteMachines 2026-11-08 in session/computer.rs: its words, "last seen", Retry and the checking spinner stay) and the held-messages card are dock rows, first after the approvals; the dock stays up for them. A row's `{"type":"hostAction","action":…}` command is the renderer's own (Pick model opens the composer's model menu), never sent to the core.
//! SEE-ALSO: extras/dock_approvals.rs, apps/desktop/src/app/native_chat/work_actions.rs (`hostAction`), apps/mobile/app/src/chat/native/cards/WorkCards.tsx (`onHostAction`).

use serde_json::{json, Value};

use crate::document::Document;
use crate::state::ChatState;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The computer's row and the held messages' row, from what the document already says.
fn rows(document: &Document) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(notice) = document.computer_notice.as_ref() {
        out.push(json!({
            "key": "computer",
            "kind": "computer",
            "status": "waiting",
            "title": notice.title,
            // "last seen 2h ago", or "checking…" while it is being checked.
            "age": notice.detail.clone().unwrap_or_default(),
            "detail": [],
            "buttons": if notice.checking {
                json!([])
            } else {
                json!([{ "label": "Retry", "recommended": false, "command": { "type": "computerRetry" } }])
            },
            "busy": notice.checking,
            "needs": false,
            "stays": true,
        }));
    }
    if let Some(card) = document.model_hold_card.as_deref() {
        let body = [text(card, "body"), text(card, "detail"), text(card, "note")]
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        out.push(json!({
            "key": "model-hold",
            "kind": "modelHold",
            "status": "blocked",
            "title": text(card, "title"),
            "body": body,
            "detail": [],
            "age": "",
            "buttons": [{
                "label": text(&card["action"], "label"),
                "recommended": false,
                "command": { "type": "hostAction", "action": text(&card["action"], "id") },
            }],
            "needs": false,
            "stays": true,
        }));
    }
    out
}

/// The keys of the rows this adds now, which collapsing the dock remembers with the others
/// (`HideWorkDock`), so they keep it collapsed until something new arrives.
pub fn keys(state: &ChatState, context: &crate::state::ChatContext) -> Vec<String> {
    let mut keys = Vec::new();
    if crate::session::computer::notice(state, context).is_some() {
        keys.push("computer".to_string());
    }
    if crate::menus::picker::hold_card::messages_held(state) {
        keys.push("model-hold".to_string());
    }
    keys
}

/// Puts the computer's and the held messages' rows into `workDock`, right after its approvals.
pub fn add(state: &ChatState, document: &mut Document) {
    let rows = rows(document);
    if rows.is_empty() {
        return;
    }
    let hidden_keys = state.extras.panels.work_dock_hidden.as_ref();
    // A row the reader has not collapsed the dock over opens it, as any new row does.
    let news = rows
        .iter()
        .any(|row| !hidden_keys.is_some_and(|keys| keys.contains(text(row, "key"))));
    let (mut dock, was_hidden) = match &document.work_dock {
        ghostex_gx_protocol::Tri::Value(dock) if dock.is_object() => {
            (dock.clone(), dock["hidden"] == true)
        }
        _ => (
            json!({
                "rows": [],
                "hidden": false,
                "limit": 4,
                "phoneLimit": 3,
                "more": { "type": "showCoordinatorBoardGroup", "group": "needs" },
                "markAllSeen": null,
                "snoozed": [],
            }),
            hidden_keys.is_some(),
        ),
    };
    let existing: Vec<Value> = dock["rows"].as_array().cloned().unwrap_or_default();
    let lead = existing
        .iter()
        .take_while(|row| matches!(text(row, "kind"), "approval" | "approvalSettled"))
        .count();
    let mut merged: Vec<Value> = existing[..lead].to_vec();
    merged.extend(rows);
    merged.extend(existing[lead..].iter().cloned());
    dock["rows"] = Value::Array(merged);
    dock["hidden"] = json!(was_hidden && !news);
    document.work_dock = ghostex_gx_protocol::Tri::Value(dock);
}
