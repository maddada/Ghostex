//! The record in memory, for readers that do no I/O (the presentation projection, a typed
//! reference, a source's sync rules). gxserver loads it before it answers its first request
//! (`load_work_source_store`), and every change publishes what it wrote (`save_work_sources`;
//! a workspace deletion when its transaction commits), so it holds what the database holds.
//!
//! A change publishes the difference between the record it read and the one it wrote, under the
//! store's writer lock: one workspace's entry and the retired ids it added or pruned. Since every
//! write goes through the writer lock, that keeps the copy equal to the database.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, OnceLock, RwLock};

use rusqlite::Connection;

use crate::domain::DomainStateError;

use super::*;

#[derive(Default)]
struct Entries {
    workspaces: HashMap<String, Arc<WorkspaceCommand>>,
    retired: HashMap<String, Arc<RetiredSource>>,
}

fn entries() -> &'static RwLock<Entries> {
    static ENTRIES: OnceLock<RwLock<Entries>> = OnceLock::new();
    ENTRIES.get_or_init(|| RwLock::new(Entries::default()))
}

/// Startup, before the first request: interns every id the record holds, active and retired, so
/// a link saved under a retired id still parses, and copies the record. A record that can't be
/// read leaves no command anywhere; the store's own writers refuse until it is fixed.
pub(crate) fn load_work_source_store(db: &Connection) -> Result<(), DomainStateError> {
    let sources = read_work_sources(db)?;
    registry()
        .intern_all(sources.known_ids())
        .map_err(too_many)?;
    publish(&WorkSources::default(), &sources);
    Ok(())
}

/// Writes `sources` and publishes what changed. Under the store's writer lock and outside a
/// transaction that could roll back. An id this process never held is interned first, so a
/// record past the bound is refused before anything is written.
pub(crate) fn save_work_sources(
    db: &Connection,
    sources: &WorkSources,
) -> Result<(), DomainStateError> {
    let old = read_work_sources(db)?;
    registry()
        .intern_all(sources.known_ids())
        .map_err(too_many)?;
    write_work_sources(db, sources)?;
    publish(&old, sources);
    Ok(())
}

/// `save_work_sources` and `also`, another write to the same database, as one unit: in one
/// savepoint, so both are kept or neither, and the copy in memory follows only once both are. A
/// clear or a Switch moves the workspace's tracker this way (`follow_command_tracker`): a tracker
/// write that failed after the record's would leave a cleared command picked, or a switched one
/// with the old id as its tracker, which no retry could repair once the old id was gone.
pub(crate) fn save_work_sources_with(
    db: &Connection,
    sources: &WorkSources,
    also: impl FnOnce(&Connection) -> Result<(), DomainStateError>,
) -> Result<(), DomainStateError> {
    // Inside a caller's transaction, releasing the savepoint commits nothing: the copy in memory
    // would be published ahead of a write the caller could still roll back.
    if !db.is_autocommit() {
        return Err(DomainStateError {
            code: "internalError",
            message: "Ghostex refused a ticket command change made inside another change, and changed nothing.".to_string(),
        });
    }
    let old = read_work_sources(db)?;
    registry()
        .intern_all(sources.known_ids())
        .map_err(too_many)?;
    db.execute_batch("SAVEPOINT work_sources_change")
        .map_err(sql_error)?;
    match write_work_sources(db, sources).and_then(|()| also(db)) {
        Ok(()) => {
            db.execute_batch("RELEASE work_sources_change")
                .map_err(sql_error)?;
            publish(&old, sources);
            Ok(())
        }
        Err(error) => {
            let _ =
                db.execute_batch("ROLLBACK TO work_sources_change; RELEASE work_sources_change;");
            Err(error)
        }
    }
}

/// The startup step: loads the record and the tickets' last titles before the first request, and
/// logs one error in fixed words for what can't be read (a damaged row can hold a credential, so
/// nothing of it is quoted).
pub(crate) fn load_work_source_store_at_startup(
    paths: &crate::paths::GxserverPaths,
    logger: &crate::logging::GxserverLogger,
) {
    let log = |event: &str, error: String| {
        let _ = logger.log(crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Error,
            event: event.to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(error),
            details: None,
        });
    };
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        log(
            "work_sources_unreadable",
            "Ticket commands are off until restart: the database can't be opened.".to_string(),
        );
        return;
    };
    if load_work_source_store(&db).is_err() {
        log(
            "work_sources_unreadable",
            "Ticket commands are off until restart: the record can't be read.".to_string(),
        );
    }
    if load_ticket_titles(&db).is_err() {
        log(
            "work_ticket_titles_unreadable",
            "Ticket commands' tickets show no last title until restart: the titles can't be read."
                .to_string(),
        );
    }
}

fn too_many(text: &'static str) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: text.to_string(),
    }
}

/// Applies the difference between `old` and `new` to the copy.
pub(super) fn publish(old: &WorkSources, new: &WorkSources) {
    let mut entries = entries()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let workspaces: BTreeSet<&String> =
        old.workspaces.keys().chain(new.workspaces.keys()).collect();
    for workspace_id in workspaces {
        match new.workspaces.get(workspace_id) {
            Some(command) if old.workspaces.get(workspace_id) != Some(command) => {
                entries
                    .workspaces
                    .insert(workspace_id.clone(), Arc::new(command.clone()));
            }
            Some(_) => {}
            None => {
                entries.workspaces.remove(workspace_id);
            }
        }
    }
    for retired in &old.retired {
        if !new.retired.iter().any(|kept| kept.id == retired.id) {
            entries.retired.remove(&retired.id);
        }
    }
    for retired in &new.retired {
        entries
            .retired
            .insert(retired.id.clone(), Arc::new(retired.clone()));
    }
}

/// The workspace's command, as last written.
pub(crate) fn workspace_command(workspace_id: &str) -> Option<Arc<WorkspaceCommand>> {
    entries()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .workspaces
        .get(workspace_id)
        .cloned()
}

/// An id's `ids.format` on this computer, the one fact that is id-wide: any workspace that pins
/// it, else its retired record.
pub(crate) fn id_format(source: CommandSourceId) -> Option<IdFormat> {
    let entries = entries()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    entries
        .workspaces
        .values()
        .find(|command| command.source_id == source.as_str())
        .map(|command| command.info.id_format)
        .or_else(|| {
            entries
                .retired
                .get(source.as_str())
                .map(|retired| retired.format)
        })
}

/// The id's retired record, when no workspace pins it any more.
pub(crate) fn retired_source(source: CommandSourceId) -> Option<Arc<RetiredSource>> {
    entries()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retired
        .get(source.as_str())
        .cloned()
}
