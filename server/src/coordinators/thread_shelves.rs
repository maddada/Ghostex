//! A coordinator's thread is never parked, pinned or snoozed: it always shows under its
//! coordinator.
//!
//! CDXC:Coordinators 2026-11-25 DECISION:
//! User: "i want threads that are under a coordinator to not be able to be away from it. they always appear below it. and no parking/pinning for those, no need." A session linked to a coordinator (a `coordinator_threads` row) is never parked, pinned or snoozed, since each of those moved it out of its coordinator's tree into another section: gxserver refuses Park, Pin and Snooze on it (`/api/updateSession`, `/api/snoozeSession`, so the CLI and older phones are refused too), linking a session as a thread takes it off those shelves, and storage migration 0089 did that once for the threads already parked, pinned or snoozed. Unparking, unpinning and unsnoozing stay allowed. A session linked to a lead (lead links) is an ordinary session and keeps them. Supersedes "Only the user parks a thread, by hand" in the 2026-11-14 decision on `set_parked` (endpoint.rs).
//! SEE-ALSO: packages/gx-core/src/sidebar_view/threads.rs `is_nestable_thread`, packages/gx-core/src/sidebar_menu/session.rs (no Park, Pin or Snooze on a thread), packages/gx-core/src/sidebar_drag/session_drop.rs (no drop into Pinned or Parked), apps/mobile/app/src/contract/coordinatorTree.ts (the phone's twin).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::records::read_thread;
use crate::domain::{DomainRepository, DomainStateError};

const REFUSED: &str =
    "A coordinator's thread always stays under its coordinator, so it can't be parked, pinned or snoozed.";

/// Refuses an `/api/updateSession` that parks or pins a coordinator's thread.
pub fn refuse_shelving_thread(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let shelves = ["isParked", "isPinned"]
        .iter()
        .any(|key| params.get(*key).and_then(Value::as_bool) == Some(true));
    if !shelves {
        return Ok(());
    }
    refuse_for_thread(db, params)
}

/// Refuses an `/api/snoozeSession` on a coordinator's thread.
pub fn refuse_snoozing_thread(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    refuse_for_thread(db, params)
}

fn refuse_for_thread(db: &Connection, params: &Map<String, Value>) -> Result<(), DomainStateError> {
    let (Some(project_id), Some(session_id)) = (
        params.get("projectId").and_then(Value::as_str),
        params.get("sessionId").and_then(Value::as_str),
    ) else {
        return Ok(());
    };
    if read_thread(db, project_id, session_id)?.is_some() {
        return Err(DomainStateError::bad_request(REFUSED));
    }
    Ok(())
}

/// Takes a session just linked as a thread out of Parked, Pinned and Snoozed, so it shows under
/// its coordinator. Returns whether anything changed.
pub fn keep_thread_under_coordinator(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    now_iso: &str,
) -> Result<bool, DomainStateError> {
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(false);
    };
    let flag = |key: &str| session.get(key).and_then(Value::as_bool) == Some(true);
    let mut changed = false;
    if flag("isParked") || flag("isPinned") {
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert("isParked".to_string(), Value::Bool(false));
        update.insert("isPinned".to_string(), Value::Bool(false));
        repository.update_session(&update)?;
        changed = true;
    }
    if session
        .get("snoozedUntil")
        .and_then(Value::as_str)
        .is_some_and(|until| !until.trim().is_empty())
    {
        changed |= crate::session_lifecycle::unsnooze_session(
            repository, project_id, session_id, now_iso,
        )?
        .changed;
    }
    Ok(changed)
}
