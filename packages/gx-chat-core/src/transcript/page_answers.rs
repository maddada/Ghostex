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
/// The longest page answer still drawn as a pick's pill; a longer one reads as text.
const MAX_PICK_CHARS: usize = 48;
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
    /// Who submitted these answers when submitted by an outside person via a shared Live page link.
    pub by: Option<String>,
    /// True when submitted by an outside visitor via a shared Live page link (does not lock out
    /// the user from answering or overriding the page).
    pub outside: bool,
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

/// Finds the closing `</ghostex-page-answers>` tag starting from `body_start`, ignoring tags embedded
/// inside valid JSON bodies (objects, arrays, or string literals) before the closing tag.
fn find_envelope_close(text: &str, body_start: usize) -> Option<usize> {
    let tail = &text[body_start..];
    let first_non_ws = tail.trim_start();
    if first_non_ws.starts_with('{')
        || first_non_ws.starts_with('[')
        || first_non_ws.starts_with('"')
    {
        let mut stream = serde_json::Deserializer::from_str(first_non_ws).into_iter::<Value>();
        if let Some(Ok(_)) = stream.next() {
            let after_json = &first_non_ws[stream.byte_offset()..];
            let trailing_ws = after_json.len() - after_json.trim_start().len();
            if after_json[trailing_ws..].starts_with(CLOSE) {
                let offset = text.len() - first_non_ws.len();
                return Some(offset + stream.byte_offset() + trailing_ws);
            }
        }
    }
    tail.find(CLOSE).map(|rel| body_start + rel)
}

/// Byte spans `(open, close)` and parsed `PageAnswers` for every valid envelope in `text`.
fn scan_page_answer_spans(text: &str) -> Vec<(usize, usize, PageAnswers)> {
    if !text.contains(OPEN) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut scan = 0usize;
    while let Some(rel_open) = text[scan..].find(OPEN) {
        let open = scan + rel_open;
        let after_open = &text[open + OPEN.len()..];
        if !after_open
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || ch == '>')
        {
            scan = open + OPEN.len();
            continue;
        }
        let Some(head_rel_end) = after_open.find('>') else {
            break;
        };
        let head = &after_open[..head_rel_end];
        if head.contains('<') {
            scan = open + OPEN.len();
            continue;
        }
        let body_start = open + OPEN.len() + head_rel_end + 1;
        let Some(close_start) = find_envelope_close(text, body_start) else {
            break;
        };
        let close = close_start + CLOSE.len();
        if let Some(answers) = parse_page_answers(&text[open..close]) {
            found.push((open, close, answers));
            scan = close;
        } else {
            scan = open + OPEN.len();
        }
    }
    found
}

/// Replaces only the envelope for `page_id` in `text` with `replacement`, keeping surrounding
/// envelopes and prose verbatim.
pub(crate) fn replace_page_answers_envelope(
    text: &str,
    page_id: &str,
    replacement: &str,
) -> Option<String> {
    let spans = scan_page_answer_spans(text);
    let (open, close, _) = spans
        .into_iter()
        .rfind(|(_, _, answers)| answers.page_id == page_id)?;
    Some(format!(
        "{}{}{}",
        &text[..open],
        replacement,
        &text[close..]
    ))
}

/// All answers envelopes a user message holds (in document order) and the text written around
/// them (empty when the message holds only envelopes).
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// The envelope is not always the whole message: answers that went out while Claude could not
/// take them came back into its input box, Ghostex handed them to the chat's composer, and the
/// user's next message carried them with their own words below. Multiple forms answered while the
/// agent was busy can also be concatenated into one user message. The chat draws an answers card
/// for each envelope and the rest as the user's text, and each form still reads Answered.
pub fn split_all_page_answers(text: &str) -> (Vec<PageAnswers>, String) {
    let spans = scan_page_answer_spans(text);
    if spans.is_empty() {
        return (Vec::new(), text.to_string());
    }
    let mut found = Vec::with_capacity(spans.len());
    let mut parts: Vec<&str> = Vec::new();
    let mut cursor = 0usize;
    for (open, close, answers) in spans {
        let gap = js_trim(&text[cursor..open]);
        if !gap.is_empty() {
            parts.push(gap);
        }
        found.push(answers);
        cursor = close;
    }
    let tail = js_trim(&text[cursor..]);
    if !tail.is_empty() {
        parts.push(tail);
    }
    (found, parts.join("\n\n"))
}

