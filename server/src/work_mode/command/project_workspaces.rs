//! Each project's workspace (`project_workspace_id`: a worktree follows its parent checkout) and
//! which workspaces are in a team, for readers that do no I/O: the projection finds a project's
//! ticket command through it. Replaced whole at startup, at the start of each card pass, and
//! inside the routes that move a project or delete its workspace, before they publish; so a moved
//! project shows its new workspace's command at once, and a deleted one keeps no entry. A project
//! added since the last replace has no command on its cards until the next one.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use rusqlite::Connection;

use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;

#[derive(Default)]
struct Placement {
    workspaces: HashMap<String, String>,
    /// Each worktree project's parent, whose workspace it follows.
    parents: HashMap<String, String>,
    teams: HashSet<String>,
}

/// One placement per gxserver database, by its file. A gxserver has one; tests run many in one
/// process, and a replace from one never drops another's projects.
fn placements() -> &'static RwLock<HashMap<PathBuf, Placement>> {
    static PLACEMENTS: OnceLock<RwLock<HashMap<PathBuf, Placement>>> = OnceLock::new();
    PLACEMENTS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Reads the workspaces document and every project (local reads, no command runs) and replaces
/// what the map held for this database, under the store's writer lock: a background job holds it
/// from its placement check until its command starts, so the map never changes in between.
pub(crate) fn replace_project_workspaces(
    paths: &GxserverPaths,
    repository: &DomainRepository,
    db: &Connection,
) -> Result<(), DomainStateError> {
    let _writer = hold_placement_writer(paths, PlacementChange::Replace)?;
    replace_placement(paths, repository, db)
}

/// A change to where projects are placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PlacementChange {
    /// A project moved, updated or removed (`place_projects_after`).
    Move,
    /// The map read again whole (`replace_project_workspaces`): the card pass, a workspace delete.
    Replace,
}

/// The store's writer lock, for a change to where projects are placed.
fn hold_placement_writer(
    paths: &GxserverPaths,
    change: PlacementChange,
) -> Result<std::sync::MutexGuard<'static, ()>, DomainStateError> {
    #[cfg(test)]
    about_to_take_placement_writer(paths, change);
    #[cfg(not(test))]
    let _ = (paths, change);
    super::hold_work_source_writer()
}

/// `replace_project_workspaces` for a caller that holds the store's writer lock. Holds the
/// placement lock from its reads through its publish, so a change to a project's placement through
/// the repository alone (which takes only that lock) lands before the reads or after the publish:
/// an older snapshot never brings back a placement the change made the projection forget
/// (`forget_project_placement`). The reads are local (the workspaces document, the project rows,
/// the team connections in the credentials file); no command runs under the lock.
fn replace_placement(
    paths: &GxserverPaths,
    repository: &DomainRepository,
    db: &Connection,
) -> Result<(), DomainStateError> {
    let _placement = crate::domain::repository::project::hold_project_placement()?;
    let workspaces = crate::workspaces::read_sidebar_workspaces(db)?;
    let projects = repository.list_projects_without_session_history()?;
    let mut placement = Placement::default();
    let mut checked: HashMap<String, bool> = HashMap::new();
    for project in &projects {
        let Some(project_id) = project.get("projectId").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let workspace_id = crate::workspaces::project_workspace_id(&workspaces, project, &projects);
        let in_team = *checked
            .entry(workspace_id.clone())
            .or_insert_with(|| crate::team_sync::workspace_has_team(paths, &workspace_id));
        if in_team {
            placement.teams.insert(workspace_id.clone());
        }
        placement
            .workspaces
            .insert(project_id.to_string(), workspace_id);
        if let Some(parent_id) = project
            .get("worktree")
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(serde_json::Value::as_str)
        {
            placement
                .parents
                .insert(project_id.to_string(), parent_id.to_string());
        }
    }
    #[cfg(test)]
    paused_replace_for_test(paths);
    placements()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(paths.state_db_file.clone(), placement);
    Ok(())
}

/// A project's placement changed (`DomainRepository::update_project`, `remove_project`): until the
/// next replace, the projection knows no workspace for it or for the worktrees that follow it, so
/// it shows no ticket command for them rather than the one of the workspace they left. Every
/// placement change comes through those two functions, so no caller has to remember this.
pub(crate) fn forget_project_placement(project_id: &str) {
    let mut placements = placements()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for placement in placements.values_mut() {
        let mut forgotten = vec![project_id.to_string()];
        while let Some(forget) = forgotten.pop() {
            placement.workspaces.remove(&forget);
            forgotten.extend(
                placement
                    .parents
                    .iter()
                    .filter(|(_, parent)| **parent == forget)
                    .map(|(child, _)| child.clone()),
            );
            placement.parents.remove(&forget);
        }
    }
}

