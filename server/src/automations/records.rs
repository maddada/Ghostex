use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::domain::DomainStateError;

use super::*;

pub(super) fn upsert_automation(
    db: &Connection,
    automation: &AutomationDefinitionRecord,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO automations (
          automationId, projectId, agentId, name, prompt, enabled, scheduleJson,
          executionModeJson, nextRunAt, createdAt, updatedAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(automationId) DO UPDATE SET
          projectId = excluded.projectId,
          agentId = excluded.agentId,
          name = excluded.name,
          prompt = excluded.prompt,
          enabled = excluded.enabled,
          scheduleJson = excluded.scheduleJson,
          executionModeJson = excluded.executionModeJson,
          nextRunAt = excluded.nextRunAt,
          updatedAt = excluded.updatedAt
        "#,
        params![
            automation.id,
            automation.project_id,
            automation.agent_id,
            automation.name,
            automation.prompt,
            bool_to_int(automation.enabled),
            stringify_json(&automation.schedule)?,
            stringify_json(&automation.execution_mode)?,
            automation.next_run_at,
            automation.created_at,
            automation.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn delete_automation(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM automations WHERE projectId = ?1 AND automationId = ?2",
        params![project.project_id, automation_id],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn set_automation_enabled(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
    enabled: bool,
) -> Result<(), DomainStateError> {
    let automation = read_automation(db, project, automation_id)?;
    let next_run_at = if enabled {
        Some(
            compute_next_run_at(&automation.schedule, None).ok_or_else(|| {
                DomainStateError::bad_request(
                "This one-time automation is in the past. Choose a new date before enabling it.",
            )
            })?,
        )
    } else {
        None
    };
    db.execute(
        r#"
        UPDATE automations
        SET enabled = ?3, nextRunAt = ?4, updatedAt = ?5
        WHERE projectId = ?1 AND automationId = ?2
        "#,
        params![
            project.project_id,
            automation_id,
            bool_to_int(enabled),
            next_run_at,
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn update_automation_next_run_at(
    db: &Connection,
    project_id: &str,
    automation_id: &str,
) -> Result<(), DomainStateError> {
    let project = ProjectRecord {
        name: String::new(),
        path: String::new(),
        project_id: project_id.to_string(),
    };
    let automation = read_automation(db, &project, automation_id)?;
    let next_run_at = compute_next_run_at(&automation.schedule, None);
    let enabled = !is_one_shot_schedule(&automation.schedule) || next_run_at.is_some();
    db.execute(
        "UPDATE automations SET enabled = ?3, nextRunAt = ?4, updatedAt = ?5 WHERE projectId = ?1 AND automationId = ?2",
        params![project_id, automation_id, bool_to_int(enabled), next_run_at, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn upsert_run(
    db: &Connection,
    run: &AutomationRunRecord,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO automation_runs (
          runId, automationId, projectId, status, sessionId, worktreeJson,
          errorMessage, findingsSummary, isArchived, isUnread, createdAt, completedAt, updatedAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(runId) DO UPDATE SET
          status = excluded.status,
          sessionId = excluded.sessionId,
          worktreeJson = excluded.worktreeJson,
          errorMessage = excluded.errorMessage,
          findingsSummary = excluded.findingsSummary,
          isArchived = excluded.isArchived,
          isUnread = excluded.isUnread,
          completedAt = excluded.completedAt,
          updatedAt = excluded.updatedAt
        "#,
        params![
            run.id,
            run.automation_id,
            run.project_id,
            run.status,
            run.session_id,
            stringify_json(&run.worktree)?,
            run.error_message,
            run.findings_summary,
            bool_to_int(run.is_archived),
            bool_to_int(run.is_unread),
            run.created_at,
            run.completed_at,
            run.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn complete_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
    status: &str,
    summary: Option<&str>,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE automation_runs
        SET status = ?3, findingsSummary = ?4, completedAt = ?5, isUnread = ?6, updatedAt = ?5
        WHERE projectId = ?1 AND runId = ?2
        "#,
        params![
            project_id,
            run_id,
            status,
            summary,
            now_iso(),
            bool_to_int(status != "no_findings"),
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn fail_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
    message: &str,
    status: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE automation_runs
        SET status = ?3, errorMessage = ?4, completedAt = ?5, isUnread = 1, updatedAt = ?5
        WHERE projectId = ?1 AND runId = ?2
        "#,
        params![project_id, run_id, status, message, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn patch_run_read(
    db: &Connection,
    project_id: &str,
    run_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE automation_runs SET isUnread = 0, updatedAt = ?3 WHERE projectId = ?1 AND runId = ?2",
        params![project_id, run_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn read_automation(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
) -> Result<AutomationDefinitionRecord, DomainStateError> {
    db.query_row(
        "SELECT * FROM automations WHERE projectId = ?1 AND automationId = ?2",
        params![project.project_id, automation_id],
        automation_from_row,
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| DomainStateError::not_found("Automation not found."))
}

pub(super) fn read_automations_for_project(
    db: &Connection,
    project_id: &str,
) -> Result<Vec<AutomationDefinitionRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automations WHERE projectId = ?1 ORDER BY updatedAt DESC, automationId ASC LIMIT ?2")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![project_id, AUTOMATION_MAX_COUNT as i64],
            automation_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

pub(super) fn read_due_automations(
    db: &Connection,
) -> Result<Vec<AutomationDefinitionRecord>, DomainStateError> {
    let now = now_iso();
    let mut statement = db
        .prepare(
            r#"
            SELECT * FROM automations
            WHERE enabled = 1 AND nextRunAt IS NOT NULL AND nextRunAt <= ?1
            ORDER BY nextRunAt ASC
            LIMIT ?2
            "#,
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![now, AUTOMATION_MAX_COUNT as i64],
            automation_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

pub(super) fn read_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
) -> Result<AutomationRunRecord, DomainStateError> {
    db.query_row(
        "SELECT * FROM automation_runs WHERE projectId = ?1 AND runId = ?2",
        params![project_id, run_id],
        run_from_row,
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| DomainStateError::not_found("Automation run not found."))
}

pub(super) fn read_runs_for_project(
    db: &Connection,
    project_id: &str,
) -> Result<Vec<AutomationRunRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automation_runs WHERE projectId = ?1 ORDER BY COALESCE(completedAt, createdAt) DESC, runId ASC LIMIT ?2")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![project_id, AUTOMATION_MAX_RUN_COUNT as i64],
            run_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

pub(super) fn read_running_runs(
    db: &Connection,
) -> Result<Vec<AutomationRunRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automation_runs WHERE status IN ('queued', 'running')")
        .map_err(sql_error)?;
    let rows = statement.query_map([], run_from_row).map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

pub(super) fn has_active_run(
    db: &Connection,
    automation_id: &str,
) -> Result<bool, DomainStateError> {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE automationId = ?1 AND status IN ('queued', 'running')",
            [automation_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(count > 0)
}

pub(super) fn is_run_active(
    db: &Connection,
    project_id: &str,
    run_id: &str,
) -> Result<bool, DomainStateError> {
    let status: Option<String> = db
        .query_row(
            "SELECT status FROM automation_runs WHERE projectId = ?1 AND runId = ?2",
            params![project_id, run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(status.as_deref().is_some_and(is_active_status))
}

fn automation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationDefinitionRecord> {
    let schedule_json: String = row.get("scheduleJson")?;
    let execution_mode_json: String = row.get("executionModeJson")?;
    Ok(AutomationDefinitionRecord {
        agent_id: row.get("agentId")?,
        created_at: row.get("createdAt")?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        execution_mode: parse_json_value(&execution_mode_json),
        id: row.get("automationId")?,
        name: row.get("name")?,
        next_run_at: row.get("nextRunAt")?,
        project_id: row.get("projectId")?,
        prompt: row.get("prompt")?,
        schedule: parse_json_value(&schedule_json),
        updated_at: row.get("updatedAt")?,
        owner_bot_id: row.get("ownerBotId")?,
        precheck: row
            .get::<_, Option<String>>("precheckJson")?
            .map(|text| parse_json_value(&text)),
        deliver: row.get("deliver")?,
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunRecord> {
    let worktree_json: String = row.get("worktreeJson")?;
    Ok(AutomationRunRecord {
        automation_id: row.get("automationId")?,
        completed_at: row.get("completedAt")?,
        created_at: row.get("createdAt")?,
        error_message: row.get("errorMessage")?,
        findings_summary: row.get("findingsSummary")?,
        id: row.get("runId")?,
        is_archived: row.get::<_, i64>("isArchived")? == 1,
        is_unread: row.get::<_, i64>("isUnread")? == 1,
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        status: row.get("status")?,
        updated_at: row.get("updatedAt")?,
        worktree: parse_json_value(&worktree_json),
    })
}

pub(super) fn automations_to_value(automations: Vec<AutomationDefinitionRecord>) -> Value {
    Value::Array(
        automations
            .into_iter()
            .map(|automation| {
                let mut value = json!({
                    "agentId": automation.agent_id,
                    "createdAt": automation.created_at,
                    "enabled": automation.enabled,
                    "executionMode": automation.execution_mode,
                    "id": automation.id,
                    "name": automation.name,
                    "nextRunAt": automation.next_run_at,
                    "projectIds": [automation.project_id],
                    "prompt": automation.prompt,
                    "schedule": automation.schedule,
                    "updatedAt": automation.updated_at,
                });
                // A GBot's routine says whose it is; an older client ignores the extra fields.
                if let Some(bot_id) = automation.owner_bot_id {
                    value["ownerBotId"] = json!(bot_id);
                    value["precheck"] = automation.precheck.unwrap_or(Value::Null);
                    value["deliver"] =
                        json!(automation.deliver.unwrap_or_else(|| "chat".to_string()));
                }
                value
            })
            .collect(),
    )
}

pub(super) fn runs_to_value(runs: Vec<AutomationRunRecord>) -> Value {
    Value::Array(
        runs.into_iter()
            .map(|run| {
                json!({
                    "automationId": run.automation_id,
                    "completedAt": run.completed_at,
                    "createdAt": run.created_at,
                    "errorMessage": run.error_message,
                    "findingsSummary": run.findings_summary,
                    "id": run.id,
                    "isArchived": run.is_archived,
                    "isUnread": run.is_unread,
                    "projectId": run.project_id,
                    "sessionId": run.session_id,
                    "status": run.status,
                    "worktree": if run.worktree.as_object().is_some_and(|value| value.is_empty()) { Value::Null } else { run.worktree },
                })
            })
            .collect(),
    )
}
