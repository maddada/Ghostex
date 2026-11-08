use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde_json::{Map, Value};

use crate::domain::{
    are_project_worktree_metadata_equal, detect_registered_git_worktree_metadata,
    find_project_by_path_in, merge_project_update, normalize_existing_directory_path,
    normalize_project_input, normalize_project_root_path, normalize_project_system_kind,
    normalize_project_visibility, now_iso, path_basename, project_from_row, project_insert_params,
    project_insert_values, project_row_from_sql, project_row_without_session_history_from_sql,
    read_optional_text, read_string_field, read_unvalidated_project_lookup_id,
    recent_project_from_project, sql_error, DomainRepository, DomainResult, DomainStateError,
};
use crate::ids::create_project_id;

/// Every `projects` column `project_row_without_session_history_from_sql` reads.
const PROJECT_COLUMNS_WITHOUT_SESSION_HISTORY: &str = "projectId, name, path, identityIconJson, isPinned, isFavorite, isRecentProject, recentClosedAt, visibility, systemKind, defaultCommand, worktreeJson, customAgentsJson, customAgentOrderJson, customCommandsJson, customCommandOrderJson, deletedDefaultCommandIdsJson, launchSettingsJson, runtimeSettingsJson, completionRulesJson, attentionRulesJson, notificationRulesJson, gitConfigJson, projectBoardConfigJson, createdAt, updatedAt";

pub(crate) const MAX_ID_GENERATION_ATTEMPTS: usize = 1024;

/// What places an existing project in a workspace (`workspaces::project_workspace_id`): its own
/// stored workspace and, for a worktree, its parent.
fn placement_inputs(project: &Value) -> (Option<String>, Option<String>) {
    (
        crate::workspaces::stored_project_workspace_id(project).map(str::to_string),
        project
            .get("worktree")
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}

/// Held while an existing project's placement changes (its stored workspace or its worktree
/// parent, or its removal), around the write, after which the projection forgets where the project
/// and its worktrees were (`forget_project_placement`); by a ticket command's background read from its
/// placement check until its command starts, which reads placement from the database under it
/// and writes nothing while it holds it (`work_mode::prepare`); by a worktree rename around its
/// transaction; by a replace of the projection's map, from its reads through its publish
/// (`work_mode::replace_project_workspaces`); and by a save of a session's links, from its read
/// through its write (`work_mode::TicketLinkLocks`). So a read never starts a command for a project that
/// has moved since its check.
///
/// The lock order is the ticket command store's writer lock, then this one, then SQLite's write
/// lock. Taking it twice on one thread, or the writer lock while holding it, fails the assertion
/// in a debug build and answers `LOCK_ORDER_REFUSED` in a release build, rather than waiting
/// forever; a placement change inside a transaction that started without it is refused
/// (`DomainRepository::hold_placement`).
///
/// CDXC:WorkMode 2026-11-03 DECISION:
/// The repository owner, on the two lock-order guards: "I'd keep it as `debug_assert!` plus an `internalError` return in release, so a future wrong order fails one request in fixed words instead of unwinding through the handler."
///
/// CDXC:WorkMode 2026-11-02 WHY:
/// Taken here, in the only functions that write those rows, not by each caller: a project's placement changes through many routes and helpers (moves, updates, removals, re-registering a path), and a lock each caller had to remember was missed four times. A placement change runs outside a transaction, or inside one that took this lock first, so a read under the lock sees every one that happened.
pub(crate) fn hold_project_placement() -> DomainResult<PlacementGuard> {
    static PLACEMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    if placement_held_here() {
        debug_assert!(false, "the placement lock is taken twice on one thread");
        return Err(lock_order_refused());
    }
    let lock = PLACEMENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    PLACEMENT_HELD.with(|held| held.set(true));
    Ok(PlacementGuard { _lock: lock })
}

/// What a lock taken out of order answers with in a release build.
pub(crate) const LOCK_ORDER_REFUSED: &str =
    "Ghostex refused a change that took its locks in the wrong order, and changed nothing.";

pub(crate) fn lock_order_refused() -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: LOCK_ORDER_REFUSED.to_string(),
    }
}

