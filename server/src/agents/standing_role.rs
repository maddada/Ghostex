//! Per-session standing role line (`ghostex agents role set|show|clear`, `--role` on
//! `agents create` and `agents link`): a short standing rule any session can register, handed back
//! after `/clear` or compaction just like a coordinator's guide line so ordinary leads and linked
//! workers compacting mid-job keep their hand-in rules.
//!
//! Stored in gxserver's SQLite `metadata` table (`sessionStandingRole:<projectId>:<sessionId>`) so
//! it survives restarts cleanly, never collides with `runtimeSettings` full-object rewrites, and
//! remains strictly additive for v11.2.1+ mobile apps and older clients.
//!
//! SEE-ALSO: server/src/agents/activity.rs (`coordinator_session_start_context`),
//! server/src/coordinators/role.rs (`coordinator_role_again`),
//! server/src/coordinators/drift.rs (`newest_compaction_for_session`),
//! server/src/session_chat_queue_runtime/send.rs (`coordinator_playbook_note`),
//! server/src/ghostex_cli/agents/role.rs.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use crate::coordinators::{now_iso, sql_error, SessionKey};
use crate::domain::{DomainRepository, DomainStateError};
use crate::ids::create_global_session_ref;
use crate::security::Principal;

pub const SESSION_STANDING_ROLE_ENDPOINT: &str = "/api/sessionStandingRole";

/// Character cap for a session's standing role line (matching coordinator memory note budget).
pub const SESSION_STANDING_ROLE_MAX_CHARS: usize = 600;

const METADATA_PREFIX: &str = "sessionStandingRole:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingRoleRecord {
    pub role: String,
    pub updated_at: String,
    /// The newest transcript compaction row id already handed back on a non-hook agent.
    pub compaction: Option<String>,
}

/// The post-compaction note to deliver with the next message to a non-hook agent, and the
/// compaction row id to record once delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingRoleDelivery {
    pub note: String,
    pub compaction: String,
}

fn metadata_key(project_id: &str, session_id: &str) -> String {
    format!("{METADATA_PREFIX}{project_id}:{session_id}")
}

fn in_write_transaction<T>(
    db: &Connection,
    work: impl FnOnce(&Connection) -> Result<T, DomainStateError>,
) -> Result<T, DomainStateError> {
    if !db.is_autocommit() {
        return work(db);
    }
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(sql_error)?;
    let value = work(&transaction)?;
    transaction.commit().map_err(sql_error)?;
    Ok(value)
}

/// Validates and normalizes a standing role line into a single trimmed line within
/// [`SESSION_STANDING_ROLE_MAX_CHARS`]. Rejects blank input.
pub fn normalize_standing_role(raw: &str) -> Result<String, DomainStateError> {
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(DomainStateError::bad_request(
            "Standing role must not be empty. Use `ghostex agents role clear` to remove it.",
        ));
    }
    if normalized.chars().count() > SESSION_STANDING_ROLE_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Standing role must be at most {SESSION_STANDING_ROLE_MAX_CHARS} characters."
        )));
    }
    Ok(normalized)
}

/// Whether the session's agent has a `SessionStart` hook that reliably fires on `/clear` and
/// compaction and injects `additionalContext` (`claude`, `codex`). Every other agent gets its
/// post-compaction role back once on the next delivered message instead.
pub(crate) fn hook_hands_role_back(session: &Value) -> bool {
    matches!(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
        Some("claude" | "codex")
    )
}

