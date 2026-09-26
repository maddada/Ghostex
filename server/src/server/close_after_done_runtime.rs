//! Close After Done's clock: once a second, every armed session's countdown is advanced, the
//! sessions whose countdown started or stopped are published, and the ones whose countdown ran
//! out while they are still Done are closed through `/api/transitionSession`.
//!
//! SEE-ALSO: server/src/close_after_done.rs (the armed set, the rule and the fields).

use super::*;

use crate::close_after_done::{self, CountdownStep};

const TICK: Duration = Duration::from_secs(1);

pub(crate) fn start_close_after_done_runtime(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let ticking = state.clone();
            let _ = tokio::task::spawn_blocking(move || tick(&ticking)).await;
        }
    });
}

/// `/api/toggleCloseAfterDone`, publishing the session so every client shows the armed state.
pub(crate) fn toggle_close_after_done(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let result = close_after_done::toggle(db, repository, params)?;
    if let (Some(project_id), Some(session_id)) = (
        result.get("projectId").and_then(Value::as_str),
        result.get("sessionId").and_then(Value::as_str),
    ) {
        schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
    }
    Ok(result)
}

fn tick(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(armed) = close_after_done::read_armed(&db) else {
        return;
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    for key in armed {
        let (project_id, session_id) = (key.0.as_str(), key.1.as_str());
        let Ok(delta) = build_presentation_session_delta(&db, &repository, project_id, session_id)
        else {
            continue;
        };
        // A session that left the presentation (closed, removed) is no longer armed.
        if delta.get("type").and_then(Value::as_str) == Some("sessionRemoved") {
            let _ = close_after_done::disarm(&db, &key);
            let _ = schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
            continue;
        }
        let done = delta.get("session").is_some_and(close_after_done::is_done);
        match close_after_done::step(&key, done, now_ms) {
            CountdownStep::Unchanged => {}
            CountdownStep::Changed => {
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                );
            }
            CountdownStep::Close => {
                let _ = close_after_done::disarm(&db, &key);
                let mut params = Map::new();
                params.insert("action".into(), json!("close"));
                params.insert("projectId".into(), json!(project_id));
                params.insert("reason".into(), json!("closeAfterDone"));
                params.insert("sessionId".into(), json!(session_id));
                let _ = dispatch_zmx_lifecycle_http_blocking(
                    state,
                    "/api/transitionSession".to_string(),
                    "close-after-done".to_string(),
                    params,
                );
                // A close the provider refused leaves the session in place and no longer armed;
                // clients must see the badge go either way.
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                );
            }
        }
    }
}
