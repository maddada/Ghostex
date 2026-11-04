use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{
    log_agent_activity_transition, schedule_presentation_session_delta,
    schedule_stale_activity_presentation_refresh, AppState,
};
use crate::session_status::parse_iso_ms;
use crate::storage::open_gxserver_database;

use super::{
    derive_program_status, forget_program_status, log_settled_program_status_mismatches,
    mark_program_status_unsupported, observe_program_opinion, program_status_activity,
    program_status_applied_key, program_status_combines_entry, program_status_entry,
    set_program_status_applied, set_program_status_applied_key, set_program_status_notified,
    start_program_status_detection_shadow, store_program_status_snapshot,
    take_program_status_detection_shadow, ProgramStatusRecord,
};

/// One program-status line of `zmx watch-title` / `wmx watch-title`.
#[derive(Debug, PartialEq)]
pub(crate) enum ProgramStatusLine {
    Snapshot {
        reported: bool,
        submits: Option<u64>,
        records: Vec<ProgramStatusRecord>,
    },
    /// The daemon predates OSC 7501 support.
    Unsupported,
}

#[derive(Deserialize)]
struct SnapshotWire {
    #[serde(default)]
    reported: bool,
    /// Absent from a daemon that predates the input count.
    #[serde(default)]
    submits: Option<u64>,
    #[serde(default)]
    records: Vec<ProgramStatusRecord>,
}

pub(crate) fn parse_program_status_line(line: &str) -> Option<ProgramStatusLine> {
    if !line.trim_start().starts_with("{\"programStatus") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("programStatusSupported") == Some(&Value::Bool(false)) {
        return Some(ProgramStatusLine::Unsupported);
    }
    let snapshot: SnapshotWire =
        serde_json::from_value(value.get("programStatus")?.clone()).ok()?;
    Some(ProgramStatusLine::Snapshot {
        reported: snapshot.reported,
        submits: snapshot.submits,
        records: snapshot.records,
    })
}

/// Record a watch line and bring the session's activity in line with it.
pub(crate) fn ingest_program_status_line(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    line: ProgramStatusLine,
) {
    let records_changed = match line {
        ProgramStatusLine::Unsupported => {
            mark_program_status_unsupported(project_id, session_id);
            false
        }
        ProgramStatusLine::Snapshot {
            reported,
            submits,
            records,
        } => store_program_status_snapshot(project_id, session_id, reported, submits, records),
    };
    apply_program_status(state, project_id, session_id, records_changed);
}

/// The watch of a session ended: its daemon and records are gone.
pub(crate) fn end_program_status_watch(state: &AppState, project_id: &str, session_id: &str) {
    let entry = program_status_entry(project_id, session_id);
    if let Some(entry) = entry.as_ref() {
        if program_raised_activity(entry) {
            hand_back_to_detection(state, project_id, session_id, entry);
        }
        take_program_status_detection_shadow(project_id, session_id);
    }
    let combined = program_status_combines_entry(entry.as_ref());
    if forget_program_status(project_id, session_id) && combined {
        publish_session(state, project_id, session_id);
    }
}

/// Working or blocked: states the program raised that Ghostex's detection
/// would never clear on its own (the projection never expires explicit
/// working). A finished mark (done, error, finished) stays for the user to see.
fn program_raised_activity(entry: &super::SessionProgramStatus) -> bool {
    matches!(entry.applied_state.as_deref(), Some("working" | "blocked"))
}