thread_local! {
    static PLACEMENT_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread holds the placement lock (`hold_project_placement`).
pub(crate) fn placement_held_here() -> bool {
    PLACEMENT_HELD.with(std::cell::Cell::get)
}

/// The placement lock, held until dropped on the thread that took it.
pub(crate) struct PlacementGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for PlacementGuard {
    fn drop(&mut self) {
        PLACEMENT_HELD.with(|held| held.set(false));
    }
}

/// The refusal when a placement change would take the placement lock after SQLite's.
pub(crate) const PLACEMENT_CHANGE_IN_TRANSACTION: &str =
    "A project's workspace can't change inside a transaction that started without the placement lock.";

/// The error when a project changed under every attempt of an update (`update_project`).
pub(crate) const PROJECT_CHANGED_DURING_UPDATE: &str =
    "The project changed while it was being updated. Try again.";

/// How many times `update_project` reads, merges and writes before it gives up.
const PROJECT_UPDATE_ATTEMPTS: usize = 3;

impl DomainRepository<'_> {
    /// The placement lock for a write on this repository's connection. `None` when this thread
    /// already holds it, which a caller took before any transaction it opened (the worktree
    /// rename). Refused, in every build, inside a transaction that started without it: the lock
    /// would come after SQLite's, and a read under it would not see the change until it commits.
    fn hold_placement(&self, project_id: &str) -> DomainResult<Option<PlacementGuard>> {
        if placement_held_here() {
            return Ok(None);
        }
        if !self.db.is_autocommit() {
            return Err(DomainStateError {
                code: "internalError",
                message: PLACEMENT_CHANGE_IN_TRANSACTION.to_string(),
            });
        }
        #[cfg(test)]
        notify_placement_for_test(project_id);
        #[cfg(not(test))]
        let _ = project_id;
        Ok(Some(hold_project_placement()?))
    }

    /// The project and its two placement columns as stored, read in one statement, for
    /// `update_project`'s compare-and-set.
    fn get_project_for_update(
        &self,
        project_id: &str,
    ) -> DomainResult<Option<(Value, StoredPlacement)>> {
        let row = self
            .db
            .prepare_cached("SELECT * FROM projects WHERE projectId = ?1")
            .map_err(sql_error)?
            .query_row([project_id], |row| {
                Ok((
                    project_row_from_sql(row)?,
                    StoredPlacement {
                        launch_settings_json: row.get("launchSettingsJson")?,
                        worktree_json: row.get("worktreeJson")?,
                    },
                ))
            })
            .optional()
            .map_err(sql_error)?;
        row.map(|(row, stored)| Ok((project_from_row(row)?, stored)))
            .transpose()
    }
}

/// A project row's `launchSettingsJson` and `worktreeJson` as read, which hold its placement.
struct StoredPlacement {
    launch_settings_json: Option<String>,
    worktree_json: Option<String>,
}

