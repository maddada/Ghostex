//! The cards that used to stack between the dock and the chat box and are rows of the dock now:
//! the session's computer being offline (Retry), messages held behind a failed model change
//! (Pick model), and Ghostex Collab's held messages (the owner's "Message from <guest>" with
//! Decline, Edit and Send; a guest's own "Waiting for <owner>"), whose pictures and form answers
//! are in the panel the row opens. Added to `workDock` after the document's other families ran, since the held
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

/// The first line of a held message's words, cut, for its row's title.
fn first_line(words: &str) -> String {
    let line = words
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut cut: String = line.chars().take(120).collect();
    if cut.len() < line.len() || words.trim().lines().count() > 1 {
        cut.push('\u{2026}');
    }
    cut
}

/// What a held Collab message says, for its row's title: its words, or the form it answers.
fn held_gist(entry: &Value) -> String {
    let words = text(entry, "text");
    if !words.trim().is_empty() {
        return format!("\u{201c}{}\u{201d}", first_line(words));
    }
    match text(&entry["answers"], "title") {
        "" => "a picture".to_string(),
        title => format!("answers to {title}"),
    }
}

/// A held Collab message whole, for the panel its row opens: the words, the pictures and each
/// form's answers, as the chat's Markdown (its pictures drawn as the chat draws them).
fn held_body(entry: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let words = text(entry, "text").trim();
    if !words.is_empty() {
        parts.push(words.to_string());
    }
    let pictures: Vec<String> = entry["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|image| {
            [text(image, "path"), text(image, "url")]
                .into_iter()
                .find(|source| !source.is_empty())
                .map(|source| format!("![]({source})"))
        })
        .collect();
    if !pictures.is_empty() {
        parts.push(pictures.join("\n"));
    }
    let answers = entry["answers"]
        .is_object()
        .then_some(&entry["answers"])
        .into_iter()
        .chain(entry["extraAnswers"].as_array().into_iter().flatten());
    for card in answers.filter(|card| card.is_object()) {
        let mut lines = vec![format!("**{}**", text(card, "title"))];
        for row in card["rows"].as_array().into_iter().flatten() {
            let label = text(row, "label");
            let value = if row["skipped"] == true {
                "Skipped"
            } else {
                text(row, "value")
            };
            lines.push(if label.is_empty() {
                format!("- {value}")
            } else {
                format!("- {label}: {value}")
            });
        }
        parts.push(lines.join("\n"));
    }
    crate::transcript::native_markdown::native_markdown(&parts.join("\n\n"), false)
}

/// Ghostex Collab's rows: the owner's "Message from <guest>" with Decline, Edit and Send, and a
/// guest's own messages waiting for the owner.
fn collab_rows(collab: &Value) -> Vec<Value> {
    if collab["shared"] != true {
        return Vec::new();
    }
    let mut out = Vec::new();
    for card in collab["cards"].as_array().into_iter().flatten() {
        let id = text(card, "id");
        let button = |label: &str, action: &str| {
            json!({
                "label": label,
                "recommended": false,
                "command": { "type": "resolveCollabMessage", "heldId": id, "action": action },
            })
        };
        out.push(json!({
            "key": format!("collab:{id}"),
            "kind": "collab",
            "status": "needs",
            "title": format!("{}: {}", text(card, "title"), held_gist(card)),
            "body": held_body(card),
            "detail": [],
            "age": "",
            "buttons": [button("Decline", "decline"), button("Edit", "edit"), button("Send", "send")],
            "busy": card["busy"] == true,
            "needs": true,
            "stays": true,
        }));
    }
    for row in collab["waiting"].as_array().into_iter().flatten() {
        out.push(json!({
            "key": format!("collab-waiting:{}", text(row, "id")),
            "kind": "collabWaiting",
            "status": "waiting",
            "title": format!("{}: {}", text(collab, "waitingForOwner"), held_gist(row)),
            "body": held_body(row),
            "detail": [],
            "age": "",
            "buttons": [],
            "needs": false,
            "stays": true,
        }));
    }
    out
}

/// The computer's row, the held messages' row and Collab's rows, from what the document already
/// says.
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
    if let ghostex_gx_protocol::Tri::Value(collab) = &document.collab {
        out.extend(collab_rows(collab));
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
    let collab = crate::extras::collab::project(state);
    keys.extend(
        collab_rows(&collab)
            .iter()
            .map(|row| text(row, "key").to_string()),
    );
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
