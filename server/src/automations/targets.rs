use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::domain::{DomainRepository, DomainStateError};

use super::*;

pub(super) fn read_automation_target_projects(
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    let mut targets = Vec::new();
    for project in repository.list_projects()? {
        let record = project_record_from_value(&project)?;
        if record.path.trim().is_empty() {
            continue;
        }
        let worktree = worktree_availability(&record);
        targets.push(json!({
            "canUseWorktrees": worktree.0,
            "label": record.name,
            "path": record.path,
            "projectId": record.project_id,
            "worktreeUnavailableReason": worktree.1,
        }));
    }
    Ok(Value::Array(targets))
}

const WORKTREE_AVAILABILITY_PROBE_TTL: Duration = Duration::from_secs(60);

fn worktree_availability_probe_cache() -> &'static Mutex<HashMap<String, (Instant, bool)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_path_inside_git_work_tree(path: &str) -> bool {
    /*
    readAutomationState recomputes worktree availability for every registered
    project on every call, so probing git per read multiplies into hundreds of
    subprocess spawns per second. Worktree membership only changes on git
    init/worktree edits; cache probes per path briefly instead of spawning git
    each time.
    */
    if let Ok(cache) = worktree_availability_probe_cache().lock() {
        if let Some((probed_at, is_work_tree)) = cache.get(path) {
            if probed_at.elapsed() < WORKTREE_AVAILABILITY_PROBE_TTL {
                return *is_work_tree;
            }
        }
    }
    let output = crate::platform::process::background_command("git")
        .current_dir(path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();
    let is_work_tree = matches!(
        output,
        Ok(ref output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == "true"
    );
    if let Ok(mut cache) = worktree_availability_probe_cache().lock() {
        cache.insert(path.to_string(), (Instant::now(), is_work_tree));
    }
    is_work_tree
}

pub(super) fn worktree_availability(project: &ProjectRecord) -> (bool, Option<String>) {
    if project.path.trim().is_empty() {
        return (
            false,
            Some("Worktree mode needs an active code project.".to_string()),
        );
    }
    if is_path_inside_git_work_tree(&project.path) {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "{} is not inside a Git work tree. Use Local mode explicitly for non-Git projects.",
                project.name
            )),
        )
    }
}

pub(super) fn hud_agents_to_automation_agents(hud: &Value) -> Value {
    /*
    CDXC:Automations 2026-07-02-04:10:
    Automation pickers consume this list directly, so it must contain only
    agents that automations can actually launch. Exclude commandless agents to
    match the native selector's rules.
    */
    Value::Array(
        hud.get("agents")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|agent| {
                let object = agent.as_object()?;
                let agent_id = read_value_text(object, "agentId")?;
                let command = read_value_text(object, "command")?;
                let label = read_value_text(object, "name").unwrap_or_else(|| agent_id.clone());
                let mut output = Map::new();
                output.insert("agentId".to_string(), Value::String(agent_id));
                output.insert("label".to_string(), Value::String(label));
                output.insert("command".to_string(), Value::String(command));
                if let Some(icon) = read_value_text(object, "icon") {
                    output.insert("icon".to_string(), Value::String(icon));
                }
                Some(Value::Object(output))
            })
            .collect(),
    )
}

pub(super) fn create_run_record(
    automation: &AutomationDefinitionRecord,
    status: &str,
    error_message: Option<&str>,
) -> AutomationRunRecord {
    let now = now_iso();
    AutomationRunRecord {
        automation_id: automation.id.clone(),
        completed_at: (!is_active_status(status)).then_some(now.clone()),
        created_at: now.clone(),
        error_message: error_message.map(str::to_string),
        findings_summary: None,
        id: format!("automation-run-{}", uuid::Uuid::new_v4()),
        is_archived: false,
        is_unread: status != "no_findings" && status != "skipped",
        project_id: automation.project_id.clone(),
        session_id: None,
        status: status.to_string(),
        updated_at: now,
        worktree: json!({}),
    }
}