/// Holds an update between its read and its write once (`pause_project_update`).
#[cfg(test)]
type UpdatePauses =
    std::collections::HashMap<String, (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>;

#[cfg(test)]
fn update_pauses() -> std::sync::MutexGuard<'static, UpdatePauses> {
    static PAUSES: std::sync::OnceLock<std::sync::Mutex<UpdatePauses>> = std::sync::OnceLock::new();
    PAUSES
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
fn paused_update_for_test(project_id: &str) {
    let pause = update_pauses().remove(project_id);
    if let Some((reached, release)) = pause {
        let _ = reached.send(());
        let _ = release.recv();
    }
}

/// Pauses the next `update_project` of `project_id` after it read and merged the row, before it
/// writes: it says so on `reached` and waits for `release`.
#[cfg(test)]
pub(crate) fn pause_project_update(
    project_id: &str,
    reached: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
) {
    update_pauses().insert(project_id.to_string(), (reached, release));
}

/// One notice per project, sent when a change to its placement is about to take
/// `hold_project_placement` (`notify_project_placement`).
#[cfg(test)]
fn placement_notices(
) -> std::sync::MutexGuard<'static, std::collections::HashMap<String, std::sync::mpsc::Sender<()>>>
{
    static NOTICES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::mpsc::Sender<()>>>,
    > = std::sync::OnceLock::new();
    NOTICES
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
fn notify_placement_for_test(project_id: &str) {
    if let Some(notify) = placement_notices().remove(project_id) {
        let _ = notify.send(());
    }
}

/// Asks for one notice when the next change to `project_id`'s placement is about to take the
/// placement lock, so a test can tell waiting for it from not having started.
#[cfg(test)]
pub(crate) fn notify_project_placement(project_id: &str) -> std::sync::mpsc::Receiver<()> {
    let (notify, notice) = std::sync::mpsc::channel();
    placement_notices().insert(project_id.to_string(), notify);
    notice
}

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

    /// Writes the merged row only while its placement columns still hold what was read, so an
    /// update that read the row before a move never writes the old workspace back. When they
    /// changed, it reads and merges again, up to `PROJECT_UPDATE_ATTEMPTS` times, then fails with
    /// `PROJECT_CHANGED_DURING_UPDATE`, or with `notFound` when the row is gone by then.
    ///
    /// CDXC:WorkMode 2026-11-03 WHY:
    /// Compare-and-set rather than the placement lock around every update: most updates (names, pins, board links, settings) don't move the project, and the lock around each read and write would serialize them all and be taken after SQLite's inside a transaction such as the worktree rename's.
    pub fn update_project(&self, params: &Map<String, Value>) -> DomainResult<Value> {
        let project_id = read_unvalidated_project_lookup_id(params);
        let updated_at = now_iso();
        for _ in 0..PROJECT_UPDATE_ATTEMPTS {
            let (current, stored) = self.get_project_for_update(&project_id)?.ok_or_else(|| {
                DomainStateError::not_found(format!("Project {project_id} does not exist."))
            })?;
            let moves = placement_inputs(&current);
            let project = merge_project_update(current, &updated_at, params)?;
            #[cfg(test)]
            paused_update_for_test(&project_id);
            let changes_placement = placement_inputs(&project) != moves;
            let _placement = if changes_placement {
                self.hold_placement(&project_id)?
            } else {
                None
            };
            let mut values = project_insert_values(&project)?;
            values.push(stored.launch_settings_json.into());
            values.push(stored.worktree_json.into());
            let written = self
                .db
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
                WHERE projectId = ?1 AND launchSettingsJson IS ?28 AND worktreeJson IS ?29
                "#,
                    rusqlite::params_from_iter(values),
                )
                .map_err(sql_error)?;
            if written == 0 {
                continue;
            }
            if changes_placement {
                crate::work_mode::forget_project_placement(&project_id);
            }
            return Ok(project);
        }
        // The last write may have missed because the row is gone, not because it changed.
        if self.get_project(&project_id)?.is_none() {
            return Err(DomainStateError::not_found(format!(
                "Project {project_id} does not exist."
            )));
        }
        Err(DomainStateError {
            code: "conflict",
            message: PROJECT_CHANGED_DURING_UPDATE.to_string(),
        })
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
        if let Some(set) = super::project_cache::cached_projects(self.db, true) {
            return Ok(set.projects.clone());
        }
        let mut statement = self
            .db
            .prepare_cached("SELECT * FROM projects ORDER BY updatedAt DESC, projectId ASC")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], project_row_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter().map(project_from_row).collect()
    }

    /// `list_projects` without each project's `previousSessionHistory`, for readers that never
    /// look at it (the key is absent from every returned project).
    ///
    /// CDXC:StateSync 2026-10-23 WHY:
    /// A project's restorable session history is its largest column (130 KB on the machine this was profiled on), and every full read parsed it, including the session deltas and the sidebar HUD reads that only want the project's settings. The history is still part of `get_project`/`list_projects`, which updates, trusted titles and the presentation's domain projects need.
    pub fn list_projects_without_session_history(&self) -> DomainResult<Vec<Value>> {
        if let Some(set) = super::project_cache::cached_projects(self.db, true) {
            return Ok(set
                .projects
                .iter()
                .map(super::project_cache::without_session_history)
                .collect());
        }
        let mut statement = self
            .db
            .prepare_cached(&format!(
                "SELECT {PROJECT_COLUMNS_WITHOUT_SESSION_HISTORY} FROM projects ORDER BY updatedAt DESC, projectId ASC"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], project_row_without_session_history_from_sql)
            .map_err(sql_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows.into_iter().map(project_from_row).collect()
    }

    /// `get_project` without the project's `previousSessionHistory` (see
    /// `list_projects_without_session_history`).
    pub fn get_project_without_session_history(
        &self,
        project_id: &str,
    ) -> DomainResult<Option<Value>> {
        if let Some(set) = super::project_cache::cached_projects(self.db, false) {
            return Ok(set
                .get(project_id)
                .map(super::project_cache::without_session_history));
        }
        let row = self
            .db
            .prepare_cached(&format!(
                "SELECT {PROJECT_COLUMNS_WITHOUT_SESSION_HISTORY} FROM projects WHERE projectId = ?1"
            ))
            .map_err(sql_error)?
            .query_row([project_id], project_row_without_session_history_from_sql)
            .optional()
            .map_err(sql_error)?;
        row.map(project_from_row).transpose()
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
        if let Some(set) = super::project_cache::cached_projects(self.db, false) {
            return Ok(set.get(project_id).cloned());
        }
        let row = self
            .db
            .prepare_cached("SELECT * FROM projects WHERE projectId = ?1")
            .map_err(sql_error)?
            .query_row([project_id], project_row_from_sql)
            .optional()
            .map_err(sql_error)?;
        row.map(project_from_row).transpose()
    }

    pub fn remove_project(&self, project_id: &str) -> DomainResult<Value> {
        let current = self.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        let _placement = self.hold_placement(project_id)?;
        self.db
            .execute("DELETE FROM projects WHERE projectId = ?1", [project_id])
            .map_err(sql_error)?;
        crate::work_mode::forget_project_placement(project_id);
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
            .is_some_and(|path| !path.is_empty());
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
