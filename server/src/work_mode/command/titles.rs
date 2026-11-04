//! The last title of each ticket a ticket command answered, for a card that can't ask the command
//! (`TicketRead::Unavailable`). Kept in its own table (`work_ticket_titles`), at most
//! `MAX_TICKET_TITLES` rows, and copied in memory for the projection, which does no I/O. A title
//! outlives a clear and a Switch, which is when it is shown; it goes only with its workspace, or
//! when newer titles push it past the bound. Titles are stored as the parser masked them, so none
//! holds a credential.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;

use crate::domain::DomainStateError;

use super::*;

pub(crate) const MAX_TICKET_TITLES: usize = 2_000;

/// (workspace, source id, ticket id).
type TitleKey = (String, String, String);

/// The stored titles in memory, and which tickets each workspace has a title for, by source, so
/// a card asks only its own workspace's (`titled_tickets`).
///
/// CDXC:WorkMode 2026-11-03 WHY:
/// The projection asks for a workspace's titled tickets for every session with a branch (`unread_command_tickets_in_branch`), and a walk of every title (up to `MAX_TICKET_TITLES`) each time added up on a busy sidebar; the index keeps it to the workspace's own.
#[derive(Default)]
struct Titles {
    titles: HashMap<TitleKey, String>,
    by_workspace: HashMap<String, BTreeMap<String, BTreeSet<String>>>,
}

impl Titles {
    fn insert(&mut self, key: TitleKey, title: String) {
        let (workspace, source, ticket) = &key;
        self.by_workspace
            .entry(workspace.clone())
            .or_default()
            .entry(source.clone())
            .or_default()
            .insert(ticket.clone());
        self.titles.insert(key, title);
    }

    fn remove(&mut self, key: &TitleKey) {
        self.titles.remove(key);
        let (workspace, source, ticket) = key;
        let Some(sources) = self.by_workspace.get_mut(workspace) else {
            return;
        };
        if let Some(tickets) = sources.get_mut(source) {
            tickets.remove(ticket);
            if tickets.is_empty() {
                sources.remove(source);
            }
        }
        if sources.is_empty() {
            self.by_workspace.remove(workspace);
        }
    }

    fn forget_workspace(&mut self, workspace_id: &str) {
        self.titles
            .retain(|(workspace, _, _), _| workspace != workspace_id);
        self.by_workspace.remove(workspace_id);
    }
}

fn mirror() -> &'static Mutex<Titles> {
    static TITLES: OnceLock<Mutex<Titles>> = OnceLock::new();
    TITLES.get_or_init(|| Mutex::new(Titles::default()))
}

fn titles() -> std::sync::MutexGuard<'static, Titles> {
    mirror()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn key(workspace_id: &str, source_id: &str, ticket_id: &str) -> TitleKey {
    (
        workspace_id.to_string(),
        source_id.to_string(),
        ticket_id.to_string(),
    )
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite ticket titles error: {error}"),
    }
}

/// The ticket's last title, as last stored.
pub(crate) fn last_ticket_title(
    workspace_id: &str,
    source_id: &str,
    ticket_id: &str,
) -> Option<String> {
    titles()
        .titles
        .get(&key(workspace_id, source_id, ticket_id))
        .cloned()
}

/// Every ticket of the workspace with a last title, by source id, from the index.
pub(crate) fn titled_tickets(workspace_id: &str) -> BTreeMap<String, Vec<String>> {
    let titled: BTreeMap<String, Vec<String>> = titles()
        .by_workspace
        .get(workspace_id)
        .map(|sources| {
            sources
                .iter()
                .map(|(source, tickets)| (source.clone(), tickets.iter().cloned().collect()))
                .collect()
        })
        .unwrap_or_default();
    #[cfg(test)]
    TITLES_LOOKED_AT.with(|looked_at| {
        looked_at.set(looked_at.get() + titled.values().map(Vec::len).sum::<usize>())
    });
    titled
}

#[cfg(test)]
thread_local! {
    /// How many titles `titled_tickets` looked at on this thread.
    static TITLES_LOOKED_AT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many titles `titled_tickets` has looked at on this thread, for a test of what it reads.
#[cfg(test)]
pub(crate) fn titles_looked_at() -> usize {
    TITLES_LOOKED_AT.with(std::cell::Cell::get)
}

/// Startup, with the ticket command record: copies every stored title in memory, so a card that
/// can't ask its command shows its last title from the first snapshot on.
pub(crate) fn load_ticket_titles(db: &Connection) -> Result<(), DomainStateError> {
    let mut statement = db
        .prepare("SELECT workspaceId, sourceId, ticketId, title FROM work_ticket_titles")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get(0)?, row.get(1)?, row.get(2)?),
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<(TitleKey, String)>, _>>()
        .map_err(sql_error)?;
    let mut titles = titles();
    for (key, title) in rows {
        titles.insert(key, title);
    }
    Ok(())
}

/// Stores each `(ticket id, title)` whose title is new or changed, one row each, then drops the
/// least recently updated rows past `MAX_TICKET_TITLES`; the copy in memory follows. Called while
/// a job commits, under the store's writer lock.
pub(crate) fn save_ticket_titles(
    db: &Connection,
    workspace_id: &str,
    source_id: &str,
    found: &[(String, String)],
) -> Result<(), DomainStateError> {
    let changed: Vec<&(String, String)> = {
        let titles = titles();
        found
            .iter()
            .filter(|(id, title)| {
                titles.titles.get(&key(workspace_id, source_id, id)) != Some(title)
            })
            .collect()
    };
    if changed.is_empty() {
        return Ok(());
    }
    let now = now_iso();
    // One transaction, the eviction included: a failure keeps none of it, and the copy in memory
    // follows only once it commits.
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    for (id, title) in &changed {
        transaction
            .execute(
                r#"
            INSERT INTO work_ticket_titles (workspaceId, sourceId, ticketId, title, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(workspaceId, sourceId, ticketId)
            DO UPDATE SET title = excluded.title, updatedAt = excluded.updatedAt
            "#,
                rusqlite::params![workspace_id, source_id, id, title, now],
            )
            .map_err(sql_error)?;
    }
    let evicted = {
        let mut statement = transaction
            .prepare(
                r#"
            SELECT rowid, workspaceId, sourceId, ticketId FROM work_ticket_titles
            ORDER BY updatedAt DESC, rowid DESC LIMIT -1 OFFSET ?1
            "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([MAX_TICKET_TITLES as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    (row.get(1)?, row.get(2)?, row.get(3)?),
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<(i64, TitleKey)>, _>>()
            .map_err(sql_error)?;
        rows
    };
    for (rowid, _) in &evicted {
        transaction
            .execute("DELETE FROM work_ticket_titles WHERE rowid = ?1", [rowid])
            .map_err(sql_error)?;
    }
    transaction.commit().map_err(sql_error)?;
    let mut titles = titles();
    for (id, title) in changed {
        titles.insert(key(workspace_id, source_id, id), title.clone());
    }
    for (_, key) in evicted {
        titles.remove(&key);
    }
    Ok(())
}

/// A workspace is being deleted: its rows go, inside the deletion's transaction. The copy in
/// memory follows once that commits (`forget_workspace_ticket_titles`).
pub(crate) fn delete_workspace_ticket_titles(
    db: &Connection,
    workspace_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM work_ticket_titles WHERE workspaceId = ?1",
        [workspace_id],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(crate) fn forget_workspace_ticket_titles(workspace_id: &str) {
    titles().forget_workspace(workspace_id);
}
