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
pub(super) const CHIEF_FIRST_REQUEST: &str = "You are Ghostex AI, the Ghostex-wide coordinator on this computer and the user's starting point in Ghostex: they ask you about any session, agent, project or computer, or about Ghostex itself, and you help them set it up. Before your first reply, and again after any context compaction, run `ghostex chief guide` and follow it; where it differs from the coordinator guide, it wins. Then greet the user as its \"Your first reply\" section says.";

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

/// Whether the recorded session is still open: not removed, and not closed.
///
/// CDXC:Chief 2026-11-22 WHY: Close on Ask Ghostex AI asked "Close Ask Ghostex AI?" and closed the
/// chat, but a closed session still counted as Ghostex AI's own, so the next open (its row, the
/// sidebar's Ask Ghostex AI, a phone) went back to that closed chat and it seemed never to close.
/// A closed chat now ends like New Chat's old one, except it stays closed (Previous Sessions, the
/// home's History): the next open starts a fresh chat that takes over its goal, memory and threads.
pub(super) fn session_open(repository: &DomainRepository<'_>, record: &ChiefRecord) -> bool {
    repository
        .get_session(&record.project_id, &record.session_id)
        .ok()
        .flatten()
        .is_some_and(|session| crate::presentation::session_attributes::is_active(&session))
}

/// The first open: the record when Chief already has a live session, otherwise a new session on
/// `agent_id` (the user's pick). `None` agent with no session asks the client to pick one.
pub(super) fn open_chief(
    state: &AppState,
    agent_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let db = database(state)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    if let Some(record) = read_record(&db).filter(|record| session_open(&repository, record)) {
        let record = move_off_missing_agent(state, &db, &repository, record)?;
        return Ok(json!({ "status": "existing", "chief": record }));
    }
    // No agent named: the user's default agent (model.rs, CDXC:Chief 2026-11-21).
    let picked = match agent_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(agent_id) => Some(agent_id.to_string()),
        None => super::model::agent_for_next_chat(
            &state.paths,
            &db,
            &chief_agent_rows(&repository.list_projects()?),
        ),
    };
    let Some(agent_id) = picked else {
        return Err(DomainStateError {
            code: "chiefNeedsAgent",
            message: "No agent can run Ghostex AI yet. Set up Claude, Codex or Antigravity in Settings > Agents first.".to_string(),
        });
    };
    let agent_id = agent_id.as_str();
    // A Chief whose session was removed keeps its earlier chats.
    let old = read_record(&db);
    let hand_pick = old.as_ref().and_then(|record| record.model.clone());
    let earlier = old
        .as_ref()
        .map(|record| live_earlier(&repository, record.earlier.clone()))
        .unwrap_or_default();
    let closed = old.as_ref().map(ChiefRecord::key);
    let (record, started) =
        create_chief_session(state, &db, &repository, agent_id, earlier, hand_pick)?;
    // A chat the user closed hands its goal, memory and threads to the new one, as New Chat does.
    if let Some(closed) = closed {
        move_coordinator(&db, &closed, &record.key())?;
    }
    Ok(json!({ "status": "created", "chief": record, "providerStarted": started }))
}

