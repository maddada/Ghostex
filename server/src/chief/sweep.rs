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
