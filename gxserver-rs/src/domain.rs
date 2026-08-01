use std::{
    collections::{HashMap, HashSet},
    env, fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use crate::ids::{
    create_global_session_ref, create_project_id, create_session_id, create_zmx_session_name,
    is_gxserver_project_id, is_gxserver_session_id,
};

const JSON_LIMIT_CHARS: usize = 1_000_000;
const JSON_MAX_DEPTH: usize = 10;
const MAX_ID_GENERATION_ATTEMPTS: usize = 1024;

type DomainResult<T> = std::result::Result<T, DomainStateError>;

#[derive(Debug, Clone)]
pub struct DomainStateError {
    pub code: &'static str,
    pub message: String,
}

impl DomainStateError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    pub fn corrupt_state(message: impl Into<String>) -> Self {
        Self {
            code: "corruptState",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
The durable settle/snooze state of one session. `settled_override_at` stamps
when the current override was recorded and is server-internal: the lifecycle
sweep compares it against gxserver's meaningful-activity clock to reproduce
t3code's "real activity resets ANY override" rule without an event log.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLifecycleFields {
    pub settled_at: Option<String>,
    pub settled_override: Option<String>,
    pub settled_override_at: Option<String>,
    pub snoozed_at: Option<String>,
    pub snoozed_until: Option<String>,
}

impl SessionLifecycleFields {
    pub fn from_session(session: &Value) -> Self {
        let text = |key: &str| {
            session
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        Self {
            settled_at: text("settledAt"),
            settled_override: normalize_settled_override(text("settledOverride").as_deref()),
            settled_override_at: text("settledOverrideAt"),
            snoozed_at: text("snoozedAt"),
            snoozed_until: text("snoozedUntil"),
        }
    }

    pub fn is_settled_override(&self) -> bool {
        self.settled_override.as_deref() == Some("settled")
    }

    pub fn is_active_override(&self) -> bool {
        self.settled_override.as_deref() == Some("active")
    }

    pub fn clear_settle(&mut self) {
        self.settled_at = None;
        self.settled_override = None;
        self.settled_override_at = None;
    }

    pub fn clear_snooze(&mut self) {
        self.snoozed_at = None;
        self.snoozed_until = None;
    }
}

impl fmt::Display for DomainStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for DomainStateError {}

/*
CDXC:GxserverRustPort 2026-06-14-22:12:
Phase 3 Rust must use the TypeScript SQLite tables as durable state instead of an in-memory compatibility stub. Preserve project/session IDs, JSON validation, corrupt-state errors, and camelCase response fields so sidebar inventory can opt into Rust without a client protocol change.
*/
pub struct DomainRepository<'a> {
    db: &'a Connection,
    server_id: String,
}

impl<'a> DomainRepository<'a> {
    pub fn new(db: &'a Connection, server_id: impl Into<String>) -> Self {
        Self {
            db,
            server_id: server_id.into(),
        }
    }

    pub fn create_project(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = self.create_unique_project_id()?;
        let timestamp = now_iso();
        let project = normalize_project_input(&project_id, &timestamp, params)?;
        self.db
            .execute(
                r#"
                INSERT INTO projects (
                  projectId, name, path, identityIconJson, isPinned, isFavorite, isRecentProject, recentClosedAt, defaultCommand, worktreeJson,
                  customAgentsJson, customAgentOrderJson, customCommandsJson, customCommandOrderJson,
                  deletedDefaultCommandIdsJson, launchSettingsJson, runtimeSettingsJson, completionRulesJson,
                  attentionRulesJson, notificationRulesJson, gitConfigJson, projectBoardConfigJson,
                  previousSessionHistoryJson, createdAt, updatedAt, visibility, systemKind
                ) VALUES (
                  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                  ?11, ?12, ?13, ?14,
                  ?15, ?16, ?17, ?18,
                  ?19, ?20, ?21, ?22,
                  ?23, ?24, ?25, ?26, ?27
                )
                "#,
                project_insert_params(&project)?,
            )
            .map_err(sql_error)?;
        self.record_id_allocation("project", "", &project_id, &timestamp)?;
        Ok(project)
    }

    pub fn update_project(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let current = self.get_project(&project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        let updated_at = now_iso();
        let project = merge_project_update(current, &updated_at, params)?;
        self.db
            .execute(
                r#"
                UPDATE projects SET
                  name = ?2,
                  path = ?3,
                  identityIconJson = ?4,
                  isPinned = ?5,
                  isFavorite = ?6,
                  isRecentProject = ?7,
                  recentClosedAt = ?8,
                  defaultCommand = ?9,
                  worktreeJson = ?10,
                  customAgentsJson = ?11,
                  customAgentOrderJson = ?12,
                  customCommandsJson = ?13,
                  customCommandOrderJson = ?14,
                  deletedDefaultCommandIdsJson = ?15,
                  launchSettingsJson = ?16,
                  runtimeSettingsJson = ?17,
                  completionRulesJson = ?18,
                  attentionRulesJson = ?19,
                  notificationRulesJson = ?20,
                  gitConfigJson = ?21,
                  projectBoardConfigJson = ?22,
                  previousSessionHistoryJson = ?23,
                  updatedAt = ?25,
                  visibility = ?26,
                  systemKind = ?27
                WHERE projectId = ?1
                "#,
                project_insert_params(&project)?,
            )
            .map_err(sql_error)?;
        Ok(project)
    }

    pub fn list_projects(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare("SELECT * FROM projects ORDER BY updatedAt DESC, projectId ASC")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], project_row_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter().map(project_from_row).collect()
    }

    pub fn list_recent_projects(&self) -> DomainResult<Vec<Value>> {
        /*
        CDXC:GPUIRecentProjects 2026-06-24-12:27:
        Recent Projects are explicit parked gxserver projects. Return only
        path-bearing rows marked `isRecentProject` and compute sessionCount
        from the domain sessions table; do not infer recency from presentation
        labels, inactive lifecycle states, shell titles, stdout, commands, or
        filesystem scans.

        CDXC:ProjectVisibility 2026-06-30-21:23:
        Hidden/system projects are not user workspaces and must not leak through
        the Recent Projects drawer. Keep Remote Attach carrier rows durable for
        session ownership while excluding them from every active or recent
        project list clients render.
        */
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT * FROM projects
                WHERE isRecentProject = 1
                  AND path IS NOT NULL
                  AND trim(path) <> ''
                  AND visibility <> 'hidden'
                  AND (systemKind IS NULL OR systemKind <> 'remoteAttachCarrier')
                ORDER BY recentClosedAt DESC, updatedAt DESC, projectId ASC
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], project_row_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter()
            .map(|row| {
                let project_id = row.project_id.clone();
                let session_count = self.count_project_sessions(&project_id)?;
                project_from_row(row)
                    .and_then(|project| recent_project_from_project(&project, session_count))
            })
            .collect()
    }

    pub fn read_app_user_data(&self) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Scratch Pad and Pinned Prompts hydrate through one gxserver-owned
        product-data snapshot. Return only the exact shared React fields and do
        not derive values from GPUI product-state files, presentation labels,
        terminal text, project paths, command text, URLs, or logs.
        */
        read_app_user_data_state(self.db)
    }

    pub fn save_scratch_pad(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Scratch Pad autosave stores freeform user text in gxserver state and
        returns the refreshed app-user-data snapshot. The daemon must not log or
        echo note content outside the authenticated RPC response.
        */
        let content = required_string_param(params, "content")?;
        let timestamp = now_iso();
        let created_at = self
            .db
            .query_row(
                "SELECT createdAt FROM app_user_data WHERE itemKind = 'scratchPad' AND itemId = 'global'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or_else(|| timestamp.clone());
        self.db
            .execute(
                r#"
                INSERT INTO app_user_data (
                  itemKind, itemId, content, title, createdAt, updatedAt
                ) VALUES (
                  'scratchPad', 'global', ?1, NULL, ?2, ?3
                )
                ON CONFLICT(itemKind, itemId) DO UPDATE SET
                  content = excluded.content,
                  updatedAt = excluded.updatedAt
                "#,
                params![content, created_at, timestamp],
            )
            .map_err(sql_error)?;
        read_app_user_data_state(self.db)
    }

    pub fn save_pinned_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Pinned Prompt saves mirror the existing SidebarPinnedPrompt behavior:
        create or update by promptId, preserve createdAt, stamp updatedAt on
        save, normalize empty titles from content, and treat an empty content
        save as removing that prompt instead of storing an unusable row.
        */
        let content = required_string_param(params, "content")?;
        let title = required_string_param(params, "title")?;
        let supplied_prompt_id = optional_trimmed_string_param(params, "promptId")?;
        if content.is_empty() {
            if let Some(prompt_id) = supplied_prompt_id {
                self.db
                    .execute(
                        "DELETE FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1",
                        [prompt_id],
                    )
                    .map_err(sql_error)?;
            }
            return read_app_user_data_state(self.db);
        }

        let prompt_id = match supplied_prompt_id {
            Some(prompt_id) => prompt_id,
            None => create_unique_app_pinned_prompt_id(self.db)?,
        };
        let timestamp = now_iso();
        let created_at = self
            .db
            .query_row(
                "SELECT createdAt FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1",
                [&prompt_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or_else(|| timestamp.clone());
        let normalized_title = normalize_app_pinned_prompt_title(title, content);
        self.db
            .execute(
                r#"
                INSERT INTO app_user_data (
                  itemKind, itemId, content, title, createdAt, updatedAt
                ) VALUES (
                  'pinnedPrompt', ?1, ?2, ?3, ?4, ?5
                )
                ON CONFLICT(itemKind, itemId) DO UPDATE SET
                  content = excluded.content,
                  title = excluded.title,
                  updatedAt = excluded.updatedAt
                "#,
                params![prompt_id, content, normalized_title, created_at, timestamp],
            )
            .map_err(sql_error)?;
        read_app_user_data_state(self.db)
    }

    pub fn save_stashed_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:StashedPrompts 2026-07-29-00:00:
        Stash saves are fired best-effort by the prompt-editor CLI after every
        save-and-close, so the same text can arrive repeatedly. Re-saving
        content that already exists for the same project bumps that row's
        updatedAt instead of inserting a duplicate, and the queue is capped by
        recency. Prompt bodies must never be logged or echoed outside the
        authenticated RPC response.
        */
        let content = required_string_param(params, "content")?;
        if content.trim().is_empty() {
            return Err(DomainStateError::bad_request(
                "content must not be empty.",
            ));
        }
        if content.chars().count() > MAX_STASHED_PROMPT_CONTENT_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "content must be at most {MAX_STASHED_PROMPT_CONTENT_CHARS} characters."
            )));
        }
        let project_id = optional_trimmed_string_param(params, "projectId")?;
        let session_id = optional_trimmed_string_param(params, "sessionId")?;
        let cwd = optional_trimmed_string_param(params, "cwd")?;
        let timestamp = now_iso();
        let existing_prompt_id: Option<String> = self
            .db
            .query_row(
                r#"
                SELECT promptId FROM stashed_prompts
                WHERE content = ?1 AND COALESCE(projectId, '') = COALESCE(?2, '')
                ORDER BY updatedAt DESC
                LIMIT 1
                "#,
                params![content, project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let prompt_id = match existing_prompt_id {
            Some(prompt_id) => {
                self.db
                    .execute(
                        r#"
                        UPDATE stashed_prompts
                        SET sessionId = COALESCE(?2, sessionId),
                            cwd = COALESCE(?3, cwd),
                            updatedAt = ?4
                        WHERE promptId = ?1
                        "#,
                        params![prompt_id, session_id, cwd, timestamp],
                    )
                    .map_err(sql_error)?;
                prompt_id
            }
            None => {
                let prompt_id = create_unique_stashed_prompt_id(self.db)?;
                self.db
                    .execute(
                        r#"
                        INSERT INTO stashed_prompts (
                          promptId, content, projectId, sessionId, cwd, createdAt, updatedAt
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                        "#,
                        params![prompt_id, content, project_id, session_id, cwd, timestamp],
                    )
                    .map_err(sql_error)?;
                self.db
                    .execute(
                        r#"
                        DELETE FROM stashed_prompts
                        WHERE promptId NOT IN (
                          SELECT promptId FROM stashed_prompts
                          ORDER BY updatedAt DESC, promptId DESC
                          LIMIT ?1
                        )
                        "#,
                        params![MAX_STASHED_PROMPTS],
                    )
                    .map_err(sql_error)?;
                prompt_id
            }
        };
        let prompt = read_stashed_prompt_row(self.db, &prompt_id)?.ok_or_else(|| {
            DomainStateError::corrupt_state("Stashed prompt vanished during save.")
        })?;
        Ok(json!({ "prompt": prompt }))
    }

    pub fn list_stashed_prompts(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        /*
        CDXC:StashedPrompts 2026-07-29-00:00:
        The default modal scope is "this project and its worktrees". Current
        worktree sessions already carry the parent projectId, and legacy
        worktree checkouts registered as their own project carry
        worktree.parentProjectId, so the family of a projectId is: its root
        (itself, or its parent for a legacy worktree project) plus every
        project whose worktree.parentProjectId is that root.
        */
        let scope_project_id = optional_trimmed_string_param(params, "projectId")?;
        let family = match scope_project_id {
            Some(project_id) => Some(stashed_prompt_project_family(self.db, &project_id)?),
            None => None,
        };
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT s.promptId, s.content, s.projectId, s.sessionId, s.cwd,
                       s.createdAt, s.updatedAt, p.name, p.identityIconJson
                FROM stashed_prompts s
                LEFT JOIN projects p ON p.projectId = s.projectId
                ORDER BY s.updatedAt DESC, s.promptId DESC
                "#,
            )
            .map_err(sql_error)?;
        let prompts = statement
            .query_map([], stashed_prompt_json_from_row)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?
            .into_iter()
            .filter(|prompt| match &family {
                None => true,
                Some(family) => prompt
                    .get("projectId")
                    .and_then(Value::as_str)
                    .is_some_and(|project_id| family.contains(project_id)),
            })
            .collect::<Vec<_>>();
        Ok(json!({ "prompts": prompts }))
    }

    pub fn delete_stashed_prompt(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let prompt_id = required_string_param(params, "promptId")?;
        let deleted = self
            .db
            .execute(
                "DELETE FROM stashed_prompts WHERE promptId = ?1",
                [prompt_id],
            )
            .map_err(sql_error)?;
        Ok(json!({ "deleted": deleted > 0 }))
    }

    /*
    CDXC:GlobalActions 2026-08-01-16:00:
    Global Actions are daemon-owned rather than project-owned, so every client
    reads one list instead of a per-project column. Rows come back in sortOrder
    with commandId as the tiebreak, so two actions saved in the same tick still
    order deterministically across reads.
    */
    pub fn list_global_sidebar_commands(&self) -> DomainResult<Vec<Value>> {
        let mut statement = self
            .db
            .prepare(
                r#"
                SELECT definitionJson FROM global_sidebar_commands
                ORDER BY sortOrder ASC, commandId ASC
                "#,
            )
            .map_err(sql_error)?;
        let stored_definitions = statement
            .query_map([], |row| row.get::<_, String>("definitionJson"))
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        Ok(stored_definitions
            .into_iter()
            .filter_map(|definition| serde_json::from_str::<Value>(&definition).ok())
            .collect())
    }

    pub fn save_global_sidebar_command(
        &self,
        command_id: &str,
        definition: &Value,
    ) -> DomainResult<()> {
        let timestamp = now_iso();
        let definition_json = serde_json::to_string(definition).map_err(|error| {
            DomainStateError::corrupt_state(format!(
                "Global action definition could not be serialized: {error}"
            ))
        })?;
        /*
        A new action lands after everything currently stored; an edit keeps the
        position it already had. COALESCE over MAX gives the first row
        sortOrder 1 without a separate empty-table branch.
        */
        self.db
            .execute(
                r#"
                INSERT INTO global_sidebar_commands (
                  commandId, definitionJson, sortOrder, createdAt, updatedAt
                )
                VALUES (
                  ?1,
                  ?2,
                  COALESCE(
                    (SELECT sortOrder FROM global_sidebar_commands WHERE commandId = ?1),
                    (SELECT COALESCE(MAX(sortOrder), 0) + 1 FROM global_sidebar_commands)
                  ),
                  ?3,
                  ?3
                )
                ON CONFLICT(commandId) DO UPDATE SET
                  definitionJson = excluded.definitionJson,
                  updatedAt = excluded.updatedAt
                "#,
                params![command_id, definition_json, timestamp],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn delete_global_sidebar_command(&self, command_id: &str) -> DomainResult<()> {
        self.db
            .execute(
                "DELETE FROM global_sidebar_commands WHERE commandId = ?1",
                [command_id],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    /*
    Reorder assigns positions from the ids the client sent, in order. Ids the
    client did not mention keep their relative order after the listed ones
    rather than being dropped, so a client on an older list cannot silently
    delete an action it had not loaded yet.
    */
    pub fn order_global_sidebar_commands(&self, command_ids: &[String]) -> DomainResult<()> {
        let timestamp = now_iso();
        let stored_ids = {
            let mut statement = self
                .db
                .prepare(
                    r#"
                    SELECT commandId FROM global_sidebar_commands
                    ORDER BY sortOrder ASC, commandId ASC
                    "#,
                )
                .map_err(sql_error)?;
            let stored_ids = statement
                .query_map([], |row| row.get::<_, String>("commandId"))
                .map_err(sql_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            stored_ids
        };
        let mut next_order = command_ids
            .iter()
            .filter(|command_id| stored_ids.contains(command_id))
            .cloned()
            .collect::<Vec<_>>();
        for command_id in stored_ids {
            if !next_order.contains(&command_id) {
                next_order.push(command_id);
            }
        }
        for (index, command_id) in next_order.iter().enumerate() {
            self.db
                .execute(
                    r#"
                    UPDATE global_sidebar_commands
                    SET sortOrder = ?2, updatedAt = ?3
                    WHERE commandId = ?1
                    "#,
                    params![command_id, (index + 1) as f64, timestamp],
                )
                .map_err(sql_error)?;
        }
        Ok(())
    }

    pub fn get_project(&self, project_id: &str) -> DomainResult<Option<Value>> {
        let row = self
            .db
            .query_row(
                "SELECT * FROM projects WHERE projectId = ?1",
                [project_id],
                project_row_from_sql,
            )
            .optional()
            .map_err(sql_error)?;
        row.map(project_from_row).transpose()
    }

    pub fn remove_project(&self, project_id: &str) -> DomainResult<Value> {
        let current = self.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        self.db
            .execute("DELETE FROM projects WHERE projectId = ?1", [project_id])
            .map_err(sql_error)?;
        Ok(current)
    }

    pub fn close_project_to_recent(&self, project_id: &str) -> DomainResult<Value> {
        /*
        CDXC:GPUIRecentProjects 2026-06-24-12:38:
        GPUI Close Project is a producer-side park mutation, not a generic project update. The daemon must verify the trusted project id still exists, require a stored path so `/api/listRecentProjects` can expose only real path-bearing rows, and stamp `recentClosedAt` with server time instead of accepting renderer-supplied timestamps.
        */
        let current = self.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        let has_path = current
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .map_or(false, |path| !path.is_empty());
        if !has_path {
            return Err(DomainStateError::bad_request(
                "Project must have a stored path before it can be parked as recent.",
            ));
        }
        let mut params = Map::new();
        params.insert(
            "projectId".to_string(),
            Value::String(project_id.to_string()),
        );
        params.insert("isRecentProject".to_string(), Value::Bool(true));
        params.insert("recentClosedAt".to_string(), Value::String(now_iso()));
        self.update_project(&params)
    }

    pub fn restore_recent_project(&self, project_id: &str) -> DomainResult<Value> {
        let current = self.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Recent project {project_id} does not exist."))
        })?;
        if current.get("isRecentProject").and_then(Value::as_bool) != Some(true) {
            return Ok(current);
        }
        let mut params = Map::new();
        params.insert(
            "projectId".to_string(),
            Value::String(project_id.to_string()),
        );
        params.insert("isRecentProject".to_string(), Value::Bool(false));
        params.insert("recentClosedAt".to_string(), Value::Null);
        self.update_project(&params)
    }

    pub fn remove_recent_project(&self, project_id: &str) -> DomainResult<Value> {
        let current = self.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Recent project {project_id} does not exist."))
        })?;
        if current.get("isRecentProject").and_then(Value::as_bool) != Some(true) {
            return Err(DomainStateError::not_found(format!(
                "Recent project {project_id} does not exist."
            )));
        }
        self.remove_project(project_id)
    }

    fn count_project_sessions(&self, project_id: &str) -> DomainResult<usize> {
        self.db
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE projectId = ?1",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as usize)
            .map_err(sql_error)
    }

    pub fn add_project_path(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let create_if_missing = params.get("createIfMissing").and_then(Value::as_bool) == Some(true);
        let path = normalize_project_root_path(
            params
                .get("path")
                .filter(|value| !value.is_null())
                .or_else(|| params.get("projectPath")),
            "path",
            create_if_missing,
        )?;
        let mut create_params = params.clone();
        create_params.remove("createIfMissing");
        create_params.insert("path".to_string(), Value::String(path.clone()));
        let name =
            read_optional_text(create_params.get("name")).unwrap_or_else(|| path_basename(&path));
        create_params.insert("name".to_string(), Value::String(name.clone()));
        let projects = self.list_projects()?;
        /*
        CDXC:WorktreeProjectRegistration 2026-06-22-00:35:
        Rust gxserver must preserve the TypeScript Add Worktree/Add Project path-registration contract. When /api/addProjectPath receives a linked Git worktree path, detect the already registered main checkout and store worktree metadata under that canonical parent project ID; if the path was registered earlier without metadata, repair that existing row in place so the macOS sidebar groups it exactly like the old server.

        CDXC:ProjectVisibility 2026-06-30-21:23:
        Project visibility and system-project roles belong to gxserver, because mobile, CLI, GPUI, and macOS all read project/session inventory from the daemon. `/api/addProjectPath` must repair an existing path row when a producer marks it hidden/system, otherwise old Remote Attach carrier rows can keep leaking into non-macOS project lists.
        */
        let worktree = detect_registered_git_worktree_metadata(&projects, &path, &name);
        if let Some(existing) = find_project_by_path_in(&projects, &path) {
            let mut update_params = Map::new();
            update_params.insert(
                "projectId".to_string(),
                Value::String(read_string_field(&existing, "projectId")?),
            );
            if let Some(worktree) = worktree {
                if !are_project_worktree_metadata_equal(existing.get("worktree"), &worktree) {
                    update_params.insert("worktree".to_string(), Value::Object(worktree));
                }
            }
            if params.contains_key("visibility") {
                let visibility = normalize_project_visibility(params.get("visibility"))?;
                if existing.get("visibility").and_then(Value::as_str) != Some(visibility.as_str()) {
                    update_params.insert("visibility".to_string(), Value::String(visibility));
                }
            }
            if params.contains_key("systemKind") {
                match normalize_project_system_kind(params.get("systemKind"))? {
                    Some(system_kind)
                        if existing.get("systemKind").and_then(Value::as_str)
                            != Some(system_kind.as_str()) =>
                    {
                        update_params.insert("systemKind".to_string(), Value::String(system_kind));
                    }
                    None if existing.get("systemKind").is_some() => {
                        update_params.insert("systemKind".to_string(), Value::Null);
                    }
                    _ => {}
                }
            }
            if update_params.len() > 1 {
                return self.update_project(&update_params);
            }
            return Ok(existing);
        }
        if let Some(worktree) = worktree {
            create_params.insert("worktree".to_string(), Value::Object(worktree));
        }
        self.create_project(&create_params)
    }

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
                  settledAt, settledOverride, settledOverrideAt, snoozedAt, snoozedUntil
                ) VALUES (
                  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                  ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                  ?16, ?17, ?18, ?19,
                  ?20, ?21, ?22, ?23, ?24, ?25,
                  ?26, ?27, ?28, ?29, ?30
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
                  snoozedUntil = ?30
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

    pub fn read_project_status(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_project_id(params)?;
        let project = self.get_project(&project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        Ok(json!({
            "project": project,
            "sessions": self.list_sessions(Some(&project_id))?,
        }))
    }

    fn find_project_by_path(&self, normalized_path: &str) -> DomainResult<Option<Value>> {
        Ok(find_project_by_path_in(
            &self.list_projects()?,
            normalized_path,
        ))
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

    fn create_unique_project_id(&self) -> DomainResult<String> {
        let existing = self.existing_project_ids()?;
        for _ in 0..MAX_ID_GENERATION_ATTEMPTS {
            let candidate = create_project_id();
            if !existing.contains(&candidate) {
                return Ok(candidate);
            }
        }
        Err(DomainStateError::bad_request(
            "Unable to generate a unique gxserver project ID.",
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

    fn existing_project_ids(&self) -> DomainResult<HashSet<String>> {
        let mut statement = self
            .db
            .prepare("SELECT projectId AS id FROM projects UNION SELECT id FROM id_allocations WHERE kind = 'project'")
            .map_err(sql_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<std::result::Result<HashSet<_>, _>>()
            .map_err(sql_error)?;
        Ok(ids)
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

    fn record_id_allocation(
        &self,
        kind: &str,
        parent_id: &str,
        id: &str,
        created_at: &str,
    ) -> DomainResult<()> {
        self.db
            .execute(
                "INSERT OR IGNORE INTO id_allocations (id, kind, parentId, createdAt) VALUES (?1, ?2, ?3, ?4)",
                params![id, kind, parent_id, created_at],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct GitWorktreeEntry {
    bare: bool,
    branch: Option<String>,
    path: String,
}

fn find_project_by_path_in(projects: &[Value], normalized_path: &str) -> Option<Value> {
    projects
        .iter()
        .find(|project| project.get("path").and_then(Value::as_str) == Some(normalized_path))
        .cloned()
}

#[derive(Clone)]
struct GitWorktreeTopologyProbe {
    entries: Vec<GitWorktreeEntry>,
    worktree_root: String,
}

const GIT_WORKTREE_TOPOLOGY_PROBE_TTL: Duration = Duration::from_secs(60);

#[allow(clippy::type_complexity)]
fn git_worktree_topology_probe_cache(
) -> &'static Mutex<HashMap<String, (Instant, Option<GitWorktreeTopologyProbe>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Option<GitWorktreeTopologyProbe>)>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_git_worktree_topology(project_path: &str) -> Option<GitWorktreeTopologyProbe> {
    /*
    Every addProjectPath re-detects worktree metadata, including registration
    repairs for paths that are already known, so clients that re-register
    projects turn each call into git subprocess spawns. Git topology only
    changes on init/worktree edits; cache probes (including non-repo results)
    per path briefly instead of spawning git each time.
    */
    if let Ok(cache) = git_worktree_topology_probe_cache().lock() {
        if let Some((probed_at, probe)) = cache.get(project_path) {
            if probed_at.elapsed() < GIT_WORKTREE_TOPOLOGY_PROBE_TTL {
                return probe.clone();
            }
        }
    }
    let probe = run_git_worktree_topology_probe(project_path);
    if let Ok(mut cache) = git_worktree_topology_probe_cache().lock() {
        cache.insert(project_path.to_string(), (Instant::now(), probe.clone()));
    }
    probe
}

fn run_git_worktree_topology_probe(project_path: &str) -> Option<GitWorktreeTopologyProbe> {
    if run_git(project_path, &["rev-parse", "--is-inside-work-tree"]) != "true" {
        return None;
    }
    let worktree_root =
        normalize_path_for_comparison(&run_git(project_path, &["rev-parse", "--show-toplevel"]));
    if worktree_root.is_empty() {
        return None;
    }
    let entries = parse_git_worktree_list_porcelain(&run_git(
        project_path,
        &["worktree", "list", "--porcelain"],
    ));
    Some(GitWorktreeTopologyProbe {
        entries,
        worktree_root,
    })
}

fn detect_registered_git_worktree_metadata(
    projects: &[Value],
    project_path: &str,
    project_name: &str,
) -> Option<Map<String, Value>> {
    let probe = probe_git_worktree_topology(project_path)?;
    let worktree_root = probe.worktree_root;
    let entries = probe.entries;
    let current_entry = entries
        .iter()
        .find(|entry| normalize_path_for_comparison(&entry.path) == worktree_root)?;
    let main_entry = entries.iter().find(|entry| !entry.bare)?;
    let main_path = normalize_path_for_comparison(&main_entry.path);
    if main_path.is_empty() || worktree_root == main_path {
        return None;
    }

    let parent_project = projects.iter().find(|project| {
        let Some(project_path) = project.get("path").and_then(Value::as_str) else {
            return false;
        };
        if project.get("worktree").is_some() {
            return false;
        }
        normalize_path_for_comparison(project_path) == main_path
    })?;
    let parent_project_id = parent_project.get("projectId").and_then(Value::as_str)?;
    let parent_project_name = parent_project.get("name").and_then(Value::as_str)?;
    let parent_project_path = parent_project.get("path").and_then(Value::as_str)?;
    let worktree_name = path_file_name(&worktree_root).unwrap_or_else(|| project_name.to_string());

    let mut metadata = Map::new();
    metadata.insert(
        "branch".to_string(),
        Value::String(normalize_git_worktree_branch(
            current_entry.branch.as_deref(),
        )),
    );
    metadata.insert("createdAt".to_string(), Value::String(now_iso()));
    metadata.insert("name".to_string(), Value::String(worktree_name));
    metadata.insert(
        "parentProjectId".to_string(),
        Value::String(parent_project_id.to_string()),
    );
    metadata.insert(
        "parentProjectName".to_string(),
        Value::String(parent_project_name.to_string()),
    );
    metadata.insert(
        "parentProjectPath".to_string(),
        Value::String(parent_project_path.to_string()),
    );
    Some(metadata)
}

fn are_project_worktree_metadata_equal(
    current: Option<&Value>,
    expected: &Map<String, Value>,
) -> bool {
    let Some(current) = current.and_then(Value::as_object) else {
        return false;
    };
    [
        "branch",
        "name",
        "parentProjectId",
        "parentProjectName",
        "parentProjectPath",
    ]
    .into_iter()
    .all(|key| current.get(key) == expected.get(key))
}

fn parse_git_worktree_list_porcelain(stdout: &str) -> Vec<GitWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_entry: Option<GitWorktreeEntry> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current_entry.take() {
                if !entry.path.is_empty() {
                    entries.push(entry);
                }
            }
            current_entry = Some(GitWorktreeEntry {
                bare: false,
                branch: None,
                path: path.trim().to_string(),
            });
            continue;
        }

        let Some(entry) = current_entry.as_mut() else {
            continue;
        };
        if line == "bare" {
            entry.bare = true;
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.branch = Some(branch.trim().to_string());
        }
    }

    if let Some(entry) = current_entry {
        if !entry.path.is_empty() {
            entries.push(entry);
        }
    }
    entries
}

fn normalize_git_worktree_branch(branch: Option<&str>) -> String {
    branch
        .map(|branch| {
            branch
                .trim()
                .strip_prefix("refs/heads/")
                .unwrap_or(branch.trim())
        })
        .filter(|branch| !branch.is_empty())
        .unwrap_or("detached")
        .to_string()
}

fn run_git(cwd: &str, args: &[&str]) -> String {
    let Ok(output) = Command::new("git").args(args).current_dir(cwd).output() else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn normalize_path_for_comparison(input: &str) -> String {
    let trimmed = input.trim();
    let without_trailing_slash = trimmed.trim_end_matches(&['/', '\\'][..]);
    let candidate = if without_trailing_slash.is_empty() {
        trimmed
    } else {
        without_trailing_slash
    };
    if candidate.is_empty() {
        return String::new();
    }
    fs::canonicalize(candidate)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| candidate.to_string())
}

fn path_file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_app_user_data_state(db: &Connection) -> DomainResult<Value> {
    let scratch_pad_content = db
        .query_row(
            "SELECT content FROM app_user_data WHERE itemKind = 'scratchPad' AND itemId = 'global'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .unwrap_or_default();
    let mut statement = db
        .prepare(
            r#"
            SELECT itemId, content, title, createdAt, updatedAt
            FROM app_user_data
            WHERE itemKind = 'pinnedPrompt'
            ORDER BY updatedAt DESC, itemId ASC
            "#,
        )
        .map_err(sql_error)?;
    let pinned_prompts = statement
        .query_map([], |row| {
            let prompt_id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let created_at: String = row.get(3)?;
            let updated_at: String = row.get(4)?;
            let normalized_title =
                normalize_app_pinned_prompt_title(title.as_deref().unwrap_or(""), &content);
            Ok(json!({
                "content": content,
                "createdAt": created_at,
                "promptId": prompt_id,
                "title": normalized_title,
                "updatedAt": updated_at,
            }))
        })
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(json!({
        "pinnedPrompts": pinned_prompts,
        "scratchPadContent": scratch_pad_content,
    }))
}

fn required_string_param<'a>(params: &'a Map<String, Value>, key: &str) -> DomainResult<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} must be a string.")))
}

fn optional_trimmed_string_param(
    params: &Map<String, Value>,
    key: &str,
) -> DomainResult<Option<String>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
        }
        Some(_) => Err(DomainStateError::bad_request(format!(
            "{key} must be a string when provided."
        ))),
    }
}

fn create_unique_app_pinned_prompt_id(db: &Connection) -> DomainResult<String> {
    let millis = chrono::Utc::now().timestamp_millis();
    for attempt in 0..MAX_ID_GENERATION_ATTEMPTS {
        let candidate = if attempt == 0 {
            format!("gxserver-prompt-{millis}")
        } else {
            format!("gxserver-prompt-{millis}-{attempt}")
        };
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_user_data WHERE itemKind = 'pinnedPrompt' AND itemId = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::corrupt_state(
        "Could not allocate a unique pinned prompt id.",
    ))
}

fn normalize_app_pinned_prompt_title(title_candidate: &str, content: &str) -> String {
    let trimmed_title = title_candidate.trim();
    if !trimmed_title.is_empty() {
        return trimmed_title.to_string();
    }
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect::<String>())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled Prompt".to_string())
}

const MAX_STASHED_PROMPTS: i64 = 200;
const MAX_STASHED_PROMPT_CONTENT_CHARS: usize = 200_000;

fn create_unique_stashed_prompt_id(db: &Connection) -> DomainResult<String> {
    let millis = chrono::Utc::now().timestamp_millis();
    for attempt in 0..MAX_ID_GENERATION_ATTEMPTS {
        let candidate = if attempt == 0 {
            format!("gxserver-stash-{millis}")
        } else {
            format!("gxserver-stash-{millis}-{attempt}")
        };
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM stashed_prompts WHERE promptId = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(DomainStateError::corrupt_state(
        "Could not allocate a unique stashed prompt id.",
    ))
}

fn stashed_prompt_json_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let prompt_id: String = row.get(0)?;
    let content: String = row.get(1)?;
    let project_id: Option<String> = row.get(2)?;
    let session_id: Option<String> = row.get(3)?;
    let cwd: Option<String> = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    let project_name: Option<String> = row.get(7)?;
    let identity_icon_json: Option<String> = row.get(8)?;
    /*
    CDXC:StashedPrompts 2026-07-29:
    Stash rows label their origin project with the same identity icon the
    sidebar and Recent Projects use, so publish only the two presentation
    fields those React components read (`icon`, `iconDataUrl`) and let the
    client fall back to a folder glyph.
    */
    let identity_icon = identity_icon_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    let project_icon = identity_icon
        .as_ref()
        .and_then(|icon| icon.get("icon").cloned());
    let project_icon_data_url = identity_icon
        .as_ref()
        .and_then(|icon| icon.get("iconDataUrl").and_then(Value::as_str))
        .map(str::to_string);
    Ok(json!({
        "content": content,
        "createdAt": created_at,
        "cwd": cwd,
        "projectIcon": project_icon,
        "projectIconDataUrl": project_icon_data_url,
        "projectId": project_id,
        "projectName": project_name,
        "promptId": prompt_id,
        "sessionId": session_id,
        "updatedAt": updated_at,
    }))
}

fn read_stashed_prompt_row(db: &Connection, prompt_id: &str) -> DomainResult<Option<Value>> {
    db.query_row(
        r#"
        SELECT s.promptId, s.content, s.projectId, s.sessionId, s.cwd,
               s.createdAt, s.updatedAt, p.name, p.identityIconJson
        FROM stashed_prompts s
        LEFT JOIN projects p ON p.projectId = s.projectId
        WHERE s.promptId = ?1
        "#,
        [prompt_id],
        stashed_prompt_json_from_row,
    )
    .optional()
    .map_err(sql_error)
}

fn stashed_prompt_project_family(
    db: &Connection,
    project_id: &str,
) -> DomainResult<HashSet<String>> {
    let mut statement = db
        .prepare("SELECT projectId, worktreeJson FROM projects")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let project_id: String = row.get(0)?;
            let worktree_json: Option<String> = row.get(1)?;
            Ok((project_id, worktree_json))
        })
        .map_err(sql_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let parent_by_project: HashMap<String, String> = rows
        .iter()
        .filter_map(|(id, worktree_json)| {
            let parent = serde_json::from_str::<Value>(worktree_json.as_deref()?)
                .ok()?
                .get("parentProjectId")?
                .as_str()
                .filter(|parent| !parent.is_empty())?
                .to_string();
            Some((id.clone(), parent))
        })
        .collect();
    let root = parent_by_project
        .get(project_id)
        .cloned()
        .unwrap_or_else(|| project_id.to_string());
    let mut family: HashSet<String> = HashSet::new();
    family.insert(project_id.to_string());
    family.insert(root.clone());
    for (id, parent) in &parent_by_project {
        if parent == &root {
            family.insert(id.clone());
        }
    }
    Ok(family)
}