/// A Chief chat that never started a conversation, on an agent whose CLI isn't installed (one
/// picked before Ghostex checked), switches in place to the installed default agent, the way the
/// chat's own agent tabs switch it (CDXC:AgentLauncher 2026-11-21 in agents/cli_missing.rs). A
/// chat with a conversation keeps its agent: Change Agent moves it.
fn move_off_missing_agent(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    record: ChiefRecord,
) -> Result<ChiefRecord, DomainStateError> {
    let rows = chief_agent_rows(&repository.list_projects()?);
    let on_missing_agent = rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(record.agent_id.as_str()))
        .is_some_and(|row| {
            crate::agents::launcher_row_cli_missing_fresh(row, state.paths.agent_config_home_dir())
        });
    if !on_missing_agent {
        return Ok(record);
    }
    let unstarted = repository
        .get_session(&record.project_id, &record.session_id)?
        .is_some_and(|session| crate::agents::session_has_no_conversation(db, &session));
    let Some(agent_id) = super::model::default_agent(&state.paths, db, &rows)
        .filter(|agent_id| unstarted && *agent_id != record.agent_id)
    else {
        return Ok(record);
    };
    let mut params = Map::new();
    params.insert("projectId".into(), json!(record.project_id));
    params.insert("sessionId".into(), json!(record.session_id));
    params.insert("agentId".into(), json!(agent_id));
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        "/api/switchDraftAgent".to_string(),
        "chief-agent-not-installed".to_string(),
        params,
    );
    Ok(read_record(db).unwrap_or(record))
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
            message: "Ghostex AI has not been opened yet. Open it and pick its agent first."
                .to_string(),
        });
    };
    let agent_id = agent_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(&old.agent_id)
        .to_string();
    let old_alive = session_open(&repository, &old);
    let mut earlier = Vec::new();
    if old_alive {
        earlier.push(EarlierChat {
            project_id: old.project_id.clone(),
            session_id: old.session_id.clone(),
        });
    }
    earlier.extend(live_earlier(&repository, old.earlier.clone()));
    let (record, started) = create_chief_session(
        state,
        &db,
        &repository,
        &agent_id,
        earlier,
        old.model.clone(),
    )?;
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

