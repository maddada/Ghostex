//! A live terminal dialog's rows read as numbered choices, the way the chat's question cards list
//! their options.
//!
//! CDXC:SessionChat 2026-10-15 DECISION:
//! User picked option D, "Numbered choices", for the terminal dialog cards (docs/2026-10-15/chat-visual-options/dialogs.html): every row of a live terminal dialog is a numbered row like the question cards' (press its number or click it), the row highlighted in the terminal is marked, and rarely used actions (homepage, repository, back to the list) fold into a "More" row. This replaces, for cards backed by a live dialog, the 2026-09-27 collapsed layout ("make buttons appear full width and on top of each other", notice.rs): those cards open with every row showing.
//! SEE-ALSO: apps/desktop/src/app/native_chat/notice.rs, apps/mobile/app/src/chat/native/cards/NoticeCard.tsx, server/src/session_chat_claude_dialog.rs.

use serde_json::{json, Map, Value};

use crate::questions::model::TerminalDialog;

/// The leading glyph a row keeps its state in: ticked (`◉`, `☒`, `✔`) or not.
const MARKS: [(char, bool); 9] = [
    ('✔', true),
    ('✓', true),
    ('◉', true),
    ('☒', true),
    ('◯', false),
    ('○', false),
    ('☐', false),
    ('●', false),
    ('+', false),
];

/// Navigation rows a dialog lists after its real choices: they leave the dialog or open
/// something beside it, so they wait behind "More".
fn is_rarely_used(title: &str) -> bool {
    ["Open ", "View ", "Back to ", "Switch to "]
        .iter()
        .any(|lead| title.starts_with(lead))
}

/// A row's own words split for the card: the name, a bracketed suffix ("user scope"), the stats
/// after its first " · ", and whether its mark says it is ticked.
fn parts(label: &str) -> (String, Option<String>, Option<String>, bool) {
    let mut text = label.trim();
    let mut ticked = false;
    if let Some((mark, on)) = MARKS
        .iter()
        .find(|(mark, _)| text.starts_with(*mark) && text[mark.len_utf8()..].starts_with(' '))
    {
        ticked = *on;
        text = text[mark.len_utf8()..].trim_start();
    }
    let text = text.trim_matches(|c: char| c == '✻' || c.is_whitespace());
    let (name, meta) = match text.split_once(" · ") {
        Some((name, meta)) => (name.trim(), Some(meta.trim().to_string())),
        None => (text, None),
    };
    let name = name.trim_matches(|c: char| c == '✻' || c.is_whitespace());
    // The agent's own key hint ("(y)", "(esc)") is not a suffix: the number badge is the row's
    // only hint (`without_key_hint`).
    let name = crate::questions::notice_choices::without_key_hint(name);
    let (name, suffix) = match name
        .strip_suffix(')')
        .and_then(|head| head.rsplit_once(" ("))
    {
        Some((title, suffix)) if !title.is_empty() => (title.trim(), Some(suffix.to_string())),
        _ => (name, None),
    };
    (name.to_string(), suffix, meta, ticked)
}

/// A "Press y to confirm or n to cancel" dialog's two keys as its numbered Yes and No rows. Neither
/// is the Cmd+Enter row (the notice has no primary choice), because Yes is often the destructive
/// one (removing a marketplace uninstalls its plugins).
fn key_rows(object: &mut Map<String, Value>, dialog: &TerminalDialog) {
    if !(dialog.actions.iter().any(|action| action == "yes")
        && dialog.actions.iter().any(|action| action == "no"))
    {
        return;
    }
    let row = |position: usize, title: &str, action: &str| {
        json!({
            "index": position,
            "label": title,
            "collapsedLabel": title,
            "selected": false,
            "number": position + 1,
            "title": title,
            "suffix": null,
            "meta": null,
            "detail": null,
            "ticked": false,
            "highlighted": false,
            "folded": false,
            "answer": { "kind": "terminalDialog", "dialogId": dialog.id, "dialogAction": action },
        })
    };
    object.insert(
        "choices".to_string(),
        json!([row(0, "Yes", "yes"), row(1, "No", "no")]),
    );
    object.insert("primaryChoice".to_string(), Value::Null);
    object.insert("secondaryChoice".to_string(), Value::Null);
    object.insert("numbered".to_string(), json!(true));
}

/// Adds `number`, `title`, `suffix`, `meta`, `detail`, `ticked`, `highlighted` and `folded` to each
/// choice of a notice backed by a live dialog, marks the notice `numbered`, and describes its
/// "More" row. The highlighted row never folds.
pub(crate) fn decorate(object: &mut Map<String, Value>, dialog: &TerminalDialog) {
    // The `/resume` picker draws its own list card (terminal_picker.rs).
    if dialog.picker.is_some() {
        return;
    }
    if dialog.rows.is_empty() {
        key_rows(object, dialog);
        return;
    }
    let Some(choices) = object.get_mut("choices").and_then(Value::as_array_mut) else {
        return;
    };
    let mut described: Vec<(String, bool)> = Vec::with_capacity(choices.len());
    for (position, choice) in choices.iter_mut().enumerate() {
        let row = choice["index"]
            .as_u64()
            .and_then(|index| dialog.rows.get(index as usize));
        let label = row.map_or_else(
            || choice["label"].as_str().unwrap_or_default().to_string(),
            |row| row.label.clone(),
        );
        let (title, suffix, meta, ticked) = parts(&label);
        let highlighted = row.map_or(choice["selected"] == true, |row| row.selected);
        let Some(fields) = choice.as_object_mut() else {
            continue;
        };
        fields.insert("number".to_string(), json!(position + 1));
        fields.insert("title".to_string(), json!(title));
        fields.insert("suffix".to_string(), json!(suffix));
        fields.insert("meta".to_string(), json!(meta));
        fields.insert(
            "detail".to_string(),
            json!(row.and_then(|row| row.description.clone())),
        );
        fields.insert("ticked".to_string(), json!(ticked));
        fields.insert("highlighted".to_string(), json!(highlighted));
        described.push((title, highlighted));
    }
    let foldable: Vec<bool> = described
        .iter()
        .map(|(title, highlighted)| !highlighted && is_rarely_used(title))
        .collect();
    let folded_count = foldable.iter().filter(|fold| **fold).count();
    let fold = folded_count >= 2 && folded_count < described.len();
    let mut folded_titles = Vec::new();
    for ((choice, folds), (title, _)) in choices.iter_mut().zip(&foldable).zip(&described) {
        let folds = fold && *folds;
        if folds {
            folded_titles.push(title.clone());
        }
        if let Some(fields) = choice.as_object_mut() {
            fields.insert("folded".to_string(), json!(folds));
        }
    }
    object.insert("numbered".to_string(), json!(true));
    if fold {
        object.insert(
            "more".to_string(),
            json!({ "label": "More", "detail": folded_titles.join(", ") }),
        );
    } else {
        object.remove("more");
    }
}
