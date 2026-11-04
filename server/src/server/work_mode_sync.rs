//! Running Issues Workflow (crate::work_mode): the background pass that finds which projects have
//! it on, refreshes Linear and `gh` status for their sessions and publishes the rows whose links,
//! titles or parts changed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};

use super::{schedule_presentation_project_delta, schedule_presentation_session_delta, AppState};
use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::storage::open_gxserver_database;
use crate::work_mode::{
    issues_workflow_settings, presentation_session_work, project_work_mode, published_project_work,
    refresh_issues_workflow_setup, refresh_work_caches, work_display_title, PublishedProjectWork,
};

/// What each project last published of Issues Workflow (which parts it has, its tracker and its
/// ticket command), so a pass republishes a project whose Linear key was set or removed, whose
/// tracker changed, whose ticket command now has another name, or every project once `gh`
/// is signed in or out.
fn published_linear_projects() -> &'static Mutex<HashMap<String, PublishedProjectWork>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<String, PublishedProjectWork>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The Title sessions by their branch setting the last pass ran with.
fn published_titles() -> &'static Mutex<Option<bool>> {
    static PUBLISHED: Mutex<Option<bool>> = Mutex::new(None);
    &PUBLISHED
}

/// What each work-mode session last published, so a pass only sends the rows that changed.
fn published_work() -> &'static Mutex<HashMap<(String, String), String>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn work_fingerprint(project: &Value, session: &Value) -> String {
    let title_source = crate::presentation::project_session_title(session)
        .get("titleSource")
        .and_then(Value::as_str)
        .map(str::to_string);
    json!({
        "work": presentation_session_work(project, session),
        "title": work_display_title(project, session, title_source.as_deref()),
    })
    .to_string()
}

