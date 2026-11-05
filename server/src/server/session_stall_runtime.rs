//! The sidebar-wide stuck watch: every working agent session that is not a coordinator's thread
//! (the coordinator supervisor watches those, with its nudge and reports) gets the same output
//! clock, and counts as stuck after one round of quiet. Both owners publish through
//! `publish_stuck_changes`: a session that becomes stuck gets its red dot (presentation
//! `stuckSince`, which the desktop answers with the not-moving sound and a banner) and one
//! notification in the bell; one that moves again loses the dot.
//!
//! SEE-ALSO: server/src/coordinators/stall.rs (the watch and the CDXC:Coordinators decision),
//! server/src/server/coordinator_runtime.rs (threads).

use super::*;

use crate::coordinators::{self, QuietOwner, SessionKey, StallWatch, StuckChanges};
use crate::presentation::{effective_lifecycle_state, presentation_activity};

const TICK: Duration = Duration::from_secs(10);

pub(crate) fn start_session_stall_runtime(state: Arc<AppState>) {
    let watches = Arc::new(Mutex::new(HashMap::<SessionKey, StallWatch>::new()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let (ticking, ticking_watches) = (state.clone(), watches.clone());
            let _ = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_watches)).await;
        }
    });
}

fn tick(state: &AppState, watches: &Mutex<HashMap<SessionKey, StallWatch>>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return;
    };
    let Ok(mut watches) = watches.lock() else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    let now_iso = crate::presentation::now_iso();
    let mut working = HashSet::new();
    for session in &sessions {
        let key = (
            text_field(session, "projectId"),
            text_field(session, "sessionId"),
        );
        if !watched(session, &key, &now_iso) {
            continue;
        }
        working.insert(key.clone());
        let _ = watches
            .entry(key)
            .or_insert_with(|| StallWatch::new(now))
            .observe(session, now);
    }
    watches.retain(|key, _| working.contains(key));
    let changes = coordinators::replace_session_quiet(
        QuietOwner::Sessions,
        watches
            .iter()
            .map(|(key, watch)| {
                (
                    key.clone(),
                    watch.view(now, coordinators::SESSION_STUCK_ROUNDS),
                )
            })
            .collect(),
    );
    publish_stuck_changes(state, &db, &repository, QuietOwner::Sessions, changes);
}

/// A running agent session that is working, has a transcript to judge by, and is not a thread the
/// coordinator supervisor already watches.
fn watched(session: &Value, key: &SessionKey, now_iso: &str) -> bool {
    if effective_lifecycle_state(session) != "running"
        || presentation_activity(session, now_iso) != "working"
    {
        return false;
    }
    let has_transcript = crate::session_chat_follower::session_chat_agent_for_session(session)
        .and_then(|agent| {
            crate::session_chat::resolve_session_chat_transcript_agent(Some(agent.as_str()))
        })
        .is_some();
    has_transcript && !coordinators::watched_as_thread(&key.0, &key.1)
}

fn text_field(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Redraws every session whose stuck state changed, and puts one notification in the bell for each
/// that became stuck.
pub(crate) fn publish_stuck_changes(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    owner: QuietOwner,
    changes: StuckChanges,
) {
    for (project_id, session_id) in changes.became_stuck.iter().chain(&changes.cleared) {
        let _ = schedule_presentation_session_delta(state, db, repository, project_id, session_id);
    }
    let rounds = match owner {
        QuietOwner::Threads => coordinators::THREAD_STUCK_ROUNDS,
        QuietOwner::Sessions => coordinators::SESSION_STUCK_ROUNDS,
    };
    let minutes = i64::from(rounds) * coordinators::QUIET_ROUND_MS / 60_000;
    let body = match owner {
        QuietOwner::Threads => format!(
            "Not moving: no new output for {minutes} minutes, even after Ghostex nudged it. Its coordinator was told."
        ),
        QuietOwner::Sessions => format!(
            "Not moving: no new output for {minutes} minutes. It may be stuck on a hung command."
        ),
    };
    for (project_id, session_id) in &changes.became_stuck {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("body".to_string(), json!(body));
        let _ =
            crate::notification_feed::create_notification_endpoint(state, repository, db, &params);
        let _ = state.logger.log_routine(
            crate::logging::DiagnosticLogScenario::ServerLifecycle,
            GxserverLogInput {
                level: LogLevel::Info,
                event: "sessionNotMoving".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "thread": owner == QuietOwner::Threads,
                })),
            },
        );
    }
}
