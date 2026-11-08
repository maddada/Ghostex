//! Chief's session: created by gxserver in the home project (the project at the home folder,
//! home.rs), as a coordinator titled "Chief of Staff".
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User: "one per computer" (Q1741); "ask which agent on first open" (Q1740); "one ongoing chat, plus New Chat that keeps its memory" (Q1742); right-click Change Agent. New Chat and Change Agent start a new session and move the coordinator's goal, instructions, memory, threads and board items to it, then point the record at it, so the head role moves with it (head.rs) and nothing it was following is lost.
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1910, "both"): New Chat asks first, with a Don't ask again box (the clients), and earlier Chief chats stay under the row, folded like a project's sessions, one click away and resumable. So the old session is put to sleep instead of closed, kept in the record's `earlier` list (presented as `chiefEarlier`), and retitled with the day it started when it still has Chief's own title. Supersedes closing the old session, which left it only in Previous Sessions. Chief's sessions are created in the home project (Q1914, home.rs) instead of the Ghostex config folder's.
//! SEE-ALSO: server/src/feedback/agent.rs (the same server-side create), server/src/server/route_http/sessions.rs (`/api/createAgentSession`, whose coordinator steps this repeats).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::{read_record, write_record, ChiefRecord, EarlierChat};
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
    restore_parked_project_for_new_session, schedule_presentation_session_delta,
    start_orchestrator_role, AppState,
};
use crate::sidebar_hud::sidebar_agent_buttons_from_projects;
use crate::storage::open_gxserver_database;
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext};

/// Chief's goal, shown in Coordinator Details.
const CHIEF_GOAL: &str =
    "Answer the user about anything in Ghostex, and steer their agents and coordinators for them.";

/// What follows the coordinator skill line in Chief's first message.
pub(super) const CHIEF_FIRST_REQUEST: &str = "You are Chief of Staff, the Ghostex-wide coordinator on this computer: the user asks you about any session, agent, project or computer, or about Ghostex itself. Before your first reply, and again after any context compaction, run `ghostex chief guide` and follow it; where it differs from the coordinator guide, it wins. Then say in one short line that you're ready and wait for the user.";

/// The launchers Chief can run on: set up to start, and an agent family a coordinator runs on.
/// GBots run on the same list (server/src/gbots/session.rs).
pub(crate) fn chief_agent_rows(projects: &[Value]) -> Vec<Value> {
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
            message: "Pick the agent Chief of Staff runs on first.".to_string(),
        });
    };
    // A Chief whose session was removed keeps its earlier chats.
    let earlier = read_record(&db)
        .map(|record| live_earlier(&repository, record.earlier))
        .unwrap_or_default();
    let (record, started) = create_chief_session(state, &db, &repository, agent_id, earlier)?;
    Ok(json!({ "status": "created", "chief": record, "providerStarted": started }))
}

/// New Chat (`agent_id` `None`) and Change Agent: a new session that takes over the old one's
/// coordinator data; the old one is put to sleep and kept as an earlier chat.
pub(super) fn replace_chief(
    state: &AppState,
    agent_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let db = database(state)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Some(old) = read_record(&db) else {
        return Err(DomainStateError {
            code: "chiefNeedsAgent",
            message: "Chief of Staff has not been opened yet. Open it and pick its agent first."
                .to_string(),
        });
    };
    let agent_id = agent_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(&old.agent_id)
        .to_string();
    let old_alive = session_exists(&repository, &old);
    let mut earlier = Vec::new();
    if old_alive {
        earlier.push(EarlierChat {
            project_id: old.project_id.clone(),
            session_id: old.session_id.clone(),
        });
    }
    earlier.extend(live_earlier(&repository, old.earlier.clone()));
    let (record, started) = create_chief_session(state, &db, &repository, &agent_id, earlier)?;
    move_coordinator(&db, &old.key(), &record.key())?;
    if old_alive {
        retitle_earlier(state, &db, &repository, &old);
        let mut params = Map::new();
        params.insert("projectId".into(), json!(old.project_id));
        params.insert("sessionId".into(), json!(old.session_id));
        params.insert("reason".into(), json!("chiefNewChat"));
        let _ = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/sleepSession".to_string(),
            "chief-new-chat".to_string(),
            params,
        );
        // Its coordinator fields and `chiefEarlier` changed with the record.
        let _ = schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &old.project_id,
            &old.session_id,
        );
    }
    Ok(json!({
        "status": "replaced",
        "chief": record,
        "previous": old,
        "providerStarted": started,
    }))
}

/// The earlier chats that still exist (the user may have closed or removed some).
fn live_earlier(repository: &DomainRepository<'_>, chats: Vec<EarlierChat>) -> Vec<EarlierChat> {
    chats
        .into_iter()
        .filter(|chat| {
            repository
                .get_session(&chat.project_id, &chat.session_id)
                .ok()
                .flatten()
                .is_some_and(|session| crate::presentation::session_attributes::is_active(&session))
        })
        .collect()
}

/// An earlier chat still titled "Chief of Staff" is renamed after the day it started ("Chief of
/// Staff · 8 Nov, 13:30"), so the rows under the header tell the chats apart. A title the user gave
/// it stays.
fn retitle_earlier(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    old: &ChiefRecord,
) {
    let titled_chief = repository
        .get_session(&old.project_id, &old.session_id)
        .ok()
        .flatten()
        .is_some_and(|session| {
            matches!(
                session.get("title").and_then(Value::as_str),
                Some(title) if title == super::CHIEF_TITLE || super::OLD_CHIEF_TITLES.contains(&title)
            )
        });
    if !titled_chief {
        return;
    }
    let started = chrono::DateTime::parse_from_rfc3339(&old.created_at)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%-d %b, %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let title = if started.is_empty() {
        super::CHIEF_TITLE.to_string()
    } else {
        format!("{} \u{b7} {started}", super::CHIEF_TITLE)
    };
    let mut params = Map::new();
    params.insert("projectId".into(), json!(old.project_id));
    params.insert("sessionId".into(), json!(old.session_id));
    params.insert("title".into(), json!(title));
    if repository.update_session(&params).is_ok() {
        let _ = schedule_presentation_session_delta(
            state,
            db,
            repository,
            &old.project_id,
            &old.session_id,
        );
    }
}

/// Creates the session, makes it a coordinator, queues its first message, starts its agent and
/// writes the record. Returns the record and whether the agent started.
fn create_chief_session(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    agent_id: &str,
    earlier: Vec<EarlierChat>,
) -> Result<(ChiefRecord, bool), DomainStateError> {
    let projects = repository.list_projects()?;
    let agent_rows = chief_agent_rows(&projects);
    if !agent_rows
        .iter()
        .any(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
    {
        return Err(DomainStateError::bad_request(format!(
            "Chief of Staff can't run on {agent_id}: pick one of {}.",
            crate::coordinators::coordinator_agent_families_text()
        )));
    }
    let project = super::home::home_project(state, db, repository)?;
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
        earlier,
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

/// Starts the agent of a session gxserver just created; answers whether it started. GBot chats
/// start the same way (server/src/gbots/session.rs).
pub(crate) fn start_provider(
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
        message: format!("SQLite Chief of Staff error: {error}"),
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
