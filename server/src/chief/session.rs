//! Chief's session: created by gxserver in the Ghostex config folder's project (where Help chats
//! and the feedback agent already live), as a coordinator titled "Chief".
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User: "one per computer", living in the hidden Ghostex project (Q1741); "ask which agent on first open" (Q1740); "one ongoing chat, plus New Chat that keeps its memory" (Q1742); right-click Change Agent. New Chat and Change Agent start a new session, move the coordinator's goal, instructions, memory, threads and board items to it, point the record at it and close the old one, so the head role moves with it (head.rs) and nothing it was following is lost.
//! SEE-ALSO: server/src/feedback/agent.rs (the same server-side create in the same project), server/src/server/route_http/sessions.rs (`/api/createAgentSession`, whose coordinator steps this repeats).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::{read_record, write_record, ChiefRecord};
use crate::agents::{
    apply_created_session_identity, create_agent_session_params_for_project, read_agent_settings,
};
use crate::coordinators::{
    coordinator_create_params, coordinator_launcher_family, now_iso, read_coordinator,
    register_created_coordinator, write_coordinator, CoordinatorLaunch, SessionKey,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{
    dispatch_zmx_lifecycle_http_blocking, prepare_coordinator_create_params,
    restore_parked_project_for_new_session, schedule_presentation_project_delta,
    schedule_presentation_session_delta, start_orchestrator_role, AppState,
};
use crate::sidebar_hud::sidebar_agent_buttons_from_projects;
use crate::storage::open_gxserver_database;
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext};

/// Name the config folder's project gets when no project has that path yet (the Help chat's).
const CONFIG_PROJECT_NAME: &str = "Ghostex";

/// Chief's goal, shown in Coordinator Details.
const CHIEF_GOAL: &str =
    "Answer the user about anything in Ghostex, and steer their agents and coordinators for them.";

/// What follows the coordinator skill line in Chief's first message.
pub(super) const CHIEF_FIRST_REQUEST: &str = "You are Chief, the Ghostex-wide coordinator on this computer: the user asks you about any session, agent, project or computer, or about Ghostex itself. Before your first reply, and again after any context compaction, run `ghostex chief guide` and follow it; where it differs from the coordinator guide, it wins. Then say in one short line that you're ready and wait for the user.";

/// The launchers Chief can run on: set up to start, and an agent family a coordinator runs on.
pub(super) fn chief_agent_rows(projects: &[Value]) -> Vec<Value> {
    sidebar_agent_buttons_from_projects(projects)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|row| {
            row.get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| !command.trim().is_empty())
                && coordinator_launcher_family(row).is_some()
        })
        .collect()
}

fn text_of(value: &Value, key: &str) -> Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state(format!("Missing {key}.")))
}

fn database(state: &AppState) -> Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })
}

/// Whether the recorded session still exists (the user may have removed it).
fn session_exists(repository: &DomainRepository<'_>, record: &ChiefRecord) -> bool {
    repository
        .get_session(&record.project_id, &record.session_id)
        .ok()
        .flatten()
        .is_some()
}

/// The first open: the record when Chief already has a live session, otherwise a new session on
/// `agent_id` (the user's pick). `None` agent with no session asks the client to pick one.
pub(super) fn open_chief(
    state: &AppState,
    agent_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let db = database(state)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    if let Some(record) = read_record(&db).filter(|record| session_exists(&repository, record)) {
        return Ok(json!({ "status": "existing", "chief": record }));
    }
    let Some(agent_id) = agent_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Err(DomainStateError {
            code: "chiefNeedsAgent",
            message: "Pick the agent Chief runs on first.".to_string(),
        });
    };
    let (record, started) = create_chief_session(state, &db, &repository, agent_id)?;
    Ok(json!({ "status": "created", "chief": record, "providerStarted": started }))
}