pub fn read_domain_rpc_params(body: &Value) -> DomainResult<Map<String, Value>> {
    let Some(object) = body.as_object() else {
        return Err(DomainStateError::bad_request(
            "RPC request body must be an object.",
        ));
    };
    match object.get("params") {
        None => Ok(Map::new()),
        Some(Value::Object(params)) => Ok(params.clone()),
        Some(_) => Err(DomainStateError::bad_request(
            "RPC params must be an object.",
        )),
    }
}

pub fn read_optional_project_id(params: &Map<String, Value>) -> DomainResult<Option<String>> {
    match params.get("projectId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        _ => read_project_id(params).map(Some),
    }
}

pub fn read_project_id(params: &Map<String, Value>) -> DomainResult<String> {
    let value = params
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_gxserver_project_id(value) {
        return Err(DomainStateError::bad_request(format!(
            "Invalid gxserver project ID: {}.",
            js_string(params.get("projectId"))
        )));
    }
    Ok(value.to_string())
}

/*
CDXC:GxserverCrudParity 2026-06-22-05:39:
TypeScript CRUD update/remove paths for projects and sessions call repository lookup methods before ID validators. Preserve that not-found behavior for stale or client-local IDs while keeping explicit readers strict for list filters, create-session project resolution, removeProject, and lifecycle APIs.
*/
fn read_unvalidated_project_lookup_id(params: &Map<String, Value>) -> String {
    params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| js_string(params.get("projectId")))
}