/// One pass: refresh the caches every open work-mode session reads, then publish the rows whose
/// links, statuses or branch title changed. Blocking; runs on the 60s background worker.
///
/// CDXC:WorkMode 2026-10-23 DECISION:
/// User chose "Only open sessions": the work-mode pass refreshes Linear and `gh` status only for sessions that are not closed (`stopped`), and a closed session's status updates when it is reopened. The pass used to read and fingerprint every session of a work-mode project each minute, closed ones included (8,148 rows for one project on 2026-10-23), and kept a fingerprint for each in memory; closed sessions' fingerprints are dropped now, so a reopened session is published on the next pass.
pub(crate) fn run_work_mode_refresh_once(state: &Arc<AppState>) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let all_projects = repository.list_projects()?;
    refresh_issues_workflow_setup(&state.paths, &all_projects);
    // Before the cards are read: which workspace, and so which ticket command, each project uses.
    crate::work_mode::replace_project_workspaces(&state.paths, &repository, &db)?;

    // What changed is kept only once its delta has gone out (below), so a failed one is sent
    // again on the next pass.
    let mut linear_changed: Vec<(String, PublishedProjectWork)> = Vec::new();
    if let Ok(mut published) = published_linear_projects().lock() {
        let mut seen = std::collections::HashSet::new();
        for project in &all_projects {
            let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
                continue;
            };
            seen.insert(project_id);
            let parts = published_project_work(project_id, project);
            match published.get(project_id) {
                Some(previous) if *previous == parts => {}
                // A project not published by a pass yet went out in the snapshot with nothing set
                // up (the caches start empty), so it only needs a delta once something is.
                None if !parts.parts.any() => {
                    published.insert(project_id.to_string(), parts);
                }
                _ => linear_changed.push((project_id.to_string(), parts)),
            }
        }
        published.retain(|project_id, _| seen.contains(project_id.as_str()));
    }

    // Branch titles need only git, so with the setting on every project's open sessions are read;
    // a project whose parts just changed is read once more so its cards drop what it lost.
    let titles = issues_workflow_settings().title_sessions_by_branch;
    // The pass after the setting was turned off reads them all once more to take the titles back.
    let titles_turned_off = published_titles()
        .lock()
        .ok()
        .and_then(|mut published| published.replace(titles))
        .is_some_and(|before| before && !titles);
    let projects: Vec<Value> = all_projects
        .iter()
        .filter(|project| {
            titles
                || titles_turned_off
                || project_work_mode(project)
                || project
                    .get("projectId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| linear_changed.iter().any(|(changed, _)| changed == id))
        })
        .cloned()
        .collect();
    let mut sessions = Vec::new();
    for project in &projects {
        if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
            sessions.extend(repository.list_sessions_excluding_stopped(Some(project_id))?);
        }
    }
    // Only projects with Issues Workflow on are refreshed (it filters them itself).
    refresh_work_caches(&state.paths, &projects, &sessions);

    // One delta that fails doesn't hold back the others; the first failure is returned once they
    // all went out, for the pass's log line.
    let mut first_error = None;
    for (project_id, parts) in linear_changed {
        match send_project_delta(state, &db, &repository, &project_id) {
            Ok(()) => {
                if let Ok(mut published) = published_linear_projects().lock() {
                    published.insert(project_id, parts);
                }
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }

    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter_map(|project| Some((project.get("projectId")?.as_str()?, project)))
        .collect();
    let mut changed = Vec::new();
    if let Ok(mut published) = published_work().lock() {
        let open: std::collections::HashSet<(&str, &str)> = sessions
            .iter()
            .filter_map(|session| {
                Some((
                    session.get("projectId")?.as_str()?,
                    session.get("sessionId")?.as_str()?,
                ))
            })
            .collect();
        published.retain(|(project_id, session_id), _| {
            open.contains(&(project_id.as_str(), session_id.as_str()))
        });
        for session in &sessions {
            let (Some(project_id), Some(session_id)) = (
                session.get("projectId").and_then(Value::as_str),
                session.get("sessionId").and_then(Value::as_str),
            ) else {
                continue;
            };
            let Some(project) = projects_by_id.get(project_id) else {
                continue;
            };
            let fingerprint = work_fingerprint(project, session);
            let key = (project_id.to_string(), session_id.to_string());
            if published.get(&key) != Some(&fingerprint) {
                changed.push((key, fingerprint));
            }
        }
    }
    for ((project_id, session_id), fingerprint) in changed {
        match send_session_delta(state, &db, &repository, &project_id, &session_id) {
            Ok(()) => {
                if let Ok(mut published) = published_work().lock() {
                    published.insert((project_id, session_id), fingerprint);
                }
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// A project's delta, with its Issues Workflow parts, its tracker and its ticket command as the
/// row shows them.
fn send_project_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<(), DomainStateError> {
    #[cfg(test)]
    delta_hook(project_id)?;
    schedule_presentation_project_delta(state, db, repository, project_id, "projectUpdated")
}

/// A session's delta, with its card's links, statuses and branch title.
fn send_session_delta(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    #[cfg(test)]
    delta_hook(session_id)?;
    schedule_presentation_session_delta(state, db, repository, project_id, session_id)
}

/// Records a delta a pass tries, by project or session id, and fails it once if a test asked.
#[cfg(test)]
fn delta_hook(id: &str) -> Result<(), DomainStateError> {
    let mut hooks = delta_hooks();
    hooks.attempted.push(id.to_string());
    if hooks.failing.remove(id) {
        return Err(DomainStateError {
            code: "internalError",
            message: "A test's delta failed.".to_string(),
        });
    }
    Ok(())
}

/// The project and session deltas a test made fail once, and every one a pass tried, in order.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct DeltaHooks {
    pub(crate) failing: std::collections::HashSet<String>,
    pub(crate) attempted: Vec<String>,
}

#[cfg(test)]
pub(crate) fn delta_hooks() -> std::sync::MutexGuard<'static, DeltaHooks> {
    static HOOKS: OnceLock<Mutex<DeltaHooks>> = OnceLock::new();
    HOOKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Publishes a project whose Issues Workflow parts may have changed (it moved to another workspace,
/// with another Linear key and tracker), and every one of its sessions, whose cards follow it.
pub(crate) fn publish_project_work_mode_change(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<(), DomainStateError> {
    schedule_presentation_project_delta(state, db, repository, project_id, "projectUpdated")?;
    if let Ok(mut published) = published_work().lock() {
        published.retain(|(published_project, _), _| published_project != project_id);
    }
    for session in repository.list_sessions(Some(project_id))? {
        if let Some(session_id) = session.get("sessionId").and_then(Value::as_str) {
            schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
        }
    }
    Ok(())
}

/// Runs a pass right away (off the request path), so a link or switch someone just set shows its
/// status without waiting for the next minute.
pub(crate) fn spawn_work_mode_refresh(state: &Arc<AppState>) {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = run_work_mode_refresh_once(&state) {
            log_work_mode_refresh_failure(&state, &error.message);
        }
    });
}

pub(crate) fn log_work_mode_refresh_failure(state: &AppState, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "workModeRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
