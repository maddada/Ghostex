use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{Map, Value};

use crate::domain::repository::project::MAX_ID_GENERATION_ATTEMPTS;
use crate::domain::{
    merge_session_update, normalize_create_agent_session_params, normalize_existing_directory_path,
    normalize_session_input, normalize_session_order_ids, normalize_settled_override, now_iso,
    path_basename, project_path_state, read_optional_text, read_project_id, read_string_field,
    read_unvalidated_project_lookup_id, read_unvalidated_session_lookup_id,
    reject_stopped_session_revive, session_from_row, session_insert_params, session_row_from_sql,
    sql_error, DomainRepository, DomainResult, DomainStateError, ProjectPathState,
    SessionLifecycleFields,
};
use crate::ids::{create_session_id, is_gxserver_project_id};

impl<'a> DomainRepository<'a> {
    pub fn create_session(
        &self,
        params: &Map<String, Value>,
        create_agent_session: bool,
    ) -> DomainResult<Value> {
        let project = self.resolve_create_session_project(params)?;
        let project_id = read_string_field(&project, "projectId")?;
        let session_id = self.create_unique_session_id(&project_id)?;
        let timestamp = now_iso();
        let normalized_params = if create_agent_session {
            normalize_create_agent_session_params(params)
        } else {
            params.clone()
        };
        let session = normalize_session_input(
            &self.server_id,
            &project_id,
            &session_id,
            &timestamp,
            &normalized_params,
        )?;
        self.db
            .execute(
                r#"
                INSERT INTO sessions (
                  projectId, sessionId, kind, title, lifecycleState, providerStateJson, zmxName, cwd,
                  agentId, commandId, isPinned, isFavorite, sessionTag, restoredFromSessionId, restoredFromHistoryId,
                  launchSettingsJson, runtimeSettingsJson, completionRulesJson, attentionRulesJson,
                  notificationRulesJson, worktreeJson, createdAt, updatedAt, lastActiveAt, sidebarOrder,
                  settledAt, settledOverride, settledOverrideAt, snoozedAt, snoozedUntil, isParked
                ) VALUES (
                  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                  ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                  ?16, ?17, ?18, ?19,
                  ?20, ?21, ?22, ?23, ?24, ?25,
                  ?26, ?27, ?28, ?29, ?30, ?31
                )
                "#,
                session_insert_params(&session)?,
            )
            .map_err(sql_error)?;
        self.record_id_allocation("session", &project_id, &session_id, &timestamp)?;
        Ok(session)
    }

