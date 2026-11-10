//! The main coordinator of each project on this computer: the one agents contact by default when
//! they want something from that project (`ghostex agents send --main-coordinator`), and the one
//! Chief of Staff hands a project's work to.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User: "we need ability to set 1 coordinator for each project as a main coordinator for that machine. first one created in a project per machine gets this label but i should be able to give it to another coordinator on that machine by right clicking on it. this just helps agents know which coordinator to contact". One main per project per gxserver. The user's pick is kept in `metadata` (`mainCoordinator:<projectId>`); without a pick that still stands, the oldest coordinator of the project that is not closed is the main one, and that answer is kept as the pick. So the first coordinator created in a project gets the label, an install that had coordinators before gets the oldest one, and closing or deleting the main one passes the label to the oldest one left (none left: no main). Chief of Staff and GBot chats are never a project's main coordinator.
//! SEE-ALSO: server/src/ghostex_cli/agents/main_coordinator.rs (the CLI that reaches it), packages/gx-core/src/sidebar_menu/session.rs ("Make main coordinator"), apps/mobile/app/src/screens/sessions-screen/use-sessions-screen-menus.tsx (the phone's long-press item).

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::records::{list_coordinators, now_iso, sql_error, CoordinatorRecord, SessionKey};
use crate::domain::{DomainRepository, DomainStateError};

const KEY_PREFIX: &str = "mainCoordinator:";

fn stored(db: &Connection, project_id: &str) -> Option<String> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [format!("{KEY_PREFIX}{project_id}")],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn store(db: &Connection, project_id: &str, session_id: &str) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![format!("{KEY_PREFIX}{project_id}"), session_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// A coordinator that can carry the label: its session is still there and not closed, and it is
/// neither Chief of Staff nor a GBot's chat.
fn can_be_main(
    record: &CoordinatorRecord,
    repository: &DomainRepository<'_>,
    chief: Option<&SessionKey>,
) -> bool {
    if chief == Some(&record.key()) {
        return false;
    }
    let Ok(Some(session)) = repository.get_session(&record.project_id, &record.session_id) else {
        return false;
    };
    // A GBot's chat carries `runtimeSettings.gbot` (gbots/session.rs `CHAT_KEY`).
    crate::presentation::effective_lifecycle_state(&session) != "stopped"
        && session.pointer("/runtimeSettings/gbot").is_none()
}

/// Every project's main coordinator, by project id (`only`: just that project's). Keeps the
/// answer as the pick where it changed (a first coordinator, or the main one closed), so it stays
/// put until the user moves it.
fn mains(
    db: &Connection,
    repository: &DomainRepository<'_>,
    only: Option<&str>,
) -> BTreeMap<String, String> {
    let Ok(records) = list_coordinators(db) else {
        return BTreeMap::new();
    };
    let chief = crate::chief::read_record(db).map(|record| record.key());
    // Oldest first (`list_coordinators` orders by createdAt).
    let mut candidates: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in records
        .iter()
        .filter(|record| only.is_none_or(|project| project == record.project_id))
        .filter(|record| can_be_main(record, repository, chief.as_ref()))
    {
        candidates
            .entry(record.project_id.clone())
            .or_default()
            .push(record.session_id.clone());
    }
    candidates
        .into_iter()
        .map(|(project_id, sessions)| {
            let picked = stored(db, &project_id).filter(|picked| sessions.contains(picked));
            let main = picked.clone().unwrap_or_else(|| sessions[0].clone());
            if picked.is_none() {
                let _ = store(db, &project_id, &main);
            }
            (project_id, main)
        })
        .collect()
}

/// Every project's main coordinator on this computer, by project id.
pub fn main_coordinators(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> BTreeMap<String, String> {
    mains(db, repository, None)
}

/// The main coordinator of one project, if it has any coordinator that can carry the label.
pub fn main_coordinator_of(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Option<SessionKey> {
    mains(db, repository, Some(project_id))
        .remove(project_id)
        .map(|session_id| (project_id.to_string(), session_id))
}

/// `/api/setMainCoordinator`: makes `key` its project's main coordinator, taking the label from
/// the one that had it. Answers the sessions whose rows changed (the old main and the new one).
pub fn set_main_coordinator(
    db: &Connection,
    repository: &DomainRepository<'_>,
    key: &SessionKey,
) -> Result<Vec<SessionKey>, DomainStateError> {
    let record = super::records::read_coordinator(db, &key.0, &key.1)?
        .ok_or_else(|| DomainStateError::not_found("That session is not a coordinator."))?;
    let chief = crate::chief::read_record(db).map(|record| record.key());
    if !can_be_main(&record, repository, chief.as_ref()) {
        return Err(DomainStateError::bad_request(
            "Only an open coordinator of a project can be its main coordinator (not Chief of Staff, a GBot's chat or a closed one).",
        ));
    }
    let previous = main_coordinator_of(db, repository, &key.0);
    store(db, &key.0, &key.1)?;
    Ok(previous
        .into_iter()
        .filter(|previous| previous != key)
        .chain(std::iter::once(key.clone()))
        .collect())
}

/// The coordinators that become their project's main one now that `closed` is closed or gone,
/// so their rows can be redrawn with the label.
pub fn successor_after_close(
    db: &Connection,
    repository: &DomainRepository<'_>,
    closed: &SessionKey,
) -> Option<SessionKey> {
    let was_main = stored(db, &closed.0).as_deref() == Some(closed.1.as_str());
    if !was_main {
        return None;
    }
    main_coordinator_of(db, repository, &closed.0).filter(|successor| successor != closed)
}

/// Whether a presentation session is its project's main coordinator, for `isMainCoordinator`.
pub(crate) fn is_main_in(mains: &BTreeMap<String, String>, session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
    mains.get(text("projectId")).map(String::as_str) == Some(text("sessionId"))
}
