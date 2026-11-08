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

/// A running agent session that is working, has a transcript to judge by, is not waiting on the
/// user, and is not a thread the coordinator supervisor already watches.
///
/// CDXC:SessionStatus 2026-11-07 DECISION:
/// A session with an open question (Claude's AskUserQuestion keeps its `working` title while the
/// card waits) or a waiting approval is quiet because the user has not answered, not because it is
/// hung, so it is left out of the watch and, once it was marked stuck, loses its mark the next tick
/// (it drops out of the quiet map). It is the same test `is_grouped_working` applies in
/// packages/gx-core/src/sidebar_view/ordering.rs: an open question or a waiting approval.
/// Issue #459.
fn watched(session: &Value, key: &SessionKey, now_iso: &str) -> bool {
    if effective_lifecycle_state(session) != "running"
        || presentation_activity(session, now_iso) != "working"
        || waits_on_the_user(session, now_iso)
    {
        return false;
    }
    let has_transcript = crate::session_chat_follower::session_chat_agent_for_session(session)
        .and_then(|agent| {
            crate::session_chat::resolve_session_chat_transcript_agent(Some(agent.as_str()))
        })
        .is_some();
    // A session whose daemon is gone cannot show output; the self-heal restarts it, and its clock
    // starts over with the new run (session_self_heal.rs).
    has_transcript
        && !coordinators::watched_as_thread(&key.0, &key.1)
        && !crate::session_self_heal::daemon_lost(&key.0, &key.1)
}

/// An open question or a waiting approval, as the presentation row publishes them.
fn waits_on_the_user(session: &Value, now_iso: &str) -> bool {
    crate::session_approval::session_has_question(session)
        || crate::session_approval::session_awaits_approval(session, now_iso)
}

fn text_field(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Redraws every session whose stuck state changed, and alerts (one notification in the bell, a
/// push to the phones, the computer's sound through `stuckSince`) for each that became stuck with
/// nobody healing it.
///
/// CDXC:Notifications 2026-11-08 DECISION:
/// "red dot on any session needs to notify if autopilot isn't enabled and we don't resolve it
/// ourselves": a session Autopilot covers keeps its red dot but alerts only when Autopilot's rules
/// leave it for the user (`announce_left_for_user`), never while Autopilot interrupts or reloads
/// it; one that moves again is Autopilot's to heal again next time. A session Ghostex's restarts
/// gave up on alerts at once (packages/gx-core/src/alert_rule.rs, the rule; `stuck_handled_by` in
/// server/src/presentation/attention_alerts.rs, the same test the presentation stamps).
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
    for (project_id, session_id) in &changes.cleared {
        crate::autopilot::clear_left_for_user(project_id, session_id);
    }
    for (project_id, session_id) in &changes.became_stuck {
        let healing = owner != QuietOwner::Heal
            && crate::presentation::stuck_handled_by(project_id, session_id).is_some();
        if !healing {
            announce_stuck(state, db, repository, owner, project_id, session_id);
        }
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
                    "selfHeal": owner == QuietOwner::Heal,
                    "autopilotHeals": healing,
                })),
            },
        );
    }
}