    pub(crate) fn create_session_transactional(
        &self,
        params: &Map<String, Value>,
        create_agent_session: bool,
    ) -> DomainResult<Value> {
        /*
        The atomic workspace-terminal endpoint must not expose a session row
        unless its never-reused id allocation is durable too. Keep ordinary
        createSession callers unchanged; this endpoint-scoped wrapper runs the
        existing normalization and writes on one SQLite transaction and only
        returns the allocated identity after commit succeeds. Acquire SQLite's
        writer reservation before reading candidate ids so a concurrent writer
        cannot invalidate this connection's WAL snapshot between allocation
        reads and inserts.
        */
        let transaction = Transaction::new_unchecked(self.db, TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let repository = DomainRepository::new(&transaction, self.server_id.as_str());
        let session = repository.create_session(params, create_agent_session)?;
        transaction.commit().map_err(sql_error)?;
        Ok(session)
    }

    pub fn update_session(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        self.update_session_inner(params, false)
    }

    pub fn update_session_for_lifecycle(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        self.update_session_inner(params, true)
    }

    fn update_session_inner(
        &self,
        params: &Map<String, Value>,
        allow_stopped_lifecycle_revive: bool,
    ) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let session_id = read_unvalidated_session_lookup_id(params);
        let current = self.get_session(&project_id, &session_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })?;
        if !allow_stopped_lifecycle_revive {
            reject_stopped_session_revive(&current, params, "update-session")?;
        }
        let updated_at = now_iso();
        let session = merge_session_update(&self.server_id, current, &updated_at, params)?;
        self.db
            .execute(
                r#"
                UPDATE sessions SET
                  kind = ?3,
                  title = ?4,
                  lifecycleState = ?5,
                  providerStateJson = ?6,
                  zmxName = ?7,
                  cwd = ?8,
                  agentId = ?9,
                  commandId = ?10,
                  isPinned = ?11,
                  isFavorite = ?12,
                  sessionTag = ?13,
                  restoredFromSessionId = ?14,
                  restoredFromHistoryId = ?15,
                  launchSettingsJson = ?16,
                  runtimeSettingsJson = ?17,
                  completionRulesJson = ?18,
                  attentionRulesJson = ?19,
                  notificationRulesJson = ?20,
                  worktreeJson = ?21,
                  createdAt = ?22,
                  updatedAt = ?23,
                  lastActiveAt = ?24,
                  sidebarOrder = ?25,
                  settledAt = ?26,
                  settledOverride = ?27,
                  settledOverrideAt = ?28,
                  snoozedAt = ?29,
                  snoozedUntil = ?30,
                  isParked = ?31
                WHERE projectId = ?1 AND sessionId = ?2
                "#,
                session_insert_params(&session)?,
            )
            .map_err(sql_error)?;
        Ok(session)
    }

    pub fn update_session_order(&self, params: &Map<String, Value>) -> DomainResult<Vec<Value>> {
        let project_id = read_project_id(params)?;
        if self.get_project(&project_id)?.is_none() {
            return Err(DomainStateError::not_found(format!(
                "Project {project_id} does not exist."
            )));
        }
        let session_ids = normalize_session_order_ids(params.get("sessionIds"))?;
        let updated_at = now_iso();
        /*
        CDXC:SidebarOrdering 2026-06-22-05:50:
        updateSessionOrder is one manual sidebar-order write in TypeScript gxserver. If a later session ID is missing or SQLite rejects a row, earlier sidebarOrder and updatedAt writes must roll back instead of leaving a partially reordered sidebar.
        */
        self.db
            .execute_batch("BEGIN IMMEDIATE TRANSACTION")
            .map_err(sql_error)?;
        let result = (|| -> DomainResult<Vec<Value>> {
            let mut sessions = Vec::new();
            for (index, session_id) in session_ids.iter().enumerate() {
                let current = self.get_session(&project_id, session_id)?.ok_or_else(|| {
                    DomainStateError::not_found(format!(
                        "Session {project_id}/{session_id} does not exist."
                    ))
                })?;
                let sidebar_order = ((index + 1) * 1000) as i64;
                let mut update = Map::new();
                update.insert("projectId".to_string(), Value::String(project_id.clone()));
                update.insert("sessionId".to_string(), Value::String(session_id.clone()));
                update.insert(
                    "sidebarOrder".to_string(),
                    Value::Number(serde_json::Number::from(sidebar_order)),
                );
                let session = merge_session_update(&self.server_id, current, &updated_at, &update)?;
                self.db
                    .execute(
                        "UPDATE sessions SET updatedAt = ?3, sidebarOrder = ?4 WHERE projectId = ?1 AND sessionId = ?2",
                        params![project_id, session_id, updated_at, sidebar_order],
                    )
                    .map_err(sql_error)?;
                sessions.push(session);
            }
            Ok(sessions)
        })();
        match result {
            Ok(sessions) => {
                if let Err(error) = self.db.execute_batch("COMMIT") {
                    let _ = self.db.execute_batch("ROLLBACK");
                    return Err(sql_error(error));
                }
                Ok(sessions)
            }
            Err(error) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Settle/snooze is written through this narrow statement instead of
    `update_session` so the guarded lifecycle RPCs stay the only way to change
    it: a generic `/api/updateSession` body can never smuggle a settle past the
    working/attention guards, and a lifecycle write can never disturb title,
    provider state, or launch settings that another agent is mutating
    concurrently.
    */
    pub fn write_session_lifecycle(
        &self,
        project_id: &str,
        session_id: &str,
        lifecycle: &SessionLifecycleFields,
        updated_at: &str,
    ) -> DomainResult<Value> {
        if self.get_session(project_id, session_id)?.is_none() {
            return Err(DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            )));
        }
        self.db
            .execute(
                r#"
                UPDATE sessions SET
                  updatedAt = ?3,
                  settledAt = ?4,
                  settledOverride = ?5,
                  settledOverrideAt = ?6,
                  snoozedAt = ?7,
                  snoozedUntil = ?8
                WHERE projectId = ?1 AND sessionId = ?2
                "#,
                params![
                    project_id,
                    session_id,
                    updated_at,
                    lifecycle.settled_at,
                    normalize_settled_override(lifecycle.settled_override.as_deref()),
                    lifecycle.settled_override_at,
                    lifecycle.snoozed_at,
                    lifecycle.snoozed_until,
                ],
            )
            .map_err(sql_error)?;
        self.get_session(project_id, session_id)?.ok_or_else(|| {
            DomainStateError::corrupt_state(format!(
                "Session {project_id}/{session_id} vanished during a lifecycle write."
            ))
        })
    }

    pub fn list_sessions(&self, project_id: Option<&str>) -> DomainResult<Vec<Value>> {
        let rows = if let Some(project_id) = project_id {
            let mut statement = self
                .db
                .prepare(
                    "SELECT * FROM sessions WHERE projectId = ?1 ORDER BY updatedAt DESC, sessionId ASC",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([project_id], session_row_from_sql)
                .map_err(sql_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        } else {
            let mut statement = self
                .db
                .prepare(
                    "SELECT * FROM sessions ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], session_row_from_sql)
                .map_err(sql_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        rows.into_iter()
            .map(|row| session_from_row(&self.server_id, row))
            .collect()
    }

    pub fn get_session(&self, project_id: &str, session_id: &str) -> DomainResult<Option<Value>> {
        let row = self
            .db
            .query_row(
                "SELECT * FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
                params![project_id, session_id],
                session_row_from_sql,
            )
            .optional()
            .map_err(sql_error)?;
        row.map(|row| session_from_row(&self.server_id, row))
            .transpose()
    }

    pub fn remove_session(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let session_id = read_unvalidated_session_lookup_id(params);
        let current = self.get_session(&project_id, &session_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })?;
        self.db
            .execute(
                "DELETE FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
                params![project_id, session_id],
            )
            .map_err(sql_error)?;
        Ok(current)
    }

    pub fn resolve_create_session_project(
        &self,
        params: &Map<String, Value>,
    ) -> DomainResult<Value> {
        let project_id = params
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if is_gxserver_project_id(project_id) {
            if let Some(project) = self.get_project(project_id)? {
                validate_project_path_for_session(&project)?;
                return Ok(project);
            }
        }

        let project_path_param = params.get("projectPath").filter(|value| !value.is_null());
        let project_path = project_path_param.or_else(|| params.get("cwd"));
        if let Some(path_value) = project_path
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            let normalized_path = normalize_existing_directory_path(
                Some(&Value::String(path_value.to_string())),
                if project_path_param.is_some() {
                    "projectPath"
                } else {
                    "cwd"
                },
            )?;
            if let Some(existing) = self.find_project_by_path(&normalized_path)? {
                return Ok(existing);
            }
            let mut create_params = Map::new();
            create_params.insert(
                "name".to_string(),
                Value::String(
                    read_optional_text(params.get("projectName"))
                        .unwrap_or_else(|| path_basename(&normalized_path)),
                ),
            );
            create_params.insert("path".to_string(), Value::String(normalized_path));
            return self.create_project(&create_params);
        }

        if !project_id.is_empty() && !is_gxserver_project_id(project_id) {
            return Err(DomainStateError::bad_request(format!(
                "Invalid gxserver project ID: {project_id}."
            )));
        }
        if !project_id.is_empty() {
            return Err(DomainStateError::not_found(format!(
                "Project {project_id} does not exist."
            )));
        }
        Err(DomainStateError::bad_request(
            "createSession requires projectId, projectPath, or cwd.",
        ))
    }

    fn create_unique_session_id(&self, project_id: &str) -> DomainResult<String> {
        let existing = self.existing_session_ids(project_id)?;
        for _ in 0..MAX_ID_GENERATION_ATTEMPTS {
            let candidate = create_session_id();
            if !existing.contains(&candidate) {
                return Ok(candidate);
            }
        }
        Err(DomainStateError::bad_request(
            "Unable to generate a unique gxserver session ID.",
        ))
    }

    fn existing_session_ids(&self, project_id: &str) -> DomainResult<HashSet<String>> {
        let mut statement = self
            .db
            .prepare("SELECT sessionId AS id FROM sessions WHERE projectId = ?1 UNION SELECT id FROM id_allocations WHERE kind = 'session' AND parentId = ?2")
            .map_err(sql_error)?;
        let ids = statement
            .query_map(params![project_id, project_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sql_error)?
            .collect::<std::result::Result<HashSet<_>, _>>()
            .map_err(sql_error)?;
        Ok(ids)
    }
}

fn validate_project_path_for_session(project: &Value) -> DomainResult<()> {
    let path = project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .unwrap_or("the saved project path");
    let message = match project_path_state(project) {
        ProjectPathState::Available => return Ok(()),
        ProjectPathState::Missing => format!("Project folder does not exist: {path}"),
        ProjectPathState::NotDirectory => {
            format!("Project path is not a directory: {path}")
        }
        ProjectPathState::Unavailable => format!("Project folder is unavailable: {path}"),
    };
    Err(DomainStateError {
        code: "projectPathUnavailable",
        message,
    })
}
