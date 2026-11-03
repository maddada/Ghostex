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
    cached_project_work_tracker, issues_workflow_parts, issues_workflow_settings,
    presentation_session_work, project_work_mode, refresh_issues_workflow_setup,
    refresh_work_caches, work_display_title, IssuesWorkflowParts, WorkSourceId,
};

/// What each project last published of Issues Workflow (which parts it has, and its tracker), so a
/// pass republishes a project whose Linear key was set or removed, whose tracker changed, or every
/// project once `gh` is signed in or out.
fn published_linear_projects(
) -> &'static Mutex<HashMap<String, (IssuesWorkflowParts, WorkSourceId)>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<String, (IssuesWorkflowParts, WorkSourceId)>>> =
        OnceLock::new();
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

    let mut linear_changed = Vec::new();
    if let Ok(mut published) = published_linear_projects().lock() {
        let mut seen = std::collections::HashSet::new();
        for project in &all_projects {
            let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
                continue;
            };
            seen.insert(project_id);
            let parts = (
                issues_workflow_parts(project),
                cached_project_work_tracker(project_id),
            );
            let previous = published.insert(project_id.to_string(), parts);
            // A project not published by a pass yet went out in the snapshot with nothing set up
            // (the caches start empty), so it only needs a delta once something is.
            if previous.map_or(parts.0.any(), |previous| previous != parts) {
                linear_changed.push(project_id.to_string());
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
                    .is_some_and(|id| linear_changed.iter().any(|changed| changed == id))
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

    for project_id in linear_changed {
        schedule_presentation_project_delta(
            state,
            &db,
            &repository,
            &project_id,
            "projectUpdated",
        )?;
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
                published.insert(key.clone(), fingerprint);
                changed.push(key);
            }
        }
    }
    for (project_id, session_id) in changed {
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
    }
    Ok(())
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
