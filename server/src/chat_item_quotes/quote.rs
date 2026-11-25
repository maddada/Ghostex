//! What the quote block under a message holds: each question, work item and Live page the message
//! points at, looked up in the sending session's scope, the first few quoted and the rest named.

use ghostex_gx_chat_core::transcript::item_references::{
    pointed_at, with_item_quotes as with_quotes, QuotedItem, MAX_QUOTED_ITEMS,
};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use crate::coordinators::SessionKey;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// The most characters of an item's own words its quote keeps.
pub(super) const QUOTED_CHARS: usize = 600;

/// Where the sending session looks things up: its own chat, and its coordinator's when it is one
/// of a coordinator's threads (they share the coordinator's question numbering and work items).
pub(super) struct Scope {
    pub(super) own: SessionKey,
    pub(super) coordinator: Option<SessionKey>,
}

impl Scope {
    pub(super) fn of(db: &Connection, project_id: &str, session_id: &str) -> Self {
        let coordinator = crate::coordinators::read_thread(db, project_id, session_id)
            .ok()
            .flatten()
            .map(|thread| thread.coordinator_key());
        Self {
            own: (project_id.to_string(), session_id.to_string()),
            coordinator,
        }
    }

    /// The scope of another chat a link names by its session id (`G9jt7.Q4933`), when this
    /// computer has that session.
    pub(super) fn of_session(db: &Connection, session_id: &str) -> Option<Self> {
        let project_id: String = db
            .query_row(
                "SELECT projectId FROM sessions WHERE sessionId = ?1 LIMIT 1",
                [session_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()?;
        Some(Self::of(db, &project_id, session_id))
    }

    /// Whose work items the session's are: its coordinator's for a thread, else its own.
    pub(super) fn work_key(&self) -> &SessionKey {
        self.coordinator.as_ref().unwrap_or(&self.own)
    }

    /// Its own chat first, then its coordinator's.
    pub(super) fn chats(&self) -> Vec<&SessionKey> {
        std::iter::once(&self.own)
            .chain(self.coordinator.iter().filter(|key| **key != self.own))
            .collect()
    }
}

/// An RFC 3339 time as this computer's local `2026-11-25 14:02`.
pub(super) fn local_time(at: &str) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    Some(
        at.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

/// Epoch milliseconds as this computer's local `2026-11-25 14:02`.
pub(super) fn local_time_ms(at: i64) -> Option<String> {
    let at = chrono::DateTime::from_timestamp_millis(at)?;
    Some(
        at.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

/// An answer as sent from a form or a page, in words: a pick, several picks, or typed text.
pub(super) fn answer_words(answer: &Value) -> String {
    match answer {
        Value::String(text) => text.trim().to_string(),
        Value::Array(picks) => picks
            .iter()
            .map(answer_words)
            .filter(|words| !words.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(fields) => fields
            .values()
            .map(answer_words)
            .filter(|words| !words.is_empty())
            .collect::<Vec<_>>()
            .join("; "),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One thing a message points at.
enum Target {
    Question {
        session_id: Option<String>,
        tag: String,
    },
    WorkItem(u64),
    Page(String),
}

impl Target {
    fn written(&self) -> String {
        match self {
            Self::Question {
                session_id: Some(session),
                tag,
            } => format!("{session}.{tag}"),
            Self::Question { tag, .. } => tag.clone(),
            Self::WorkItem(number) => format!("W{number}"),
            Self::Page(id) => format!("Live page {id}"),
        }
    }

    /// Whether the link target `href` (`question:G9jt7.Q3.Q4`, `board:W12`, `page:<id>`) names it.
    fn written_as(&self, href: &str) -> bool {
        match self {
            Self::Question { tag, .. } => href
                .strip_prefix("question:")
                .is_some_and(|rest| rest.split('.').any(|part| part == tag)),
            Self::WorkItem(number) => href.strip_prefix("board:").is_some_and(|rest| {
                rest.get(1..) == Some(number.to_string().as_str())
                    && rest.starts_with(['B', 'b', 'W', 'w'])
            }),
            Self::Page(id) => href.strip_prefix("page:") == Some(id.as_str()),
        }
    }
}

/// Every Markdown link target in `text` (what sits between `](` and `)`), in order.
fn link_targets(text: &str) -> Vec<&str> {
    text.match_indices("](")
        .filter_map(|(at, _)| {
            let rest = &text[at + 2..];
            rest.find(')').map(|end| rest[..end].trim())
        })
        .collect()
}

/// `text` with the quotes of the questions, work items and Live pages `pointed_text` (the user's
/// own words) points at under it, in one block; `text` itself when it points at none. Blocking.
pub(crate) fn with_item_quotes(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    pointed_text: &str,
    text: &str,
) -> String {
    let pointed = pointed_at(pointed_text);
    if pointed.is_empty() {
        return text.to_string();
    }
    let mut targets: Vec<Target> = Vec::new();
    for reference in pointed.questions {
        for tag in reference.tags {
            targets.push(Target::Question {
                session_id: reference.session_id.clone(),
                tag,
            });
        }
    }
    targets.extend(pointed.work_items.into_iter().map(Target::WorkItem));
    targets.extend(pointed.pages.into_iter().map(Target::Page));
    // In the order the user wrote them, so the first 5 quoted are the first 5 they pointed at.
    let hrefs = link_targets(pointed_text);
    targets.sort_by_key(|target| {
        hrefs
            .iter()
            .position(|href| target.written_as(href))
            .unwrap_or(usize::MAX)
    });
    crate::chat_message_numbers::off_worker(|| {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return text.to_string();
        };
        let scope = Scope::of(&db, project_id, session_id);
        let (quoted, named) = targets.split_at(targets.len().min(MAX_QUOTED_ITEMS));
        let items: Vec<QuotedItem> = quoted
            .iter()
            .map(|target| quote_target(state, &db, &scope, target))
            .collect();
        let unquoted: Vec<String> = named.iter().map(Target::written).collect();
        with_quotes(text, &items, &unquoted)
    })
}

fn quote_target(state: &AppState, db: &Connection, scope: &Scope, target: &Target) -> QuotedItem {
    match target {
        Target::Question {
            session_id: Some(other),
            tag,
        } if *other != scope.own.1 => {
            let found = Scope::of_session(db, other)
                .and_then(|scope| super::question::find_question(state, db, &scope, tag));
            super::question::quote(found, &target.written())
        }
        Target::Question { tag, .. } => {
            let found = super::question::find_question(state, db, scope, tag);
            super::question::quote(found, &target.written())
        }
        Target::WorkItem(number) => super::work_item::quote(state, db, scope, *number),
        Target::Page(id) => super::page::quote(&state.paths.root_dir, id),
    }
}