/// Every page or question form a message's answers envelopes answer, in document order.
/// Outside answers from shared Live page links are excluded so they never lock out the user.
pub fn answers_page_ids(text: &str) -> Vec<String> {
    if !text.contains(OPEN) {
        return Vec::new();
    }
    split_all_page_answers(text)
        .0
        .into_iter()
        .filter(|answers| !answers.outside)
        .map(|answers| answers.page_id)
        .collect()
}

/// All projected answer cards a transcript row carries: the primary `pageAnswers` object plus any
/// additional cards in `extraPageAnswers`.
pub(crate) fn row_page_answers(row: &Value) -> impl Iterator<Item = &Value> {
    let first = row
        .get("pageAnswers")
        .filter(|value| value.is_object())
        .into_iter();
    let extra = row
        .get("extraPageAnswers")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|value| value.is_object());
    first.chain(extra)
}

/// The answers a message holds, when the whole message is one answers envelope.
pub fn parse_page_answers(text: &str) -> Option<PageAnswers> {
    let text = js_trim(text);
    let rest = text.strip_prefix(OPEN)?;
    if !rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ch == '>')
    {
        return None;
    }
    let head_end = rest.find('>')?;
    let head = &rest[..head_end];
    if head.contains('<') {
        return None;
    }
    let body = rest[head_end + 1..].strip_suffix(CLOSE)?;
    let page_id = attribute(head, "page").filter(|id| is_page_id(id))?;
    let title = attribute(head, "title")
        .map(|title| js_trim(&title).to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Live page".to_string());
    let body = js_trim(body);
    if body.contains(OPEN) && serde_json::from_str::<Value>(body).is_err() {
        return None;
    }
    let answers = serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_string()));
    let tags = attribute(head, "tags")
        .map(|tags| tags.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let by = attribute(head, "by")
        .map(|name| js_trim(&name).to_string())
        .filter(|name| !name.is_empty());
    let outside = attribute(head, "source").as_deref() == Some("shared-link")
        || attribute(head, "outside").as_deref() == Some("true");
    Some(PageAnswers {
        page_id,
        title,
        answers,
        tags,
        by,
        outside,
    })
}

pub(crate) fn is_unsent_or_failed(message: &ghostex_gx_protocol::ChatMessage) -> bool {
    message.startup_delivery.as_ref().is_some_and(|delivery| {
        delivery.state == "failed" || crate::composer::delivery::is_unsent(&delivery.prompt_id)
    })
}

