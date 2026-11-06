//! Resume sessions that were running when the computer shut down or lost power.

use std::{sync::Arc, time::Duration};

use serde_json::{json, Value};

use crate::{
    domain::DomainRepository,
    logging::{GxserverLogInput, LogLevel},
    paths::GxserverPaths,
    server::{read_session_persistence_provider, read_session_text, AppState},
    session_chat_send_wake::{wake_session, WakeOutcome},
    storage::open_gxserver_database,
};

/// Lets clients reconnect and the startup passes settle before the first agent starts.
const FIRST_RESUME_DELAY: Duration = Duration::from_secs(5);
/// One session at a time, the way bulk wakes are paced elsewhere.
const RESUME_PACING: Duration = Duration::from_secs(2);
/// A resume that failed (an agent CLI still installing, a slow disk at login) is tried again.
const RESUME_ATTEMPTS: u32 = 3;
const RESUME_RETRY_DELAY: Duration = Duration::from_secs(30);

/// One session to resume.
pub(crate) struct SessionToResume {
    project_id: String,
    session_id: String,
}

/// CDXC:ComputerStartup 2026-11-06 WHY: the record of what was running is the session rows themselves. While gxserver runs, the provider pass marks a session whose daemon ended `missing` (session_state_sync.rs), sleeping writes `sleeping` and closing writes `stopped`. A shutdown or power loss takes gxserver down with the daemons, so nothing marks those rows: they still say `running` with the daemon `exists`. On the first start after the computer restarted (`boot_mark.rs`), before any client can trigger that pass, the agent rows still saying so whose daemon is gone are the sessions that were running. A session the user closed, slept or that ended by itself before the shutdown is already marked otherwise and is never picked, and nothing is picked when gxserver merely restarted, since the daemons outlive that.
pub(crate) fn find_sessions_to_resume(
    paths: &GxserverPaths,
    server_id: &str,
) -> Vec<SessionToResume> {
    let Ok(live_daemons) = crate::zmx::read_zmx_existing_session_names() else {
        // Without the list of live daemons every row would look dead; resume nothing.
        return Vec::new();
    };
    let Ok(db) = open_gxserver_database(paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, server_id);
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return Vec::new();
    };
    let Ok(agent_settings) = crate::agents::read_agent_settings(&db) else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter(|session| {
            session.get("kind").and_then(Value::as_str) == Some("agent")
                && session.get("surface").and_then(Value::as_str) != Some("commands")
                && read_session_persistence_provider(session).as_deref() == Some("zmx")
                && session
                    .pointer("/providerState/lifecycleState")
                    .and_then(Value::as_str)
                    == Some("exists")
                && read_session_text(session, "zmxName")
                    .is_some_and(|name| !live_daemons.contains(&name))
        })
        .filter_map(|session| {
            let project_id = read_session_text(session, "projectId")?;
            let session_id = read_session_text(session, "sessionId")?;
            let project = repository.get_project(&project_id).ok()??;
            // Only sessions their agent can resume; the others come back as before, when opened.
            crate::zmx::get_persisted_provider_startup_text_for_session(
                &project,
                session,
                &agent_settings,
            )?;
            Some(SessionToResume {
                project_id,
                session_id,
            })
        })
        .collect()
}

/// Wakes each session the way opening a sleeping one does: its agent's own resume command runs in
/// a new daemon under the same session, so the same chat continues.
pub(crate) fn resume_sessions(state: Arc<AppState>, sessions: Vec<SessionToResume>) {
    if sessions.is_empty() {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_RESUME_DELAY).await;
        let mut pending = sessions;
        for attempt in 1..=RESUME_ATTEMPTS {
            let mut failed = Vec::new();
            for (index, session) in pending.into_iter().enumerate() {
                if index > 0 {
                    tokio::time::sleep(RESUME_PACING).await;
                }
                if !still_waiting(&state, &session) {
                    continue;
                }
                match wake_session(&state, &session.project_id, &session.session_id).await {
                    WakeOutcome::Woke | WakeOutcome::AlreadyWaking => {}
                    WakeOutcome::Blocked(why) => log(
                        &state,
                        LogLevel::Warn,
                        "computerStartup.resumeBlocked",
                        json!({ "projectId": session.project_id, "sessionId": session.session_id, "reason": why }),
                    ),
                    WakeOutcome::Failed(why) => {
                        log(
                            &state,
                            LogLevel::Warn,
                            "computerStartup.resumeFailed",
                            json!({ "projectId": session.project_id, "sessionId": session.session_id, "attempt": attempt, "reason": why }),
                        );
                        failed.push(session);
                    }
                }
            }
            if failed.is_empty() || attempt == RESUME_ATTEMPTS {
                break;
            }
            pending = failed;
            tokio::time::sleep(RESUME_RETRY_DELAY).await;
        }
    });
}

/// Whether the session still waits to be resumed: someone may have closed, slept or opened it
/// since the server started.
fn still_waiting(state: &AppState, session: &SessionToResume) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(row)) = repository.get_session(&session.project_id, &session.session_id) else {
        return false;
    };
    if row.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return false;
    }
    let Some(zmx_name) = read_session_text(&row, "zmxName") else {
        return false;
    };
    crate::zmx::read_zmx_existing_session_names()
        .is_ok_and(|live_daemons| !live_daemons.contains(&zmx_name))
}

/// Warnings only: routine lines would need a diagnostic area (AGENTS.md, Diagnostic logging).
fn log(state: &AppState, level: LogLevel, event: &str, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}
