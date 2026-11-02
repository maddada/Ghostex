//! The supervisor for threads on another computer: every three seconds it reads those computers'
//! sessions over the connection the desktop app keeps to them, and tells each coordinator here when
//! its thread there finished a turn, started waiting on an answer, or was closed, with the same
//! reports a thread on this computer sends.
//!
//! CDXC:RemoteMachines 2026-11-03 WHY:
//! The other computer's gxserver cannot reach this one on its own (the desktop app's tunnel only goes out) and may predate threads on another computer, so this side watches: it reads the other computer's presentation snapshot, which every gxserver serves, and that computer's chat for the final message or the question. Nothing is installed or changed there, and a report is marked delivered only after the coordinator took it, as for local threads.
//! SEE-ALSO: server/src/coordinators/remote_threads.rs (the link and bookkeeping), server/src/server/coordinator_runtime.rs (the same supervisor for local threads), server/src/connected_computers/ (how the other computer is reached).

use super::*;
use std::collections::BTreeMap;

use crate::connected_computers::ConnectedComputer;
use crate::coordinators::{
    agent_message, list_remote_threads, prompt_summary, record_remote_thread_progress,
    record_remote_thread_report, report_body, set_remote_thread_resolved, MessageSender,
    RemoteThreadKey, RemoteThreadRecord, ThreadReport,
};
use crate::session_chat_interactive::SessionChatInteractivePrompt;

const TICK: Duration = Duration::from_secs(3);
/// As for local threads: hooks dip between tool calls, so a turn counts as finished only after
/// the thread stayed out of work this long.
const FINISH_STABILITY_MS: i64 = 4_000;
const REMOTE_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// What one pass saw of a thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RemoteObservation {
    Working,
    /// A question or approval waits on its chat.
    Waiting,
    /// Out of work (idle or done with its turn).
    Idle,
    Sleeping,
    /// Its session was closed or is gone.
    Closed,
}

impl RemoteObservation {
    fn as_state(&self, observed_working: bool) -> &'static str {
        match self {
            RemoteObservation::Working => "working",
            RemoteObservation::Waiting => "waiting",
            RemoteObservation::Idle if observed_working => "working",
            RemoteObservation::Idle => "finished",
            RemoteObservation::Sleeping => "sleeping",
            RemoteObservation::Closed => "closed",
        }
    }
}

/// Classifies a thread from its row in the other computer's presentation snapshot.
pub(super) fn observe_remote_session(session: Option<&Value>) -> RemoteObservation {
    let Some(session) = session else {
        return RemoteObservation::Closed;
    };
    match session
        .get("lifecycleState")
        .and_then(Value::as_str)
        .unwrap_or("running")
    {
        "running" => {}
        "sleeping" => return RemoteObservation::Sleeping,
        _ => return RemoteObservation::Closed,
    }
    let waiting = session
        .get("pendingQuestionCount")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
        || session.get("awaitingApproval").and_then(Value::as_bool) == Some(true);
    if waiting {
        return RemoteObservation::Waiting;
    }
    if session.get("activity").and_then(Value::as_str) == Some("working") {
        RemoteObservation::Working
    } else {
        RemoteObservation::Idle
    }
}

/// The id and text of the last assistant message in a `readSessionChat` answer.
pub(super) fn last_assistant_message(chat: &Value) -> Option<(Option<String>, String)> {
    chat.get("messages")
        .and_then(Value::as_array)?
        .iter()
        .rev()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
        .filter(|message| message.get("queued").and_then(Value::as_bool) != Some(true))
        .find_map(|message| {
            let text = message
                .get("blocks")
                .and_then(Value::as_array)?
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            (!text.is_empty()).then(|| {
                (
                    message
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    text,
                )
            })
        })
}

/// When the row last moved: the later of its meaningful-activity and last-active times. It only
/// moves forward, so a turn that ended between two passes still shows as newer than the last
/// report.
pub(super) fn activity_marker_ms(session: &Value) -> Option<i64> {
    ["meaningfulActivityAt", "lastActiveAt"]
        .iter()
        .filter_map(|key| session.get(*key).and_then(Value::as_str))
        .filter_map(iso_ms)
        .max()
}

fn iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|time| time.timestamp_millis())
}