pub fn read_standing_role(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Option<StandingRoleRecord> {
    let raw = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [metadata_key(project_id, session_id)],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()?;
    let value = serde_json::from_str::<Value>(&raw).ok()?;
    let role = value.get("role")?.as_str()?.trim();
    if role.is_empty() {
        return None;
    }
    Some(StandingRoleRecord {
        role: role.to_string(),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        compaction: value
            .get("compaction")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
    })
}

fn store_record(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    record: &StandingRoleRecord,
) -> Result<(), DomainStateError> {
    let payload = json!({
        "role": record.role,
        "updatedAt": record.updated_at,
        "compaction": record.compaction,
    });
    db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        params![
            metadata_key(project_id, session_id),
            payload.to_string(),
            record.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Stores or replaces the standing role for `key`. Seeds `compaction` from the session's current
/// transcript tail (when available) so an older compaction that predates `role set` does not
/// trigger a stale reminder on the next message.
pub fn write_standing_role(
    db: &Connection,
    key: &SessionKey,
    raw_role: &str,
    session: Option<&Value>,
) -> Result<StandingRoleRecord, DomainStateError> {
    let role = normalize_standing_role(raw_role)?;
    let seen_compaction = session.and_then(crate::coordinators::newest_compaction_for_session);
    in_write_transaction(db, |db| {
        let existing = read_standing_role(db, &key.0, &key.1);
        let compaction = seen_compaction.or_else(|| existing.and_then(|record| record.compaction));
        let record = StandingRoleRecord {
            role,
            updated_at: now_iso(),
            compaction,
        };
        store_record(db, &key.0, &key.1, &record)?;
        Ok(record)
    })
}

/// Removes the standing role for `(project_id, session_id)`. Returns whether one was present.
pub fn clear_standing_role(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    in_write_transaction(db, |db| {
        let removed = db
            .execute(
                "DELETE FROM metadata WHERE key = ?1",
                [metadata_key(project_id, session_id)],
            )
            .map_err(sql_error)?;
        Ok(removed > 0)
    })
}

/// Moves a closed session's standing role onto the restored session (when the restored session does
/// not already have one).
pub fn carry_standing_role(db: &Connection, from: (&str, &str), to: (&str, &str)) {
    if from == to {
        return;
    }
    let _ = in_write_transaction(db, |db| {
        let Some(record) = read_standing_role(db, from.0, from.1) else {
            return Ok(());
        };
        if read_standing_role(db, to.0, to.1).is_none() {
            store_record(db, to.0, to.1, &record)?;
        }
        db.execute(
            "DELETE FROM metadata WHERE key = ?1",
            [metadata_key(from.0, from.1)],
        )
        .map_err(sql_error)?;
        Ok(())
    });
}

/// Records that the standing role was handed back after `compaction`, so later messages do not
/// repeat it until a newer compaction appears. Does nothing if the standing role was cleared.
pub fn record_standing_role_compaction(db: &Connection, key: &SessionKey, compaction: &str) {
    let _ = in_write_transaction(db, |db| {
        let Some(mut record) = read_standing_role(db, &key.0, &key.1) else {
            return Ok(());
        };
        if record.compaction.as_deref() == Some(compaction) {
            return Ok(());
        }
        record.compaction = Some(compaction.to_string());
        store_record(db, &key.0, &key.1, &record)
    });
}

/// Combines coordinator `SessionStart` context (if any) with a session's standing role line on
/// Claude and Codex `SessionStart` (`clear`, `compact`, `startup`). Returns `None` for `resume`,
/// `fork`, and any other source whose conversation still holds its earlier context.
pub(crate) fn session_start_context(
    coordinator_context: Option<&str>,
    standing_role: Option<&str>,
    source: &str,
) -> Option<String> {
    let role = standing_role.map(str::trim).filter(|role| !role.is_empty());
    match (coordinator_context, role) {
        (Some(coordinator), Some(role)) => {
            Some(format!("{coordinator}\n\nStanding role for this session: {role}"))
        }
        (Some(coordinator), None) => Some(coordinator.to_string()),
        (None, Some(role)) => match source {
            "clear" => Some(format!(
                "Your conversation was cleared. Standing role for this session (keep following this): {role}"
            )),
            "compact" => Some(format!(
                "Your conversation was compacted. Standing role for this session (keep following this): {role}"
            )),
            "startup" => Some(format!("Standing role for this session: {role}")),
            _ => None,
        },
        (None, None) => None,
    }
}

/// Standing role paragraph added to `SessionStart` / `agentSpawn` context for
/// `SESSION_START_NOTE_AGENTS` (`zcode`, `antigravity`, `kiro`, `codebuddy`, `qoder`).
pub(crate) fn session_start_note_for_other_agent(standing_role: &str, source: &str) -> String {
    let role = standing_role.trim();
    match source {
        "clear" => format!(
            "Your conversation was cleared. Standing role for this session (keep following this): {role}"
        ),
        "compact" => format!(
            "Your conversation was compacted. Standing role for this session (keep following this): {role}"
        ),
        _ => format!("Standing role for this session: {role}"),
    }
}

/// Neutralizes `<ghostex-system>` tags and sender markers so a standing role delivered inside a
/// `<ghostex-system>` block can never close the block early or forge another agent's message.
fn neutralised_standing_role(role: &str) -> String {
    ghostex_gx_chat_core::session::appended_notes::neutralised_held_note(role)
}

/// Post-compaction standing role note for the next delivered message to an agent whose hooks cannot
/// inject `SessionStart` context on compaction (anything other than Claude and Codex).
///
/// When `coordinator_compaction_noted` is true (this session is also a coordinator already getting
/// `CONTEXT_COMPACTED_NOTE` in the same delivery), omits the redundant compaction preamble so
/// coordinator reminders are never duplicated.
pub fn standing_role_compaction_delivery(
    db: &Connection,
    key: &SessionKey,
    session: &Value,
    coordinator_compaction_noted: bool,
) -> Option<StandingRoleDelivery> {
    if hook_hands_role_back(session) {
        return None;
    }
    let record = read_standing_role(db, &key.0, &key.1)?;
    let compaction = crate::coordinators::newest_compaction_for_session(session)?;
    if record.compaction.as_deref() == Some(compaction.as_str()) {
        return None;
    }
    let safe_role = neutralised_standing_role(&record.role);
    let note = if coordinator_compaction_noted {
        format!("Standing role for this session: {safe_role}")
    } else {
        format!("Your conversation was compacted. Standing role for this session: {safe_role}")
    };
    Some(StandingRoleDelivery { note, compaction })
}

fn param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn caller_allowed(caller: &Principal) -> bool {
    matches!(
        caller,
        Principal::Owner { .. }
            | Principal::Device { .. }
            | Principal::LocalAgent
            | Principal::Session { .. }
            | Principal::Peer { .. }
    )
}

/// `/api/sessionStandingRole`: `{ op: "set" | "show" | "clear", projectId, sessionId, role? }`.
pub fn handle_standing_role_endpoint(
    caller: &Principal,
    db: &Connection,
    repository: &DomainRepository<'_>,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if !caller_allowed(caller) {
        return Err(DomainStateError::bad_request(
            "Only the user or an agent session can read or change a session's standing role.",
        ));
    }
    let op = param_text(params, "op").unwrap_or_default();
    let (Some(project_id), Some(session_id)) =
        (param_text(params, "projectId"), param_text(params, "sessionId"))
    else {
        return Err(DomainStateError::bad_request(
            "This standing role request needs projectId and sessionId.",
        ));
    };
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found("That session no longer exists."))?;
    let global_ref = create_global_session_ref(server_id, &project_id, &session_id);
    let key = (project_id.clone(), session_id.clone());
    match op.as_str() {
        "set" => {
            let role = params
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let record = write_standing_role(db, &key, role, Some(&session))?;
            Ok(json!({
                "ok": true,
                "status": "set",
                "projectId": project_id,
                "sessionId": session_id,
                "globalRef": global_ref,
                "role": record.role,
                "updatedAt": record.updated_at,
            }))
        }
        "show" => {
            let record = read_standing_role(db, &project_id, &session_id);
            Ok(json!({
                "ok": true,
                "projectId": project_id,
                "sessionId": session_id,
                "globalRef": global_ref,
                "role": record.as_ref().map(|item| item.role.as_str()),
                "updatedAt": record.as_ref().map(|item| item.updated_at.as_str()),
            }))
        }
        "clear" => {
            let cleared = clear_standing_role(db, &project_id, &session_id)?;
            Ok(json!({
                "ok": true,
                "status": if cleared { "cleared" } else { "unchanged" },
                "cleared": cleared,
                "projectId": project_id,
                "sessionId": session_id,
                "globalRef": global_ref,
            }))
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown standing role op \"{other}\"."
        ))),
    }
}