pub fn read_session_id(params: &Map<String, Value>) -> DomainResult<String> {
    let value = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_gxserver_session_id(value) {
        return Err(DomainStateError::bad_request(format!(
            "Invalid gxserver session ID: {}.",
            js_string(params.get("sessionId"))
        )));
    }
    Ok(value.to_string())
}

fn read_unvalidated_session_lookup_id(params: &Map<String, Value>) -> String {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| js_string(params.get("sessionId")))
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| js_string(Some(item)))
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

fn normalize_project_input(
    project_id: &str,
    timestamp: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let mut project = Map::new();
    project.insert(
        "attentionRules".to_string(),
        Value::Object(normalize_object(input.get("attentionRules"))),
    );
    project.insert(
        "completionRules".to_string(),
        Value::Object(normalize_object(input.get("completionRules"))),
    );
    project.insert(
        "createdAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    project.insert(
        "customAgentOrder".to_string(),
        Value::Array(
            normalize_string_array(input.get("customAgentOrder"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "customAgents".to_string(),
        Value::Array(normalize_object_array(input.get("customAgents"))),
    );
    project.insert(
        "customCommandOrder".to_string(),
        Value::Array(
            normalize_string_array(input.get("customCommandOrder"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "customCommands".to_string(),
        Value::Array(normalize_object_array(input.get("customCommands"))),
    );
    insert_optional_string(
        &mut project,
        "defaultCommand",
        read_optional_text(input.get("defaultCommand")),
    );
    project.insert(
        "deletedDefaultCommandIds".to_string(),
        Value::Array(
            normalize_string_array(input.get("deletedDefaultCommandIds"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "gitConfig".to_string(),
        Value::Object(normalize_object(input.get("gitConfig"))),
    );
    insert_optional_object(
        &mut project,
        "identityIcon",
        normalize_object(input.get("identityIcon")),
    );
    project.insert(
        "isFavorite".to_string(),
        Value::Bool(input.get("isFavorite").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "isPinned".to_string(),
        Value::Bool(input.get("isPinned").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "isRecentProject".to_string(),
        Value::Bool(input.get("isRecentProject").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "launchSettings".to_string(),
        Value::Object(normalize_object(input.get("launchSettings"))),
    );
    project.insert(
        "name".to_string(),
        Value::String(normalize_required_text(input.get("name"), "name")?),
    );
    project.insert(
        "notificationRules".to_string(),
        Value::Object(normalize_object(input.get("notificationRules"))),
    );
    insert_optional_string(&mut project, "path", read_optional_text(input.get("path")));
    project.insert(
        "previousSessionHistory".to_string(),
        Value::Array(normalize_object_array(input.get("previousSessionHistory"))),
    );
    project.insert(
        "projectBoardConfig".to_string(),
        Value::Object(normalize_object(input.get("projectBoardConfig"))),
    );
    project.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    insert_optional_string(
        &mut project,
        "recentClosedAt",
        read_optional_text(input.get("recentClosedAt")),
    );
    project.insert(
        "runtimeSettings".to_string(),
        Value::Object(normalize_object(input.get("runtimeSettings"))),
    );
    insert_optional_string(
        &mut project,
        "systemKind",
        normalize_project_system_kind(input.get("systemKind"))?,
    );
    project.insert(
        "updatedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    project.insert(
        "visibility".to_string(),
        Value::String(normalize_project_visibility(input.get("visibility"))?),
    );
    insert_optional_object(
        &mut project,
        "worktree",
        normalize_object(input.get("worktree")),
    );
    Ok(Value::Object(project))
}

fn merge_project_update(
    current: Value,
    updated_at: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let current = current.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Project row did not decode as an object.")
    })?;
    let mut next = current.clone();
    update_object_field(&mut next, input, "attentionRules");
    update_object_field(&mut next, input, "completionRules");
    update_string_array_field(&mut next, input, "customAgentOrder");
    update_object_array_field(&mut next, input, "customAgents");
    update_string_array_field(&mut next, input, "customCommandOrder");
    update_object_array_field(&mut next, input, "customCommands");
    update_optional_text_field(&mut next, input, "defaultCommand");
    update_string_array_field(&mut next, input, "deletedDefaultCommandIds");
    update_object_field(&mut next, input, "gitConfig");
    update_optional_object_field(&mut next, input, "identityIcon");
    if let Some(value) = input.get("isFavorite") {
        next.insert(
            "isFavorite".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    if let Some(value) = input.get("isPinned") {
        next.insert(
            "isPinned".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    if let Some(value) = input.get("isRecentProject") {
        next.insert(
            "isRecentProject".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    update_object_field(&mut next, input, "launchSettings");
    if input.contains_key("name") {
        next.insert(
            "name".to_string(),
            Value::String(normalize_required_text(input.get("name"), "name")?),
        );
    }
    update_object_field(&mut next, input, "notificationRules");
    update_optional_text_field(&mut next, input, "path");
    update_object_array_field(&mut next, input, "previousSessionHistory");
    update_object_field(&mut next, input, "projectBoardConfig");
    update_optional_text_field(&mut next, input, "recentClosedAt");
    update_object_field(&mut next, input, "runtimeSettings");
    if input.contains_key("systemKind") {
        insert_optional_string(
            &mut next,
            "systemKind",
            normalize_project_system_kind(input.get("systemKind"))?,
        );
    }
    if input.contains_key("visibility") {
        next.insert(
            "visibility".to_string(),
            Value::String(normalize_project_visibility(input.get("visibility"))?),
        );
    }
    update_optional_object_field(&mut next, input, "worktree");
    next.insert(
        "updatedAt".to_string(),
        Value::String(updated_at.to_string()),
    );
    Ok(Value::Object(next))
}

fn normalize_project_visibility(value: Option<&Value>) -> DomainResult<String> {
    match normalize_optional_project_enum_text(value, "visibility")?.as_deref() {
        None => Ok("visible".to_string()),
        Some("visible") => Ok("visible".to_string()),
        Some("hidden") => Ok("hidden".to_string()),
        Some(_) => Err(DomainStateError::bad_request(
            "visibility must be either visible or hidden.",
        )),
    }
}

fn normalize_project_system_kind(value: Option<&Value>) -> DomainResult<Option<String>> {
    match normalize_optional_project_enum_text(value, "systemKind")?.as_deref() {
        None => Ok(None),
        Some("remoteAttachCarrier") => Ok(Some("remoteAttachCarrier".to_string())),
        Some(_) => Err(DomainStateError::bad_request(
            "systemKind must be remoteAttachCarrier when provided.",
        )),
    }
}

fn normalize_optional_project_enum_text(
    value: Option<&Value>,
    field: &str,
) -> DomainResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(value
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string())
        .map(|value| if value.is_empty() { None } else { Some(value) }),
        Some(_) => Err(DomainStateError::bad_request(format!(
            "{field} must be a string when provided."
        ))),
    }
}

fn normalize_session_input(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    timestamp: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let zmx_name = create_zmx_session_name(server_id, project_id, session_id);
    let title = read_optional_text(input.get("title")).unwrap_or_else(|| session_id.to_string());
    let mut runtime_settings = normalize_object(input.get("runtimeSettings"));
    if is_temporary_session_title(&title) && !has_string_field(&runtime_settings, "titleSource") {
        runtime_settings.insert(
            "titleSource".to_string(),
            Value::String("placeholder".to_string()),
        );
    }
    if !runtime_settings.contains_key("agentActivity") {
        runtime_settings.insert(
            "agentActivity".to_string(),
            default_agent_activity(input.get("agentId").and_then(Value::as_str), timestamp),
        );
    }
    let mut launch_settings = normalize_object(input.get("launchSettings"));
    normalize_launch_settings_with_surface(&mut launch_settings, input.get("surface"));
    let surface = resolve_surface(input.get("surface"), &launch_settings, &runtime_settings);
    let session_tag = normalize_optional_session_tag(input.get("sessionTag"))?;
    let provider_state =
        normalize_zmx_provider_state(normalize_object(input.get("providerState")), &zmx_name);

    let mut session = Map::new();
    insert_optional_string(
        &mut session,
        "agentId",
        read_optional_text(input.get("agentId")),
    );
    session.insert(
        "attentionRules".to_string(),
        Value::Object(normalize_object(input.get("attentionRules"))),
    );
    insert_optional_string(
        &mut session,
        "commandId",
        read_optional_text(input.get("commandId")),
    );
    session.insert(
        "completionRules".to_string(),
        Value::Object(normalize_object(input.get("completionRules"))),
    );
    session.insert(
        "createdAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    insert_optional_string(&mut session, "cwd", read_optional_text(input.get("cwd")));
    session.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(server_id, project_id, session_id)),
    );
    let mut hidden = Map::new();
    insert_optional_string(
        &mut hidden,
        "restoredFromHistoryId",
        read_optional_text(input.get("restoredFromHistoryId")),
    );
    if let Some(restored) = normalize_session_restore_id(input.get("restoredFromSessionId"))? {
        hidden.insert("restoredFromSessionId".to_string(), Value::String(restored));
    }
    session.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    session.insert(
        "isFavorite".to_string(),
        Value::Bool(
            session_tag.as_deref() == Some("favorite")
                || (session_tag.is_none()
                    && input.get("isFavorite").and_then(Value::as_bool) == Some(true)),
        ),
    );
    session.insert(
        "isPinned".to_string(),
        Value::Bool(input.get("isPinned").and_then(Value::as_bool) == Some(true)),
    );
    session.insert(
        "kind".to_string(),
        Value::String(normalize_session_kind(input.get("kind"))),
    );
    insert_optional_string(
        &mut session,
        "lastActiveAt",
        read_optional_text(input.get("lastActiveAt")),
    );
    session.insert("launchSettings".to_string(), Value::Object(launch_settings));
    session.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_domain_lifecycle_state(
            input.get("lifecycleState"),
        )),
    );
    session.insert(
        "notificationRules".to_string(),
        Value::Object(normalize_object(input.get("notificationRules"))),
    );
    session.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    session.insert("providerState".to_string(), Value::Object(provider_state));
    session.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    session.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    if let Some(tag) = session_tag {
        session.insert("sessionTag".to_string(), Value::String(tag));
    }
    if let Some(order) = normalize_optional_sidebar_order(input.get("sidebarOrder")) {
        session.insert("sidebarOrder".to_string(), json!(order));
    } else {
        session.insert("sidebarOrder".to_string(), json!(0));
    }
    session.insert("surface".to_string(), Value::String(surface));
    session.insert("title".to_string(), Value::String(title));
    session.insert(
        "updatedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    insert_optional_object(
        &mut session,
        "worktree",
        normalize_object(input.get("worktree")),
    );
    session.insert("zmxName".to_string(), Value::String(zmx_name));
    Ok(Value::Object(session))
}

fn normalize_zmx_provider_state(
    mut provider_state: Map<String, Value>,
    zmx_name: &str,
) -> Map<String, Value> {
    /*
    CDXC:RemotePresentation 2026-06-30-00:11:
    Remote sidebar clients depend on presentation publishing both the canonical zmx session name and its provider label so titles, status dots, and native idle indicators agree. Store `provider: "zmx"` with every gxserver session provider state instead of forcing clients to infer it from zmxName.
    */
    provider_state.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_provider_lifecycle_state(
            provider_state.get("lifecycleState"),
        )),
    );
    provider_state.insert("provider".to_string(), Value::String("zmx".to_string()));
    provider_state.insert("zmxName".to_string(), Value::String(zmx_name.to_string()));
    provider_state
}

fn merge_session_update(
    server_id: &str,
    current: Value,
    updated_at: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let current = current.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Session row did not decode as an object.")
    })?;
    let project_id = current
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state("projectId missing from session domain state.")
        })?;
    let session_id = current
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state("sessionId missing from session domain state.")
        })?;
    let zmx_name = create_zmx_session_name(server_id, &project_id, &session_id);
    let mut next = current.clone();
    update_optional_text_field(&mut next, input, "agentId");
    update_object_field(&mut next, input, "attentionRules");
    update_optional_text_field(&mut next, input, "commandId");
    update_object_field(&mut next, input, "completionRules");
    update_optional_text_field(&mut next, input, "cwd");
    let mut hidden = next
        .get("hiddenMetadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if input.contains_key("restoredFromHistoryId") {
        set_optional_string(
            &mut hidden,
            "restoredFromHistoryId",
            read_optional_text(input.get("restoredFromHistoryId")),
        );
    }
    if input.contains_key("restoredFromSessionId") {
        if let Some(restored) = normalize_session_restore_id(input.get("restoredFromSessionId"))? {
            hidden.insert("restoredFromSessionId".to_string(), Value::String(restored));
        } else {
            hidden.remove("restoredFromSessionId");
        }
    }
    next.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    if input.contains_key("isPinned") {
        next.insert(
            "isPinned".to_string(),
            Value::Bool(input.get("isPinned").and_then(Value::as_bool) == Some(true)),
        );
    }
    if input.contains_key("kind") {
        next.insert(
            "kind".to_string(),
            Value::String(normalize_session_kind(input.get("kind"))),
        );
    }
    update_optional_text_field(&mut next, input, "lastActiveAt");
    let mut launch_settings = if input.contains_key("launchSettings") {
        normalize_object(input.get("launchSettings"))
    } else {
        next.get("launchSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let runtime_settings = if input.contains_key("runtimeSettings") {
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .or_else(|| next.get("title").and_then(Value::as_str));
        let mut settings = normalize_object(input.get("runtimeSettings"));
        if title.map(is_temporary_session_title).unwrap_or(false)
            && !has_string_field(&settings, "titleSource")
        {
            settings.insert(
                "titleSource".to_string(),
                Value::String("placeholder".to_string()),
            );
        }
        settings
    } else {
        next.get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    if input.contains_key("launchSettings") || input.contains_key("surface") {
        let explicit_surface = input
            .get("surface")
            .filter(|_| input.contains_key("surface"));
        normalize_launch_settings_with_surface(&mut launch_settings, explicit_surface);
        let surface = resolve_surface(explicit_surface, &launch_settings, &runtime_settings);
        next.insert("surface".to_string(), Value::String(surface));
    } else {
        next.insert(
            "surface".to_string(),
            Value::String(resolve_surface(None, &launch_settings, &runtime_settings)),
        );
    }
    next.insert("launchSettings".to_string(), Value::Object(launch_settings));
    if input.contains_key("lifecycleState") {
        next.insert(
            "lifecycleState".to_string(),
            Value::String(normalize_domain_lifecycle_state(
                input.get("lifecycleState"),
            )),
        );
    }
    update_object_field(&mut next, input, "notificationRules");
    if input.contains_key("providerState") {
        let provider_state =
            normalize_zmx_provider_state(normalize_object(input.get("providerState")), &zmx_name);
        next.insert("providerState".to_string(), Value::Object(provider_state));
    } else if let Some(provider_state) = next
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
    {
        let provider_state = normalize_zmx_provider_state(provider_state, &zmx_name);
        next.insert("providerState".to_string(), Value::Object(provider_state));
    }
    next.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if input.contains_key("sessionTag") || input.contains_key("isFavorite") {
        let session_tag = if input.contains_key("sessionTag") {
            normalize_optional_session_tag(input.get("sessionTag"))?
        } else if input.get("isFavorite").and_then(Value::as_bool) == Some(true) {
            Some("favorite".to_string())
        } else {
            None
        };
        if let Some(tag) = session_tag {
            next.insert("sessionTag".to_string(), Value::String(tag.clone()));
            next.insert("isFavorite".to_string(), Value::Bool(tag == "favorite"));
        } else {
            next.remove("sessionTag");
            next.insert("isFavorite".to_string(), Value::Bool(false));
        }
    }
    if input.contains_key("sidebarOrder") {
        match normalize_optional_sidebar_order(input.get("sidebarOrder")) {
            Some(order) => {
                next.insert("sidebarOrder".to_string(), json!(order));
            }
            None => {
                next.remove("sidebarOrder");
            }
        }
    }
    if input.contains_key("title") {
        next.insert(
            "title".to_string(),
            Value::String(read_optional_text(input.get("title")).unwrap_or(session_id.clone())),
        );
    }
    update_optional_object_field(&mut next, input, "worktree");
    next.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(
            server_id,
            &project_id,
            &session_id,
        )),
    );
    next.insert(
        "updatedAt".to_string(),
        Value::String(updated_at.to_string()),
    );
    next.insert("zmxName".to_string(), Value::String(zmx_name));
    Ok(Value::Object(next))
}

fn normalize_create_agent_session_params(input: &Map<String, Value>) -> Map<String, Value> {
    let mut params = input.clone();
    let agent_id = read_optional_text(input.get("agentId")).unwrap_or_else(|| "codex".to_string());
    let mut launch_settings = normalize_object(input.get("launchSettings"));
    let mut runtime_settings = normalize_object(input.get("runtimeSettings"));
    let base_command = read_optional_text(launch_settings.get("agentCommand"))
        .or_else(|| default_agent_command(&agent_id).map(str::to_string))
        .unwrap_or_default();
    let command = apply_agent_accept_all(&agent_id, &base_command);
    let startup_text = if command.is_empty() {
        String::new()
    } else {
        format!(" {command}\r")
    };
    let mut plan = Map::new();
    if !base_command.is_empty() {
        plan.insert(
            "agentCommand".to_string(),
            Value::String(base_command.clone()),
        );
    }
    plan.insert("command".to_string(), Value::String(command.clone()));
    plan.insert("startupText".to_string(), Value::String(startup_text));
    plan.insert(
        "startupTextDisposition".to_string(),
        Value::String(
            if command.is_empty() {
                "none"
            } else {
                "queueAfterTerminalReady"
            }
            .to_string(),
        ),
    );
    if let Some(first_user_message) = read_optional_text(runtime_settings.get("firstUserMessage")) {
        plan.insert(
            "firstUserMessage".to_string(),
            Value::String(first_user_message),
        );
    }
    launch_settings.insert("agentLaunchPlan".to_string(), Value::Object(plan));
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        json!({ "queueProviderStartupText": !command.is_empty() }),
    );
    if !command.is_empty() {
        runtime_settings.insert("agentCommand".to_string(), Value::String(base_command));
    }
    runtime_settings.insert("agentName".to_string(), Value::String(agent_id.clone()));
    runtime_settings.insert("launchAgentId".to_string(), Value::String(agent_id.clone()));
    params.insert("agentId".to_string(), Value::String(agent_id));
    params.insert("kind".to_string(), Value::String("agent".to_string()));
    params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    params.insert(
        "lifecycleState".to_string(),
        input
            .get("lifecycleState")
            .cloned()
            .unwrap_or_else(|| Value::String("running".to_string())),
    );
    params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    params
}

fn default_agent_command(agent_id: &str) -> Option<&'static str> {
    /*
    CDXC:AgentDefaults 2026-07-02-14:20:
    The launcher default-command registry is owned by the agents module so every
    create path resolves the same command set; keeping a second literal map here
    silently dropped newer built-in agents from this normalization path.
    */
    crate::agents::default_agent_command(agent_id)
}

fn apply_agent_accept_all(agent_id: &str, command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if agent_id == "codex" && !trimmed.split_whitespace().any(|token| token == "--yolo") {
        return format!("{trimmed} --yolo");
    }
    trimmed.to_string()
}

fn default_agent_activity(agent_id: Option<&str>, timestamp: &str) -> Value {
    let mut activity = Map::new();
    activity.insert("activity".to_string(), Value::String("idle".to_string()));
    if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
        activity.insert("agentName".to_string(), Value::String(agent_id.to_string()));
    }
    activity.insert("hasSeenWorking".to_string(), Value::Bool(false));
    activity.insert("isAcknowledged".to_string(), Value::Bool(true));
    activity.insert(
        "lastChangedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    activity.insert(
        "suppressedUntil".to_string(),
        Value::String(timestamp.to_string()),
    );
    Value::Object(activity)
}

