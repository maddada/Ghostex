//! A question found by its tag, wherever the sending session's scope holds it, in this order: a
//! work item's ask, a Needs you to-do, a ```questions form (or one-click question) in the chat, a
//! Live page's question. The quote under a message and `ghostex question show` both read it.

use ghostex_gx_chat_core::transcript::item_references::{quoted_lines, QuotedItem};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::quote::{answer_words, local_time, local_time_ms, Scope, QUOTED_CHARS};
use crate::coordinators::{list_work_items, parse_answers, SessionKey, WorkAsk, WorkItem};
use crate::server::AppState;

/// Where a question lives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    WorkItem,
    NeedsYou,
    Form,
    Page,
}

impl Source {
    fn id(self) -> &'static str {
        match self {
            Self::WorkItem => "workItem",
            Self::NeedsYou => "needsYou",
            Self::Form => "form",
            Self::Page => "page",
        }
    }
}

/// One question, as Ghostex knows it.
pub(super) struct FoundQuestion {
    pub(super) tag: String,
    /// The chat it was found for.
    key: SessionKey,
    source: Source,
    /// `W1071` and its title, for a work item's ask.
    work_item: Option<(String, String)>,
    /// The page's id and title, for a Live page's question.
    page: Option<(String, String)>,
    asked: Option<String>,
    /// open, answered, skipped, withdrawn, done, dropped or dismissed.
    state: String,
    title: String,
    detail: String,
    command: String,
    choices: Vec<(String, bool)>,
    answer: Option<String>,
}

impl FoundQuestion {
    /// Where it was asked, in words: `work item W1071 "Fix the login check"`, `a Needs you
    /// to-do`, `a question form in the chat`, `Live page "Settings redesign"`.
    fn where_words(&self) -> String {
        match (self.source, &self.work_item, &self.page) {
            (Source::WorkItem, Some((reference, title)), _) => {
                format!("work item {reference} \"{title}\"")
            }
            (Source::Page, _, Some((_, title))) => format!("Live page \"{title}\""),
            (Source::NeedsYou, ..) => "a Needs you to-do".to_string(),
            _ => "a question form in the chat".to_string(),
        }
    }

    fn choices_line(&self) -> Option<String> {
        (!self.choices.is_empty()).then(|| {
            let choices: Vec<String> = self
                .choices
                .iter()
                .map(|(label, recommended)| {
                    if *recommended {
                        format!("{label} (recommended)")
                    } else {
                        label.clone()
                    }
                })
                .collect();
            format!("Choices: {}", choices.join(", "))
        })
    }

    /// The question as `/api/chatQuestions` answers it.
    pub(super) fn json(&self, written: &str) -> Value {
        let mut out = json!({
            "tag": self.tag,
            "ref": written,
            "projectId": self.key.0,
            "sessionId": self.key.1,
            "source": self.source.id(),
            "where": self.where_words(),
            "asked": self.asked,
            "state": self.state,
            "title": self.title,
            "detail": self.detail,
            "choices": self.choices.iter().map(|(label, recommended)| {
                json!({ "label": label, "recommended": recommended })
            }).collect::<Vec<_>>(),
            "answer": self.answer,
        });
        if !self.command.is_empty() {
            out["command"] = json!(self.command);
        }
        if let Some((reference, title)) = &self.work_item {
            out["workItem"] = json!({ "ref": reference, "title": title });
        }
        if let Some((id, title)) = &self.page {
            out["page"] = json!({ "id": id, "title": title });
        }
        out
    }
}

/// The quote of a question: `Q4933 (W1071, asked 2026-11-25 14:02, open)`, its words, its
/// choices, its answer, and the command that prints it whole. `written` is the tag as the link
/// wrote it (`Q4933` or `G9jt7.Q4933`).
pub(super) fn quote(found: Option<FoundQuestion>, written: &str) -> QuotedItem {
    let Some(found) = found else {
        return QuotedItem {
            heading: written.to_string(),
            lines: None,
        };
    };
    let mut facts: Vec<String> = Vec::new();
    match (found.source, &found.work_item, &found.page) {
        (Source::WorkItem, Some((reference, _)), _) => facts.push(reference.clone()),
        (Source::NeedsYou, ..) => facts.push("Needs you".to_string()),
        (Source::Page, _, Some((_, title))) => facts.push(format!("Live page \"{title}\"")),
        _ => {}
    }
    if let Some(asked) = &found.asked {
        facts.push(format!("asked {asked}"));
    }
    facts.push(found.state.clone());
    let words = if found.detail.trim().is_empty() {
        found.title.clone()
    } else {
        format!("{}\n\n{}", found.title, found.detail)
    };
    let mut lines = quoted_lines(&words, QUOTED_CHARS);
    lines.extend(found.choices_line());
    if let Some(answer) = found.answer.as_deref().filter(|answer| !answer.is_empty()) {
        lines.push(format!(
            "Answered: {}",
            ghostex_gx_chat_core::transcript::item_references::cut(answer, 200)
        ));
    }
    lines.push(format!("`ghostex question show {written}` prints all of it."));
    QuotedItem {
        heading: format!("{written} ({})", facts.join(", ")),
        lines: Some(lines),
    }
}

