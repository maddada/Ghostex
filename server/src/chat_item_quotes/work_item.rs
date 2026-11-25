//! The quote of a work item a message points at (`[…](board:B1071)`): what `ghostex work item
//! show W1071` says of it, short. Looked up among the sending session's work items, its
//! coordinator's for a thread.

use ghostex_gx_chat_core::transcript::item_references::{cut, quoted_lines, QuotedItem};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::quote::Scope;
use crate::domain::DomainRepository;
use crate::server::AppState;

/// The most characters of the item's latest line its quote keeps, and the most lines.
const NOW_CHARS: usize = 300;
const NOW_LINES: usize = 3;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `W1071 Fix the login check (in progress)`, its latest line, the threads on it, its open asks
/// and the command that prints it whole; `W1071: Ghostex has nothing …` when the scope has no
/// such item.
pub(super) fn quote(state: &AppState, db: &Connection, scope: &Scope, number: u64) -> QuotedItem {
    let key = scope.work_key();
    let missing = || QuotedItem {
        heading: format!("W{number}"),
        lines: None,
    };
    let threads = crate::coordinators::cached_thread_rows(key);
    let params = json!({ "op": "list", "all": true });
    let Some(params) = params.as_object() else {
        return missing();
    };
    let Ok((work, _)) =
        crate::coordinators::handle_work_request(db, key, &threads, params, 0, &Default::default())
    else {
        return missing();
    };
    let Some(item) = work["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["kind"] == "item" && item["number"].as_u64() == Some(number))
    else {
        return missing();
    };
    let reference = text(item, "ref");
    let status = crate::coordinators::status_heading(text(item, "status")).to_lowercase();
    let mut lines: Vec<String> = Vec::new();
    // Where it stands, else its latest change.
    let latest = match text(item, "now") {
        "" => item["history"]
            .as_array()
            .and_then(|history| history.last())
            .map(|event| format!("{}: {}", text(event, "kind"), text(event, "text")))
            .unwrap_or_default(),
        now => now.to_string(),
    };
    let mut now_lines = quoted_lines(&latest, NOW_CHARS);
    if now_lines.len() > NOW_LINES {
        now_lines.truncate(NOW_LINES);
        if let Some(last) = now_lines.last_mut() {
            last.push('\u{2026}');
        }
    }
    lines.extend(now_lines);
    let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
    let on_it: Vec<String> = item["threads"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|thread| thread_words(&repository, &state.metadata.server_id, thread))
        .collect();
    if !on_it.is_empty() {
        lines.push(format!(
            "{}: {}",
            if on_it.len() == 1 {
                "Thread"
            } else {
                "Threads"
            },
            on_it.join(", ")
        ));
    }
    let open_asks: Vec<&str> = item["asks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|ask| ask["open"] == true)
        .map(|ask| text(ask, "tag"))
        .filter(|tag| !tag.is_empty())
        .collect();
    if !open_asks.is_empty() {
        lines.push(format!("Open asks: {}", open_asks.join(", ")));
    }
    let total = item["steps"]["total"].as_u64().unwrap_or(0);
    if total > 0 {
        lines.push(format!(
            "Steps: {} of {total} done",
            item["steps"]["done"].as_u64().unwrap_or(0)
        ));
    }
    lines.push(if *key == scope.own {
        format!("`ghostex work item show {reference}` prints all of it.")
    } else {
        format!(
            "`ghostex work item show {reference} --session {}` prints all of it.",
            key.1
        )
    });
    QuotedItem {
        heading: format!("{reference} {} ({status})", cut(text(item, "title"), 80)),
        lines: Some(lines),
    }
}

/// A thread on the item as a link the agent can follow: `[Fix login](S1:P1:G2b) (working)`.
fn thread_words(repository: &DomainRepository<'_>, server_id: &str, thread: &Value) -> String {
    let (project_id, session_id) = (text(thread, "projectId"), text(thread, "sessionId"));
    let title = match text(thread, "title") {
        "" => repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .and_then(|session| session.get("title")?.as_str().map(str::to_string))
            .unwrap_or_else(|| session_id.to_string()),
        title => title.to_string(),
    };
    let global_ref = match text(thread, "globalRef") {
        "" => crate::ids::create_global_session_ref(
            match text(thread, "serverId") {
                "" => server_id,
                other => other,
            },
            project_id,
            session_id,
        ),
        known => known.to_string(),
    };
    let state = text(thread, "state");
    let link = ghostex_gx_chat_core::transcript::item_references::session_link(&title, &global_ref);
    if state.is_empty() {
        link
    } else {
        format!("{link} ({state})")
    }
}