/// The project's workspace, as last replaced.
pub(crate) fn cached_project_workspace_id(project_id: &str) -> Option<String> {
    placements()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .find_map(|placement| placement.workspaces.get(project_id).cloned())
}

/// Whether the workspace was in a team at the last replace. A ticket command is stopped there
/// (it keeps its settings): it reads nothing, and its tickets stay on the cards as unavailable.
pub(crate) fn cached_workspace_in_team(workspace_id: &str) -> bool {
    placements()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .any(|placement| placement.teams.contains(workspace_id))
}

/// Runs `change` (a project move, or a project update that sets its workspace) and replaces the
/// map, both under the store's writer lock, which
/// a background job holds from its placement check until its command starts and again while it
/// keeps the result: so a move lands before the check or after the start, and a job that saw the
/// old placement then keeps nothing (`plan_holds`).
pub(crate) fn place_projects_after<T>(
    paths: &GxserverPaths,
    repository: &DomainRepository,
    db: &Connection,
    change: impl FnOnce() -> Result<T, DomainStateError>,
) -> Result<T, DomainStateError> {
    let _writer = hold_placement_writer(paths, PlacementChange::Move)?;
    let changed = change()?;
    replace_placement(paths, repository, db)?;
    Ok(changed)
}

/// Tells a test, once, that a placement change of its kind for `paths`' database is about to take
/// the store's writer lock (`notify_placement_writer`), so it can tell waiting for the lock from
/// not having started. Kinds are apart: a card pass a route started in the background replaces
/// the map too.
#[cfg(test)]
fn about_to_take_placement_writer(paths: &GxserverPaths, change: PlacementChange) {
    if let Some(notify) = placement_writer_notices().remove(&(paths.state_db_file.clone(), change))
    {
        let _ = notify.send(());
    }
}

#[cfg(test)]
type PlacementNotices = HashMap<(PathBuf, PlacementChange), std::sync::mpsc::Sender<()>>;

#[cfg(test)]
fn placement_writer_notices() -> std::sync::MutexGuard<'static, PlacementNotices> {
    static NOTICES: OnceLock<std::sync::Mutex<PlacementNotices>> = OnceLock::new();
    NOTICES
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Asks for one notice when the next placement change of `change`'s kind for `paths`' database is
/// about to take the store's writer lock.
#[cfg(test)]
pub(crate) fn notify_placement_writer(
    paths: &GxserverPaths,
    change: PlacementChange,
) -> std::sync::mpsc::Receiver<()> {
    let (notify, notice) = std::sync::mpsc::channel();
    placement_writer_notices().insert((paths.state_db_file.clone(), change), notify);
    notice
}

#[cfg(test)]
type ReplacePauses = HashMap<PathBuf, (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>;

#[cfg(test)]
fn replace_pauses() -> std::sync::MutexGuard<'static, ReplacePauses> {
    static PAUSES: OnceLock<std::sync::Mutex<ReplacePauses>> = OnceLock::new();
    PAUSES
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
fn paused_replace_for_test(paths: &GxserverPaths) {
    let pause = replace_pauses().remove(&paths.state_db_file);
    if let Some((reached, release)) = pause {
        let _ = reached.send(());
        let _ = release.recv();
    }
}

/// Pauses the next replace of `paths`' map after its reads, before it publishes: it says so on
/// `reached` and waits for `release`.
#[cfg(test)]
pub(crate) fn pause_placement_replace(
    paths: &GxserverPaths,
    reached: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
) {
    replace_pauses().insert(paths.state_db_file.clone(), (reached, release));
}

/// The startup step after the record loads: one replace before the first snapshot, so its project
/// rows already know their workspace's command. A failure leaves the map empty until the first
/// card pass.
pub(crate) fn replace_project_workspaces_at_startup(paths: &GxserverPaths, server_id: &str) {
    if let Ok(db) = crate::storage::open_gxserver_database(paths) {
        let repository = DomainRepository::new(&db, server_id);
        let _ = replace_project_workspaces(paths, &repository, &db);
    }
}
