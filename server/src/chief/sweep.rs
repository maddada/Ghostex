//! Turning Chief off stops its agent: every five seconds this reads the switch, and when it goes
//! off (or gxserver starts with it off) it puts Chief's session to sleep. Its chat, memory and
//! threads stay; the head role is already gone by then (head.rs reads the switch per request).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map};

use crate::server::{dispatch_zmx_lifecycle_http_blocking, AppState};

const POLL: Duration = Duration::from_secs(5);

pub(crate) fn start_chief_sweep(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let renamer = state.clone();
        let _ = tokio::task::spawn_blocking(move || rename_old_title(&renamer)).await;
        let mut was_on: Option<bool> = None;
        let mut poll = crate::ticks::steady_interval(POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            let on = super::chief_on(&state.paths);
            let turned_off = !on && was_on != Some(false);
            was_on = Some(on);
            if !turned_off {
                continue;
            }
            let Some((project_id, session_id)) = super::chief_key(&state.paths) else {
                continue;
            };
            let sleeper = state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let mut params = Map::new();
                params.insert("projectId".into(), json!(project_id));
                params.insert("sessionId".into(), json!(session_id));
                params.insert("reason".into(), json!("chiefTurnedOff"));
                dispatch_zmx_lifecycle_http_blocking(
                    &sleeper,
                    "/api/sleepSession".to_string(),
                    "chief-turned-off".to_string(),
                    params,
                )
            })
            .await;
        }
    });
}

/// A Chief session created before the renames (Q1896, Q1931) is still titled "Chief" or "Chief of
/// Staff": it takes the current name once, at gxserver start. A title the user gave it is left alone.
fn rename_old_title(state: &AppState) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let Some(record) = super::read_record(&db) else {
        return;
    };
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let titled_old = repository
        .get_session(&record.project_id, &record.session_id)
        .ok()
        .flatten()
        .is_some_and(|session| {
            session
                .get("title")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|title| super::OLD_CHIEF_TITLES.contains(&title))
        });
    if !titled_old {
        return;
    }
    let mut params = Map::new();
    params.insert("projectId".into(), json!(record.project_id));
    params.insert("sessionId".into(), json!(record.session_id));
    params.insert("title".into(), json!(super::CHIEF_TITLE));
    if repository.update_session(&params).is_ok() {
        let _ = crate::server::schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &record.project_id,
            &record.session_id,
        );
    }
}