fn reject_stopped_session_revive(
    current: &Value,
    input: &Map<String, Value>,
    reason: &str,
) -> DomainResult<()> {
    if current.get("lifecycleState").and_then(Value::as_str) != Some("stopped") {
        return Ok(());
    }
    if let Some(requested) = input.get("lifecycleState").and_then(Value::as_str) {
        if requested != "stopped" {
            return Err(DomainStateError::bad_request(format!(
                "{reason} cannot change a stopped session to {requested}; use a lifecycle endpoint to wake or start it."
            )));
        }
    }
    if input
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|provider| provider.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists")
    {
        return Err(DomainStateError::bad_request(format!(
            "{reason} cannot mark a stopped session provider as exists; use a lifecycle endpoint to wake or start it."
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct ProjectRow {
    attention_rules_json: String,
    completion_rules_json: String,
    created_at: String,
    custom_agent_order_json: String,
    custom_agents_json: String,
    custom_command_order_json: String,
    custom_commands_json: String,
    default_command: Option<String>,
    deleted_default_command_ids_json: String,
    git_config_json: String,
    identity_icon_json: String,
    is_favorite: i64,
    is_pinned: i64,
    is_recent_project: i64,
    launch_settings_json: String,
    name: String,
    notification_rules_json: String,
    path: Option<String>,
    previous_session_history_json: String,
    project_board_config_json: String,
    project_id: String,
    recent_closed_at: Option<String>,
    runtime_settings_json: String,
    system_kind: Option<String>,
    updated_at: String,
    visibility: String,
    worktree_json: String,
}

fn project_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        project_id: row.get("projectId")?,
        name: row.get("name")?,
        path: row.get("path")?,
        identity_icon_json: row.get("identityIconJson")?,
        is_pinned: row.get("isPinned")?,
        is_favorite: row.get("isFavorite")?,
        is_recent_project: row.get("isRecentProject")?,
        recent_closed_at: row.get("recentClosedAt")?,
        visibility: row.get("visibility")?,
        system_kind: row.get("systemKind")?,
        default_command: row.get("defaultCommand")?,
        worktree_json: row.get("worktreeJson")?,
        custom_agents_json: row.get("customAgentsJson")?,
        custom_agent_order_json: row.get("customAgentOrderJson")?,
        custom_commands_json: row.get("customCommandsJson")?,
        custom_command_order_json: row.get("customCommandOrderJson")?,
        deleted_default_command_ids_json: row.get("deletedDefaultCommandIdsJson")?,
        launch_settings_json: row.get("launchSettingsJson")?,
        runtime_settings_json: row.get("runtimeSettingsJson")?,
        completion_rules_json: row.get("completionRulesJson")?,
        attention_rules_json: row.get("attentionRulesJson")?,
        notification_rules_json: row.get("notificationRulesJson")?,
        git_config_json: row.get("gitConfigJson")?,
        project_board_config_json: row.get("projectBoardConfigJson")?,
        previous_session_history_json: row.get("previousSessionHistoryJson")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

fn project_from_row(row: ProjectRow) -> DomainResult<Value> {
    let mut project = Map::new();
    project.insert(
        "attentionRules".to_string(),
        parse_object(
            &row.attention_rules_json,
            "attentionRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "completionRules".to_string(),
        parse_object(
            &row.completion_rules_json,
            "completionRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert("createdAt".to_string(), Value::String(row.created_at));
    project.insert(
        "customAgentOrder".to_string(),
        parse_string_array(
            &row.custom_agent_order_json,
            "customAgentOrderJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customAgents".to_string(),
        parse_object_array(
            &row.custom_agents_json,
            "customAgentsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customCommandOrder".to_string(),
        parse_string_array(
            &row.custom_command_order_json,
            "customCommandOrderJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customCommands".to_string(),
        parse_object_array(
            &row.custom_commands_json,
            "customCommandsJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "defaultCommand", row.default_command);
    project.insert(
        "deletedDefaultCommandIds".to_string(),
        parse_string_array(
            &row.deleted_default_command_ids_json,
            "deletedDefaultCommandIdsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "gitConfig".to_string(),
        parse_object(
            &row.git_config_json,
            "gitConfigJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_parsed_optional_object(
        &mut project,
        "identityIcon",
        &row.identity_icon_json,
        "identityIconJson",
        "project",
        &row.project_id,
    )?;
    project.insert("isFavorite".to_string(), Value::Bool(row.is_favorite == 1));
    project.insert("isPinned".to_string(), Value::Bool(row.is_pinned == 1));
    project.insert(
        "isRecentProject".to_string(),
        Value::Bool(row.is_recent_project == 1),
    );
    project.insert(
        "launchSettings".to_string(),
        parse_object(
            &row.launch_settings_json,
            "launchSettingsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert("name".to_string(), Value::String(row.name));
    project.insert(
        "notificationRules".to_string(),
        parse_object(
            &row.notification_rules_json,
            "notificationRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "path", row.path);
    project.insert(
        "previousSessionHistory".to_string(),
        parse_object_array(
            &row.previous_session_history_json,
            "previousSessionHistoryJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "projectBoardConfig".to_string(),
        parse_object(
            &row.project_board_config_json,
            "projectBoardConfigJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "projectId".to_string(),
        Value::String(row.project_id.clone()),
    );
    insert_optional_string(&mut project, "recentClosedAt", row.recent_closed_at);
    project.insert(
        "runtimeSettings".to_string(),
        parse_object(
            &row.runtime_settings_json,
            "runtimeSettingsJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "systemKind", row.system_kind);
    project.insert("updatedAt".to_string(), Value::String(row.updated_at));
    project.insert("visibility".to_string(), Value::String(row.visibility));
    insert_parsed_optional_object(
        &mut project,
        "worktree",
        &row.worktree_json,
        "worktreeJson",
        "project",
        &row.project_id,
    )?;
    Ok(Value::Object(project))
}

fn recent_project_from_project(project: &Value, session_count: usize) -> DomainResult<Value> {
    let object = project.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Project row did not decode as an object.")
    })?;
    let project_id = required_string(object, "projectId")?;
    let title = required_string(object, "name")?;
    let path = optional_string(object, "path")
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| {
            DomainStateError::corrupt_state(format!(
                "Recent project {project_id} did not have a path."
            ))
        })?;
    let mut recent_project = Map::new();
    recent_project.insert("path".to_string(), Value::String(path));
    recent_project.insert("projectId".to_string(), Value::String(project_id));
    recent_project.insert(
        "sessionCount".to_string(),
        Value::Number(serde_json::Number::from(session_count)),
    );
    recent_project.insert("title".to_string(), Value::String(title));
    insert_optional_string(
        &mut recent_project,
        "recentClosedAt",
        optional_string(object, "recentClosedAt"),
    );
    if let Some(identity_icon) = object.get("identityIcon").and_then(Value::as_object) {
        insert_optional_value(
            &mut recent_project,
            "icon",
            identity_icon.get("icon").cloned(),
        );
        insert_optional_string(
            &mut recent_project,
            "iconDataUrl",
            identity_icon
                .get("iconDataUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
        insert_optional_string(
            &mut recent_project,
            "theme",
            identity_icon
                .get("theme")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
        insert_optional_string(
            &mut recent_project,
            "themeColor",
            identity_icon
                .get("themeColor")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
    }
    Ok(Value::Object(recent_project))
}

#[derive(Debug)]
struct SessionRow {
    agent_id: Option<String>,
    attention_rules_json: String,
    command_id: Option<String>,
    completion_rules_json: String,
    created_at: String,
    cwd: Option<String>,
    is_favorite: i64,
    is_pinned: i64,
    kind: String,
    last_active_at: Option<String>,
    launch_settings_json: String,
    lifecycle_state: String,
    notification_rules_json: String,
    project_id: String,
    provider_state_json: String,
    restored_from_history_id: Option<String>,
    restored_from_session_id: Option<String>,
    runtime_settings_json: String,
    session_id: String,
    session_tag: Option<String>,
    settled_at: Option<String>,
    settled_override: Option<String>,
    settled_override_at: Option<String>,
    sidebar_order: Option<f64>,
    snoozed_at: Option<String>,
    snoozed_until: Option<String>,
    title: String,
    updated_at: String,
    worktree_json: String,
    zmx_name: String,
}

fn session_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        lifecycle_state: row.get("lifecycleState")?,
        provider_state_json: row.get("providerStateJson")?,
        zmx_name: row.get("zmxName")?,
        cwd: row.get("cwd")?,
        agent_id: row.get("agentId")?,
        command_id: row.get("commandId")?,
        is_pinned: row.get("isPinned")?,
        is_favorite: row.get("isFavorite")?,
        restored_from_session_id: row.get("restoredFromSessionId")?,
        restored_from_history_id: row.get("restoredFromHistoryId")?,
        launch_settings_json: row.get("launchSettingsJson")?,
        runtime_settings_json: row.get("runtimeSettingsJson")?,
        completion_rules_json: row.get("completionRulesJson")?,
        attention_rules_json: row.get("attentionRulesJson")?,
        notification_rules_json: row.get("notificationRulesJson")?,
        worktree_json: row.get("worktreeJson")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
        last_active_at: row.get("lastActiveAt")?,
        sidebar_order: row.get("sidebarOrder")?,
        session_tag: row.get("sessionTag")?,
        settled_at: row.get("settledAt")?,
        settled_override: row.get("settledOverride")?,
        settled_override_at: row.get("settledOverrideAt")?,
        snoozed_at: row.get("snoozedAt")?,
        snoozed_until: row.get("snoozedUntil")?,
    })
}

fn session_from_row(server_id: &str, row: SessionRow) -> DomainResult<Value> {
    let row_id = format!("{}/{}", row.project_id, row.session_id);
    let zmx_name = create_zmx_session_name(server_id, &row.project_id, &row.session_id);
    let mut provider_state = parse_object_map(
        &row.provider_state_json,
        "providerStateJson",
        "session",
        &row_id,
    )?;
    provider_state = normalize_zmx_provider_state(provider_state, &zmx_name);
    let launch_settings = parse_object_map(
        &row.launch_settings_json,
        "launchSettingsJson",
        "session",
        &row_id,
    )?;
    let runtime_settings = parse_object_map(
        &row.runtime_settings_json,
        "runtimeSettingsJson",
        "session",
        &row_id,
    )?;
    let worktree = parse_object_map(&row.worktree_json, "worktreeJson", "session", &row_id)?;
    /*
    CDXC:SessionTags 2026-06-22-05:58:
    Stored sessionTag values are durable gxserver metadata, so Rust row hydration must reject the same invalid non-empty values as TypeScript instead of silently hiding corrupt or retired tags from clients.
    Legacy rows that only have isFavorite still hydrate as the Favorite tag so old state.db files keep the expanded tag model after migration.
    */
    let tag = match row.session_tag.as_deref() {
        Some(value) => normalize_optional_session_tag(Some(&Value::String(value.to_string())))?,
        None if row.is_favorite == 1 => Some("favorite".to_string()),
        None => None,
    };
    let mut session = Map::new();
    insert_optional_string(&mut session, "agentId", row.agent_id);
    session.insert(
        "attentionRules".to_string(),
        parse_object(
            &row.attention_rules_json,
            "attentionRulesJson",
            "session",
            &row_id,
        )?,
    );
    insert_optional_string(&mut session, "commandId", row.command_id);
    session.insert(
        "completionRules".to_string(),
        parse_object(
            &row.completion_rules_json,
            "completionRulesJson",
            "session",
            &row_id,
        )?,
    );
    session.insert("createdAt".to_string(), Value::String(row.created_at));
    insert_optional_string(&mut session, "cwd", row.cwd);
    session.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(
            server_id,
            &row.project_id,
            &row.session_id,
        )),
    );
    let mut hidden = Map::new();
    insert_optional_string(
        &mut hidden,
        "restoredFromHistoryId",
        row.restored_from_history_id,
    );
    insert_optional_string(
        &mut hidden,
        "restoredFromSessionId",
        row.restored_from_session_id,
    );
    session.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    session.insert(
        "isFavorite".to_string(),
        Value::Bool(tag.as_deref() == Some("favorite") || row.is_favorite == 1),
    );
    session.insert("isPinned".to_string(), Value::Bool(row.is_pinned == 1));
    session.insert(
        "kind".to_string(),
        Value::String(normalize_session_kind(Some(&Value::String(row.kind)))),
    );
    insert_optional_string(&mut session, "lastActiveAt", row.last_active_at);
    session.insert(
        "launchSettings".to_string(),
        Value::Object(launch_settings.clone()),
    );
    session.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_domain_lifecycle_state(Some(&Value::String(
            row.lifecycle_state,
        )))),
    );
    session.insert(
        "notificationRules".to_string(),
        parse_object(
            &row.notification_rules_json,
            "notificationRulesJson",
            "session",
            &row_id,
        )?,
    );
    session.insert(
        "projectId".to_string(),
        Value::String(row.project_id.clone()),
    );
    session.insert("providerState".to_string(), Value::Object(provider_state));
    session.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings.clone()),
    );
    session.insert("sessionId".to_string(), Value::String(row.session_id));
    if let Some(tag) = tag {
        session.insert("sessionTag".to_string(), Value::String(tag));
    }
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Settle/snooze columns are absent (NULL) on every state.db written before
    migration 0016, and the whole lifecycle is "no state" in that case. Hydrate
    them as omitted keys rather than explicit nulls so presentation, the CLI
    contract, and the client predicates all see the same "never settled, never
    snoozed" shape old rows already produce.
    */
    insert_optional_trimmed_string(&mut session, "settledAt", row.settled_at);
    insert_optional_string(
        &mut session,
        "settledOverride",
        normalize_settled_override(row.settled_override.as_deref()),
    );
    insert_optional_trimmed_string(&mut session, "settledOverrideAt", row.settled_override_at);
    if let Some(order) = row.sidebar_order.filter(|value| value.is_finite()) {
        session.insert("sidebarOrder".to_string(), json!(order));
    }
    insert_optional_trimmed_string(&mut session, "snoozedAt", row.snoozed_at);
    insert_optional_trimmed_string(&mut session, "snoozedUntil", row.snoozed_until);
    session.insert(
        "surface".to_string(),
        Value::String(resolve_surface(None, &launch_settings, &runtime_settings)),
    );
    session.insert("title".to_string(), Value::String(row.title));
    session.insert("updatedAt".to_string(), Value::String(row.updated_at));
    if !worktree.is_empty() {
        session.insert("worktree".to_string(), Value::Object(worktree));
    }
    session.insert("zmxName".to_string(), Value::String(zmx_name));
    let _ = row.zmx_name;
    Ok(Value::Object(session))
}

fn project_insert_params(
    project: &Value,
) -> DomainResult<rusqlite::ParamsFromIter<Vec<rusqlite::types::Value>>> {
    let object = project
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Project must be an object."))?;
    let values = vec![
        sql_text(required_string(object, "projectId")?),
        sql_text(required_string(object, "name")?),
        sql_optional_text(optional_string(object, "path")),
        sql_text(stringify_domain_json_field(
            "identityIcon",
            object.get("identityIcon").unwrap_or(&json!({})),
        )?),
        sql_i64(bool_field(object, "isPinned") as i64),
        sql_i64(bool_field(object, "isFavorite") as i64),
        sql_i64(bool_field(object, "isRecentProject") as i64),
        sql_optional_text(optional_string(object, "recentClosedAt")),
        sql_optional_text(optional_string(object, "defaultCommand")),
        sql_text(stringify_domain_json_field(
            "worktree",
            object.get("worktree").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "customAgents",
            object.get("customAgents").unwrap_or(&json!([])),
        )?),
        sql_text(
            serde_json::to_string(object.get("customAgentOrder").unwrap_or(&json!([]))).unwrap(),
        ),
        sql_text(stringify_domain_json_field(
            "customCommands",
            object.get("customCommands").unwrap_or(&json!([])),
        )?),
        sql_text(
            serde_json::to_string(object.get("customCommandOrder").unwrap_or(&json!([]))).unwrap(),
        ),
        sql_text(
            serde_json::to_string(object.get("deletedDefaultCommandIds").unwrap_or(&json!([])))
                .unwrap(),
        ),
        sql_text(stringify_domain_json_field(
            "launchSettings",
            object.get("launchSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "runtimeSettings",
            object.get("runtimeSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "completionRules",
            object.get("completionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "attentionRules",
            object.get("attentionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "notificationRules",
            object.get("notificationRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "gitConfig",
            object.get("gitConfig").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "projectBoardConfig",
            object.get("projectBoardConfig").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "previousSessionHistory",
            object.get("previousSessionHistory").unwrap_or(&json!([])),
        )?),
        sql_text(required_string(object, "createdAt")?),
        sql_text(required_string(object, "updatedAt")?),
        sql_text(required_string(object, "visibility")?),
        sql_optional_text(optional_string(object, "systemKind")),
    ];
    Ok(rusqlite::params_from_iter(values))
}

fn session_insert_params(
    session: &Value,
) -> DomainResult<rusqlite::ParamsFromIter<Vec<rusqlite::types::Value>>> {
    let object = session
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Session must be an object."))?;
    let hidden = object
        .get("hiddenMetadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let values = vec![
        sql_text(required_string(object, "projectId")?),
        sql_text(required_string(object, "sessionId")?),
        sql_text(required_string(object, "kind")?),
        sql_text(required_string(object, "title")?),
        sql_text(required_string(object, "lifecycleState")?),
        sql_text(stringify_domain_json_field(
            "providerState",
            object.get("providerState").unwrap_or(&json!({})),
        )?),
        sql_text(required_string(object, "zmxName")?),
        sql_optional_text(optional_string(object, "cwd")),
        sql_optional_text(optional_string(object, "agentId")),
        sql_optional_text(optional_string(object, "commandId")),
        sql_i64(bool_field(object, "isPinned") as i64),
        sql_i64(bool_field(object, "isFavorite") as i64),
        sql_optional_text(optional_string(object, "sessionTag")),
        sql_optional_text(optional_string(&hidden, "restoredFromSessionId")),
        sql_optional_text(optional_string(&hidden, "restoredFromHistoryId")),
        sql_text(stringify_domain_json_field(
            "launchSettings",
            object.get("launchSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "runtimeSettings",
            object.get("runtimeSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "completionRules",
            object.get("completionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "attentionRules",
            object.get("attentionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "notificationRules",
            object.get("notificationRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "worktree",
            object.get("worktree").unwrap_or(&json!({})),
        )?),
        sql_text(required_string(object, "createdAt")?),
        sql_text(required_string(object, "updatedAt")?),
        sql_optional_text(optional_string(object, "lastActiveAt")),
        match object
            .get("sidebarOrder")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
        {
            Some(value) => rusqlite::types::Value::Real(value),
            None => rusqlite::types::Value::Null,
        },
        sql_optional_text(optional_string(object, "settledAt")),
        sql_optional_text(normalize_settled_override(
            optional_string(object, "settledOverride").as_deref(),
        )),
        sql_optional_text(optional_string(object, "settledOverrideAt")),
        sql_optional_text(optional_string(object, "snoozedAt")),
        sql_optional_text(optional_string(object, "snoozedUntil")),
    ];
    Ok(rusqlite::params_from_iter(values))
}

fn sql_text(value: String) -> rusqlite::types::Value {
    rusqlite::types::Value::Text(value)
}

fn sql_i64(value: i64) -> rusqlite::types::Value {
    rusqlite::types::Value::Integer(value)
}

fn sql_optional_text(value: Option<String>) -> rusqlite::types::Value {
    value.map(sql_text).unwrap_or(rusqlite::types::Value::Null)
}

fn normalize_required_text(value: Option<&Value>, field: &str) -> DomainResult<String> {
    read_optional_text(value).ok_or_else(|| {
        DomainStateError::bad_request(format!("{field} must be a non-empty string."))
    })
}

fn read_optional_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn normalize_object_array(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_object().cloned().map(Value::Object))
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_session_order_ids(value: Option<&Value>) -> DomainResult<Vec<String>> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Err(DomainStateError::bad_request(
            "sessionIds must contain at least one session ID.",
        ));
    };
    if items.is_empty() {
        return Err(DomainStateError::bad_request(
            "sessionIds must contain at least one session ID.",
        ));
    }
    let mut seen = HashSet::new();
    let mut session_ids = Vec::new();
    for item in items {
        let Some(session_id) = item.as_str() else {
            return Err(DomainStateError::bad_request(format!(
                "Invalid sessionId: {item}."
            )));
        };
        if !is_gxserver_session_id(session_id) {
            return Err(DomainStateError::bad_request(format!(
                "Invalid sessionId: {session_id}."
            )));
        }
        if !seen.insert(session_id.to_string()) {
            return Err(DomainStateError::bad_request(format!(
                "Duplicate sessionId: {session_id}."
            )));
        }
        session_ids.push(session_id.to_string());
    }
    Ok(session_ids)
}

/*
CDXC:GxserverIds 2026-06-22-05:29:
Restored session references are user-provided gxserver session IDs. Match TypeScript by accepting only undefined, null, the exact empty string, or a valid G-id; whitespace and non-string values must be rejected instead of silently dropping the restore link.
*/
fn normalize_session_restore_id(value: Option<&Value>) -> DomainResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) if is_gxserver_session_id(value) => Ok(Some(value.clone())),
        _ => Err(DomainStateError::bad_request(format!(
            "Invalid restoredFromSessionId: {}.",
            js_string(value)
        ))),
    }
}

fn normalize_optional_sidebar_order(value: Option<&Value>) -> Option<i64> {
    let number = value.and_then(Value::as_f64)?;
    if number.is_finite() && number >= 0.0 {
        Some(number.floor() as i64)
    } else {
        None
    }
}

fn normalize_optional_session_tag(value: Option<&Value>) -> DomainResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let Some(tag) = value.as_str() else {
        return Err(DomainStateError::bad_request(
            "sessionTag must be a supported session tag.",
        ));
    };
    match tag {
        "favorite" | "high-priority" | "research" | "todo" | "in-progress" | "testing"
        | "blocked" | "low-priority" | "on-hold" | "done" | "bug" | "feature" | "design" => {
            Ok(Some(tag.to_string()))
        }
        _ => Err(DomainStateError::bad_request(
            "sessionTag must be a supported session tag.",
        )),
    }
}

fn normalize_session_kind(value: Option<&Value>) -> String {
    /*
    CDXC:T3Code 2026-06-23-06:19:
    Native T3 panes are no longer sidebar-only records. Preserve kind=t3 in
    gxserver so presentation, restore, and lifecycle writes address the same
    daemon session that stores the resolved T3 thread binding.
    */
    match value.and_then(Value::as_str) {
        Some("agent") => "agent".to_string(),
        Some("t3") => "t3".to_string(),
        _ => "terminal".to_string(),
    }
}

fn normalize_domain_lifecycle_state(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("running" | "sleeping" | "stopped" | "missing" | "unknown") => {
            value.unwrap().as_str().unwrap().to_string()
        }
        _ => "unknown".to_string(),
    }
}

fn normalize_provider_lifecycle_state(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("exists" | "missing" | "unknown") => value.unwrap().as_str().unwrap().to_string(),
        _ => "unknown".to_string(),
    }
}

fn normalize_launch_settings_with_surface(
    launch_settings: &mut Map<String, Value>,
    explicit_surface: Option<&Value>,
) {
    if let Some(surface) = normalize_session_surface(explicit_surface)
        .or_else(|| normalize_session_surface(launch_settings.get("surface")))
    {
        launch_settings.insert("surface".to_string(), Value::String(surface));
    }
}

fn resolve_surface(
    explicit: Option<&Value>,
    launch_settings: &Map<String, Value>,
    runtime_settings: &Map<String, Value>,
) -> String {
    for value in [
        explicit.and_then(Value::as_str),
        launch_settings.get("surface").and_then(Value::as_str),
        runtime_settings.get("surface").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    {
        if value == "commands" || value == "workspace" {
            return value.to_string();
        }
    }
    "workspace".to_string()
}

fn normalize_session_surface(value: Option<&Value>) -> Option<String> {
    match value.and_then(Value::as_str) {
        Some(surface @ ("commands" | "workspace")) => Some(surface.to_string()),
        _ => None,
    }
}

fn is_temporary_session_title(title: &str) -> bool {
    /*
    CDXC:GxserverDomainState 2026-06-22-05:22:
    TypeScript domain normalization only auto-persists placeholder title provenance for Search by Text launches. Broader generic session labels are presentation and restore-filtering concerns, so the Rust repository must not store titleSource=placeholder for them at the durable row boundary.
    */
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .eq_ignore_ascii_case("search by text")
}

fn has_string_field(map: &Map<String, Value>, key: &str) -> bool {
    matches!(map.get(key), Some(Value::String(_)))
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_trimmed_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    let trimmed = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    insert_optional_string(map, key, trimmed);
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
`settledOverride` is the explicit user pin: "settled" forces the settled shelf,
"active" pins a session into the inbox and suppresses auto-settle. Any other
stored value is corrupt or retired state and hydrates as "no override", the same
way an old state.db row without the column does.
*/
pub fn normalize_settled_override(value: Option<&str>) -> Option<String> {
    match value.map(str::trim) {
        Some("settled") => Some("settled".to_string()),
        Some("active") => Some("active".to_string()),
        _ => None,
    }
}

fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        if !value.is_null() {
            map.insert(key.to_string(), value);
        }
    }
}

fn set_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    } else {
        map.remove(key);
    }
}

fn insert_optional_object(map: &mut Map<String, Value>, key: &str, value: Map<String, Value>) {
    if !value.is_empty() {
        map.insert(key.to_string(), Value::Object(value));
    }
}

fn insert_parsed_optional_object(
    map: &mut Map<String, Value>,
    key: &str,
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<()> {
    let parsed = parse_object_map(value, column, row_kind, row_id)?;
    if !parsed.is_empty() {
        map.insert(key.to_string(), Value::Object(parsed));
    }
    Ok(())
}

fn update_object_field(next: &mut Map<String, Value>, input: &Map<String, Value>, key: &str) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Object(normalize_object(input.get(key))),
        );
    }
}

fn update_optional_object_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        let value = normalize_object(input.get(key));
        if value.is_empty() {
            next.remove(key);
        } else {
            next.insert(key.to_string(), Value::Object(value));
        }
    }
}

fn update_object_array_field(next: &mut Map<String, Value>, input: &Map<String, Value>, key: &str) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Array(normalize_object_array(input.get(key))),
        );
    }
}

fn update_string_array_field(next: &mut Map<String, Value>, input: &Map<String, Value>, key: &str) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Array(
                normalize_string_array(input.get(key))
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
}

fn update_optional_text_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        set_optional_string(next, key, read_optional_text(input.get(key)));
    }
}

fn parse_object(value: &str, column: &str, row_kind: &str, row_id: &str) -> DomainResult<Value> {
    Ok(Value::Object(parse_object_map(
        value, column, row_kind, row_id,
    )?))
}

fn parse_object_map(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Map<String, Value>> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    parsed
        .as_object()
        .cloned()
        .ok_or_else(|| corrupt_json_column(column, row_kind, row_id, "expected a JSON object"))
}

fn parse_object_array(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    let Some(items) = parsed.as_array() else {
        return Err(corrupt_json_column(
            column,
            row_kind,
            row_id,
            "expected a JSON array of objects",
        ));
    };
    let mut output = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            return Err(corrupt_json_column(
                column,
                row_kind,
                row_id,
                &format!("expected object at array index {index}"),
            ));
        };
        output.push(Value::Object(object.clone()));
    }
    Ok(Value::Array(output))
}

fn parse_string_array(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    let Some(items) = parsed.as_array() else {
        return Err(corrupt_json_column(
            column,
            row_kind,
            row_id,
            "expected a JSON array of strings",
        ));
    };
    let mut output = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(corrupt_json_column(
                column,
                row_kind,
                row_id,
                &format!("expected non-empty string at array index {index}"),
            ));
        };
        output.push(Value::String(text.to_string()));
    }
    Ok(Value::Array(output))
}

fn parse_json_column(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    serde_json::from_str(value).map_err(|error| {
        corrupt_json_column(column, row_kind, row_id, &format!("invalid JSON ({error})"))
    })
}

fn corrupt_json_column(
    column: &str,
    row_kind: &str,
    row_id: &str,
    detail: &str,
) -> DomainStateError {
    DomainStateError::corrupt_state(format!(
        "Corrupt gxserver domain-state JSON in {row_kind} {row_id} column {column}: {detail}. Refusing to read or update the row so persisted state is not overwritten."
    ))
}

fn stringify_domain_json_field(field: &str, value: &Value) -> DomainResult<String> {
    assert_domain_json_depth(field, value, 0)?;
    let text = serde_json::to_string(value).map_err(|_| {
        DomainStateError::bad_request(format!("{field} must be JSON-serializable."))
    })?;
    if domain_json_text_length(&text) > JSON_LIMIT_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the gxserver domain-state JSON size limit of {JSON_LIMIT_CHARS} characters."
        )));
    }
    Ok(text)
}

