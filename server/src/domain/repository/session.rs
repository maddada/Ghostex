use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{Map, Value};

use crate::domain::repository::project::MAX_ID_GENERATION_ATTEMPTS;
use crate::domain::{
    insert_optional_string, merge_session_update, normalize_create_agent_session_params,
    normalize_domain_lifecycle_state, normalize_existing_directory_path, normalize_session_input,
    normalize_session_order_ids, normalize_settled_override, normalize_zmx_provider_state, now_iso,
    path_basename, project_path_state, read_optional_text, read_project_id, read_string_field,
    read_unvalidated_project_lookup_id, read_unvalidated_session_lookup_id,
    reject_stopped_session_revive, session_from_row, session_insert_params, session_row_from_sql,
    sql_error, DomainRepository, DomainResult, DomainStateError, ProjectPathState,
    SessionLifecycleFields,
};
use crate::ids::{create_session_id, create_zmx_session_name, is_gxserver_project_id};

impl<'a> DomainRepository<'a> {
    pub fn create_session(
        &self,
        params: &Map<String, Value>,
        create_agent_session: bool,
    ) -> DomainResult<Value> {
        let mut restored_params;
        let params = if let (Some(project_id), Some(source_id)) = (
            params.get("projectId").and_then(Value::as_str),
            params.get("restoredFromSessionId").and_then(Value::as_str),
        ) {
            let source = self.get_session(project_id, source_id)?;
            if let Some(source) = source.filter(|s| {
                s.pointer("/runtimeSettings/externalSession")
                    .and_then(Value::as_bool)
                    == Some(true)
            }) {
                let cwd = source
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !std::path::Path::new(cwd).is_dir() {
                    return Err(DomainStateError::bad_request("The original project folder no longer exists. Restore it before resuming this conversation."));
                }
                if !source
                    .pointer("/runtimeSettings/agentSessionPath")
                    .and_then(Value::as_str)
                    .is_some_and(|p| std::path::Path::new(p).is_file())
                {
                    return Err(DomainStateError::bad_request(
                        "The original agent transcript no longer exists.",
                    ));
                }
                restored_params = params.clone();
                for key in ["agentId", "cwd", "runtimeSettings", "launchSettings"] {
                    if let Some(value) = source.get(key) {
                        restored_params.insert(key.to_string(), value.clone());
                    }
                }
                &restored_params
            } else {
                params
            }
        } else {
            params
        };
        let project = self.resolve_create_session_project(params)?;
        self.insert_session_for_project(&project, params, create_agent_session)
    }

