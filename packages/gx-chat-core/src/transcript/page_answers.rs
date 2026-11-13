//! Answers a reader sent from an agent's HTML page (`ghostex.submit(...)` in a page published with
//! `ghostex show`), as gxserver types them into the publishing session:
//!
//! ```text
//! <ghostex-page-answers page="<32 hex id>" title="Open questions">
//! { …the answers as JSON… }
//! </ghostex-page-answers>
//! ```
//!
//! The chat draws that user message as an answers card instead of a bubble of JSON, and marks the
//! page's own card Answered (`answered_page_ids`, the document's `answeredPages`).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The envelope is written by server/src/visual_pages/answers.rs; the cards are drawn by apps/desktop/src/app/native_chat/page_answers.rs and visual.rs, and on the phone by apps/mobile/app/src/chat/native/transcript/PageAnswersCard.tsx and VisualBlock.tsx. Change them together.

use crate::questions::form_links::{plain_words, word_runs};
use ghostex_gx_protocol::ChatRole;
use serde_json::{json, Value};

use crate::transcript::jsstr::js_trim;
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_forms::{FormQuestion, QuestionForm};

const OPEN: &str = "<ghostex-page-answers";
const CLOSE: &str = "</ghostex-page-answers>";
/// Rows the card lists before it summarises the rest.
const MAX_ROWS: usize = 40;
const MAX_VALUE_CHARS: usize = 600;
/// How the question kit (server/src/visual_pages/kit.rs) names a question's note.
const NOTE_SUFFIX: &str = " (note)";
/// Between a question's key and a choice in the note a page keeps under that choice's mockup.
const OPTION_SEPARATOR: &str = " \u{b7} ";

/// One answered page, as its message carries it.
#[derive(Clone, Debug, PartialEq)]
pub struct PageAnswers {
    pub page_id: String,
    pub title: String,
    pub answers: Value,
    /// The envelope's `tags`, one per answer row key (`-` where none), when gxserver knew the
    /// page's questions (`ghostex_gx_visual::questions::page_answers_message_tagged`); empty for
    /// a form (its tags are on the form) and for answers sent before tags were written.
    pub tags: Vec<String>,
}

