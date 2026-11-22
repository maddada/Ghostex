//! Turning Main Hub off stops Chief's agent: every five seconds this reads the switch, and when it
//! goes off (or gxserver starts with it off) it puts Chief's session to sleep. A flip either way
//! also republishes the sidebar, which draws Main Hub only while it is on (presentation.rs). Its chat, memory and
//! threads stay; the head role is already gone by then (head.rs reads the switch per request).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, json};

use crate::server::{AppState, dispatch_zmx_lifecycle_http_blocking};

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

/// The metadata row that says the rename below has run on this computer.
const TITLES_RENAMED_KEY: &str = "chiefTitlesRenamed";

/// Once per computer, at gxserver start: a Chief chat still titled with a name Ghostex gave it under
/// an earlier name ("Chief", "Main Hub", "Chief of Staff", or an earlier chat's "Chief of Staff ·
/// 8 Nov, 13:30") takes the current name. A chat the user renamed is left alone, whatever its
/// title, and so is any other title.
///
/// WHY: it runs once (the `chiefTitlesRenamed` row), so a user who later names a chat "Chief of
/// Staff" keeps it (review of 55635e490: it ran at every start and undid such a rename).
fn rename_old_title(state: &AppState) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let ran = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [TITLES_RENAMED_KEY],
            |row| row.get::<_, String>(0),
        )
        .is_ok();
    if ran {
        return;
    }
    if let Some(record) = super::read_record(&db) {
        let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
        let chats = std::iter::once((record.project_id.clone(), record.session_id.clone())).chain(
            record
                .earlier
                .iter()
                .map(|chat| (chat.project_id.clone(), chat.session_id.clone())),
        );
        for (project_id, session_id) in chats {
            let Some(session) = repository
                .get_session(&project_id, &session_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            let renamed_by_user = session
                .pointer("/runtimeSettings/pendingAgentTitleRequestTitleSource")
                .and_then(serde_json::Value::as_str)
                == Some("user");
            let Some(new_title) = session
                .get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|_| !renamed_by_user)
                .and_then(renamed_title)
            else {
                continue;
            };
            let mut params = Map::new();
            params.insert("projectId".into(), json!(project_id));
            params.insert("sessionId".into(), json!(session_id));
            params.insert("title".into(), json!(new_title));
            if repository.update_session(&params).is_ok() {
                let _ = crate::server::schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                );
            }
        }
    }
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, 'true', ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![TITLES_RENAMED_KEY, crate::coordinators::now_iso()],
    );
}

/// The current name for a title Ghostex gave a Chief chat under an earlier name: one of the old
/// names exactly, or an earlier chat's "<old name> · 8 Nov, 13:30" (the day `retitle_earlier` in
/// session.rs writes). `None` for any other title.
fn renamed_title(title: &str) -> Option<String> {
    if super::OLD_CHIEF_TITLES.contains(&title) {
        return Some(super::CHIEF_TITLE.to_string());
    }
    super::OLD_CHIEF_TITLES.iter().find_map(|old| {
        let started = title.strip_prefix(old)?.strip_prefix(" \u{b7} ")?;
        chrono::NaiveTime::parse_from_str(started.split(", ").nth(1)?, "%H:%M").ok()?;
        let day = started.split(", ").next()?;
        let mut parts = day.split(' ');
        let (date, month) = (parts.next()?, parts.next()?);
        (parts.next().is_none()
            && date
                .parse::<u32>()
                .is_ok_and(|date| (1..=31).contains(&date))
            && [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ]
            .contains(&month)
            && started.split(", ").count() == 2)
            .then(|| format!("{} \u{b7} {started}", super::CHIEF_TITLE))
    })
}