/// The question tagged `tag` in `scope`, looked for in a work item's ask, a Needs you to-do, a
/// form in the chat and a Live page, in that order. Blocking.
pub(super) fn find_question(
    state: &AppState,
    db: &Connection,
    scope: &Scope,
    tag: &str,
) -> Option<FoundQuestion> {
    let tag = tag.trim().to_ascii_uppercase();
    if let Some(mut found) = work_ask(db, scope.work_key(), &tag) {
        // The ask of a question the chat asked (`item ask --tag`): its form or page has the
        // detail, the choices and the answer the ask may not.
        if let Some(chat) = chat_question(state, db, scope, &tag) {
            if found.detail.is_empty() {
                found.detail = chat.detail;
            }
            if found.choices.is_empty() {
                found.choices = chat.choices;
            }
            if found.answer.is_none() {
                found.answer = chat.answer;
            }
        }
        return Some(found);
    }
    scope
        .chats()
        .into_iter()
        .find_map(|key| todo(db, key, &tag))
        .or_else(|| chat_question(state, db, scope, &tag))
}

/// The latest ask tagged `tag` among the work items of `key`.
fn work_ask(db: &Connection, key: &SessionKey, tag: &str) -> Option<FoundQuestion> {
    let items = list_work_items(db, key).ok()?;
    let (item, ask): (&WorkItem, &WorkAsk) = items
        .iter()
        .flat_map(|item| item.asks.iter().map(move |ask| (item, ask)))
        .filter(|(_, ask)| ask.tag.eq_ignore_ascii_case(tag))
        .max_by(|(_, left), (_, right)| left.asked_at.cmp(&right.asked_at))?;
    let state = if ask.withdrawn {
        "withdrawn"
    } else if ask.is_open() {
        "open"
    } else if !ask.answered.is_empty() {
        "answered"
    } else {
        "done"
    };
    Some(FoundQuestion {
        tag: tag.to_string(),
        key: key.clone(),
        source: Source::WorkItem,
        work_item: Some((item.reference(), item.title.clone())),
        page: None,
        asked: local_time(&ask.asked_at),
        state: state.to_string(),
        title: ask.text.clone(),
        detail: ask.detail.clone(),
        command: ask.command.clone(),
        choices: parse_answers(&ask.answers)
            .iter()
            .map(|choice| {
                (
                    choice["label"].as_str().unwrap_or_default().to_string(),
                    choice["recommended"] == true,
                )
            })
            .collect(),
        answer: (!ask.answered.is_empty()).then(|| ask.answered.clone()),
    })
}

/// The Needs you to-do tagged `tag` of the chat `key`.
fn todo(db: &Connection, key: &SessionKey, tag: &str) -> Option<FoundQuestion> {
    let todo = crate::session_needs_you::todo_question(db, key, tag)?;
    Some(FoundQuestion {
        tag: tag.to_string(),
        key: key.clone(),
        source: Source::NeedsYou,
        work_item: None,
        page: None,
        asked: local_time(&todo.asked_at),
        state: todo.outcome.unwrap_or_else(|| "open".to_string()),
        title: todo.title,
        detail: todo.detail,
        command: todo.command,
        choices: todo
            .answers
            .iter()
            .map(|answer| match answer.strip_suffix('*') {
                Some(label) => (label.trim().to_string(), true),
                None => (answer.clone(), false),
            })
            .collect(),
        answer: (!todo.answer.is_empty()).then_some(todo.answer),
    })
}

/// The form or Live page question tagged `tag` in the scope's chats (its own first).
fn chat_question(
    state: &AppState,
    db: &Connection,
    scope: &Scope,
    tag: &str,
) -> Option<FoundQuestion> {
    scope.chats().into_iter().find_map(|key| {
        let found =
            crate::session_chat_open_questions::chat_question(state, db, &key.0, &key.1, tag)?;
        let answer = found
            .answer
            .as_ref()
            .map(answer_words)
            .filter(|words| !words.is_empty());
        let state = match (found.state, answer.as_deref()) {
            (_, Some("skipped")) => "skipped".to_string(),
            (Some(state), _) => state.as_str().to_string(),
            (None, Some(_)) => "answered".to_string(),
            (None, None) => "open".to_string(),
        };
        Some(FoundQuestion {
            tag: tag.to_string(),
            key: key.clone(),
            source: if found.on_page {
                Source::Page
            } else {
                Source::Form
            },
            work_item: None,
            page: found
                .page_title
                .map(|title| (found.asker_id.clone(), title)),
            asked: found.asked_at_ms.and_then(local_time_ms),
            state,
            title: if found.title.trim().is_empty() {
                tag.to_string()
            } else {
                found.title
            },
            detail: found.detail,
            command: String::new(),
            choices: found.choices,
            answer: answer.filter(|words| words != "skipped"),
        })
    })
}
