//! The one-time move of Main Hub's chats into No project.
//!
//! CDXC:Projects 2026-11-27 WHY:
//! The brief: Main Hub's existing chats (its Chats list) move into No project on upgrade, none lost. A chat is an agent session of the home project that is not Chief of Staff's (current or earlier), a coordinator, a coordinator's thread or a GBot's; terminals, coordinators and their threads stay in Main Hub. Each one keeps its session id, zmx daemon, conversation, folder (~) and every row Ghostex keeps about it: the move re-keys `projectId` in every table that pairs a project id with a session id (found from the schema, so a table added later is covered too), in one transaction with foreign keys checked at commit. Only its ref changes (S90:<No project>:<same id>). Hooks and the `ghostex` CLI find a session by its zmx name, so even so a running agent is never moved under its own feet: a chat running at the upgrade is skipped and moved at a later gxserver start once it is not, and chats made in Main Hub after the upgrade (an older phone's No project pill) stay there.

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{schedule_presentation_project_delta, AppState};

/// When the move first ran here; chats created later are not moved.
const STARTED_KEY: &str = "noProjectMoveStartedAt";
/// Set once no chat is left to move.
const DONE_KEY: &str = "noProjectMoveDone";

fn metadata(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
}

fn write_metadata(db: &Connection, key: &str, value: &str) {
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        params![key, value, crate::coordinators::now_iso()],
    );
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn in_table(db: &Connection, table: &str, project_id: &str, session_id: &str) -> bool {
    db.query_row(
        &format!("SELECT 1 FROM {table} WHERE projectId = ?1 AND sessionId = ?2"),
        params![project_id, session_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Whether `session` of the home project is a chat that belongs in No project.
fn is_main_hub_chat(db: &Connection, session: &Value) -> bool {
    let (project_id, session_id) = (text(session, "projectId"), text(session, "sessionId"));
    if text(session, "kind") != "agent" {
        return false;
    }
    let key = (project_id.to_string(), session_id.to_string());
    if crate::chief::read_record(db).is_some_and(|record| record.owns(&key)) {
        return false;
    }
    if in_table(db, "coordinators", project_id, session_id)
        || in_table(db, "coordinator_threads", project_id, session_id)
    {
        return false;
    }
    let runtime = session.get("runtimeSettings").and_then(Value::as_object);
    !crate::agents::is_gbot_chat(runtime)
}

pub(super) fn move_main_hub_chats(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    no_project: &Value,
) {
    if metadata(db, DONE_KEY).is_some() {
        return;
    }
    let started_at = metadata(db, STARTED_KEY).unwrap_or_else(|| {
        let now = crate::coordinators::now_iso();
        write_metadata(db, STARTED_KEY, &now);
        now
    });
    let target = text(no_project, "projectId").to_string();
    let Ok(projects) = repository.list_projects() else {
        return;
    };
    let homes: Vec<String> = projects
        .iter()
        .filter(|project| crate::chief::is_home_project(project))
        .map(|project| text(project, "projectId").to_string())
        .filter(|id| !id.is_empty() && *id != target)
        .collect();
    let mut deferred = 0usize;
    for home in homes {
        let Ok(sessions) = repository.list_sessions(Some(&home)) else {
            continue;
        };
        let mut chats = Vec::new();
        for session in sessions.iter().filter(|s| is_main_hub_chat(db, s)) {
            // ISO times in one format compare as text.
            if text(session, "createdAt") > started_at.as_str() {
                continue;
            }
            if text(session, "lifecycleState") == "running" {
                deferred += 1;
                continue;
            }
            chats.push(text(session, "sessionId").to_string());
        }
        if chats.is_empty() {
            continue;
        }
        // A chat whose id is taken in No project stays in Main Hub rather than change its id.
        let moves: Vec<(String, String)> = chats
            .iter()
            .filter(|id| !session_id_taken(db, &target, id))
            .map(|id| (id.clone(), id.clone()))
            .collect();
        if moves.is_empty() {
            continue;
        }
        match rekey_sessions(db, &home, &target, &moves) {
            Ok(()) => {
                eprintln!(
                    "Ghostex moved {} of Main Hub's chats to No project.",
                    moves.len()
                );
                let _ = schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    &home,
                    "projectUpdated",
                );
                let _ = schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    &target,
                    "projectUpdated",
                );
            }
            Err(error) => {
                eprintln!(
                    "Ghostex could not move Main Hub's chats to No project: {}",
                    error.message
                );
                return;
            }
        }
    }
    if deferred == 0 {
        write_metadata(db, DONE_KEY, "true");
    }
}

/// Every `(…projectId, …sessionId)` column pair in the database, by table.
fn session_key_columns(db: &Connection) -> rusqlite::Result<Vec<(String, String, String)>> {
    let mut statement = db.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let tables: Vec<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut pairs = Vec::new();
    for table in tables {
        let mut info = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let columns: BTreeSet<String> = info
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<_>>()?;
        for column in &columns {
            let session_column = if let Some(prefix) = column.strip_suffix("ProjectId") {
                format!("{prefix}SessionId")
            } else if column == "projectId" {
                "sessionId".to_string()
            } else {
                continue;
            };
            if columns.contains(&session_column) {
                pairs.push((table.clone(), column.clone(), session_column));
            }
        }
    }
    Ok(pairs)
}

/// Whether `session_id` is already used (or was ever allocated) in `project_id`.
pub(super) fn session_id_taken(db: &Connection, project_id: &str, session_id: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM id_allocations WHERE kind = 'session' AND parentId = ?1 AND id = ?2",
        params![project_id, session_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
        || in_table(db, "sessions", project_id, session_id)
}

/// Moves each `(old id, new id)` session from project `from` to `to` in one transaction, all or
/// none: every row that pairs a project id with a session id follows it. The caller picks new ids
/// that are free in `to` ([`session_id_taken`]).
pub(super) fn rekey_sessions(
    db: &Connection,
    from: &str,
    to: &str,
    moves: &[(String, String)],
) -> Result<(), DomainStateError> {
    let error = |error: rusqlite::Error| DomainStateError::corrupt_state(error.to_string());
    let pairs = session_key_columns(db).map_err(error)?;
    db.execute_batch("BEGIN IMMEDIATE; PRAGMA defer_foreign_keys = ON;")
        .map_err(error)?;
    let result = (|| {
        let now = crate::coordinators::now_iso();
        for (old_id, new_id) in moves {
            for (table, project_column, session_column) in &pairs {
                db.execute(
                    &format!(
                        "UPDATE \"{table}\" SET \"{project_column}\" = ?1, \"{session_column}\" = ?2 WHERE \"{project_column}\" = ?3 AND \"{session_column}\" = ?4"
                    ),
                    params![to, new_id, from, old_id],
                )?;
            }
            // The old id stays allocated in `from` too, so no later session there reuses it.
            db.execute(
                "INSERT OR IGNORE INTO id_allocations (id, kind, parentId, createdAt) VALUES (?1, 'session', ?2, ?3)",
                params![new_id, to, now],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => db.execute_batch("COMMIT;").map_err(error),
        Err(failure) => {
            let _ = db.execute_batch("ROLLBACK;");
            Err(error(failure))
        }
    }
}
