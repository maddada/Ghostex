//! Turning Main Hub off stops Chief's agent: every five seconds this reads the switch, and when it
//! goes off (or gxserver starts with it off) it puts Chief's session to sleep. A flip either way
//! also republishes the sidebar, which draws Main Hub only while it is on (presentation.rs). Its chat, memory and
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
        let mut default_on = DefaultOnPass::default();
        let mut poll = crate::ticks::steady_interval(POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            default_on.run(&state).await;
            let on = super::chief_on(&state.paths);
            let turned_off = !on && was_on != Some(false);
            let flipped = was_on.is_some_and(|was| was != on);
            was_on = Some(on);
            super::remember_chief_shown(on);
            // Main Hub on or off changes the home's header, Chief's rows and every client's list:
            // one fresh snapshot, as the Workspaces switch sends (server/workspaces_switch.rs).
            if flipped {
                let publisher = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    crate::server::publish_presentation_snapshot(&publisher)
                })
                .await;
            }
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

/// How often the on-by-default pass tries again while the app that saves settings isn't up yet.
const DEFAULT_ON_RETRY: Duration = Duration::from_secs(60);

/// Turns Main Hub on once for a computer that never chose (default_on.rs), retrying until the
/// settings save goes through.
#[derive(Default)]
struct DefaultOnPass {
    done: bool,
    last_try: Option<std::time::Instant>,
    logged_failure: bool,
}

impl DefaultOnPass {
    async fn run(&mut self, state: &Arc<AppState>) {
        if self.done
            || self
                .last_try
                .is_some_and(|at| at.elapsed() < DEFAULT_ON_RETRY)
        {
            return;
        }
        self.last_try = Some(std::time::Instant::now());
        let reader = state.clone();
        let decided = tokio::task::spawn_blocking(move || {
            let db = crate::storage::open_gxserver_database(&reader.paths).ok()?;
            if super::default_on::settled(&db) {
                return Some(None);
            }
            let settings =
                crate::shared_settings_file::read_shared_settings_checked(&reader.paths).ok()?;
            let decision =
                super::default_on::decide(settings.as_ref(), super::read_record(&db).as_ref());
            if decision != super::default_on::Decision::TurnOn {
                super::default_on::settle(&db, decision).ok()?;
            }
            Some(Some(decision))
        })
        .await
        .ok()
        .flatten();
        let decision = match decided {
            // The database or the settings file can't be read now: try again later.
            None => return,
            Some(None) => {
                self.done = true;
                return;
            }
            Some(Some(decision)) => decision,
        };
        if decision != super::default_on::Decision::TurnOn {
            log_default_on(state, decision, None);
            self.done = true;
            return;
        }
        let mut patch = Map::new();
        patch.insert("mainHubHidden".into(), json!(false));
        let mut payload = Map::new();
        payload.insert("patch".into(), serde_json::Value::Object(patch));
        payload.insert("source".into(), json!("settings:mainHubDefaultOn"));
        match crate::server::save_settings_patch(state, payload, 15_000).await {
            Ok(_) => {
                let writer = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    crate::storage::open_gxserver_database(&writer.paths)
                        .ok()
                        .map(|db| super::default_on::settle(&db, decision))
                })
                .await;
                log_default_on(state, decision, None);
                self.done = true;
            }
            // No app to save through yet (it saves this computer's settings): the next try.
            Err(failure) => {
                if !std::mem::replace(&mut self.logged_failure, true) {
                    log_default_on(state, decision, Some(failure.message));
                }
            }
        }
    }
}

fn log_default_on(state: &AppState, decision: super::default_on::Decision, error: Option<String>) {
    let reason = match decision {
        super::default_on::Decision::KeepOff(reason) => Some(reason),
        _ => None,
    };
    // A save that failed changed nothing: it says so, and the pass tries again.
    let word = if error.is_some() {
        "turnOnNotSavedYet"
    } else {
        decision.word()
    };
    let _ = crate::logging::GxserverLogger::new(state.paths.clone()).log(
        crate::logging::GxserverLogInput {
            level: if error.is_some() {
                crate::logging::LogLevel::Warn
            } else {
                crate::logging::LogLevel::Info
            },
            event: "chief.mainHubDefaultOn".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error,
            details: Some(json!({ "decision": word, "reason": reason })),
        },
    );
}

/// A Chief session created under an earlier name is still titled "Chief" or "Main Hub": it takes
/// the current name once, at gxserver start. A title the user gave it is left alone.
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
