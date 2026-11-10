//! The questions a published page asks, read from its `<gx-questions>` markup the way the page's
//! question kit (`kit.rs`) reads it in the browser: each `<gx-question>`'s answer key (its `name`,
//! else its title, numbered when repeated), tag, kind and choices (each choice's `value`, else its
//! label without its `<small>` detail). gxserver's voice agent lists and answers them from here
//! (server/src/voice_agent/question_forms.rs), and the answers go through the page's own route.
//!
//! CDXC:SessionChat 2026-10-20 SEE-ALSO:
//! `question()` and `choice()` in kit.rs's script decide the same keys and values in the browser; change them together.

use std::path::Path;

use super::store::{read_page, read_record};

/// One choice of a page question.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageChoice {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) recommended: bool,
}

/// One `<gx-question>` of a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageQuestion {
    /// What the answers are keyed by.
    pub(crate) key: String,
    pub(crate) tag: String,
    pub(crate) title: String,
    pub(crate) multiple: bool,
    pub(crate) text: bool,
    pub(crate) choices: Vec<PageChoice>,
}

/// A page's title and questions, when it is a page gxserver published.
pub(crate) fn page_questions(
    gxserver_state_dir: &Path,
    id: &str,
) -> Option<(String, Vec<PageQuestion>)> {
    let record = read_record(gxserver_state_dir, id)?;
    let html = String::from_utf8(read_page(gxserver_state_dir, id)?).ok()?;
    let title = record
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Ghostex page")
        .to_string();
    Some((title, parse_questions(&html)))
}

/// Whether a question on the page still has no answer in the reader's draft: its kit writes every
/// question's key into the draft (`collect()` in kit.rs), as `"skipped"` while a question has no
/// pick and no text, so a key that is missing or skipped is a question left. False once the page
/// was sent, and for a page with no questions. The desktop's page window reads it for the dot on a
/// hidden page's tab in the chat header (apps/desktop/src/app/visual_page_hidden.rs).
pub(crate) fn page_questions_left(gxserver_state_dir: &Path, id: &str) -> bool {
    if page_answered(gxserver_state_dir, id) {
        return false;
    }
    let Some((_, questions)) = page_questions(gxserver_state_dir, id) else {
        return false;
    };
    let draft = read_record(gxserver_state_dir, id)
        .and_then(|record| record.get("draft")?.get("kit").cloned())
        .unwrap_or(serde_json::Value::Null);
    questions
        .iter()
        .any(|question| match draft.get(&question.key) {
            None => true,
            Some(serde_json::Value::String(value)) => value == "skipped",
            Some(_) => false,
        })
}

/// Whether the page's answers were already sent (they may still wait in the session's queue).
pub(crate) fn page_answered(gxserver_state_dir: &Path, id: &str) -> bool {
    read_record(gxserver_state_dir, id)
        .is_some_and(|record| record.get("answeredAt").is_some_and(|at| !at.is_null()))
}

fn parse_questions(html: &str) -> Vec<PageQuestion> {
    let mut questions = Vec::new();
    let mut used: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(at) = find_tag(rest, "gx-question") {
        let after = &rest[at + "<gx-question".len()..];
        let head_end = after.find('>').unwrap_or(after.len());
        let head = &after[..head_end];
        let body_start = (head_end + 1).min(after.len());
        let body_end = after[body_start..]
            .find("</gx-question>")
            .map_or(after.len(), |end| body_start + end);
        let body = &after[body_start..body_end];
        rest = &after[body_end..];
        let title = decode(attribute(head, "title").unwrap_or_default().trim());
        let mut key = decode(attribute(head, "name").unwrap_or_default().trim());
        if key.is_empty() {
            key = if title.is_empty() {
                format!("Question {}", questions.len() + 1)
            } else {
                title.clone()
            };
        }
        let base = key.clone();
        let mut copy = 2;
        while used.contains(&key) {
            key = format!("{base} ({copy})");
            copy += 1;
        }
        used.push(key.clone());
        let text = attribute(head, "type").is_some_and(|kind| kind.eq_ignore_ascii_case("text"));
        questions.push(PageQuestion {
            key,
            tag: decode(attribute(head, "tag").unwrap_or_default().trim()),
            title,
            multiple: has_attribute(head, "multiple"),
            text,
            choices: if text {
                Vec::new()
            } else {
                parse_choices(body)
            },
        });
    }
    questions
}

fn parse_choices(body: &str) -> Vec<PageChoice> {
    let mut choices = Vec::new();
    let mut rest = body;
    while let Some(at) = find_tag(rest, "gx-choice") {
        let after = &rest[at + "<gx-choice".len()..];
        let head_end = after.find('>').unwrap_or(after.len());
        let head = &after[..head_end];
        let inner_start = (head_end + 1).min(after.len());
        let inner_end = after[inner_start..]
            .find("</gx-choice>")
            .map_or(after.len(), |end| inner_start + end);
        let label = label_text(&after[inner_start..inner_end]);
        rest = &after[inner_end..];
        let value = decode(attribute(head, "value").unwrap_or_default().trim());
        choices.push(PageChoice {
            value: if value.is_empty() {
                label.clone()
            } else {
                value
            },
            label,
            recommended: has_attribute(head, "recommended"),
        });
    }
    choices
}

/// Where `<name` starts as a whole tag name.
fn find_tag(text: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(found) = text[from..].find(&open) {
        let at = from + found;
        let next = text[at + open.len()..].chars().next();
        if next.is_some_and(|character| {
            character.is_whitespace() || character == '>' || character == '/'
        }) {
            return Some(at);
        }
        from = at + open.len();
    }
    None
}

/// The choice's text the kit shows as its label: its content without `<small>` details or tags,
/// whitespace collapsed.
fn label_text(inner: &str) -> String {
    let mut text = String::new();
    let mut rest = inner;
    while let Some(at) = rest.find("<small") {
        text.push_str(&rest[..at]);
        rest = rest[at..]
            .find("</small>")
            .map_or("", |end| &rest[at + end + "</small>".len()..]);
    }
    text.push_str(rest);
    let mut plain = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    decode(&plain.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn decode(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// An attribute's value in a start tag's text (`tag="Q1"` or `tag='Q1'`).
fn attribute(head: &str, name: &str) -> Option<String> {
    let mut search = head;
    while let Some(at) = search.find(name) {
        let before = search[..at].chars().next_back();
        let after = search[at + name.len()..].trim_start();
        search = &search[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')?;
        let value = &value[1..];
        return Some(value[..value.find(quote)?].to_string());
    }
    None
}

/// Whether a start tag carries a bare or valued attribute.
fn has_attribute(head: &str, name: &str) -> bool {
    head.split(|character: char| character.is_whitespace())
        .any(|word| word == name || word.starts_with(&format!("{name}=")))
}