/// The question a `readSessionChat` answer shows, with the key it is reported once by.
pub(super) fn waiting_prompt(chat: &Value) -> (String, String) {
    match chat.get("prompt") {
        Some(prompt) if !prompt.is_null() => {
            let key = {
                use sha2::{Digest, Sha256};
                hex::encode(Sha256::digest(prompt.to_string().as_bytes()))
                    .chars()
                    .take(16)
                    .collect::<String>()
            };
            let summary = serde_json::from_value::<SessionChatInteractivePrompt>(prompt.clone())
                .map(|prompt| prompt_summary(&prompt))
                .unwrap_or_else(|_| "It is waiting on an answer in its chat.".to_string());
            (format!("prompt:{key}"), summary)
        }
        _ => (
            "remote-question".to_string(),
            "It asked a question in its chat.".to_string(),
        ),
    }
}

#[derive(Default)]
pub(super) struct RemoteSupervisorMemory {
    pub(super) not_working_since: HashMap<RemoteThreadKey, i64>,
    pub(super) in_flight: HashSet<RemoteThreadKey>,
    /// The activity marker whose chat was last read for a finished turn, so an idle thread's chat
    /// is read once per change, not every pass.
    pub(super) checked_marker: HashMap<RemoteThreadKey, i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RemoteReport {
    Finished,
    Waiting,
    Closed,
}

pub(super) struct PendingRemoteReport {
    pub(super) thread: RemoteThreadRecord,
    computer: ConnectedComputer,
    session: Option<Value>,
    pub(super) kind: RemoteReport,
    marker: Option<i64>,
}

pub(crate) fn start_coordinator_remote_runtime(state: Arc<AppState>) {
    let memory = Arc::new(Mutex::new(RemoteSupervisorMemory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let ticking = state.clone();
            let ticking_memory = memory.clone();
            let reports = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for report in reports {
                tokio::spawn(deliver(state.clone(), memory.clone(), report));
            }
        }
    });
}