fn domain_json_text_length(text: &str) -> usize {
    /*
    CDXC:GxserverDomainState 2026-06-22-05:22:
    TypeScript enforces the domain JSON limit with JavaScript string length, which counts UTF-16 code units rather than UTF-8 bytes. Match that boundary so non-ASCII project/session metadata is not rejected earlier in Rust.
    */
    text.encode_utf16().count()
}

fn assert_domain_json_depth(field: &str, value: &Value, depth: usize) -> DomainResult<()> {
    if depth > JSON_MAX_DEPTH {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the gxserver domain-state JSON depth limit of {JSON_MAX_DEPTH}."
        )));
    }
    match value {
        Value::Array(items) => {
            for item in items {
                assert_domain_json_depth(field, item, depth + 1)?;
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                assert_domain_json_depth(field, item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/*
CDXC:GxserverProjectPaths 2026-06-22-06:07:
Add Project and session cwd/projectPath resolution must match TypeScript's `normalizeExistingDirectoryPath`: accept absolute paths plus `~` shortcuts, reject non-string/blank/relative inputs with path-specific messages, and store the `path.resolve`-style normalized string so duplicate adds with `..`, `.`, or trailing separators return the existing project.
JSON `null` follows the TypeScript nullish fallback contract (`path ?? projectPath`, `projectPath ?? cwd`); blank strings and non-strings stay selected and fail validation instead of falling through.
*/
fn normalize_existing_directory_path(value: Option<&Value>, field: &str) -> DomainResult<String> {
    normalize_project_root_path(value, field, false)
}

/*
CDXC:AddProjectDialog 2026-07-30:
The t3code-style Add Project dialog submits a typed path that may not exist yet
("Create & Add"), so `/api/addProjectPath` accepts `createIfMissing` and creates
the workspace root before registering it. The path syntax, absolute/`~` rules,
and the not-found/not-a-directory messages stay exactly what they were, so the
flag-absent behavior is byte-identical to the previous contract; only the
mkdir-failure message is new.
*/
fn normalize_project_root_path(
    value: Option<&Value>,
    field: &str,
    create_if_missing: bool,
) -> DomainResult<String> {
    let Some(path) = value.and_then(Value::as_str).map(str::trim) else {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    };
    if path.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(path);
    if !Path::new(&expanded).is_absolute() {
        return Err(DomainStateError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    let normalized = path_to_string(&resolve_path_syntax(PathBuf::from(expanded)));
    if create_if_missing && !Path::new(&normalized).exists() {
        fs::create_dir_all(&normalized).map_err(|_| {
            DomainStateError::bad_request(format!("Failed to create workspace root: {normalized}"))
        })?;
    }
    let metadata = fs::metadata(&normalized).map_err(|_| {
        DomainStateError::not_found(format!("{field} does not exist: {normalized}"))
    })?;
    if !metadata.is_dir() {
        return Err(DomainStateError::bad_request(format!(
            "{field} is not a directory: {normalized}"
        )));
    }
    Ok(normalized)
}

fn expand_user_path(path: &str) -> String {
    if path == "~" {
        return env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

fn resolve_path_syntax(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        path
    } else {
        normalized
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn path_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn read_string_field(value: &Value, key: &str) -> DomainResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state(format!("{key} missing from domain state.")))
}

fn required_string(object: &Map<String, Value>, key: &str) -> DomainResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} must be a string.")))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_string)
}

fn bool_field(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool) == Some(true)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite domain-state error: {error}"),
    }
}

pub fn initialize_for_tests(db: &Connection) -> Result<()> {
    db.execute_batch("PRAGMA foreign_keys = ON;")
        .context("enable foreign keys")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };
    use std::path::Path;

    fn open_test_database() -> (tempfile::TempDir, Connection) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn run_git_for_test(cwd: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn path_str(path: &Path) -> &str {
        path.to_str().expect("test path is valid UTF-8")
    }

    fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{key} string missing"))
    }

    fn object_field<'a>(value: &'a Value, key: &str) -> &'a Map<String, Value> {
        value
            .get(key)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{key} object missing"))
    }

    fn number_field(value: &Value, key: &str) -> f64 {
        value
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("{key} number missing"))
    }

    #[test]
    fn optional_project_id_rejects_whitespace_filter_ids() {
        let mut params = Map::new();
        assert_eq!(read_optional_project_id(&params).expect("missing id"), None);

        params.insert("projectId".to_string(), json!(""));
        assert_eq!(read_optional_project_id(&params).expect("empty id"), None);

        params.insert("projectId".to_string(), json!("   "));
        let error = read_optional_project_id(&params).expect_err("whitespace id rejected");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "Invalid gxserver project ID:    .");

        params.insert("projectId".to_string(), json!("P3a91"));
        assert_eq!(
            read_optional_project_id(&params).expect("valid id"),
            Some("P3a91".to_string())
        );
    }

    #[test]
    fn restored_session_ids_reject_invalid_provided_values() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Restore IDs" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();

        for restored in [json!("   "), json!(42), json!({ "sessionId": "G8v20" })] {
            let error = repository
                .create_session(
                    json!({
                        "projectId": project_id,
                        "restoredFromSessionId": restored,
                        "title": "Invalid restore",
                    })
                    .as_object()
                    .expect("session params"),
                    false,
                )
                .expect_err("invalid restore id rejected");
            assert_eq!(error.code, "badRequest");
            assert!(error.message.starts_with("Invalid restoredFromSessionId: "));
        }

        let source = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "title": "Source",
                })
                .as_object()
                .expect("source params"),
                false,
            )
            .expect("source session created");
        let source_session_id = value_str(&source, "sessionId").to_string();
        let restored = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "restoredFromSessionId": source_session_id,
                    "title": "Restored",
                })
                .as_object()
                .expect("restored params"),
                false,
            )
            .expect("restored session created");
        assert_eq!(
            object_field(&restored, "hiddenMetadata")
                .get("restoredFromSessionId")
                .and_then(Value::as_str),
            Some(source_session_id.as_str())
        );

        let restored_session_id = value_str(&restored, "sessionId").to_string();
        let cleared = repository
            .update_session(
                json!({
                    "projectId": project_id,
                    "restoredFromSessionId": "",
                    "sessionId": restored_session_id,
                })
                .as_object()
                .expect("clear params"),
            )
            .expect("restore id cleared");
        assert!(object_field(&cleared, "hiddenMetadata")
            .get("restoredFromSessionId")
            .is_none());

        let error = repository
            .update_session(
                json!({
                    "projectId": project_id,
                    "restoredFromSessionId": "   ",
                    "sessionId": restored_session_id,
                })
                .as_object()
                .expect("invalid update params"),
            )
            .expect_err("invalid update restore id rejected");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "Invalid restoredFromSessionId:    .");
    }

    #[test]
    fn t3_session_kind_is_preserved() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "T3 Project" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "t3",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "provider": "t3code",
                        "t3": { "threadId": "thread-1" }
                    },
                    "title": "T3 Code",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("t3 session created");

        assert_eq!(value_str(&session, "kind"), "t3");
        assert_eq!(
            object_field(&session, "runtimeSettings")
                .get("provider")
                .and_then(Value::as_str),
            Some("t3code")
        );
    }

    #[test]
    fn session_provider_state_carries_canonical_zmx_provider() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Provider Metadata" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id.as_str(),
                    "providerState": { "lifecycleState": "exists", "provider": "tmux" },
                    "title": "Remote provider metadata",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");
        let session_id = value_str(&session, "sessionId").to_string();
        let zmx_name = format!("S7k-{project_id}-{session_id}");
        let provider_state = object_field(&session, "providerState");
        assert_eq!(provider_state.get("provider"), Some(&json!("zmx")));
        assert_eq!(provider_state.get("zmxName"), Some(&json!(zmx_name)));
        assert_eq!(provider_state.get("lifecycleState"), Some(&json!("exists")));

        let updated = repository
            .update_session(
                json!({
                    "projectId": project_id.as_str(),
                    "providerState": { "lifecycleState": "missing" },
                    "sessionId": session_id.as_str(),
                })
                .as_object()
                .expect("update params"),
            )
            .expect("session updated");
        let updated_provider_state = object_field(&updated, "providerState");
        assert_eq!(updated_provider_state.get("provider"), Some(&json!("zmx")));
        assert_eq!(
            updated_provider_state.get("zmxName"),
            Some(&json!(zmx_name))
        );
        assert_eq!(
            updated_provider_state.get("lifecycleState"),
            Some(&json!("missing"))
        );

        let reloaded = repository
            .get_session(&project_id, &session_id)
            .expect("session reloaded")
            .expect("session exists");
        let reloaded_provider_state = object_field(&reloaded, "providerState");
        assert_eq!(reloaded_provider_state.get("provider"), Some(&json!("zmx")));
        assert_eq!(
            reloaded_provider_state.get("zmxName"),
            Some(&json!(zmx_name))
        );
        assert_eq!(
            reloaded_provider_state.get("lifecycleState"),
            Some(&json!("missing"))
        );
    }

    #[test]
    fn session_tags_normalize_persist_and_clear_like_typescript() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Session Tags" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let supported_tags = [
            "favorite",
            "high-priority",
            "research",
            "todo",
            "in-progress",
            "testing",
            "blocked",
            "low-priority",
            "on-hold",
            "done",
            "bug",
            "feature",
            "design",
        ];

        for tag in supported_tags {
            let session = repository
                .create_session(
                    json!({
                        "projectId": project_id.as_str(),
                        "sessionTag": tag,
                        "title": format!("Tagged {tag}"),
                    })
                    .as_object()
                    .expect("tagged session params"),
                    false,
                )
                .expect("tagged session created");
            assert_eq!(session.get("sessionTag"), Some(&json!(tag)));
            assert_eq!(
                session.get("isFavorite").and_then(Value::as_bool),
                Some(tag == "favorite")
            );

            let reloaded = repository
                .get_session(&project_id, value_str(&session, "sessionId"))
                .expect("tagged session reloaded")
                .expect("tagged session exists");
            assert_eq!(reloaded.get("sessionTag"), Some(&json!(tag)));
        }

        let legacy_favorite = repository
            .create_session(
                json!({
                    "isFavorite": true,
                    "projectId": project_id.as_str(),
                    "title": "Legacy favorite",
                })
                .as_object()
                .expect("legacy favorite params"),
                false,
            )
            .expect("legacy favorite created");
        assert_eq!(
            legacy_favorite.get("isFavorite").and_then(Value::as_bool),
            Some(true)
        );
        assert!(legacy_favorite.get("sessionTag").is_none());

        let legacy_favorite_id = value_str(&legacy_favorite, "sessionId").to_string();
        let reloaded_legacy_favorite = repository
            .get_session(&project_id, &legacy_favorite_id)
            .expect("legacy favorite reloaded")
            .expect("legacy favorite exists");
        assert_eq!(
            reloaded_legacy_favorite.get("sessionTag"),
            Some(&json!("favorite"))
        );
        assert_eq!(
            reloaded_legacy_favorite
                .get("isFavorite")
                .and_then(Value::as_bool),
            Some(true)
        );

        let mutable = repository
            .create_session(
                json!({
                    "projectId": project_id.as_str(),
                    "title": "Mutable tag",
                })
                .as_object()
                .expect("mutable session params"),
                false,
            )
            .expect("mutable session created");
        let mutable_id = value_str(&mutable, "sessionId").to_string();

        let research = repository
            .update_session(
                json!({
                    "projectId": project_id.as_str(),
                    "sessionId": mutable_id.as_str(),
                    "sessionTag": "research",
                })
                .as_object()
                .expect("research tag params"),
            )
            .expect("research tag update");
        assert_eq!(research.get("sessionTag"), Some(&json!("research")));
        assert_eq!(
            research.get("isFavorite").and_then(Value::as_bool),
            Some(false)
        );

        let favorite = repository
            .update_session(
                json!({
                    "isFavorite": true,
                    "projectId": project_id.as_str(),
                    "sessionId": mutable_id.as_str(),
                })
                .as_object()
                .expect("favorite params"),
            )
            .expect("favorite update");
        assert_eq!(favorite.get("sessionTag"), Some(&json!("favorite")));
        assert_eq!(
            favorite.get("isFavorite").and_then(Value::as_bool),
            Some(true)
        );

        for cleared_value in [Value::Null, json!("")] {
            let cleared = repository
                .update_session(
                    json!({
                        "projectId": project_id.as_str(),
                        "sessionId": mutable_id.as_str(),
                        "sessionTag": cleared_value,
                    })
                    .as_object()
                    .expect("clear tag params"),
                )
                .expect("tag cleared");
            assert!(cleared.get("sessionTag").is_none());
            assert_eq!(
                cleared.get("isFavorite").and_then(Value::as_bool),
                Some(false)
            );
        }

        for invalid_tag in [json!("retired-type"), json!("   "), json!(42)] {
            let error = repository
                .create_session(
                    json!({
                        "projectId": project_id.as_str(),
                        "sessionTag": invalid_tag,
                        "title": "Invalid tag",
                    })
                    .as_object()
                    .expect("invalid tag params"),
                    false,
                )
                .expect_err("invalid tag rejected");
            assert_eq!(error.code, "badRequest");
            assert_eq!(error.message, "sessionTag must be a supported session tag.");
        }
    }

    #[test]
    fn persisted_invalid_session_tag_is_rejected_on_hydration() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Invalid Stored Tags" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id.as_str(),
                    "title": "Stored invalid tag",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");
        let session_id = value_str(&session, "sessionId").to_string();

        db.execute_batch("PRAGMA ignore_check_constraints = ON;")
            .expect("disable tag check");
        db.execute(
            "UPDATE sessions SET sessionTag = ?3 WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![project_id.as_str(), session_id.as_str(), "retired-type"],
        )
        .expect("write invalid stored tag");
        db.execute_batch("PRAGMA ignore_check_constraints = OFF;")
            .expect("restore tag check");

        let error = repository
            .get_session(&project_id, &session_id)
            .expect_err("invalid stored tag rejected");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "sessionTag must be a supported session tag.");
    }

    #[test]
    fn update_session_order_defaults_returns_touched_rows_and_list_remains_updated_at_ordered() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Sidebar Order" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let first = repository
            .create_session(
                json!({ "projectId": project_id.as_str(), "title": "First" })
                    .as_object()
                    .expect("first params"),
                false,
            )
            .expect("first session");
        let second = repository
            .create_session(
                json!({ "projectId": project_id.as_str(), "title": "Second" })
                    .as_object()
                    .expect("second params"),
                false,
            )
            .expect("second session");
        let third = repository
            .create_session(
                json!({ "projectId": project_id.as_str(), "title": "Third" })
                    .as_object()
                    .expect("third params"),
                false,
            )
            .expect("third session");
        let first_id = value_str(&first, "sessionId").to_string();
        let second_id = value_str(&second, "sessionId").to_string();
        let third_id = value_str(&third, "sessionId").to_string();

        assert_eq!(number_field(&first, "sidebarOrder"), 0.0);
        assert_eq!(number_field(&second, "sidebarOrder"), 0.0);

        let ordered = repository
            .update_session_order(
                json!({
                    "projectId": project_id.as_str(),
                    "sessionIds": [second_id.as_str(), first_id.as_str()],
                })
                .as_object()
                .expect("order params"),
            )
            .expect("order updated");
        assert_eq!(ordered.len(), 2);
        assert_eq!(
            ordered
                .iter()
                .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec![second_id.as_str(), first_id.as_str()]
        );
        assert_eq!(
            ordered
                .iter()
                .map(|session| number_field(session, "sidebarOrder"))
                .collect::<Vec<_>>(),
            vec![1000.0, 2000.0]
        );
        let untouched = repository
            .get_session(&project_id, &third_id)
            .expect("get untouched")
            .expect("untouched exists");
        assert_eq!(number_field(&untouched, "sidebarOrder"), 0.0);

        for (session_id, updated_at) in [
            (second_id.as_str(), "2026-06-02T12:00:00.000Z"),
            (first_id.as_str(), "2026-06-01T12:00:00.000Z"),
            (third_id.as_str(), "2026-05-31T12:00:00.000Z"),
        ] {
            db.execute(
                "UPDATE sessions SET updatedAt = ?3 WHERE projectId = ?1 AND sessionId = ?2",
                rusqlite::params![project_id, session_id, updated_at],
            )
            .expect("updatedAt patched");
        }
        let listed = repository
            .list_sessions(Some(&project_id))
            .expect("sessions listed");
        assert_eq!(
            listed
                .iter()
                .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec![second_id.as_str(), first_id.as_str(), third_id.as_str()]
        );
    }

    #[test]
    fn update_session_order_rejects_invalid_duplicate_and_unknown_ids_without_partial_writes() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Sidebar Rollback" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let first = repository
            .create_session(
                json!({ "projectId": project_id.as_str(), "title": "First" })
                    .as_object()
                    .expect("first params"),
                false,
            )
            .expect("first session");
        let second = repository
            .create_session(
                json!({ "projectId": project_id.as_str(), "title": "Second" })
                    .as_object()
                    .expect("second params"),
                false,
            )
            .expect("second session");
        let first_id = value_str(&first, "sessionId").to_string();
        let second_id = value_str(&second, "sessionId").to_string();
        let original_updated_at = value_str(&first, "updatedAt").to_string();

        let empty_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": [] })
                    .as_object()
                    .expect("empty params"),
            )
            .expect_err("empty order rejected");
        assert_eq!(empty_error.code, "badRequest");
        assert_eq!(
            empty_error.message,
            "sessionIds must contain at least one session ID."
        );

        let invalid_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": ["session-local"] })
                    .as_object()
                    .expect("invalid params"),
            )
            .expect_err("invalid order rejected");
        assert_eq!(invalid_error.code, "badRequest");
        assert_eq!(invalid_error.message, "Invalid sessionId: session-local.");

        let duplicate_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": [first_id.as_str(), first_id.as_str()] })
                    .as_object()
                    .expect("duplicate params"),
            )
            .expect_err("duplicate order rejected");
        assert_eq!(duplicate_error.code, "badRequest");
        assert_eq!(
            duplicate_error.message,
            format!("Duplicate sessionId: {first_id}.")
        );

        let mut missing_id = "G9zzz".to_string();
        if missing_id == first_id || missing_id == second_id {
            missing_id = "G9zzy".to_string();
        }
        assert!(is_gxserver_session_id(&missing_id));
        let missing_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": [first_id.as_str(), missing_id.as_str()] })
                    .as_object()
                    .expect("missing params"),
            )
            .expect_err("missing order rejected");
        assert_eq!(missing_error.code, "notFound");
        assert_eq!(
            missing_error.message,
            format!("Session {project_id}/{missing_id} does not exist.")
        );
        let first_after = repository
            .get_session(&project_id, &first_id)
            .expect("get first after rollback")
            .expect("first exists after rollback");
        assert_eq!(number_field(&first_after, "sidebarOrder"), 0.0);
        assert_eq!(
            first_after.get("updatedAt").and_then(Value::as_str),
            Some(original_updated_at.as_str())
        );
    }

    #[test]
    fn update_and_remove_crud_paths_report_not_found_for_unvalidated_ids() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "CRUD IDs" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();

        let missing_project = repository
            .update_project(
                json!({ "projectId": "project-local" })
                    .as_object()
                    .expect("update project params"),
            )
            .expect_err("invalid project lookup is not found");
        assert_eq!(missing_project.code, "notFound");
        assert_eq!(
            missing_project.message,
            "Project project-local does not exist."
        );

        let missing_session = repository
            .update_session(
                json!({ "projectId": project_id, "sessionId": "session-local" })
                    .as_object()
                    .expect("update session params"),
            )
            .expect_err("invalid session lookup is not found");
        assert_eq!(missing_session.code, "notFound");
        assert!(missing_session
            .message
            .contains("/session-local does not exist."));

        let missing_remove = repository
            .remove_session(json!({}).as_object().expect("remove session params"))
            .expect_err("missing session lookup is not found");
        assert_eq!(missing_remove.code, "notFound");
        assert_eq!(
            missing_remove.message,
            "Session undefined/undefined does not exist."
        );
    }

    #[test]
    fn add_project_path_repairs_visibility_metadata_for_existing_path() {
        /*
        CDXC:ProjectVisibility 2026-06-30-21:23:
        Remote Attach carrier registration may reuse an existing gxserver path row. Repair hidden/system project metadata on `/api/addProjectPath` so every inventory client hides that carrier through daemon-owned state instead of macOS-only project filters.
        */
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let carrier_path = temp.path().join("remote-attach-carriers");
        std::fs::create_dir_all(&carrier_path).expect("carrier dir");

        let initial = repository
            .add_project_path(
                json!({
                    "name": "Remote Attach",
                    "path": path_str(&carrier_path),
                })
                .as_object()
                .expect("initial project params"),
            )
            .expect("initial project");
        let project_id = value_str(&initial, "projectId").to_string();
        assert_eq!(
            initial.get("visibility").and_then(Value::as_str),
            Some("visible")
        );
        assert!(initial.get("systemKind").is_none());

        let repaired = repository
            .add_project_path(
                json!({
                    "name": "Remote Attach",
                    "path": path_str(&carrier_path),
                    "systemKind": "remoteAttachCarrier",
                    "visibility": "hidden",
                })
                .as_object()
                .expect("repair project params"),
            )
            .expect("repaired project");
        assert_eq!(value_str(&repaired, "projectId"), project_id);
        assert_eq!(
            repaired.get("visibility").and_then(Value::as_str),
            Some("hidden")
        );
        assert_eq!(
            repaired.get("systemKind").and_then(Value::as_str),
            Some("remoteAttachCarrier")
        );

        let invalid = repository
            .add_project_path(
                json!({
                    "name": "Remote Attach",
                    "path": path_str(&carrier_path),
                    "visibility": "archived",
                })
                .as_object()
                .expect("invalid project params"),
            )
            .expect_err("invalid visibility rejected");
        assert_eq!(invalid.code, "badRequest");
    }

    #[test]
    fn records_project_and_session_id_allocations() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Allocated" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId").to_string();
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "title": "Allocated session",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");
        let session_id = value_str(&session, "sessionId").to_string();

        let project_allocation: Option<String> = db
            .query_row(
                "SELECT id FROM id_allocations WHERE kind = 'project' AND parentId = '' AND id = ?1",
                [&project_id],
                |row| row.get(0),
            )
            .optional()
            .expect("project allocation query");
        assert_eq!(project_allocation, Some(project_id.clone()));

        let session_allocation: Option<String> = db
            .query_row(
                "SELECT id FROM id_allocations WHERE kind = 'session' AND parentId = ?1 AND id = ?2",
                rusqlite::params![project_id, session_id],
                |row| row.get(0),
            )
            .optional()
            .expect("session allocation query");
        assert_eq!(session_allocation, Some(session_id));
    }

    #[test]
    fn rejects_domain_json_deeper_than_contract_limit() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let mut nested = json!("leaf");
        for _ in 0..12 {
            nested = json!({ "child": nested });
        }
        let params = json!({
            "name": "Deep JSON",
            "runtimeSettings": nested,
        });
        let error = repository
            .create_project(params.as_object().expect("params object"))
            .expect_err("deep JSON rejected");
        assert_eq!(error.code, "badRequest");
        assert!(error.message.contains("depth limit"));
    }

    #[test]
    fn domain_json_size_limit_counts_utf16_code_units_like_typescript() {
        let value = json!({ "emoji": "👻".repeat(260_000) });
        assert!(stringify_domain_json_field("runtimeSettings", &value).is_ok());
    }

    #[test]
    fn maps_corrupt_project_json_to_corrupt_state() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let params = json!({ "name": "Corrupt JSON" });
        let project = repository
            .create_project(params.as_object().expect("params object"))
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id");
        db.execute(
            "UPDATE projects SET runtimeSettingsJson = ?1 WHERE projectId = ?2",
            rusqlite::params!["{not-json", project_id],
        )
        .expect("corrupt project row");
        let error = repository
            .list_projects()
            .expect_err("corrupt row rejected");
        assert_eq!(error.code, "corruptState");
        assert!(error.message.contains("runtimeSettingsJson"));
    }

    #[test]
    fn title_runtime_settings_only_default_search_by_text_to_placeholder() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Title Defaults" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId");

        let terminal = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "title": "Terminal Session",
                })
                .as_object()
                .expect("terminal session params"),
                false,
            )
            .expect("terminal session created");
        let terminal_runtime = object_field(&terminal, "runtimeSettings");
        assert_eq!(terminal_runtime.get("titleSource"), None);

        let search = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "runtimeSettings": { "titleSource": null },
                    "title": "Search   by\nText",
                })
                .as_object()
                .expect("search session params"),
                false,
            )
            .expect("search session created");
        let search_runtime = object_field(&search, "runtimeSettings");
        assert_eq!(
            search_runtime.get("titleSource"),
            Some(&json!("placeholder"))
        );
    }

    #[test]
    fn invalid_surface_values_do_not_create_persisted_launch_surface_defaults() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Surface Defaults" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = value_str(&project, "projectId");

        let explicit_invalid = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "surface": "invalid",
                    "title": "Invalid Explicit Surface",
                })
                .as_object()
                .expect("explicit invalid params"),
                false,
            )
            .expect("explicit invalid session created");
        assert_eq!(explicit_invalid.get("surface"), Some(&json!("workspace")));
        assert_eq!(
            object_field(&explicit_invalid, "launchSettings").get("surface"),
            None
        );

        let launch_invalid = repository
            .create_session(
                json!({
                    "launchSettings": { "surface": "invalid" },
                    "projectId": project_id,
                    "title": "Invalid Launch Surface",
                })
                .as_object()
                .expect("launch invalid params"),
                false,
            )
            .expect("launch invalid session created");
        assert_eq!(launch_invalid.get("surface"), Some(&json!("workspace")));
        assert_eq!(
            object_field(&launch_invalid, "launchSettings").get("surface"),
            Some(&json!("invalid"))
        );

        let updated = repository
            .update_session(
                json!({
                    "launchSettings": { "surface": "invalid" },
                    "projectId": project_id,
                    "sessionId": value_str(&explicit_invalid, "sessionId"),
                    "surface": "invalid",
                })
                .as_object()
                .expect("update invalid params"),
            )
            .expect("invalid surface update");
        assert_eq!(updated.get("surface"), Some(&json!("workspace")));
        assert_eq!(
            object_field(&updated, "launchSettings").get("surface"),
            Some(&json!("invalid"))
        );
    }

    #[test]
    fn add_project_path_normalizes_nullish_fallback_and_deduplicates_path_syntax() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let parent = temp.path().join("paths");
        let repo_path = parent.join("repo");
        let fallback_path = parent.join("fallback");
        std::fs::create_dir_all(&repo_path).expect("repo dir");
        std::fs::create_dir_all(&fallback_path).expect("fallback dir");

        let first_input = repo_path.join("..").join("repo").join(".");
        let first = repository
            .add_project_path(
                json!({ "path": path_str(&first_input) })
                    .as_object()
                    .expect("first params"),
            )
            .expect("first add");
        assert_eq!(value_str(&first, "path"), path_str(&repo_path));
        assert_eq!(value_str(&first, "name"), "repo");

        let trailing_input = format!("{}/", path_str(&repo_path));
        let second = repository
            .add_project_path(
                json!({ "path": trailing_input })
                    .as_object()
                    .expect("second params"),
            )
            .expect("second add");
        assert_eq!(
            value_str(&second, "projectId"),
            value_str(&first, "projectId")
        );
        assert_eq!(repository.list_projects().expect("projects").len(), 1);

        let fallback = repository
            .add_project_path(
                json!({ "path": null, "projectPath": path_str(&fallback_path) })
                    .as_object()
                    .expect("fallback params"),
            )
            .expect("null path falls back to projectPath");
        assert_eq!(value_str(&fallback, "path"), path_str(&fallback_path));

        let empty_error = repository
            .add_project_path(
                json!({ "path": "", "projectPath": path_str(&repo_path) })
                    .as_object()
                    .expect("empty params"),
            )
            .expect_err("blank path does not fall back");
        assert_eq!(empty_error.code, "badRequest");
        assert_eq!(empty_error.message, "path must be a non-empty path.");
    }

    #[test]
    fn add_project_path_creates_workspace_root_when_create_if_missing_is_requested() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let missing = temp.path().join("created-parent").join("created-project");

        let without_flag = repository
            .add_project_path(
                json!({ "path": path_str(&missing) })
                    .as_object()
                    .expect("params"),
            )
            .expect_err("missing path is rejected without the flag");
        assert_eq!(without_flag.code, "notFound");
        assert!(!missing.exists());

        let project = repository
            .add_project_path(
                json!({ "createIfMissing": true, "path": path_str(&missing) })
                    .as_object()
                    .expect("params"),
            )
            .expect("missing path created and registered");
        assert_eq!(value_str(&project, "path"), path_str(&missing));
        assert_eq!(value_str(&project, "name"), "created-project");
        assert!(missing.is_dir());

        let repeated = repository
            .add_project_path(
                json!({ "createIfMissing": true, "path": path_str(&missing) })
                    .as_object()
                    .expect("params"),
            )
            .expect("second add is idempotent");
        assert_eq!(
            value_str(&repeated, "projectId"),
            value_str(&project, "projectId")
        );

        let file_path = temp.path().join("create-if-missing-file");
        std::fs::write(&file_path, "file\n").expect("file");
        let file_error = repository
            .add_project_path(
                json!({ "createIfMissing": true, "path": path_str(&file_path) })
                    .as_object()
                    .expect("params"),
            )
            .expect_err("existing file is still rejected");
        assert_eq!(file_error.code, "badRequest");
        assert_eq!(
            file_error.message,
            format!("path is not a directory: {}", path_str(&file_path))
        );

        let unwritable = file_path.join("child");
        let create_error = repository
            .add_project_path(
                json!({ "createIfMissing": true, "path": path_str(&unwritable) })
                    .as_object()
                    .expect("params"),
            )
            .expect_err("mkdir failure surfaces the workspace-root message");
        assert_eq!(create_error.code, "badRequest");
        assert_eq!(
            create_error.message,
            format!("Failed to create workspace root: {}", path_str(&unwritable))
        );
    }

    #[test]
    fn add_project_path_rejects_invalid_path_inputs_with_typescript_messages() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let missing_input = temp
            .path()
            .join("missing-parent")
            .join("..")
            .join("missing");
        let missing_normalized = temp.path().join("missing");
        let file_path = temp.path().join("not-a-directory");
        std::fs::write(&file_path, "file\n").expect("file");

        let cases = vec![
            (
                json!({ "path": null }),
                "badRequest",
                "path must be a non-empty path.".to_string(),
            ),
            (
                json!({ "path": 42 }),
                "badRequest",
                "path must be a non-empty path.".to_string(),
            ),
            (
                json!({ "path": "   " }),
                "badRequest",
                "path must be a non-empty path.".to_string(),
            ),
            (
                json!({ "path": "relative/repo" }),
                "badRequest",
                "path must be an absolute path or start with ~/".to_string(),
            ),
            (
                json!({ "path": path_str(&missing_input) }),
                "notFound",
                format!("path does not exist: {}", path_str(&missing_normalized)),
            ),
            (
                json!({ "path": path_str(&file_path) }),
                "badRequest",
                format!("path is not a directory: {}", path_str(&file_path)),
            ),
        ];

        for (params, code, message) in cases {
            let error = repository
                .add_project_path(params.as_object().expect("params"))
                .expect_err("invalid add path rejected");
            assert_eq!(error.code, code);
            assert_eq!(error.message, message);
        }
    }

    #[test]
    fn normalize_existing_directory_path_expands_home_shortcut() {
        let Some(home) = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
        else {
            return;
        };
        let normalized = normalize_existing_directory_path(Some(&json!("~")), "path")
            .expect("home shortcut normalized");
        assert_eq!(normalized, path_to_string(&resolve_path_syntax(home)));
    }

    #[test]
    fn create_session_project_resolution_uses_nullish_path_fallback() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let repo_path = temp.path().join("session-project");
        std::fs::create_dir_all(&repo_path).expect("repo dir");
        let project = repository
            .add_project_path(
                json!({ "path": path_str(&repo_path) })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project added");
        let project_id = value_str(&project, "projectId").to_string();

        let cwd_input = repo_path.join("..").join("session-project").join(".");
        let session = repository
            .create_session(
                json!({
                    "cwd": path_str(&cwd_input),
                    "projectId": "project-local",
                    "projectPath": null,
                    "title": "Terminal",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");
        assert_eq!(value_str(&session, "projectId"), project_id);
        assert_eq!(value_str(&session, "cwd"), path_str(&cwd_input));

        let empty_error = repository
            .create_session(
                json!({
                    "cwd": path_str(&repo_path),
                    "projectId": "project-local",
                    "projectPath": "",
                    "title": "Terminal",
                })
                .as_object()
                .expect("empty session params"),
                false,
            )
            .expect_err("blank projectPath does not fall back to cwd");
        assert_eq!(empty_error.code, "badRequest");
        assert_eq!(
            empty_error.message,
            "Invalid gxserver project ID: project-local."
        );
    }

    #[test]
    fn add_project_path_attaches_and_repairs_linked_worktree_metadata() {
        if !git_available() {
            return;
        }

        let (temp, db) = open_test_database();
        let root = temp.path();
        let repo_path = root.join("registered-main");
        let worktree_path = root.join("registered-main-feature");
        let orphan_worktree_path = root.join("registered-main-orphan");
        std::fs::create_dir_all(&repo_path).expect("repo dir");
        run_git_for_test(&repo_path, &["init"]);
        run_git_for_test(
            &repo_path,
            &["config", "user.email", "ghostex@example.invalid"],
        );
        run_git_for_test(&repo_path, &["config", "user.name", "Ghostex Test"]);
        std::fs::write(repo_path.join("README.md"), "main\n").expect("write readme");
        run_git_for_test(&repo_path, &["add", "README.md"]);
        run_git_for_test(&repo_path, &["commit", "-m", "Initial commit"]);
        run_git_for_test(
            &repo_path,
            &[
                "worktree",
                "add",
                "-b",
                "feature/existing-worktree",
                path_str(&worktree_path),
            ],
        );
        run_git_for_test(
            &repo_path,
            &[
                "worktree",
                "add",
                "-b",
                "feature/orphan-worktree",
                path_str(&orphan_worktree_path),
            ],
        );

        let repository = DomainRepository::new(&db, "S7k");
        let main_params = json!({ "name": "Registered Main", "path": path_str(&repo_path) });
        let main_project = repository
            .add_project_path(main_params.as_object().expect("main params"))
            .expect("main project");
        let main_project_id = value_str(&main_project, "projectId").to_string();

        let worktree_params = json!({ "path": path_str(&worktree_path) });
        let worktree_project = repository
            .add_project_path(worktree_params.as_object().expect("worktree params"))
            .expect("worktree project");
        assert_eq!(
            value_str(&worktree_project, "path"),
            path_str(&worktree_path)
        );
        let metadata = object_field(&worktree_project, "worktree");
        assert_eq!(
            metadata.get("parentProjectId").and_then(Value::as_str),
            Some(main_project_id.as_str())
        );
        assert_eq!(
            metadata.get("parentProjectName").and_then(Value::as_str),
            Some("Registered Main")
        );
        assert_eq!(
            metadata.get("parentProjectPath").and_then(Value::as_str),
            Some(path_str(&repo_path))
        );
        assert_eq!(
            metadata.get("branch").and_then(Value::as_str),
            Some("feature/existing-worktree")
        );
        assert!(metadata.get("createdAt").and_then(Value::as_str).is_some());

        let second_add = repository
            .add_project_path(worktree_params.as_object().expect("worktree params"))
            .expect("second worktree add");
        assert_eq!(
            value_str(&second_add, "projectId"),
            value_str(&worktree_project, "projectId")
        );

        let orphan_params =
            json!({ "name": "Orphan Worktree", "path": path_str(&orphan_worktree_path) });
        let orphan_project = repository
            .create_project(orphan_params.as_object().expect("orphan params"))
            .expect("orphan project");
        assert!(orphan_project.get("worktree").is_none());
        let repaired = repository
            .add_project_path(orphan_params.as_object().expect("orphan params"))
            .expect("repaired worktree project");
        assert_eq!(
            value_str(&repaired, "projectId"),
            value_str(&orphan_project, "projectId")
        );
        let repaired_metadata = object_field(&repaired, "worktree");
        assert_eq!(
            repaired_metadata
                .get("parentProjectId")
                .and_then(Value::as_str),
            Some(main_project_id.as_str())
        );
        assert_eq!(
            repaired_metadata.get("branch").and_then(Value::as_str),
            Some("feature/orphan-worktree")
        );
    }

    fn stash_params(content: &str, project_id: Option<&str>) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("content".to_string(), json!(content));
        if let Some(project_id) = project_id {
            params.insert("projectId".to_string(), json!(project_id));
        }
        params.insert("sessionId".to_string(), json!("G1abc"));
        params.insert("cwd".to_string(), json!("/tmp/example"));
        params
    }

    fn listed_stash_contents(repository: &DomainRepository, project_id: Option<&str>) -> Vec<String> {
        let mut params = Map::new();
        if let Some(project_id) = project_id {
            params.insert("projectId".to_string(), json!(project_id));
        }
        repository
            .list_stashed_prompts(&params)
            .expect("list stashed prompts")
            .get("prompts")
            .and_then(Value::as_array)
            .expect("prompts array")
            .iter()
            .map(|prompt| value_str(prompt, "content").to_string())
            .collect()
    }

    #[test]
    fn stashed_prompts_save_dedupes_same_project_content() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");

        let first = repository
            .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
            .expect("first save");
        let second = repository
            .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
            .expect("duplicate save");
        assert_eq!(
            value_str(first.get("prompt").expect("prompt"), "promptId"),
            value_str(second.get("prompt").expect("prompt"), "promptId"),
        );
        assert_eq!(listed_stash_contents(&repository, None).len(), 1);

        // Same content in another project stays a separate stash.
        repository
            .save_stashed_prompt(&stash_params("fix the login bug", Some("P2bbb")))
            .expect("other-project save");
        assert_eq!(listed_stash_contents(&repository, None).len(), 2);

        let error = repository
            .save_stashed_prompt(&stash_params("   \n  ", Some("P1aaa")))
            .expect_err("blank content rejected");
        assert_eq!(error.code, "badRequest");
    }

    #[test]
    fn stashed_prompts_list_scopes_to_project_and_delete_removes() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");

        repository
            .save_stashed_prompt(&stash_params("prompt in project A", Some("P1aaa")))
            .expect("save A");
        repository
            .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
            .expect("save B");
        repository
            .save_stashed_prompt(&stash_params("projectless prompt", None))
            .expect("save projectless");

        assert_eq!(
            listed_stash_contents(&repository, Some("P1aaa")),
            vec!["prompt in project A".to_string()]
        );
        let all = listed_stash_contents(&repository, None);
        assert_eq!(all.len(), 3);
        // Newest first.
        assert_eq!(all[0], "projectless prompt");

        let saved = repository
            .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
            .expect("re-save B");
        let prompt_id = value_str(saved.get("prompt").expect("prompt"), "promptId").to_string();
        let mut delete_params = Map::new();
        delete_params.insert("promptId".to_string(), json!(prompt_id));
        let deleted = repository
            .delete_stashed_prompt(&delete_params)
            .expect("delete");
        assert_eq!(deleted.get("deleted"), Some(&json!(true)));
        assert_eq!(listed_stash_contents(&repository, None).len(), 2);
        let deleted_again = repository
            .delete_stashed_prompt(&delete_params)
            .expect("delete again");
        assert_eq!(deleted_again.get("deleted"), Some(&json!(false)));
    }

    #[test]
    fn stashed_prompts_project_scope_includes_legacy_worktree_family() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");

        // Register a parent project and a legacy worktree-as-project child by
        // writing the rows directly; detect_registered_git_worktree_metadata
        // needs a real git checkout, which this scoping test does not.
        let parent = repository
            .create_project(
                json!({ "name": "Main", "path": "/tmp/stash-main" })
                    .as_object()
                    .expect("parent params"),
            )
            .expect("parent project");
        let parent_id = value_str(&parent, "projectId").to_string();
        let child = repository
            .create_project(
                json!({ "name": "Main Worktree", "path": "/tmp/stash-worktree" })
                    .as_object()
                    .expect("child params"),
            )
            .expect("child project");
        let child_id = value_str(&child, "projectId").to_string();
        db.execute(
            "UPDATE projects SET worktreeJson = ?1 WHERE projectId = ?2",
            params![
                json!({ "parentProjectId": parent_id }).to_string(),
                child_id
            ],
        )
        .expect("mark worktree project");

        repository
            .save_stashed_prompt(&stash_params("parent prompt", Some(&parent_id)))
            .expect("save parent");
        repository
            .save_stashed_prompt(&stash_params("worktree prompt", Some(&child_id)))
            .expect("save worktree");
        repository
            .save_stashed_prompt(&stash_params("unrelated prompt", Some("P9zzz")))
            .expect("save unrelated");

        let mut from_parent = listed_stash_contents(&repository, Some(&parent_id));
        let mut from_child = listed_stash_contents(&repository, Some(&child_id));
        from_parent.sort();
        from_child.sort();
        let expected = vec!["parent prompt".to_string(), "worktree prompt".to_string()];
        assert_eq!(from_parent, expected);
        assert_eq!(from_child, expected);
    }
}
