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

/// A guest's words as literal text in the chat's Markdown: every character Markdown would read
/// (a link, a picture, emphasis, a heading) escaped, so they are drawn as written and a guest's
/// `![](https://…)` never loads a picture.
fn literal(words: &str) -> String {
    let mut out = String::with_capacity(words.len());
    for ch in words.chars() {
        if "\\`*_[](){}#+-.!<>|~".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// A held Collab message whole, for the panel its row opens: the words as plain text, its own
/// uploaded pictures (`images`, gx-chat-core `extras::collab` keeps only those), and each form's
/// answers, also as plain text.
fn held_body(entry: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let words = text(entry, "text").trim();
    if !words.is_empty() {
        parts.push(words.lines().map(literal).collect::<Vec<_>>().join("  \n"));
    }
    let pictures: Vec<String> = entry["images"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|image| text(image, "path"))
        .filter(|path| !path.is_empty())
        .map(|path| format!("![]({path})"))
        .collect();
    let mut body = crate::transcript::native_markdown::native_markdown(&pictures.join("\n"), false);
    let answers = entry["answers"]
        .is_object()
        .then_some(&entry["answers"])
        .into_iter()
        .chain(entry["extraAnswers"].as_array().into_iter().flatten());
    for card in answers.filter(|card| card.is_object()) {
        let mut lines = vec![format!("**{}**", literal(text(card, "title")))];
        for row in card["rows"].as_array().into_iter().flatten() {
            let label = literal(text(row, "label"));
            let value = if row["skipped"] == true {
                "Skipped".to_string()
            } else {
                literal(text(row, "value"))
            };
            lines.push(if label.is_empty() {
                format!("\\- {value}")
            } else {
                format!("\\- {label}: {value}")
            });
        }
        parts.push(lines.join("  \n"));
    }
    if !body.is_empty() {
        parts.insert(parts.len().min(1), std::mem::take(&mut body));
    }
    parts.join("\n\n")
}

/// What a held message says, for its row's block: its words, then each form's answers ("Answers to
/// <title>:" and every "question: answer"), so the owner reads all that Send delivers on the row
/// itself (security review of the dock rows, T2).
fn held_words(entry: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let words = text(entry, "text").trim();
    if !words.is_empty() {
        parts.push(words.to_string());
    }
    let answers = entry["answers"]
        .is_object()
        .then_some(&entry["answers"])
        .into_iter()
        .chain(entry["extraAnswers"].as_array().into_iter().flatten());
    for card in answers.filter(|card| card.is_object()) {
        let mut lines = vec![format!("Answers to {}:", text(card, "title"))];
        for row in card["rows"].as_array().into_iter().flatten() {
            let value = if row["skipped"] == true {
                "Skipped".to_string()
            } else {
                text(row, "value").to_string()
            };
            let mut line = match text(row, "label") {
                "" => value,
                label => format!("{label}: {value}"),
            };
            if let Some(note) = row["note"].as_str().filter(|note| !note.trim().is_empty()) {
                line.push_str(&format!(" (note: {note})"));
            }
            lines.push(line);
        }
        parts.push(lines.join("\n"));
    }
    if parts.is_empty() {
        "(a picture)".to_string()
    } else {
        parts.join("\n\n")
    }
}

/// Ghostex Collab's rows: the owner's "Message from <guest>" with Decline, Edit and Send, and a
/// guest's own messages waiting for the owner. The whole message shows on the row as plain text
/// (its first rows, then "Show N more lines"), so Send is never pressed on a first line alone.
fn collab_rows(collab: &Value) -> Vec<Value> {
    if collab["shared"] != true {
        return Vec::new();
    }
    let block = |entry: &Value| {
        let words = held_words(entry);
        let (preview, more) = crate::extras::dock_approvals::block_preview(&words);
        json!({
            "command": words,
            "commandLines": true,
            // Drawn in the chat's own type, copied as a message (renderers' command block).
            "commandPlain": true,
            "commandPreview": preview,
            "commandMore": more,
        })
    };
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
        let mut row = json!({
            "key": format!("collab:{id}"),
            "kind": "collab",
            "status": "needs",
            "title": text(card, "title"),
            "body": held_body(card),
            "detail": [],
            "age": "",
            "buttons": [button("Decline", "decline"), button("Edit", "edit"), button("Send", "send")],
            "busy": card["busy"] == true,
            "needs": true,
            "stays": true,
        });
        merge(&mut row, block(card));
        out.push(row);
    }
    for entry in collab["waiting"].as_array().into_iter().flatten() {
        let mut row = json!({
            "key": format!("collab-waiting:{}", text(entry, "id")),
            "kind": "collabWaiting",
            "status": "waiting",
            "title": text(collab, "waitingForOwner"),
            "body": held_body(entry),
            "detail": [],
            "age": "",
            "buttons": [],
            "needs": false,
            "stays": true,
        });
        merge(&mut row, block(entry));
        out.push(row);
    }
    out
}

fn merge(row: &mut Value, more: Value) {
    if let (Some(row), Value::Object(more)) = (row.as_object_mut(), more) {
        row.extend(more);
    }
}

/// The computer's row, the held messages' row and Collab's rows, from what the document already
/// says.
fn rows(state: &ChatState, document: &Document) -> Vec<Value> {
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
    // Read from the state, not the document: the document's `collab` is written after this runs
    // (`document::assemble`), and a held message must never be drawn nowhere.
    out.extend(collab_rows(&crate::extras::collab::project(state)));
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
    let rows = rows(state, document);
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
                "undoMarkAllRead": null,
                "done": [],
                "doneArchiveAll": null,
                "snoozed": [],
                "answered": [],
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