/// New Chat (`agent_id` `None`) and Change Agent: a new session that takes over the old one's
/// coordinator data, then the old one is closed.
pub(super) fn replace_chief(
    state: &AppState,
    agent_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let db = database(state)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Some(old) = read_record(&db) else {
        return Err(DomainStateError {
            code: "chiefNeedsAgent",
            message: "Chief has not been opened yet. Open it and pick its agent first.".to_string(),
        });
    };
    let agent_id = agent_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(&old.agent_id)
        .to_string();
    let (record, started) = create_chief_session(state, &db, &repository, &agent_id)?;
    move_coordinator(&db, &old.key(), &record.key())?;
    if session_exists(&repository, &old) {
        let mut params = Map::new();
        params.insert("projectId".into(), json!(old.project_id));
        params.insert("sessionId".into(), json!(old.session_id));
        let _ = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/killSession".to_string(),
            "chief-new-chat".to_string(),
            params,
        );
    }
    Ok(json!({
        "status": "replaced",
        "chief": record,
        "previous": old,
        "providerStarted": started,
    }))
}

/// Creates the session, makes it a coordinator, queues its first message, starts its agent and
/// writes the record. Returns the record and whether the agent started.
fn create_chief_session(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    agent_id: &str,
) -> Result<(ChiefRecord, bool), DomainStateError> {
    let projects = repository.list_projects()?;
    let agent_rows = chief_agent_rows(&projects);
    if !agent_rows
        .iter()
        .any(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
    {
        return Err(DomainStateError::bad_request(format!(
            "Chief can't run on {agent_id}: pick one of {}.",
            crate::coordinators::coordinator_agent_families_text()
        )));
    }
    let project = config_folder_project(state, db, repository)?;
    let project_id = text_of(&project, "projectId")?;
    let mut params = coordinator_create_params(CoordinatorLaunch {
        agent_rows: &agent_rows,
        project_id: &project_id,
        agent_id,
        title: Some(super::CHIEF_TITLE.to_string()),
        goal: CHIEF_GOAL.to_string(),
        task: CHIEF_FIRST_REQUEST.to_string(),
        model: None,
        effort: None,
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    params.insert("surface".into(), json!("workspace"));
    params.insert("requireLaunchCommand".into(), json!(true));
    let params = prepare_coordinator_create_params(&params)?;
    let create_params = create_agent_session_params_for_project(db, &project, &params)?;
    let created = repository.create_session(&create_params, false)?;
    let session = apply_created_session_identity(repository, &created, &create_params)?;
    register_created_coordinator(db, &params, &session)?;
    let session_id = text_of(&session, "sessionId")?;
    let record = ChiefRecord {
        project_id: project_id.clone(),
        session_id: session_id.clone(),
        agent_id: agent_id.to_string(),
        created_at: now_iso(),
    };
    write_record(db, &state.paths, &record)?;
    if let Some(family) = crate::agents::session_agent_family_id(&project, &session) {
        // The record is written: a failed skill install or queue is the role's problem to report,
        // not a reason to leave a session nobody points at.
        let _ = start_orchestrator_role(state, &project_id, &session_id, &family, &params);
    }
    restore_parked_project_for_new_session(state, db, repository, &project_id)?;
    schedule_presentation_session_delta(state, db, repository, &project_id, &session_id)?;
    let started = start_provider(state, db, repository, &project_id, &session_id);
    Ok((record, started))
}

/// The Ghostex config folder's project: the one already registered for that path, or a new one.
fn config_folder_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    let known: Vec<String> = repository
        .list_projects()?
        .iter()
        .filter_map(|project| project.get("projectId").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let mut project_params = Map::new();
    project_params.insert(
        "path".into(),
        json!(state.paths.app_config_dir.to_string_lossy().to_string()),
    );
    project_params.insert("name".into(), json!(CONFIG_PROJECT_NAME));
    let project = repository.add_project_path(&project_params)?;
    let project_id = text_of(&project, "projectId")?;
    if known.contains(&project_id) {
        return Ok(repository.get_project(&project_id)?.unwrap_or(project));
    }
    let project = crate::workspaces::place_added_project(repository, db, project, &project_params)?;
    schedule_presentation_project_delta(state, db, repository, &project_id, "projectAdded")?;
    Ok(project)
}

fn start_provider(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    let Ok(agent_settings) = read_agent_settings(db) else {
        return false;
    };
    let mut provider_params = Map::new();
    provider_params.insert("projectId".into(), json!(project_id));
    provider_params.insert("sessionId".into(), json!(session_id));
    let started = match dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        &provider_params,
        &context,
        &agent_settings,
    ) {
        Ok(_) => true,
        Err(ZmxEndpointError::DependencyUnavailable(_) | ZmxEndpointError::Domain(_)) => false,
    };
    let _ = schedule_presentation_session_delta(state, db, repository, project_id, session_id);
    started
}

/// Moves a coordinator's goal, instructions, memory, threads (here and on other computers) and
/// board items from session `old` to session `new`, and drops the old coordinator row.
pub(super) fn move_coordinator(
    db: &Connection,
    old: &SessionKey,
    new: &SessionKey,
) -> Result<(), DomainStateError> {
    let sql = |error: rusqlite::Error| DomainStateError {
        code: "internalError",
        message: format!("SQLite Chief error: {error}"),
    };
    if let Some(mut record) = read_coordinator(db, &old.0, &old.1)? {
        record.project_id = new.0.clone();
        record.session_id = new.1.clone();
        write_coordinator(db, &record)?;
    }
    let params = rusqlite::params![new.0, new.1, old.0, old.1];
    for table in [
        "coordinator_threads",
        "coordinator_remote_threads",
        "coordinator_board_items",
    ] {
        db.execute(
            &format!(
                "UPDATE {table} SET coordinatorProjectId = ?1, coordinatorSessionId = ?2 \
                 WHERE coordinatorProjectId = ?3 AND coordinatorSessionId = ?4"
            ),
            params,
        )
        .map_err(sql)?;
    }
    db.execute(
        "DELETE FROM coordinators WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![old.0, old.1],
    )
    .map_err(sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinators::{insert_coordinator, link_thread, list_threads};

    #[test]
    fn a_new_chat_keeps_the_memory_and_the_threads() {
        let db = Connection::open_in_memory().unwrap();
        for migration in [
            include_str!("../storage/migrations/0041_coordinators.sql"),
            include_str!("../storage/migrations/0042_coordinator_pending_messages.sql"),
            include_str!("../storage/migrations/0050_coordinator_close_threads.sql"),
            include_str!("../storage/migrations/0051_coordinator_remote_threads.sql"),
            include_str!("../storage/migrations/0055_coordinator_board.sql"),
        ] {
            db.execute_batch(migration).unwrap();
        }
        insert_coordinator(&db, "P0", "OLD", "goal", "be brief").unwrap();
        let mut old = read_coordinator(&db, "P0", "OLD").unwrap().unwrap();
        old.memory.push(crate::coordinators::CoordinatorMemoryNote {
            text: "The user likes short replies.".into(),
            created_at: now_iso(),
        });
        write_coordinator(&db, &old).unwrap();
        link_thread(&db, "P1", "T1", "P0", "OLD", "a task").unwrap();
        insert_coordinator(&db, "P0", "NEW", "goal", "").unwrap();

        move_coordinator(
            &db,
            &("P0".into(), "OLD".into()),
            &("P0".into(), "NEW".into()),
        )
        .unwrap();

        assert!(read_coordinator(&db, "P0", "OLD").unwrap().is_none());
        let new = read_coordinator(&db, "P0", "NEW").unwrap().unwrap();
        assert_eq!(new.instructions, "be brief");
        assert_eq!(new.memory.len(), 1);
        let threads = list_threads(&db).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].coordinator_key(), ("P0".into(), "NEW".into()));
    }
}