fn remote_rpc(computer: &ConnectedComputer, path: &str, params: Value) -> Option<Value> {
    let body = json!({ "params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION });
    let response = crate::http_agent::answering_agent()
        .post(format!("{}{path}", computer.base_url))
        .header("authorization", format!("Bearer {}", computer.token()))
        .header("content-type", "application/json")
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(REMOTE_READ_TIMEOUT))
        .build()
        .send(body.to_string())
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let envelope: Value = crate::http_agent::read_json(response).ok()?;
    envelope.get("result").cloned()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(super) fn tick(
    state: &AppState,
    memory: &Mutex<RemoteSupervisorMemory>,
) -> Vec<PendingRemoteReport> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let Ok(threads) = list_remote_threads(&db) else {
        return Vec::new();
    };
    let open: Vec<RemoteThreadRecord> = threads
        .into_iter()
        .filter(|thread| thread.resolved_at.is_none())
        .filter(|thread| {
            super::coordinator_runtime::coordinator_is_open(state, &thread.coordinator_key())
        })
        .collect();
    if open.is_empty() {
        return Vec::new();
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut by_computer: BTreeMap<String, Vec<RemoteThreadRecord>> = BTreeMap::new();
    for thread in open {
        by_computer
            .entry(thread.server_id.clone())
            .or_default()
            .push(thread);
    }
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let now = now_ms();
    let mut reports = Vec::new();
    for (server_id, threads) in by_computer {
        // A computer that is not connected (or whose Agent access is off) is skipped, not judged:
        // its threads keep their state until it is back.
        let computer =
            crate::connected_computers::find(&server_id).filter(|computer| computer.agent_access);
        let snapshot = computer
            .as_ref()
            .and_then(|computer| remote_rpc(computer, "/api/readPresentationSnapshot", json!({})));
        let (Some(computer), Some(snapshot)) = (computer, snapshot) else {
            for thread in &threads {
                if record_remote_thread_progress(
                    &db,
                    &thread.key(),
                    thread.observed_working,
                    "unreachable",
                )
                .unwrap_or(false)
                {
                    let (project_id, session_id) = thread.coordinator_key();
                    let _ = schedule_presentation_session_delta(
                        state,
                        &db,
                        &repository,
                        &project_id,
                        &session_id,
                    );
                }
            }
            continue;
        };
        let sessions = snapshot
            .pointer("/snapshot/sessions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for thread in threads {
            let key = thread.key();
            if memory.in_flight.contains(&key) {
                continue;
            }
            let session = sessions
                .iter()
                .find(|session| {
                    session.get("projectId").and_then(Value::as_str) == Some(&thread.project_id)
                        && session.get("sessionId").and_then(Value::as_str)
                            == Some(&thread.session_id)
                })
                .cloned();
            if let Some(title) = session
                .as_ref()
                .and_then(|session| session.get("title"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
            {
                let _ = crate::coordinators::set_remote_thread_title(&db, &key, title);
            }
            let mut observation = observe_remote_session(session.as_ref());
            // A row missing from the snapshot is closed only once its computer's session list
            // says so (stopped, or gone); a snapshot can leave out a running session.
            if session.is_none() {
                observation = match confirm_missing_session(&computer, &thread) {
                    Some(observation) => observation,
                    None => continue,
                };
            }
            let marker = session.as_ref().and_then(activity_marker_ms);
            let mut observed_working = thread.observed_working;
            let report = match observation {
                RemoteObservation::Working => {
                    observed_working = true;
                    memory.not_working_since.remove(&key);
                    None
                }
                RemoteObservation::Waiting => {
                    memory.not_working_since.remove(&key);
                    Some(RemoteReport::Waiting)
                }
                RemoteObservation::Idle
                    if thread.observed_working
                        || activity_moved_since_report(&thread, marker, &memory, &key) =>
                {
                    let since = *memory.not_working_since.entry(key.clone()).or_insert(now);
                    (now - since >= FINISH_STABILITY_MS).then_some(RemoteReport::Finished)
                }
                RemoteObservation::Closed => Some(RemoteReport::Closed),
                RemoteObservation::Idle | RemoteObservation::Sleeping => None,
            };
            if record_remote_thread_progress(
                &db,
                &key,
                observed_working,
                observation.as_state(observed_working),
            )
            .unwrap_or(false)
            {
                let (project_id, session_id) = thread.coordinator_key();
                let _ = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                );
            }
            if let Some(kind) = report {
                memory.in_flight.insert(key.clone());
                memory.not_working_since.remove(&key);
                let mut thread = thread;
                thread.observed_working = observed_working;
                reports.push(PendingRemoteReport {
                    thread,
                    computer: computer.clone(),
                    session,
                    kind,
                    marker,
                });
            }
        }
    }
    reports
}

/// What a finished report carries: the new reply's id and text, `(None, None)` for a turn seen
/// working that left no new reply, or `None` when there is nothing to report (the row moved, but
/// its last reply is the one already reported and no work was seen: a rename or a read).
pub(super) fn finished_reply(
    last: Option<(Option<String>, String)>,
    reported_message_id: Option<&str>,
    observed_working: bool,
) -> Option<(Option<String>, Option<String>)> {
    match last {
        Some((id, text)) if id.is_none() || id.as_deref() != reported_message_id => {
            Some((id, Some(text)))
        }
        _ if observed_working => Some((None, None)),
        _ => None,
    }
}

/// The row moved after the last finished report and its chat was not read at that marker yet.
fn activity_moved_since_report(
    thread: &RemoteThreadRecord,
    marker: Option<i64>,
    memory: &RemoteSupervisorMemory,
    key: &RemoteThreadKey,
) -> bool {
    let Some(marker) = marker else {
        return false;
    };
    let reported = thread.reported_at.as_deref().and_then(iso_ms);
    reported.is_none_or(|reported| marker > reported)
        && memory.checked_marker.get(key) != Some(&marker)
}

/// What a thread missing from the snapshot is: closed when its computer's session list has it
/// stopped or not at all, sleeping or idle when it is still there; `None` when the list could not
/// be read, so nothing is judged this pass.
fn confirm_missing_session(
    computer: &ConnectedComputer,
    thread: &RemoteThreadRecord,
) -> Option<RemoteObservation> {
    let listed = remote_rpc(
        computer,
        "/api/listSessions",
        json!({ "projectId": thread.project_id, "includeStopped": true }),
    )?;
    let row = listed
        .get("sessions")
        .and_then(Value::as_array)?
        .iter()
        .find(|row| row.get("sessionId").and_then(Value::as_str) == Some(&thread.session_id))
        .cloned();
    Some(match row {
        None => RemoteObservation::Closed,
        Some(row) => match crate::presentation::effective_lifecycle_state(&row).as_str() {
            "running" => RemoteObservation::Idle,
            "sleeping" => RemoteObservation::Sleeping,
            _ => RemoteObservation::Closed,
        },
    })
}

fn sender_for(report: &PendingRemoteReport) -> MessageSender {
    let text = |key: &str| {
        report
            .session
            .as_ref()
            .and_then(|session| session.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let title = Some(text("title"))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| report.thread.title.clone());
    MessageSender {
        agent_name: text("agentName"),
        title: format!("{title} (on {})", report.computer.name),
        session_id: report.thread.session_id.clone(),
        agent_id: text("agentId"),
        agent_session_id: text("agentSessionId"),
        global_ref: report.thread.global_ref(),
    }
}

async fn deliver(
    state: Arc<AppState>,
    memory: Arc<Mutex<RemoteSupervisorMemory>>,
    report: PendingRemoteReport,
) {
    let key = report.thread.key();
    let thread_ref = report.thread.global_ref();
    // Waiting reports a question once per prompt; a finished one says the turn's last reply.
    let computer = report.computer.clone();
    let chat_params = json!({
        "projectId": report.thread.project_id,
        "sessionId": report.thread.session_id,
        "limit": 30,
    });
    let chat = match report.kind {
        RemoteReport::Closed => None,
        _ => tokio::task::spawn_blocking(move || {
            remote_rpc(&computer, "/api/readSessionChat", chat_params)
        })
        .await
        .ok()
        .flatten(),
    };
    let mut message_id = None;
    let (body, prompt_key, excerpt) = match report.kind {
        RemoteReport::Finished => {
            if let (Some(marker), Ok(mut memory)) = (report.marker, memory.lock()) {
                memory.checked_marker.insert(key.clone(), marker);
            }
            let Some(reply) = finished_reply(
                chat.as_ref().and_then(last_assistant_message),
                report.thread.reported_message_id.as_deref(),
                report.thread.observed_working,
            ) else {
                if let Ok(mut memory) = memory.lock() {
                    memory.in_flight.remove(&key);
                }
                return;
            };
            message_id = reply.0;
            let message = reply.1;
            (
                report_body(
                    &ThreadReport::Finished {
                        message: message.as_deref(),
                    },
                    &thread_ref,
                ),
                None,
                message.unwrap_or_default(),
            )
        }
        RemoteReport::Waiting => {
            let (prompt_key, summary) = chat
                .as_ref()
                .map(waiting_prompt)
                .unwrap_or_else(|| waiting_prompt(&Value::Null));
            if report.thread.reported_prompt_key.as_deref() == Some(prompt_key.as_str()) {
                if let Ok(mut memory) = memory.lock() {
                    memory.in_flight.remove(&key);
                }
                return;
            }
            (
                report_body(&ThreadReport::Waiting { prompt: &summary }, &thread_ref),
                Some(prompt_key),
                summary,
            )
        }
        RemoteReport::Closed => (
            report_body(&ThreadReport::Closed, &thread_ref),
            None,
            String::new(),
        ),
    };
    let message = agent_message(&sender_for(&report), &body);
    let request_id = format!(
        "remote-thread:{}:{}:{}:{}",
        key.0,
        key.1,
        key.2,
        prompt_key
            .clone()
            .or_else(|| message_id.clone())
            .or_else(|| report.thread.reported_at.clone())
            .unwrap_or_else(|| "first".to_string())
    );
    let sent = super::coordinator_runtime::send_to_coordinator(
        &state,
        &report.thread.coordinator_key(),
        &message,
        &request_id,
    )
    .await
    .is_ok();
    let closed = matches!(report.kind, RemoteReport::Closed);
    let settle_state = state.clone();
    let coordinator = report.thread.coordinator_key();
    let _ = tokio::task::spawn_blocking(move || {
        if !sent {
            return;
        }
        let Ok(db) = open_gxserver_database(&settle_state.paths) else {
            return;
        };
        if closed {
            let _ = set_remote_thread_resolved(&db, &key, true);
        } else {
            let _ = record_remote_thread_report(
                &db,
                &key,
                prompt_key.as_deref(),
                message_id.as_deref(),
                &excerpt,
            );
        }
        let repository = DomainRepository::new(&db, settle_state.metadata.server_id.as_str());
        let _ = schedule_presentation_session_delta(
            &settle_state,
            &db,
            &repository,
            &coordinator.0,
            &coordinator.1,
        );
    })
    .await;
    if let Ok(mut memory) = memory.lock() {
        memory.in_flight.remove(&report.thread.key());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_on_another_computer_is_read_from_its_presentation_row() {
        assert_eq!(observe_remote_session(None), RemoteObservation::Closed);
        let row = |value: Value| observe_remote_session(Some(&value));
        assert_eq!(
            row(json!({"lifecycleState": "running", "activity": "working"})),
            RemoteObservation::Working
        );
        assert_eq!(
            row(
                json!({"lifecycleState": "running", "activity": "attention", "pendingQuestionCount": 1})
            ),
            RemoteObservation::Waiting
        );
        assert_eq!(
            row(json!({"lifecycleState": "running", "activity": "idle", "awaitingApproval": true})),
            RemoteObservation::Waiting
        );
        assert_eq!(
            row(json!({"lifecycleState": "running", "activity": "attention"})),
            RemoteObservation::Idle
        );
        assert_eq!(
            row(json!({"lifecycleState": "sleeping"})),
            RemoteObservation::Sleeping
        );
        assert_eq!(
            row(json!({"lifecycleState": "stopped"})),
            RemoteObservation::Closed
        );
        assert_eq!(RemoteObservation::Idle.as_state(true), "working");
        assert_eq!(RemoteObservation::Idle.as_state(false), "finished");
    }

    #[test]
    fn the_report_carries_the_last_reply_or_the_question() {
        let chat = json!({"messages": [
            {"role": "user", "blocks": [{"type": "text", "text": "Do it"}]},
            {"role": "assistant", "blocks": [{"type": "text", "text": "First"}]},
            {"role": "tool", "blocks": [{"type": "tool-result", "output": "x"}]},
            {"id": "m2", "role": "assistant", "blocks": [{"type": "text", "text": "Done: all green."}]},
            {"role": "assistant", "queued": true, "blocks": [{"type": "text", "text": "not yet"}]},
        ]});
        assert_eq!(
            last_assistant_message(&chat),
            Some((Some("m2".to_string()), "Done: all green.".to_string()))
        );
        // The activity marker is the later of the two times, so it only moves forward.
        assert_eq!(
            activity_marker_ms(&json!({
                "meaningfulActivityAt": "2026-11-03T10:00:00.000Z",
                "lastActiveAt": "2026-11-03T10:00:05.000Z",
            })),
            Some(1_793_700_005_000)
        );
        assert_eq!(activity_marker_ms(&json!({})), None);
    }

    #[test]
    fn a_new_reply_is_reported_once_whether_or_not_a_pass_saw_the_turn_work() {
        let reply = |id: &str| Some((Some(id.to_string()), format!("reply {id}")));
        // A short turn that ended between two passes: never seen working, a new reply.
        assert_eq!(
            finished_reply(reply("m2"), Some("m1"), false),
            Some((Some("m2".to_string()), Some("reply m2".to_string())))
        );
        // The same reply again: nothing new to say.
        assert_eq!(finished_reply(reply("m2"), Some("m2"), false), None);
        // Seen working but no new reply: a finished report without its message.
        assert_eq!(
            finished_reply(reply("m2"), Some("m2"), true),
            Some((None, None))
        );
        assert_eq!(finished_reply(None, None, false), None);
        // The first reply of a thread that never reported.
        assert_eq!(
            finished_reply(reply("m1"), None, false),
            Some((Some("m1".to_string()), Some("reply m1".to_string())))
        );
        assert_eq!(last_assistant_message(&json!({"messages": []})), None);
        let (first_key, summary) = waiting_prompt(&json!({"prompt": {"kind": "unknown"}}));
        assert!(first_key.starts_with("prompt:"));
        assert!(!summary.is_empty());
        let (second_key, _) = waiting_prompt(&json!({"prompt": {"kind": "other"}}));
        assert_ne!(first_key, second_key, "a new question is reported again");
        assert_eq!(waiting_prompt(&json!({})).0, "remote-question");
    }
}
