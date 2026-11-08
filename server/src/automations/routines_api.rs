//! What GBots (server/src/gbots/routines.rs) ask of Automations: save, read, pause, remove and run
//! a GBot's routine, which is an automation in the bot's project with an owner.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};

use super::*;

/// A routine as a GBot sets it.
pub(crate) struct RoutineSave<'a> {
    /// The routine to change; a new one without.
    pub id: Option<&'a str>,
    pub project_id: &'a str,
    pub agent_id: &'a str,
    pub bot_id: &'a str,
    pub name: &'a str,
    pub prompt: &'a str,
    pub schedule: &'a Value,
    pub precheck: Option<&'a Value>,
    pub deliver: &'a str,
    /// `new` (a fresh chat each run) or `standing` (one chat the routine keeps).
    pub continuity: &'a str,
    pub enabled: bool,
    pub created_by: &'a str,
}

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

/// Refuses a schedule whose runs come closer than 15 minutes.
fn check_gap(schedule: &Value) -> Result<(), DomainStateError> {
    let first = compute_next_run_at(schedule, None);
    let Some(first) = first.and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok()) else {
        return Err(bad(
            "That schedule never runs. Pick a time in the future, or a schedule that repeats.",
        ));
    };
    let first = first.with_timezone(&chrono::Utc);
    if let Some(second) = compute_next_run_at(schedule, Some(first))
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
    {
        let gap = second.with_timezone(&chrono::Utc) - first;
        if gap.num_milliseconds() < ROUTINE_MIN_GAP_MS {
            return Err(bad(
                "A routine runs at most every 15 minutes. Pick a schedule with runs further apart.",
            ));
        }
    }
    Ok(())
}