pub(super) fn resolve_automation_project(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<ProjectRecord, DomainStateError> {
    if let Some(project_id) = read_param_text(params, "projectId") {
        return resolve_project_by_id(repository, &project_id);
    }
    if let Some(path) =
        read_param_text(params, "projectPath").or_else(|| read_param_text(params, "path"))
    {
        if let Some(project) = find_project_by_path(repository, &path)? {
            return Ok(project);
        }
        let mut add_params = Map::new();
        add_params.insert("path".to_string(), Value::String(path));
        return project_record_from_value(&repository.add_project_path(&add_params)?);
    }
    repository
        .list_projects()?
        .into_iter()
        .find_map(|project| {
            project_record_from_value(&project)
                .ok()
                .filter(|project| !project.path.trim().is_empty())
        })
        .ok_or_else(|| DomainStateError::bad_request("projectId or projectPath is required."))
}

pub(super) fn resolve_project_by_id(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<ProjectRecord, DomainStateError> {
    let value = repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
    project_record_from_value(&value)
}

pub(super) fn find_project_by_path(
    repository: &DomainRepository<'_>,
    path: &str,
) -> Result<Option<ProjectRecord>, DomainStateError> {
    let normalized = normalize_path(path);
    let mut records = Vec::new();
    for project in repository.list_projects()? {
        let record = project_record_from_value(&project)?;
        let record_path = normalize_path(&record.path);
        records.push((record, record_path));
    }
    // The exact path first; else the same folder in other capitals on a case-insensitive volume
    // (CDXC:Projects in packages/paths/src/same_folder.rs).
    Ok(ghostex_paths::same_folder::find_same_folder(
        &normalized,
        records,
    ))
}

pub(super) fn project_record_from_value(value: &Value) -> Result<ProjectRecord, DomainStateError> {
    let object = value
        .as_object()
        .ok_or_else(|| DomainStateError::corrupt_state("Project row is not an object."))?;
    Ok(ProjectRecord {
        name: read_value_text(object, "name").unwrap_or_else(|| "Project".to_string()),
        path: read_value_text(object, "path").unwrap_or_default(),
        project_id: read_value_text(object, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Project row missing projectId."))?,
    })
}

pub(super) fn resolve_thread_session(
    repository: &DomainRepository<'_>,
    default_project_id: &str,
    session_id: &str,
) -> Result<(String, String), DomainStateError> {
    if let Some((project_id, session_id)) = session_id.split_once(':') {
        if repository.get_session(project_id, session_id)?.is_some() {
            return Ok((project_id.to_string(), session_id.to_string()));
        }
    }
    if repository
        .get_session(default_project_id, session_id)?
        .is_some()
    {
        return Ok((default_project_id.to_string(), session_id.to_string()));
    }
    for project in repository.list_projects()? {
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if repository.get_session(project_id, session_id)?.is_some() {
            return Ok((project_id.to_string(), session_id.to_string()));
        }
    }
    Err(DomainStateError::not_found(
        "Thread session is no longer available.",
    ))
}

pub(super) fn resolve_agent_thread_session(
    repository: &DomainRepository<'_>,
    default_project_id: &str,
    session_id_hint: Option<&str>,
    agent_id: &str,
    agent_session_id: &str,
) -> Result<Option<(String, String)>, DomainStateError> {
    if let Some(session_id_hint) = session_id_hint {
        if let Ok((project_id, session_id)) =
            resolve_thread_session(repository, default_project_id, session_id_hint)
        {
            if let Some(session) = repository.get_session(&project_id, &session_id)? {
                if session_owns_agent_conversation(&session, agent_id, agent_session_id) {
                    return Ok(Some((project_id, session_id)));
                }
            }
        }
    }

    let matches = conversation_candidates(repository, agent_session_id)?
        .into_iter()
        .filter(|session| session_owns_agent_conversation(session, agent_id, agent_session_id))
        .filter_map(|session| {
            Some((
                required_value_text(&session, "projectId").ok()?,
                required_value_text(&session, "sessionId").ok()?,
            ))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [target] => Ok(Some(target.clone())),
        _ => Err(DomainStateError {
            code: "needsAttention",
            message: format!(
                "Multiple Ghostex sessions own agent conversation {agent_session_id}. Close the duplicate panes before running this automation."
            ),
        }),
    }
}

/// The rows that can own `agent_session_id`, in `list_sessions` order: a text
/// `runtimeSettings.agentSessionId` that contains it. `session_owns_agent_conversation` makes the
/// exact (trimmed, agent-matched) decision on each.
///
/// CDXC:Automations 2026-10-23 WHY:
/// Resolving a thread by its conversation hydrated every session row (about 148 MB for 10,900 rows) to find the one or two that own it. SQLite now picks the rows whose stored id contains the conversation id, a superset of the trimmed match (SQLite's `trim` strips only spaces, Rust's every whitespace), and only those are read in full.
fn conversation_candidates(
    repository: &DomainRepository<'_>,
    agent_session_id: &str,
) -> Result<Vec<Value>, DomainStateError> {
    let mut statement = repository
        .db
        .prepare_cached(
            "SELECT projectId, sessionId FROM sessions
             WHERE json_type(runtimeSettingsJson, '$.agentSessionId') = 'text'
               AND instr(json_extract(runtimeSettingsJson, '$.agentSessionId'), ?1) > 0
             ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
        )
        .map_err(crate::domain::sql_error)?;
    let keys = statement
        .query_map([agent_session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(crate::domain::sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::domain::sql_error)?;
    let mut sessions = Vec::with_capacity(keys.len());
    for (project_id, session_id) in keys {
        if let Some(session) = repository.get_session(&project_id, &session_id)? {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

fn session_owns_agent_conversation(
    session: &Value,
    agent_id: &str,
    agent_session_id: &str,
) -> bool {
    let runtime_settings = session.get("runtimeSettings").and_then(Value::as_object);
    let stored_agent_session_id = runtime_settings
        .and_then(|settings| settings.get("agentSessionId"))
        .and_then(Value::as_str)
        .map(str::trim);
    if stored_agent_session_id != Some(agent_session_id) {
        return false;
    }
    session
        .get("agentId")
        .and_then(Value::as_str)
        .or_else(|| {
            runtime_settings
                .and_then(|settings| settings.get("launchAgentId"))
                .and_then(Value::as_str)
        })
        .is_some_and(|stored_agent_id| stored_agent_id.eq_ignore_ascii_case(agent_id))
}

pub(super) fn find_run_session_project_id(
    repository: &DomainRepository<'_>,
    run: &AutomationRunRecord,
) -> Option<String> {
    let session_id = run.session_id.as_ref()?;
    if let Some(path) = run.worktree.get("path").and_then(Value::as_str) {
        if let Ok(Some(project)) = find_project_by_path(repository, path) {
            return Some(project.project_id);
        }
    }
    if repository
        .get_session(&run.project_id, session_id)
        .ok()
        .flatten()
        .is_some()
    {
        return Some(run.project_id.clone());
    }
    repository
        .list_projects()
        .ok()?
        .into_iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .find(|project_id| {
            repository
                .get_session(project_id, session_id)
                .ok()
                .flatten()
                .is_some()
        })
}