/// The answers this transcript received, one per page or question form (the latest sent, where the
/// first one arrived). Failed or unsent client recovery rows and outside visitor answers on shared
/// Live pages are excluded so they never settle an open form or page before the user answers.
pub fn page_answers_in<'a>(
    messages: impl IntoIterator<Item = &'a ghostex_gx_protocol::ChatMessage>,
) -> Vec<PageAnswers> {
    let mut found: Vec<PageAnswers> = Vec::new();
    for message in messages
        .into_iter()
        .filter(|message| message.role == ChatRole::User && !is_unsent_or_failed(message))
    {
        let (envelopes, _) = split_all_page_answers(&prose_markdown(&message.blocks));
        for answers in envelopes.into_iter().filter(|answers| !answers.outside) {
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
    let shown = |text: &str| shown_pick(text, asked);
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

/// One picked value as the card shows it (`display_answer`).
fn shown_pick(text: &str, asked: Option<&FormQuestion>) -> String {
    asked
        .and_then(|asked| asked.choices.iter().find(|choice| choice.value == text))
        .map(|choice| choice.label.clone())
        .unwrap_or_else(|| sentence_case(text))
}

/// The choices a choice answer picked, as the card shows them one pill each, and whether
/// "Other / none of these" was among them (it is left out of the picks: the question's note holds
/// the reader's own words). `None` for a text answer, or for a page's answer that is too long or
/// more than a line to read as a pick (a page's question carries no choices here).
///
/// CDXC:SessionChat 2026-11-17 DECISION:
/// User picked Q3764 B from ~/.local/share/agent-work/Ghostex/answers-card/page.html: each answer on
/// the answers card is a quiet pill under its question, so the card needs the picks one by one
/// (`picks`, `other` on the row) rather than the comma-joined `value`, which stays for older apps
/// and Quiet View's count.
fn answer_picks(value: &Value, asked: Option<&FormQuestion>) -> Option<(Vec<String>, bool)> {
    if asked.is_some_and(|asked| asked.text || asked.choices.is_empty()) {
        return None;
    }
    let items: Vec<&str> = match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_string) => {
            items.iter().filter_map(Value::as_str).collect()
        }
        _ => return None,
    };
    let other = items.contains(&ghostex_gx_visual::questions::OTHER_CHOICE);
    let picks: Vec<String> = items
        .into_iter()
        .filter(|item| *item != ghostex_gx_visual::questions::OTHER_CHOICE)
        .map(|item| plain_words(&shown_pick(item, asked)))
        .collect();
    let fits = |pick: &String| pick.chars().count() <= MAX_PICK_CHARS && !pick.contains('\n');
    if asked.is_none() && !picks.iter().all(fits) {
        return None;
    }
    Some((picks, other))
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

/// The message's `pageAnswers` key: `{pageId, title, rows: [{label, value, picks?, other?, note?}], more,
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
                        || value.as_str() == Some(crate::transcript::question_forms::SKIPPED));
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
                if let Some((picks, other)) = (!skipped && !extra && note_as_answer.is_none())
                    .then(|| answer_picks(value, asked))
                    .flatten()
                {
                    row["picks"] = json!(picks);
                    if other {
                        row["other"] = json!(true);
                    }
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
    let summary = answers_summary(&rows);
    // CDXC:SessionChat 2026-11-25 DECISION: The user asked for their overall note ("Anything else?")
    // at the bottom of the opened card, "where it actually goes in the real live page". The notes
    // row comes after every question's row (and is never the one cut off by the row cap), so the
    // desktop, web and phone cards, which draw rows in this order, all agree.
    let (mut rows, notes): (Vec<Value>, Vec<Value>) =
        rows.into_iter().partition(|row| row["extra"] != true);
    let more = rows.len().saturating_sub(MAX_ROWS);
    rows.truncate(MAX_ROWS);
    rows.extend(notes);
    let mut out = json!({
        "pageId": answers.page_id,
        "title": plain_words(&answers.title),
        "summary": summary,
        "rows": rows,
        "more": more,
    });
    if let Some(by) = &answers.by {
        out["by"] = json!(by);
    }
    if answers.outside {
        let who = answers
            .by
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("Visitor");
        out["outside"] = json!(true);
        out["noteLabel"] = json!(format!("{who}'s note"));
        out["ownAnswerLabel"] = json!(format!("{who}'s own answer"));
        out["headLabel"] = json!(format!(
            "{who} answered \u{201c}{}\u{201d} via share link",
            plain_words(&answers.title)
        ));
    }
    out
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
                && card["outside"] != true
                && row[crate::transcript::folded_cards::FOLDED_INTO_WORK] != true
            {
                let reply_id = reply["id"].as_str().unwrap_or_default();
                for envelope in row_page_answers(row) {
                    let page_id = envelope["pageId"].as_str().unwrap_or_default();
                    if askers
                        .iter()
                        .any(|asker| asker.id == page_id && asker.message_id == reply_id)
                    {
                        below.push(page_id.to_string());
                    }
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
            by: None,
            outside: false,
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