/// Autopilot's rules left a session for the user (`Act::Park`): from now on its red dot is the
/// user's, so a session stuck now alerts at once and its row drops `stuckHandledBy`.
pub(crate) fn announce_left_for_user(state: &AppState, project_id: &str, session_id: &str) {
    let already = crate::autopilot::left_for_user(project_id, session_id);
    crate::autopilot::note_left_for_user(project_id, session_id);
    let stuck_now = crate::coordinators::session_stuck_since(project_id, session_id).is_some();
    if already || !stuck_now {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let owner = if crate::coordinators::watched_as_thread(project_id, session_id) {
        QuietOwner::Threads
    } else {
        QuietOwner::Sessions
    };
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    announce_stuck(state, &db, &repository, owner, project_id, session_id);
}

/// The stuck session's alert: its bell row, and the push that carries it to the phones.
fn announce_stuck(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    owner: QuietOwner,
    project_id: &str,
    session_id: &str,
) {
    let body = match owner {
        // A thread is stuck either working without output or idle although its background job
        // waits (coordinators/idle_wait.rs); either way its nudge came first.
        QuietOwner::Threads => {
            "Not moving: no progress even after Ghostex nudged it. Its coordinator was told."
                .to_string()
        }
        QuietOwner::Sessions => format!(
            "Not moving: no new output for {} minutes. It may be stuck on a hung command.",
            coordinators::minutes_of(
                i64::from(coordinators::SESSION_STUCK_ROUNDS) * coordinators::quiet_round_ms()
            )
        ),
        QuietOwner::Heal => "Stopped and couldn't restart: it ended unexpectedly and Ghostex couldn't keep it running. Open it to start it again.".to_string(),
    };
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("body".to_string(), json!(body));
    let Ok(created) =
        crate::notification_feed::create_notification_endpoint(state, repository, db, &params)
    else {
        return;
    };
    let (Some(id), Ok(Some(session))) = (
        created.pointer("/notification/id").and_then(Value::as_str),
        repository.get_session(project_id, session_id),
    ) else {
        return;
    };
    crate::push::announce_blocked(
        state,
        repository,
        &session,
        id,
        crate::push::BlockedWhy::Stuck,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-07T00:00:00.000Z";

    fn claude_session(prompt: Option<Value>) -> Value {
        let mut agent_activity = json!({ "activity": "working" });
        if let Some(prompt) = prompt {
            agent_activity["sessionChatPrompt"] = json!(prompt.to_string());
        }
        json!({
            "projectId": "p-stall-watch",
            "sessionId": "s-stall-watch",
            "lifecycleState": "running",
            "launchSettings": { "icon": "claude" },
            "runtimeSettings": { "activity": "working", "agentActivity": agent_activity },
        })
    }

    fn key() -> SessionKey {
        ("p-stall-watch".to_string(), "s-stall-watch".to_string())
    }

    /// The card Claude's AskUserQuestion PreToolUse hook stores (the real derivation).
    fn asked_question() -> Value {
        let prompt = crate::session_chat_interactive::derive_session_chat_prompt(
            Some("AskUserQuestion"),
            Some(&json!({
                "questions": [{
                    "question": "Which one?", "header": "Pick", "multiSelect": false,
                    "options": [{ "label": "A", "description": "First" }, { "label": "B", "description": "Second" }],
                }],
            })),
            Some("PreToolUse"),
        )
        .expect("a question card");
        serde_json::to_value(prompt).expect("stored card")
    }

    #[test]
    fn a_plain_working_session_is_watched() {
        assert!(watched(&claude_session(None), &key(), NOW));
    }

    #[test]
    fn a_working_session_with_an_open_question_is_not_watched() {
        let session = claude_session(Some(asked_question()));
        assert_eq!(presentation_activity(&session, NOW), "working");
        assert!(!watched(&session, &key(), NOW));
    }

    #[test]
    fn a_session_waiting_for_approval_is_not_watched() {
        // The test of "waiting for approval" that the presentation row publishes: a stored
        // approval card while the title is not working. Seen from the stall watch the session may
        // still read as working through a screen that shows the approval (checked on the helper).
        let approval = json!({ "kind": "approval", "tool": "Bash", "summary": "rm -rf build" });
        let mut session = claude_session(Some(approval));
        session["runtimeSettings"]["agentActivity"]["activity"] = json!("attention");
        assert!(crate::session_approval::session_awaits_approval(
            &session, NOW
        ));
        assert!(waits_on_the_user(&session, NOW));
        assert!(!watched(&session, &key(), NOW));
    }

    #[test]
    fn an_answered_question_puts_the_session_back_in_the_watch() {
        // The card is gone: the plain working session is watched again.
        let session = claude_session(None);
        assert!(!waits_on_the_user(&session, NOW));
        assert!(watched(&session, &key(), NOW));
    }
}