fn attribute(head: &str, name: &str) -> Option<String> {
    let start = head.find(&format!("{name}=\""))? + name.len() + 2;
    let end = head[start..].find('"')? + start;
    Some(
        head[start..end]
            .replace("&quot;", "\"")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

fn is_page_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The answers envelope a user message holds and the text written around it (empty when the
/// message is only the envelope).
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// The envelope is not always the whole message: answers that went out while Claude could not
/// take them came back into its input box, Ghostex handed them to the chat's composer, and the
/// user's next message carried them with their own words below. The chat draws the answers card
/// for the envelope and the rest as the user's text, and the form still reads Answered.
pub fn split_page_answers(text: &str) -> Option<(PageAnswers, String)> {
    let open = text.find(OPEN)?;
    let close = text[open..].find(CLOSE)? + open + CLOSE.len();
    let answers = parse_page_answers(&text[open..close])?;
    let before = js_trim(&text[..open]);
    let after = js_trim(&text[close..]);
    let rest = match (before.is_empty(), after.is_empty()) {
        (true, _) => after.to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{before}\n\n{after}"),
    };
    Some((answers, rest))
}

/// The page or question form a message's answers envelope answers, when it holds one.
pub fn answers_page_id(text: &str) -> Option<String> {
    if !text.contains(OPEN) {
        return None;
    }
    split_page_answers(text).map(|(answers, _)| answers.page_id)
}

/// The answers a message holds, when the whole message is one answers envelope.
pub fn parse_page_answers(text: &str) -> Option<PageAnswers> {
    let text = js_trim(text);
    let rest = text.strip_prefix(OPEN)?;
    let head_end = rest.find('>')?;
    let head = &rest[..head_end];
    let body = rest[head_end + 1..].strip_suffix(CLOSE)?;
    let page_id = attribute(head, "page").filter(|id| is_page_id(id))?;
    let title = attribute(head, "title")
        .map(|title| js_trim(&title).to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Ghostex page".to_string());
    let body = js_trim(body);
    let answers = serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_string()));
    let tags = attribute(head, "tags")
        .map(|tags| tags.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    Some(PageAnswers {
        page_id,
        title,
        answers,
        tags,
    })
}

/// The answers this transcript received, one per page or question form (the latest sent, where the
/// first one arrived).
pub fn page_answers_in<'a>(
    messages: impl IntoIterator<Item = &'a ghostex_gx_protocol::ChatMessage>,
) -> Vec<PageAnswers> {
    let mut found: Vec<PageAnswers> = Vec::new();
    for message in messages
        .into_iter()
        .filter(|message| message.role == ChatRole::User)
    {
        if let Some((answers, _)) = split_page_answers(&prose_markdown(&message.blocks)) {
            // A later answer to the same question replaces the earlier one in place: a one-click
            // answer changed after the agent read it (`questions/inline_answers.rs`).
            match found
                .iter_mut()
                .find(|known| known.page_id == answers.page_id)
            {
                Some(known) => *known = answers,
                None => found.push(answers),
            }
        }
    }
    found
}

fn clip(text: String) -> String {
    if text.chars().count() <= MAX_VALUE_CHARS {
        return text;
    }
    let mut clipped: String = text.chars().take(MAX_VALUE_CHARS).collect();
    clipped.push('…');
    clipped
}

/// A value as the card shows it: text as written, a list joined, anything else as compact JSON.
fn display_value(value: &Value) -> String {
    clip(match value {
        Value::String(text) if text == crate::transcript::question_forms::SKIPPED => {
            "Skipped".to_string()
        }
        Value::String(text) => text.clone(),
        Value::Null => "Not answered".to_string(),
        Value::Bool(true) => "Yes".to_string(),
        Value::Bool(false) => "No".to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(items)
            if items
                .iter()
                .all(|item| !item.is_object() && !item.is_array()) =>
        {
            items
                .iter()
                .map(display_value)
                .collect::<Vec<_>>()
                .join(", ")
        }
        other => other.to_string(),
    })
}

/// An answer as text that starts like a sentence: "no confirm" reads "No confirm". A first word
/// with capitals of its own ("macOS", "iOS") is a name and stays as written.
fn sentence_case(text: &str) -> String {
    let first_word = text.split_whitespace().next().unwrap_or_default();
    let mut characters = text.chars();
    match characters.next() {
        Some(first) if first.is_lowercase() && !first_word.chars().any(char::is_uppercase) => {
            first.to_uppercase().chain(characters).collect()
        }
        _ => text.to_string(),
    }
}

/// A picked value as the answers card shows it: the choice's own label when the form that asked is
/// known (an in-chat question form), else the value with a capital first letter. What the agent
/// received is unchanged.
///
/// CDXC:SessionChat 2026-10-11 DECISION:
/// User: the answers card shows answers starting with a capital letter ("No confirm", not "no confirm") on desktop and the phone, without changing what the agent receives; when the card knows the choice's label it shows the label.
fn display_answer(value: &Value, asked: Option<&FormQuestion>) -> String {
    let shown = |text: &str| {
        asked
            .and_then(|asked| asked.choices.iter().find(|choice| choice.value == text))
            .map(|choice| choice.label.clone())
            .unwrap_or_else(|| sentence_case(text))
    };
    match value {
        Value::String(text) => clip(shown(text)),
        Value::Array(items) if items.iter().all(Value::is_string) => clip(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(shown)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        other => display_value(other),
    }
}

/// A field name as a label: `preferred_name` and `preferredName` read as "Preferred name". A key
/// that is already words (the question kit keys answers by question title) is shown as written.
fn display_label(key: &str) -> String {
    if !key
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_' || character == '-')
    {
        return js_trim(key).to_string();
    }
    let mut words = String::new();
    let mut previous_lower = false;
    for character in key.chars() {
        if character == '_' || character == '-' {
            words.push(' ');
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = character.is_lowercase();
        words.extend(character.to_lowercase());
    }
    let words = js_trim(&words).to_string();
    let mut characters = words.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => key.to_string(),
    }
}

/// Which question's row a note belongs to: `"<key> (note)"` is that question's own note, and
/// `"<key> · <choice> (note)"` a page's note under one choice's mockup (the question kit,
/// server/src/visual_pages/kit.rs). `None` when the key is not a note of a question in `fields`.
fn note_owner<'a>(
    fields: &serde_json::Map<String, Value>,
    key: &'a str,
) -> Option<(&'a str, Option<&'a str>)> {
    let base = key.strip_suffix(NOTE_SUFFIX)?;
    if fields.contains_key(base) {
        return Some((base, None));
    }
    let (question, choice) = base.rsplit_once(OPTION_SEPARATOR)?;
    fields
        .contains_key(question)
        .then_some((question, Some(choice)))
}

/// The message's `pageAnswers` key: `{pageId, title, rows: [{label, value, note?}], more,
/// withText}`, `withText` when the message has the user's own words besides (drawn below the card
/// as an ordinary message). The
/// question kit sends a question's note as `"<key> (note)"`; it is shown under that question's
/// answer instead of as a row of its own.
pub fn project_page_answers(answers: &PageAnswers, form: Option<&QuestionForm>) -> Value {
    // A page's tags, by answer key, lined up with the keys gxserver tagged.
    let page_tags: Vec<(&str, &str)> = {
        let keys = ghostex_gx_visual::questions::answer_row_keys(&answers.answers);
        if keys.len() == answers.tags.len() {
            keys.into_iter()
                .zip(answers.tags.iter().map(String::as_str))
                .filter(|(_, tag)| *tag != "-")
                .collect()
        } else {
            Vec::new()
        }
    };
    let rows: Vec<Value> = match &answers.answers {
        Value::Object(fields) => fields
            .iter()
            .filter(|(key, _)| note_owner(fields, key).is_none())
            .map(|(key, value)| {
                let asked = form
                    .and_then(|form| form.questions.iter().find(|question| question.key == *key));
                // The form's own question title when the card knows the form, else the key.
                let label = asked.map_or_else(|| display_label(key), |asked| asked.title.clone());
                // A question answered only in its note (no pick) shows the note as the answer
                // rather than "Not answered" over it.
                let unpicked = value.is_null() || value.as_array().is_some_and(Vec::is_empty);
                let own_note = fields
                    .get(&format!("{key}{NOTE_SUFFIX}"))
                    .filter(|note| note.as_str().is_some_and(|note| !note.trim().is_empty()));
                let note_as_answer = unpicked.then_some(own_note).flatten();
                let tag = asked
                    .map(|asked| asked.tag.as_str())
                    .filter(|tag| !tag.is_empty())
                    .or_else(|| {
                        page_tags
                            .iter()
                            .find(|(tagged, _)| *tagged == key.as_str())
                            .map(|(_, tag)| *tag)
                    })
                    .unwrap_or_default();
                let skipped = note_as_answer.is_none()
                    && (value.is_null()
                        || value.as_array().is_some_and(Vec::is_empty)
                        || value.as_str()
                            == Some(crate::transcript::question_forms::SKIPPED));
                // The reader's own notes box ("Anything else?"), not a question.
                let extra = asked.is_none()
                    && tag.is_empty()
                    && key.starts_with(ghostex_gx_visual::questions::NOTES_KEY);
                let mut row = json!({
                    "label": plain_words(&label),
                    "value": plain_words(&match note_as_answer {
                        Some(note) => display_value(note),
                        None => display_answer(value, asked),
                    }),
                });
                if !tag.is_empty() {
                    row["tag"] = json!(tag);
                }
                if skipped {
                    row["skipped"] = json!(true);
                }
                if extra {
                    row["extra"] = json!(true);
                }
                // The question's links, drawn as links (`questions/form_links.rs`).
                if let Some(runs) = word_runs(&label) {
                    row["labelRuns"] = runs;
                }
                let notes: Vec<String> = fields
                    .iter()
                    .filter_map(|(note_key, note)| match note_owner(fields, note_key) {
                        Some((owner, None))
                            if owner == key.as_str() && note_as_answer.is_none() =>
                        {
                            Some(display_value(note))
                        }
                        Some((owner, Some(choice))) if owner == key.as_str() => Some(format!(
                            "{}: {}",
                            display_answer(&json!(choice), asked),
                            display_value(note)
                        )),
                        _ => None,
                    })
                    .collect();
                if !notes.is_empty() {
                    row["note"] = json!(notes.join("\n"));
                }
                row
            })
            .collect(),
        other => vec![json!({"label": "", "value": display_value(other)})],
    };
    let more = rows.len().saturating_sub(MAX_ROWS);
    let summary = answers_summary(&rows);
    json!({
        "pageId": answers.page_id,
        "title": plain_words(&answers.title),
        "summary": summary,
        "rows": rows.into_iter().take(MAX_ROWS).collect::<Vec<_>>(),
        "more": more,
    })
}

/// The folded card's count: "3 answered · 1 skipped", "3 answered", "4 skipped" (the reader's
/// own notes box is not a question and is not counted).
fn answers_summary(rows: &[Value]) -> String {
    let questions = rows.iter().filter(|row| row["extra"] != true);
    let (skipped, answered): (Vec<&Value>, Vec<&Value>) =
        questions.partition(|row| row["skipped"] == true);
    match (answered.len(), skipped.len()) {
        (0, 0) => String::new(),
        (0, skipped) => format!("{skipped} skipped"),
        (answered, 0) => format!("{answered} answered"),
        (answered, skipped) => format!("{answered} answered \u{b7} {skipped} skipped"),
    }
}

/// The page the answers came from, as the agent's ```visual block points at it (`{url, title,
/// file?}`), so the folded card can open it again; `None` for a form, which lives in the chat.
pub fn answered_page_link<'a>(
    messages: impl IntoIterator<Item = &'a ghostex_gx_protocol::ChatMessage>,
    page_id: &str,
) -> Option<Value> {
    messages
        .into_iter()
        .filter(|message| message.role == ChatRole::Assistant)
        .flat_map(|message| {
            crate::transcript::question_forms::content_fences(&prose_markdown(&message.blocks))
        })
        .filter(|fence| fence.kind == crate::transcript::question_forms::ContentKind::Page)
        .filter_map(|fence| ghostex_gx_visual::page_reference(&fence.body))
        .find(|page| page.page_id() == Some(page_id))
        .map(|page| {
            let mut link = json!({"url": page.url, "title": page.title});
            if let Some(file) = page.file {
                link["file"] = json!(file);
            }
            link
        })
}

/// The forms and pages whose answers card is the very next row the transcript draws after the
/// reply that asks them: their own Answered line is left out, since the card under it names the
/// same form or page (the document's `answersBelow`).
///
/// CDXC:SessionChat 2026-11-13 DECISION:
/// User (Q2995, sent answers page): "Hide the form's line when your answers sit right under it; keep it when other messages came between". Answers folded into a "Worked for" row (`folded_cards.rs`) are not right under it, so that line stays.
pub fn answers_below<'a>(
    items: impl IntoIterator<Item = &'a crate::document::TranscriptItem>,
    askers: &[crate::transcript::open_questions::Asker],
) -> Vec<String> {
    use crate::document::TranscriptItem;
    let mut below = Vec::new();
    let mut last_reply: Option<&Value> = None;
    for item in items {
        let opener: Option<&Value> = match item {
            TranscriptItem::Message { message } => Some(message),
            TranscriptItem::Summary { user, .. } => Some(user),
            _ => None,
        };
        if let (Some(reply), Some(row)) = (last_reply, opener) {
            let card = &row["pageAnswers"];
            if row["role"] == "user"
                && card.is_object()
                && card["withText"] != true
                && row[crate::transcript::folded_cards::FOLDED_INTO_WORK] != true
            {
                let page_id = card["pageId"].as_str().unwrap_or_default();
                let reply_id = reply["id"].as_str().unwrap_or_default();
                if askers
                    .iter()
                    .any(|asker| asker.id == page_id && asker.message_id == reply_id)
                {
                    below.push(page_id.to_string());
                }
            }
        }
        last_reply = match item {
            TranscriptItem::Message { message } if message["role"] == "assistant" => Some(message),
            TranscriptItem::CompletedWork { final_message, .. } => final_message.as_deref(),
            TranscriptItem::Summary { final_message, .. } => final_message.as_deref(),
            _ => None,
        };
    }
    below
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_with_links_shows_their_words_and_runs() {
        let answers = PageAnswers {
            page_id: "f".to_string(),
            title: "About [the picker](https://discord.com/c/1)".to_string(),
            answers: json!({
                "randomsand's asks ([Discord](https://discord.com/c/2)): what now?": "Fix all 4",
                "Plain one?": "Yes",
            }),
            tags: Vec::new(),
        };
        let card = project_page_answers(&answers, None);
        assert_eq!(card["title"], "About the picker");
        let row = &card["rows"][0];
        assert_eq!(row["label"], "randomsand's asks (Discord): what now?");
        assert_eq!(
            row["labelRuns"],
            json!([
                {"text": "randomsand's asks ("},
                {"text": "Discord", "href": "https://discord.com/c/2"},
                {"text": "): what now?"},
            ])
        );
        assert!(card["rows"][1].get("labelRuns").is_none());
    }
}