/// CDXC:SessionStatus 2026-10-15 WHY:
/// When a program that had set a session working or blocked stops driving it, the session takes detection's own view of it (the shadow `compute_activity_update` kept moving with every hook, title and key while the program drove it), never a forced idle: a forced idle when the program stopped dropped a Claude thread that was mid-command to idle, Ghostex reported its turn finished and the auto-sleep sweep slept it. A shadow that started from what the program wrote (after a gxserver restart) knows nothing of its own, so then a program that is gone or idle leaves the session idle (its working ended; a script that exits while working must not stay working, regression G342s).
fn hand_back_to_detection(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    entry: &super::SessionProgramStatus,
) {
    let idle = || json!({"activity": "idle"});
    let shadow = entry.detection_shadow.clone();
    let restored = match shadow {
        Some(shadow) if entry.shadow_known => shadow,
        _ => idle(),
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("restoreDetectionActivity".to_string(), restored);
    let endpoint = "/api/updateAgentActivity";
    let Ok(output) = dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        endpoint,
        &params,
        None,
    ) else {
        return;
    };
    log_agent_activity_transition(state, endpoint, &params, &output.result);
    crate::notification_feed::observe_agent_activity_result(
        state,
        endpoint,
        &params,
        &output.result,
    );
    crate::session_chat_question_resend::observe_turn_end(state, &params, &output.result);
    crate::session_chat_open_questions::observe_activity(state, &params, &output.result);
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

fn publish_session(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
}

/// CDXC:SessionStatus 2026-10-14 WHY:
/// A program's report is written into the session's stored activity (not only shown) so everything that already hangs off activity keeps working from it: notifications and sounds on attention, the chat queue delivering after a finished turn, auto-sleep, recency and acknowledgement. The write goes through `/api/updateAgentActivity` with `programStatusActivity`, the one input the detection gate in `compute_activity_update` lets through for a governed session.
pub(crate) fn apply_program_status(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    records_changed: bool,
) {
    let Some(entry) = program_status_entry(project_id, session_id) else {
        return;
    };
    if !program_status_combines_entry(Some(&entry)) {
        // The program stopped joining detection (it exited, its records
        // cleared, or it no longer reports): give back what it raised.
        if program_raised_activity(&entry) {
            hand_back_to_detection(state, project_id, session_id, &entry);
        } else if records_changed {
            publish_session(state, project_id, session_id);
        }
        take_program_status_detection_shadow(project_id, session_id);
        if entry.applied_key.is_some() || entry.applied_state.is_some() {
            set_program_status_applied_key(project_id, session_id, None);
        }
        return;
    }
    let derived = derive_program_status(&entry.records);
    let key = program_status_applied_key(&derived);
    if entry.applied_key.as_deref() == Some(key.as_str()) {
        if records_changed {
            publish_session(state, project_id, session_id);
        }
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    // Detection's view up to the moment the program starts driving the session.
    start_program_status_detection_shadow(
        project_id,
        session_id,
        session.pointer("/runtimeSettings/agentActivity"),
    );
    let effective = match finished_turn_step(&entry, &derived, &session) {
        FinishedTurnStep::Promote => "finished",
        FinishedTurnStep::StillFinished => {
            set_program_status_applied(project_id, session_id, Some(key), Some("finished"));
            if records_changed {
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                );
            }
            return;
        }
        FinishedTurnStep::None => derived.state,
    };
    set_program_status_applied(project_id, session_id, Some(key.clone()), Some(effective));
    if entry.applied_key.is_none() && already_seen(&session, derived.state, derived.record.as_ref())
    {
        if records_changed {
            let _ = schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
        }
        return;
    }
    observe_program_opinion(project_id, session_id, &session);
    // agy reports done while a background task will wake it again; its hooks keep the turn
    // working (CDXC:SessionStatus 2026-10-23 in registry.rs).
    if matches!(effective, "done" | "finished")
        && super::antigravity_background_wait(project_id, session_id)
    {
        if records_changed {
            let _ = schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
        }
        return;
    }
    // Detection owns idle. A program's idle after its own
    // working or blocked hands the session to detection's view (its
    // shadow, which may still say working); a finished turn is `finished`
    // above.
    if effective == "idle" && matches!(entry.applied_state.as_deref(), Some("working" | "blocked"))
    {
        let entry = program_status_entry(project_id, session_id).unwrap_or(entry);
        hand_back_to_detection(state, project_id, session_id, &entry);
        return;
    }
    if effective == "idle" {
        if records_changed {
            let _ = schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                project_id,
                session_id,
            );
        }
        return;
    }
    let (activity, attention_source) = program_status_activity(effective);
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("programStatusActivity".to_string(), json!(activity));
    if let Some(source) = attention_source {
        params.insert("attentionSource".to_string(), json!(source));
    }
    /*
    CDXC:SessionStatus 2026-10-15 WHY:
    The same report applied again (a Combined -> Detection -> Combined round trip on a program that is still blocked, done or failed) brings its attention back under the event id it was announced with, and skips the notification feed: switching modes must not notify the user twice about one state (the regression test saw two "Waiting for your input" rows for one block).
    */
    let reused_event_id = entry
        .notified
        .as_ref()
        .filter(|(notified_key, _)| *notified_key == key)
        .map(|(_, event_id)| event_id.clone());
    if let Some(event_id) = reused_event_id.as_ref() {
        params.insert("reuseAttentionEventId".to_string(), json!(event_id));
    }
    let endpoint = "/api/updateAgentActivity";
    let Ok(output) = dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        endpoint,
        &params,
        None,
    ) else {
        return;
    };
    log_agent_activity_transition(state, endpoint, &params, &output.result);
    if reused_event_id.is_none() {
        crate::notification_feed::observe_agent_activity_result(
            state,
            endpoint,
            &params,
            &output.result,
        );
        crate::session_chat_question_resend::observe_turn_end(state, &params, &output.result);
        crate::session_chat_open_questions::observe_activity(state, &params, &output.result);
        if let Some(event_id) = output
            .result
            .pointer("/activity/attentionEventId")
            .and_then(Value::as_str)
            .filter(|_| activity == "attention")
        {
            set_program_status_notified(project_id, session_id, key.clone(), event_id.to_string());
        }
    }
    let _ = schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
        schedule_stale_activity_presentation_refresh(state, &session, "program-status");
    }
}