    /// CDXC:Sessions 2026-09-08 WHY:
    /// Discovery records stopped history, including conversations whose old worktree has been removed.
    /// Running the launch-time directory check here aborted the entire import and hid both external sessions and project facets.
    /// Normal creation and restore still validate the project directory before reaching the shared insertion path.
    pub(crate) fn import_external_session(
        &self,
        params: &Map<String, Value>,
    ) -> DomainResult<Value> {
        if params.get("lifecycleState").and_then(Value::as_str) != Some("stopped")
            || params
                .get("runtimeSettings")
                .and_then(|settings| settings.get("externalSession"))
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err(DomainStateError::bad_request(
                "External imports must be stopped history records.",
            ));
        }
        let project_id = read_unvalidated_project_lookup_id(params);
        let project = self.get_project(&project_id)?.ok_or_else(|| {
            DomainStateError::not_found("External session project does not exist.")
        })?;
        self.insert_session_for_project(&project, params, false)
    }

    fn insert_session_for_project(
        &self,
        project: &Value,
        params: &Map<String, Value>,
        create_agent_session: bool,
    ) -> DomainResult<Value> {
        let project_id = read_string_field(project, "projectId")?;
        let session_id = self.create_unique_session_id(&project_id)?;
        let timestamp = now_iso();
        let mut normalized_params = if create_agent_session {
            normalize_create_agent_session_params(params)
        } else {
            params.clone()
        };
        /*
        A project-backed terminal starts in the project directory whenever the
        caller does not request a more specific cwd. Persist that effective
        launch directory on the session row as well: Linux shells commonly
        publish only its final path component as their OSC title, and title
        ingestion needs the daemon-owned cwd to distinguish that shell label
        from a real terminal or agent session name.
        */
        if read_optional_text(normalized_params.get("cwd")).is_none() {
            if let Some(project_path) = read_optional_text(project.get("path")) {
                normalized_params.insert("cwd".to_string(), Value::String(project_path));
            }
        }
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
        /*
        CDXC:Telemetry 2026-08-26:
        The one INSERT INTO sessions in the crate, so every route that creates a
        session — chat, terminal, worktree, fork, board worker, automation —
        counts exactly once here, with no per-caller instrumentation to keep in
        sync.

        Only AGENT sessions are reported. A plain terminal has no `agentId`, and
        emitting those as `custom` would swamp the agent-CLI distribution this
        event exists to measure with rows that carry no agent at all. Unknown
        (user-authored) agent ids are resolved by the executable of their base
        command inside the emitter, so the id itself never leaves the machine.
        */
        if read_optional_text(session.get("agentId")).is_some()
            && !(session
                .pointer("/runtimeSettings/externalSession")
                .and_then(Value::as_bool)
                == Some(true)
                && session.get("lifecycleState").and_then(Value::as_str) == Some("stopped"))
        {
            crate::telemetry::session_started(&session);
        }
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
        CDXC:Sessions 2026-06-22-05:50:
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
    CDXC:StateSync 2026-07-29-00:00:
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

    /*
    CDXC:StateSync 2026-09-01:
    `list_sessions` hydrates all thirty session fields and parses six JSON
    columns per row, which is hundreds of milliseconds on a registry with a few
    thousand rows. Callers that consume a narrow slice of the row get their own
    statement instead, so a hot path never pays for the columns it ignores.
    */

    /// The rows `SessionForkFamilies::build` reads: fork/restore edges plus
    /// just enough lifecycle to tell an active row from a closed one. Every key
    /// carries exactly the value `session_from_row` would have produced for it,
    /// so family derivation is identical to the full-list version.
    ///
    /// CDXC:SessionFork 2026-09-11 WHY:
    /// Family derivation reads six scalar keys and one array out of three JSON columns, but this statement used to hand back the whole columns and parse every object into serde maps, on every presentation delta and every snapshot poll.
    /// On a registry with 8,600 stopped rows that parse was the single largest CPU cost of an otherwise idle gxserver.
    /// SQLite's json_extract pulls just those keys in C and leaves the rest of each column unparsed; the `json_type` guards keep the exact `Value::as_str` / `as_array` semantics the family reader applies to a fully hydrated row.
    pub fn list_session_fork_rows(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT projectId, sessionId, lifecycleState,
                       CASE WHEN json_type(providerStateJson, '$.lifecycleState') = 'text'
                            THEN json_extract(providerStateJson, '$.lifecycleState') END,
                       CASE WHEN json_type(launchSettingsJson, '$.forkedFromSessionId') = 'text'
                            THEN json_extract(launchSettingsJson, '$.forkedFromSessionId') END,
                       CASE WHEN json_type(runtimeSettingsJson, '$.forkedFromSessionId') = 'text'
                            THEN json_extract(runtimeSettingsJson, '$.forkedFromSessionId') END,
                       CASE WHEN json_type(runtimeSettingsJson, '$.agentSessionId') = 'text'
                            THEN json_extract(runtimeSettingsJson, '$.agentSessionId') END,
                       CASE WHEN json_type(runtimeSettingsJson, '$.sessionPersistenceProvider') = 'text'
                            THEN json_extract(runtimeSettingsJson, '$.sessionPersistenceProvider') END,
                       CASE WHEN json_type(runtimeSettingsJson, '$.previousAgentSessionIds') = 'array'
                            THEN json_extract(runtimeSettingsJson, '$.previousAgentSessionIds') END,
                       restoredFromSessionId
                FROM sessions
                ORDER BY updatedAt DESC, projectId ASC, sessionId ASC
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(SessionForkRow {
                    project_id: row.get(0)?,
                    session_id: row.get(1)?,
                    lifecycle_state: row.get(2)?,
                    provider_lifecycle_state: row.get(3)?,
                    launch_forked_from_session_id: row.get(4)?,
                    runtime_forked_from_session_id: row.get(5)?,
                    agent_session_id: row.get(6)?,
                    session_persistence_provider: row.get(7)?,
                    previous_agent_session_ids_json: row.get(8)?,
                    restored_from_session_id: row.get(9)?,
                })
            })
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| session_fork_row_value(&self.server_id, row))
            .collect()
    }

    /// The project's rows that can satisfy `identities_match` for an agent
    /// identity: same agent session id or same agent session path. A superset
    /// of the in-memory match (agent-family checks stay with the caller), read
    /// without hydrating the rest of the project.
    ///
    /// CDXC:SessionIdentity 2026-09-11 WHY:
    /// The live-process identity pass runs on every presentation poll and, while a session's title is still a placeholder, re-hunts a trusted title among the project's other rows each time.
    /// That hunt hydrated the whole project, 7,000 stopped rows included, for every such session on every poll; it was the largest CPU cost left in gxserver after the presentation reads were scoped.
    /// SEE-ALSO: `select_trusted_title_for_identity` and `live_process_identity_update_is_noop` in agents/identity.rs.
    pub fn list_sessions_matching_identity(
        &self,
        project_id: &str,
        agent_session_id: Option<&str>,
        agent_session_path: Option<&str>,
    ) -> DomainResult<Vec<Value>> {
        if agent_session_id.is_none() && agent_session_path.is_none() {
            return Ok(Vec::new());
        }
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT * FROM sessions
                WHERE projectId = ?1
                  AND (trim(json_extract(runtimeSettingsJson, '$.agentSessionId')) = ?2
                       OR trim(json_extract(runtimeSettingsJson, '$.agentSessionPath')) = ?3)
                ORDER BY updatedAt DESC, sessionId ASC
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                params![project_id, agent_session_id, agent_session_path],
                session_row_from_sql,
            )
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| session_from_row(&self.server_id, row))
            .collect()
    }

    /// `list_sessions` narrowed to the rows a presentation snapshot can
    /// publish: every row that is not durably `stopped`, plus the stopped rows
    /// `should_include_presentation_session` keeps (pinned, parked, favorite,
    /// or tagged). The SQL predicate is a superset of that in-memory check,
    /// which still runs on the result, so a snapshot built from this list is
    /// identical to one built from the full registry.
    ///
    /// CDXC:StateSync 2026-09-11 WHY:
    /// The snapshot poll and the presentation subscribe hydrated every row of the registry on each call and then discarded all but the pinned stopped ones; with 8,600 stopped rows next to 190 live ones that was a third of an idle gxserver's CPU.
    /// Fork families are not derived from this list; they need the whole registry and come from `list_session_fork_rows`.
    /// SEE-ALSO: `select_presentation_sessions` in presentation/session_projection.rs, `should_include_presentation_session` in presentation/session_attributes.rs.
    pub fn list_presentation_sessions(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT * FROM sessions
                WHERE lifecycleState <> 'stopped'
                   OR isPinned = 1
                   OR isParked = 1
                   OR isFavorite = 1
                   OR (sessionTag IS NOT NULL AND sessionTag <> '')
                ORDER BY updatedAt DESC, projectId ASC, sessionId ASC
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], session_row_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| session_from_row(&self.server_id, row))
            .collect()
    }

    /// Whether the project owns any session row at all — the same answer as
    /// `!list_sessions(Some(project_id))?.is_empty()`, without hydrating every
    /// row of the project to produce a boolean.
    pub fn has_sessions(&self, project_id: &str) -> DomainResult<bool> {
        self.db
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sessions WHERE projectId = ?1)",
                params![project_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists != 0)
            .map_err(sql_error)
    }

    /// `list_sessions` narrowed to rows that are not durably `stopped`, i.e.
    /// `running`, `sleeping`, `missing`, and `unknown`.
    ///
    /// The `missing`/`unknown` rows have to stay: presentation's
    /// `effective_lifecycle_state` promotes any non-`stopped` row whose
    /// `providerState` says `exists` to `running`, so dropping them here would
    /// hide live sessions. A `stopped` row, by contrast, can never be active —
    /// that same promotion explicitly refuses to override `stopped` — so this
    /// is a lossless narrowing for every active-only caller.
    ///
    /// The column is `TEXT NOT NULL CHECK (lifecycleState IN ('running',
    /// 'sleeping', 'stopped', 'missing', 'unknown'))`, and hydration passes
    /// each of those five values through unchanged, so the SQL predicate and
    /// an in-memory `lifecycleState != "stopped"` filter select the same rows.
    pub fn list_sessions_excluding_stopped(
        &self,
        project_id: Option<&str>,
    ) -> DomainResult<Vec<Value>> {
        let rows = if let Some(project_id) = project_id {
            let mut statement = self
                .db
                .prepare(
                    "SELECT * FROM sessions WHERE projectId = ?1 AND lifecycleState <> 'stopped' ORDER BY updatedAt DESC, sessionId ASC",
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
                    "SELECT * FROM sessions WHERE lifecycleState <> 'stopped' ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
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

    /// Full session rows narrowed to one durable `lifecycleState`. The stored
    /// column and the hydrated field agree for every recognized state, so this
    /// selects exactly the rows a caller's in-memory `lifecycleState` filter
    /// would have kept.
    pub fn list_sessions_with_lifecycle_state(
        &self,
        lifecycle_state: &str,
    ) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare(
                "SELECT * FROM sessions WHERE lifecycleState = ?1 ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([lifecycle_state], session_row_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
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

struct SessionForkRow {
    project_id: String,
    session_id: String,
    lifecycle_state: String,
    provider_lifecycle_state: Option<String>,
    launch_forked_from_session_id: Option<String>,
    runtime_forked_from_session_id: Option<String>,
    agent_session_id: Option<String>,
    session_persistence_provider: Option<String>,
    /// The `previousAgentSessionIds` array as JSON text, present only when the
    /// stored value is an array.
    previous_agent_session_ids_json: Option<String>,
    restored_from_session_id: Option<String>,
}

fn session_fork_row_value(server_id: &str, row: SessionForkRow) -> DomainResult<Value> {
    let row_id = format!("{}/{}", row.project_id, row.session_id);
    let zmx_name = create_zmx_session_name(server_id, &row.project_id, &row.session_id);
    let mut provider_state = Map::new();
    insert_optional_string(
        &mut provider_state,
        "lifecycleState",
        row.provider_lifecycle_state,
    );
    let provider_state = normalize_zmx_provider_state(provider_state, &zmx_name);
    let mut hidden = Map::new();
    insert_optional_string(
        &mut hidden,
        "restoredFromSessionId",
        row.restored_from_session_id,
    );
    let mut launch_settings = Map::new();
    insert_optional_string(
        &mut launch_settings,
        "forkedFromSessionId",
        row.launch_forked_from_session_id,
    );
    let mut runtime_settings = Map::new();
    insert_optional_string(
        &mut runtime_settings,
        "agentSessionId",
        row.agent_session_id,
    );
    insert_optional_string(
        &mut runtime_settings,
        "forkedFromSessionId",
        row.runtime_forked_from_session_id,
    );
    insert_optional_string(
        &mut runtime_settings,
        "sessionPersistenceProvider",
        row.session_persistence_provider,
    );
    if let Some(previous_ids_json) = row.previous_agent_session_ids_json {
        let previous_ids: Value = serde_json::from_str(&previous_ids_json).map_err(|error| {
            DomainStateError::corrupt_state(format!(
                "session {row_id} runtimeSettingsJson.previousAgentSessionIds did not decode: {error}"
            ))
        })?;
        runtime_settings.insert("previousAgentSessionIds".to_string(), previous_ids);
    }
    let mut session = Map::new();
    session.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    session.insert("launchSettings".to_string(), Value::Object(launch_settings));
    session.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_domain_lifecycle_state(Some(&Value::String(
            row.lifecycle_state,
        )))),
    );
    session.insert("projectId".to_string(), Value::String(row.project_id));
    session.insert("providerState".to_string(), Value::Object(provider_state));
    session.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    session.insert("sessionId".to_string(), Value::String(row.session_id));
    Ok(Value::Object(session))
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