/// Saves a routine and answers it as [`routine_view`] shows it.
pub(crate) fn save_routine(
    db: &Connection,
    save: RoutineSave<'_>,
) -> Result<Value, DomainStateError> {
    let schedule = normalize_schedule(Some(save.schedule))?;
    check_gap(&schedule)?;
    let precheck = save
        .precheck
        .filter(|value| !value.is_null())
        .map(normalize_precheck)
        .transpose()
        .map_err(bad)?;
    if !matches!(save.deliver, "chat" | "chief") {
        return Err(bad(
            "A routine's result goes to its own chat (chat) or to Chief of Staff (chief).",
        ));
    }
    if !matches!(save.continuity, "new" | "standing") {
        return Err(bad(
            "A routine runs in a new chat each time (new) or keeps one chat (standing).",
        ));
    }
    let now = now_iso();
    let previous = match save.id {
        Some(id) => read_automation_by_id(db, id)?,
        None => None,
    };
    let id = save
        .id
        .map(str::to_string)
        .unwrap_or_else(|| format!("automation-{}", uuid::Uuid::new_v4()));
    let next_run_at = if save.enabled {
        compute_next_run_at(&schedule, None)
    } else {
        None
    };
    let record = AutomationDefinitionRecord {
        agent_id: save.agent_id.to_string(),
        created_at: previous
            .as_ref()
            .map(|routine| routine.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        enabled: save.enabled,
        execution_mode: json!({ "kind": "local" }),
        id: id.clone(),
        name: save.name.trim().to_string(),
        next_run_at,
        project_id: save.project_id.to_string(),
        prompt: save.prompt.trim().to_string(),
        schedule,
        updated_at: now,
        owner_bot_id: Some(save.bot_id.to_string()),
        precheck: precheck.clone(),
        deliver: Some(save.deliver.to_string()),
    };
    upsert_automation(db, &record)?;
    // A new or changed check starts from a fresh look, so it never runs on what was there before.
    let check_changed =
        previous.as_ref().map(|routine| routine.precheck.clone()) != Some(precheck.clone());
    let precheck_text = precheck.as_ref().map(|value| value.to_string());
    db.execute(
        "UPDATE automations SET ownerBotId = ?2, precheckJson = ?3, deliver = ?4, \
         lastCheckFingerprint = CASE WHEN ?5 THEN NULL ELSE lastCheckFingerprint END, \
         failureStreak = CASE WHEN ?5 THEN 0 ELSE failureStreak END WHERE automationId = ?1",
        params![id, save.bot_id, precheck_text, save.deliver, check_changed],
    )
    .map_err(sql_error)?;
    db.execute(
        "INSERT INTO gbots_routine_checks (automationId, continuity, createdBy) VALUES (?1, ?2, ?3) \
         ON CONFLICT(automationId) DO UPDATE SET continuity = excluded.continuity",
        params![id, save.continuity, save.created_by],
    )
    .map_err(sql_error)?;
    if check_changed {
        db.execute(
            "UPDATE gbots_routine_checks SET lastCheckAt = NULL, lastCheckOutcome = NULL, \
             lastCheckNote = NULL, quietChecks = 0, retryAt = NULL WHERE automationId = ?1",
            [&id],
        )
        .map_err(sql_error)?;
    }
    routine_view(db, &id)?.ok_or_else(|| DomainStateError::not_found("The routine was not saved."))
}

fn continuity_and_creator(db: &Connection, id: &str) -> (String, String) {
    db.query_row(
        "SELECT continuity, createdBy FROM gbots_routine_checks WHERE automationId = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_else(|| ("new".to_string(), "user".to_string()))
}

/// How a routine's chats continue (`new` or `standing`) and the standing chat it keeps.
pub(crate) fn routine_continuity(db: &Connection, id: &str) -> (String, Option<String>) {
    let (continuity, _) = continuity_and_creator(db, id);
    let standing = db
        .query_row(
            "SELECT standingChatKey FROM gbots_routine_checks WHERE automationId = ?1",
            [id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten();
    (continuity, standing)
}

pub(crate) fn set_routine_standing_chat(db: &Connection, id: &str, chat: &str) {
    let _ = db.execute(
        "UPDATE gbots_routine_checks SET standingChatKey = ?2 WHERE automationId = ?1",
        params![id, chat],
    );
}

/// The routine's definition as the change journal keeps it (no run state).
pub(crate) fn routine_definition(
    db: &Connection,
    id: &str,
) -> Result<Option<Value>, DomainStateError> {
    let Some(routine) = read_automation_by_id(db, id)? else {
        return Ok(None);
    };
    let (continuity, _) = continuity_and_creator(db, id);
    Ok(Some(json!({
        "id": routine.id,
        "projectId": routine.project_id,
        "agentId": routine.agent_id,
        "botId": routine.owner_bot_id,
        "name": routine.name,
        "prompt": routine.prompt,
        "schedule": routine.schedule,
        "precheck": routine.precheck,
        "deliver": routine.deliver.unwrap_or_else(|| "chat".to_string()),
        "continuity": continuity,
    })))
}

/// A routine as GBots shows it: its definition in words, its next run and its last run and check.
pub(crate) fn routine_view(db: &Connection, id: &str) -> Result<Option<Value>, DomainStateError> {
    let Some(routine) = read_automation_by_id(db, id)? else {
        return Ok(None);
    };
    let (continuity, created_by) = continuity_and_creator(db, id);
    let last_run = db
        .query_row(
            "SELECT status, COALESCE(completedAt, createdAt), findingsSummary, errorMessage, sessionId \
             FROM automation_runs WHERE automationId = ?1 AND status != 'skipped' \
             ORDER BY createdAt DESC, rowid DESC LIMIT 1",
            [id],
            |row| {
                Ok(json!({
                    "status": row.get::<_, String>(0)?,
                    "at": row.get::<_, String>(1)?,
                    "summary": row.get::<_, Option<String>>(2)?.or(row.get::<_, Option<String>>(3)?),
                    "sessionId": row.get::<_, Option<String>>(4)?,
                }))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let next_label = routine
        .next_run_at
        .as_deref()
        .and_then(|at| next_run_label(&routine.schedule, at));
    Ok(Some(json!({
        "id": routine.id,
        "projectId": routine.project_id,
        "botId": routine.owner_bot_id,
        "agentId": routine.agent_id,
        "name": routine.name,
        "prompt": routine.prompt,
        "schedule": routine.schedule,
        "scheduleSentence": schedule_sentence(&routine.schedule),
        "enabled": routine.enabled,
        "paused": !routine.enabled,
        "nextRunAt": routine.next_run_at,
        "nextLabel": next_label,
        "precheck": routine.precheck,
        "precheckSentence": routine.precheck.as_ref().map(precheck_sentence),
        "deliver": routine.deliver.unwrap_or_else(|| "chat".to_string()),
        "continuity": continuity,
        "createdBy": created_by,
        "createdAt": routine.created_at,
        "updatedAt": routine.updated_at,
        "lastRun": last_run,
        "check": routine_check_state(db, id),
    })))
}

/// A GBot's routines, newest first.
pub(crate) fn bot_routines(db: &Connection, bot_id: &str) -> Result<Vec<Value>, DomainStateError> {
    let mut statement = db
        .prepare(
            "SELECT automationId FROM automations WHERE ownerBotId = ?1 ORDER BY createdAt DESC",
        )
        .map_err(sql_error)?;
    let ids = statement
        .query_map([bot_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let mut routines = Vec::new();
    for id in ids {
        if let Some(view) = routine_view(db, &id)? {
            routines.push(view);
        }
    }
    Ok(routines)
}

/// Pauses or resumes a routine; a one-time routine whose time passed can't resume.
pub(crate) fn set_routine_enabled_by_id(
    db: &Connection,
    id: &str,
    enabled: bool,
) -> Result<(), DomainStateError> {
    let routine = read_automation_by_id(db, id)?
        .ok_or_else(|| DomainStateError::not_found("There is no such routine."))?;
    let project = ProjectRecord {
        name: String::new(),
        path: String::new(),
        project_id: routine.project_id,
    };
    set_automation_enabled(db, &project, id, enabled)
}

pub(crate) fn delete_routine(db: &Connection, id: &str) -> Result<(), DomainStateError> {
    db.execute("DELETE FROM automations WHERE automationId = ?1", [id])
        .map_err(sql_error)?;
    Ok(())
}

/// Starts a run now, without its check (you asked for it).
pub(crate) fn run_routine_now(
    runtime: &AutomationRuntime,
    db: &Connection,
    repository: &DomainRepository<'_>,
    id: &str,
) -> Result<Value, DomainStateError> {
    let routine = read_automation_by_id(db, id)?
        .ok_or_else(|| DomainStateError::not_found("There is no such routine."))?;
    let run = queue_automation_run(runtime, repository, db, &routine)?;
    Ok(json!({
        "status": run.status,
        "sessionId": run.session_id,
        "error": run.error_message,
    }))
}