enum FinishedTurnStep {
    /// working → idle on an agent session the user did not interrupt: finished.
    Promote,
    /// The idle that followed a promoted finish, re-sent: keep the finished mark.
    StillFinished,
    None,
}

/// CDXC:SessionStatus 2026-10-15 DECISION:
/// User (Q116): "treat an agent's working → idle as finished" (blue finished dot until the user looks), so an agent that ends its turn with `idle` instead of `done` (Codex 0.163 does) still shows as finished. Only for agent sessions, only for a working stint that began after input was submitted to the session since the previous working stint began (an Enter from the terminal, a chat send, an orchestrator message, `ghostex send-text`; the daemon counts them), so a program's own start-up (Codex 0.163 reports idle, working while it loads, then idle) is never a turn whatever its length, only while the program is still there (its idle record, not an empty list after it exited), and not when the user interrupted the turn from Ghostex: Escape in the terminal pane and Stop in the chat both send `escape`, which `compute_activity_update` notes for the session. A Ctrl-C typed straight into the terminal cannot be told apart and does count as finished.
fn finished_turn_step(
    entry: &super::SessionProgramStatus,
    derived: &super::DerivedProgramStatus,
    session: &Value,
) -> FinishedTurnStep {
    if derived.state != "idle" || derived.record.is_none() {
        return FinishedTurnStep::None;
    }
    match entry.applied_state.as_deref() {
        Some("finished") => FinishedTurnStep::StillFinished,
        Some("working")
            if entry.working_after_input && !entry.interrupted && is_agent_session(session) =>
        {
            FinishedTurnStep::Promote
        }
        _ => FinishedTurnStep::None,
    }
}

fn is_agent_session(session: &Value) -> bool {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    text("kind") == Some("agent") || text("agentId").is_some()
}

/// A finished or failed record that was already there when gxserver first
/// saw it (after a gxserver restart, or the setting turned on) and that the
/// user acknowledged after it was reported is not news: do not raise it again.
fn already_seen(session: &Value, state: &str, record: Option<&ProgramStatusRecord>) -> bool {
    if !matches!(state, "done" | "error") {
        return false;
    }
    let Some(record) = record else {
        return false;
    };
    let activity = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("agentActivity"));
    let is = |key: &str| activity.and_then(|value| value.get(key));
    is("activity").and_then(Value::as_str) == Some("idle")
        && is("isAcknowledged").and_then(Value::as_bool) == Some(true)
        && is("lastChangedAt")
            .and_then(Value::as_str)
            .and_then(parse_iso_ms)
            .is_some_and(|acknowledged_at| acknowledged_at >= record.updated_at_ms)
}

/// Log the disagreements between Ghostex's detection and a program's own
/// report that outlasted their settle window.
pub(crate) async fn run_program_status_watch(_state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        log_settled_program_status_mismatches();
    }
}
