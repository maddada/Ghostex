use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::{
    are_project_worktree_metadata_equal, detect_registered_git_worktree_metadata,
    find_project_by_path_in, merge_project_update, normalize_existing_directory_path,
    normalize_project_input, normalize_project_root_path, normalize_project_system_kind,
    normalize_project_visibility, now_iso, path_basename, project_from_row, project_insert_params,
    project_row_from_sql, read_optional_text, read_project_id, read_string_field,
    read_unvalidated_project_lookup_id, recent_project_from_project, sql_error, DomainRepository,
    DomainResult, DomainStateError,
};
use crate::ids::create_project_id;

pub(crate) const MAX_ID_GENERATION_ATTEMPTS: usize = 1024;

impl<'a> DomainRepository<'a> {
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

    pub fn relocate_project(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        if self.get_project(&project_id)?.is_none() {
            return Err(DomainStateError::not_found(format!(
                "Project {project_id} does not exist."
            )));
        }
        let path = normalize_existing_directory_path(params.get("path"), "path")?;
        if let Some(existing) = self.find_project_by_path(&path)? {
            let existing_project_id = existing
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or("");
            if existing_project_id != project_id {
                return Err(DomainStateError::bad_request(format!(
                    "Project folder is already registered by project {existing_project_id}: {path}"
                )));
            }
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), Value::String(project_id));
        update.insert("path".to_string(), Value::String(path));
        self.update_project(&update)
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
        CDXC:Projects 2026-06-24-12:27:
        Recent Projects are explicit parked gxserver projects. Return only
        path-bearing rows marked `isRecentProject` and compute sessionCount
        from the domain sessions table; do not infer recency from presentation
        labels, inactive lifecycle states, shell titles, stdout, commands, or
        filesystem scans.

        CDXC:Projects 2026-06-30-21:23:
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
        CDXC:Projects 2026-06-24-12:38:
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
        let create_if_missing =
            params.get("createIfMissing").and_then(Value::as_bool) == Some(true);
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
        CDXC:Worktrees 2026-06-22-00:35:
        Rust gxserver must preserve the TypeScript Add Worktree/Add Project path-registration contract. When /api/addProjectPath receives a linked Git worktree path, detect the already registered main checkout and store worktree metadata under that canonical parent project ID; if the path was registered earlier without metadata, repair that existing row in place so the macOS sidebar groups it exactly like the old server.

        CDXC:Projects 2026-06-30-21:23:
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

    pub(crate) fn find_project_by_path(
        &self,
        normalized_path: &str,
    ) -> DomainResult<Option<Value>> {
        Ok(find_project_by_path_in(
            &self.list_projects()?,
            normalized_path,
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

    pub(crate) fn record_id_allocation(
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