/// Ghostex AI's earlier chats that are still open, newest first: `projectId`, `sessionId`, `title`
/// and `lastActiveAt`. Closed ones are in the home's History (Previous Sessions).
///
/// CDXC:Chief 2026-11-22 WHY: New Chat keeps the old chat asleep as an earlier chat (the 2026-11-08
/// decision above: "earlier Chief chats stay under the row ... one click away"), and gxserver marked
/// them `chiefEarlier`, but no client ever listed them, so they sat among Main Hub's other sleeping
/// sessions where the user could not find them. The row's Earlier Chats reads this list.
pub(super) fn earlier_chats(repository: &DomainRepository<'_>, db: &Connection) -> Vec<Value> {
    let Some(record) = read_record(db) else {
        return Vec::new();
    };
    let mut chats: Vec<Value> = live_earlier(repository, record.earlier)
        .into_iter()
        .filter_map(|chat| {
            let session = repository
                .get_session(&chat.project_id, &chat.session_id)
                .ok()
                .flatten()?;
            let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
            Some(json!({
                "projectId": chat.project_id,
                "sessionId": chat.session_id,
                "title": text("title").unwrap_or_else(|| super::CHIEF_TITLE.to_string()),
                "lastActiveAt": text("lastActiveAt").or_else(|| text("createdAt")),
            }))
        })
        .collect();
    chats.sort_by(|a, b| {
        b["lastActiveAt"]
            .as_str()
            .unwrap_or_default()
            .cmp(a["lastActiveAt"].as_str().unwrap_or_default())
    });
    chats
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
    hand_pick: Option<super::model::ChiefModel>,
) -> Result<(ChiefRecord, bool), DomainStateError> {
    let projects = repository.list_projects()?;
    let agent_rows = chief_agent_rows(&projects);
    if !agent_rows
        .iter()
        .any(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
    {
        return Err(DomainStateError::bad_request(format!(
            "Ghostex AI can't run on {agent_id}: pick one of {}.",
            crate::coordinators::coordinator_agent_families_text()
        )));
    }
    let project = super::home::home_project(state, db, repository)?;
    let project_id = text_of(&project, "projectId")?;
    let family = agent_rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .and_then(crate::coordinators::coordinator_launcher_family);
    if let Some(family) = family {
        crate::agent_model_catalog::ensure_lineup(
            family,
            &state.paths,
            &state.event_hub,
            state.metadata.server_id.as_str(),
        );
    }
    let model = family.and_then(|family| super::model::for_new_chat(family, hand_pick.as_ref()));
    let mut params = coordinator_create_params(CoordinatorLaunch {
        agent_rows: &agent_rows,
        project_id: &project_id,
        agent_id,
        title: Some(super::CHIEF_TITLE.to_string()),
        goal: CHIEF_GOAL.to_string(),
        task: CHIEF_FIRST_REQUEST.to_string(),
        model: model.as_ref().map(|model| model.model.clone()),
        effort: model.as_ref().and_then(|model| model.effort.clone()),
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    // CDXC:Chief 2026-11-21 DECISION:
    // User, via Chief of Staff: "pls dont send the prompt to the chief of staff instead put it in the chat box like we do for coordinator thread and wait for the user to hit send/start on it ok? so that we dont cost him tokens unnecessarily". Every Chief of Staff chat Ghostex starts (first open, New Chat, Change Agent) holds its first message on the chat's Start card, like New Coordinator (session_chat_first_message.rs); nothing reaches the agent until the user starts it. Until then it counts as a chat with no conversation (agents/no_conversation.rs), so its agent and model switch in place.
    if let Some(coordinator) = params.get_mut("coordinator").and_then(Value::as_object_mut) {
        coordinator.insert("holdFirstMessage".into(), json!(true));
    }
    params.insert("surface".into(), json!("workspace"));
    params.insert("requireLaunchCommand".into(), json!(true));
    // CDXC:GBots 2026-11-09 DECISION: Codex's sandbox below Full blocks `ghostex` from reaching
    // gxserver (server/src/gbots/session.rs `default_approval`), so Chief of Staff on Codex starts
    // at Full. At the home folder that becomes "never ask" inside the `ghostex-home` profile, which
    // still denies the secret folders (server/src/agents/home_secret_folders.rs).
    let codex = family == Some("codex");
    if codex {
        if let Some(launch) = params
            .get_mut("launchSettings")
            .and_then(Value::as_object_mut)
        {
            launch.insert("acceptAllMode".into(), json!("enabled"));
        }
    }
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
        // What this chat starts on; with none to pick, an earlier hand pick stays remembered.
        model: model.or(hand_pick),
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
/// board items, and the record of the guide it last read, from session `old` to session `new`,
/// and drops the old coordinator row. All of it is one transaction: a coordinator reading its
/// guide meanwhile never sees the old row gone with its guide records not yet moved (it would
/// drop them as a removed coordinator's), and a failure moves nothing.
pub(super) fn move_coordinator(
    db: &Connection,
    old: &SessionKey,
    new: &SessionKey,
) -> Result<(), DomainStateError> {
    let sql = |error: rusqlite::Error| DomainStateError {
        code: "internalError",
        message: format!("SQLite Ghostex AI error: {error}"),
    };
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
            .map_err(sql)?;
    let db = &*transaction;
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
        "coordinator_work_items",
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
    crate::coordinators::carry_guide_records(db, old, new)?;
    transaction.commit().map_err(sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinators::insert_coordinator;
    use crate::paths::get_gxserver_paths;
    use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

    #[test]
    fn a_move_that_fails_to_carry_the_guide_records_moves_nothing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        let old = ("P1".to_string(), "OLD".to_string());
        let new = ("P1".to_string(), "NEW".to_string());
        insert_coordinator(&db, &old.0, &old.1, "The goal", "").expect("old");
        insert_coordinator(&db, &new.0, &new.1, "", "").expect("new");
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES ('coordinatorGuideReads', ?1, '')",
            [json!({ "P1:OLD": "version" }).to_string()],
        )
        .expect("read record");
        db.execute_batch(
            "CREATE TRIGGER refuse_metadata_update BEFORE UPDATE ON metadata \
             BEGIN SELECT RAISE(ABORT, 'disk I/O error'); END; \
             CREATE TRIGGER refuse_metadata_insert BEFORE INSERT ON metadata \
             BEGIN SELECT RAISE(ABORT, 'disk I/O error'); END;",
        )
        .expect("trigger");

        assert!(move_coordinator(&db, &old, &new).is_err());
        let old_record = read_coordinator(&db, &old.0, &old.1).expect("read old");
        assert_eq!(old_record.expect("old row kept").goal, "The goal");
        let new_record = read_coordinator(&db, &new.0, &new.1).expect("read new");
        assert_eq!(new_record.expect("new row").goal, "");
    }
}
