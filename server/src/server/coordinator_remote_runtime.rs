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
    record_remote_thread_report, remote_chat_last_row, remote_chat_reading,
    remote_screen_fingerprint, remote_undelivered_messages, report_body,
    set_remote_thread_resolved, MessageSender, RemoteStallDue, RemoteStallWatch, RemoteThreadKey,
    RemoteThreadRecord, RemoteUndelivered, StallNudge, ThreadReport, REMOTE_STALL_CHAT_ROWS,
    REMOTE_UNDELIVERED_CHECK_EVERY_MS,
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
    let waiting = ghostex_gx_protocol::session_has_question(
        session
            .get("pendingQuestionCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        session.get("awaitingAnswer").and_then(Value::as_bool) == Some(true),
    ) || session.get("awaitingApproval").and_then(Value::as_bool) == Some(true);
    if waiting {
        return RemoteObservation::Waiting;
    }
    if session.get("activity").and_then(Value::as_str) == Some("working") {
        RemoteObservation::Working
    } else {
        RemoteObservation::Idle
    }
}

/// The agent of a row in the other computer's presentation snapshot.
fn remote_session_agent(session: &Value) -> Option<&str> {
    session
        .get("agentId")
        .or_else(|| session.get("agentName"))
        .and_then(Value::as_str)
}

/// Whether an OpenCode thread on another computer is still running its turn by OpenCode's own
/// record: the chat's `lifecycle`, which that computer's gxserver reads from OpenCode's service.
///
/// CDXC:RemoteMachines 2026-11-10 WHY:
/// GitHub #538: a computer on Ghostex 11.2.1 reads OpenCode's working only from its TUI plugin, which reports nothing while the TUI shows another page, so its row said idle and the coordinator here listed the thread as Finished for the 15 minutes it went on working. That computer's own fix (opencode_turn_activity.rs, which reports working from the service) needs it updated; until then this side checks the service's record through the chat before it reports such a thread finished. The check runs only on the pass that would report it, and an unreadable chat leaves the report as it was.
fn remote_opencode_turn_running(computer: &ConnectedComputer, thread: &RemoteThreadRecord) -> bool {
    remote_rpc(
        computer,
        "/api/readSessionChat",
        json!({ "projectId": thread.project_id, "sessionId": thread.session_id, "limit": 1 }),
    )
    .is_some_and(|chat| chat.pointer("/lifecycle/state").and_then(Value::as_str) == Some("working"))
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
    /// Each working thread's output clock (coordinators/remote_stall.rs).
    pub(super) stalls: HashMap<RemoteThreadKey, RemoteStallWatch>,
    /// When an idle thread's queue there was last read for messages it marked Not sent
    /// (coordinators/remote_undelivered.rs).
    pub(super) queue_checked_at: HashMap<RemoteThreadKey, i64>,
    /// The queue rows already reported as not delivered. In memory only: after a restart a row is
    /// reported under the same send id, which the coordinator's send ledger types once.
    pub(super) undelivered_reported: HashSet<(RemoteThreadKey, String)>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RemoteReport {
    Finished,
    Waiting,
    Closed,
    /// Working with no new output for a round, its transcript's turn still open: the first round
    /// nudges the thread.
    Stalled {
        due: RemoteStallDue,
    },
    /// Working by its hooks, but no output for a round and its transcript's turn ended: a finished
    /// turn the other computer's hooks never reported (issue #475).
    TurnEnded {
        due: RemoteStallDue,
    },
    /// A message its coordinator sent that the other computer's queue marked Not sent.
    Undelivered(RemoteUndelivered),
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
        let mut clock = crate::ticks::steady_interval(TICK);
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

pub(super) fn remote_rpc(computer: &ConnectedComputer, path: &str, params: Value) -> Option<Value> {
    // The grant goes only to a listener of this account: once the tunnel drops, its port could be
    // someone else's (ghostex_paths::loopback_owner, as relay.rs and settings_sync do).
    ghostex_paths::loopback_owner::check_url(&computer.base_url).ok()?;
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
    tick_at(state, memory, now_ms())
}

/// One pass at `now` (ms): `tick`, with the clock a test can move.
pub(super) fn tick_at(
    state: &AppState,
    memory: &Mutex<RemoteSupervisorMemory>,
    now: i64,
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
    let Ok(mut guard) = memory.lock() else {
        return Vec::new();
    };
    let mut reports = Vec::new();
    for (server_id, threads) in by_computer {
        // A computer that is not connected (or whose Agent access is off) is skipped, not judged:
        // its threads keep their state until it is back.
        let computer =
            crate::connected_computers::find(&server_id).filter(|computer| computer.agent_access);
        // Only this computer's threads (`onlySessions`, route_http/sessions.rs); an older gxserver
        // sends its whole snapshot, which is searched the same way.
        let only_sessions: Vec<Value> = threads
            .iter()
            .map(|thread| json!({ "projectId": thread.project_id, "sessionId": thread.session_id }))
            .collect();
        let snapshot = computer.as_ref().and_then(|computer| {
            remote_rpc(
                computer,
                "/api/readPresentationSnapshot",
                json!({ "onlySessions": only_sessions }),
            )
        });
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
            if guard.in_flight.contains(&key) {
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
            // A working thread's output clock reads its chat and screen there; a turn its
            // transcript ended and that was reported is finished until its rows move.
            if observation == RemoteObservation::Working {
                let mut watch = guard
                    .stalls
                    .remove(&key)
                    .unwrap_or_else(|| RemoteStallWatch::new(now));
                // The reads go over the network: deliveries may take the lock meanwhile.
                drop(guard);
                check_remote_stall(&computer, &thread, &mut watch, now);
                guard = memory.lock().unwrap_or_else(|error| error.into_inner());
                if watch.finished() {
                    observation = RemoteObservation::Idle;
                }
                guard.stalls.insert(key.clone(), watch);
            } else {
                guard.stalls.remove(&key);
            }
            let marker = session.as_ref().and_then(activity_marker_ms);
            // A thread linked as it was (or reported before 0053) has no time of the other
            // computer's yet: this pass sets one, from that computer's own clock.
            if let (None, Some(_), Some(marker)) = (
                thread.checked_activity_ms,
                thread.reported_at.as_ref(),
                marker,
            ) {
                let _ = crate::coordinators::record_remote_thread_checked(&db, &key, marker);
            }
            let mut observed_working = thread.observed_working;
            let report = match observation {
                RemoteObservation::Working => {
                    observed_working = true;
                    guard.not_working_since.remove(&key);
                    guard
                        .stalls
                        .get(&key)
                        .and_then(|watch| watch.due(now))
                        .map(|due| {
                            if due.turn_ended {
                                RemoteReport::TurnEnded { due }
                            } else {
                                RemoteReport::Stalled { due }
                            }
                        })
                }
                RemoteObservation::Waiting => {
                    guard.not_working_since.remove(&key);
                    Some(RemoteReport::Waiting)
                }
                RemoteObservation::Idle
                    if thread.observed_working || activity_moved_since_check(&thread, marker) =>
                {
                    let since = *guard.not_working_since.entry(key.clone()).or_insert(now);
                    (now - since >= FINISH_STABILITY_MS).then_some(RemoteReport::Finished)
                }
                RemoteObservation::Closed => Some(RemoteReport::Closed),
                RemoteObservation::Idle | RemoteObservation::Sleeping => None,
            };
            let report = if matches!(report, Some(RemoteReport::Finished))
                && session
                    .as_ref()
                    .is_some_and(|session| remote_session_agent(session) == Some("opencode"))
            {
                // The read goes over the network: deliveries may take the lock meanwhile.
                drop(guard);
                let still_running = remote_opencode_turn_running(&computer, &thread);
                guard = memory.lock().unwrap_or_else(|error| error.into_inner());
                if still_running {
                    observed_working = true;
                    guard.not_working_since.remove(&key);
                    None
                } else {
                    report
                }
            } else {
                report
            };
            let report = match report {
                None if observation == RemoteObservation::Idle
                    && guard.queue_checked_at.get(&key).is_none_or(|checked| {
                        now - checked >= REMOTE_UNDELIVERED_CHECK_EVERY_MS
                    }) =>
                {
                    guard.queue_checked_at.insert(key.clone(), now);
                    // The read goes over the network: deliveries may take the lock meanwhile.
                    drop(guard);
                    let failed = remote_rpc(
                        &computer,
                        "/api/readSessionChatQueue",
                        json!({ "projectId": thread.project_id, "sessionId": thread.session_id }),
                    )
                    .map(|queue| {
                        let (project_id, session_id) = thread.coordinator_key();
                        remote_undelivered_messages(
                            &queue,
                            &crate::ids::create_global_session_ref(
                                state.metadata.server_id.as_str(),
                                &project_id,
                                &session_id,
                            ),
                        )
                    })
                    .unwrap_or_default();
                    guard = memory.lock().unwrap_or_else(|error| error.into_inner());
                    failed
                        .into_iter()
                        .find(|row| {
                            !guard
                                .undelivered_reported
                                .contains(&(key.clone(), row.row_id.clone()))
                        })
                        .map(RemoteReport::Undelivered)
                }
                report => report,
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
                guard.in_flight.insert(key.clone());
                guard.not_working_since.remove(&key);
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

/// BLOCKING (one or two reads of the other computer, at most every 30 seconds): feeds a working
/// thread's output clock its chat there and, once quiet, its screen.
fn check_remote_stall(
    computer: &ConnectedComputer,
    thread: &RemoteThreadRecord,
    watch: &mut RemoteStallWatch,
    now: i64,
) {
    if crate::network_status::is_offline() {
        watch.hold(now);
        return;
    }
    if !watch.check_due(now) {
        return;
    }
    let ids = json!({ "projectId": thread.project_id, "sessionId": thread.session_id });
    let mut chat_params = ids.clone();
    chat_params["limit"] = json!(REMOTE_STALL_CHAT_ROWS);
    let chat = remote_rpc(computer, "/api/readSessionChat", chat_params);
    let reading = remote_chat_reading(chat.as_ref());
    let screen = if reading.rows.is_some() && watch.wants_screen(now) {
        remote_rpc(computer, "/api/readSessionTerminalTail", ids)
            .as_ref()
            .and_then(remote_screen_fingerprint)
    } else {
        None
    };
    watch.record(now, reading, screen);
}

/// The message a quiet thread on another computer is nudged with, typed there under a send id
/// that is the same for every try in one quiet stretch, so a retried report never types it twice.
fn nudge_remote_thread(
    computer: &ConnectedComputer,
    thread: &RemoteThreadRecord,
    due: &RemoteStallDue,
) -> std::result::Result<(), String> {
    let params = json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "text": crate::coordinators::nudge_message(),
        "sendRequestId": format!(
            "remote-nudge:{}:{}:{}:{}",
            thread.server_id, thread.project_id, thread.session_id, due.quiet_since
        ),
    });
    remote_rpc(computer, "/api/sendSessionChatMessage", params)
        .map(|_| ())
        .ok_or_else(|| format!("{} did not take the message.", computer.name))
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

/// The row moved since the last finished check, judged on the other computer's own clock: its
/// activity time is newer than the one that check covered. A thread never checked or reported
/// counts as moved; one reported but not yet checked waits for the baseline `tick` sets.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// Comparing the other computer's activity time with `reportedAt` from this computer's clock missed a short turn whenever the other computer's clock was behind, so both sides of the comparison now come from that computer.
pub(super) fn activity_moved_since_check(thread: &RemoteThreadRecord, marker: Option<i64>) -> bool {
    let Some(marker) = marker else {
        return false;
    };
    match thread.checked_activity_ms {
        Some(checked) => marker > checked,
        None => thread.reported_at.is_none(),
    }
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
        // A round of quiet is checked against the same page its watch hashed.
        "limit": match report.kind {
            RemoteReport::Stalled { .. } | RemoteReport::TurnEnded { .. } => REMOTE_STALL_CHAT_ROWS,
            _ => 30,
        },
    });
    let chat = match report.kind {
        RemoteReport::Closed | RemoteReport::Undelivered(_) => None,
        _ => tokio::task::spawn_blocking(move || {
            remote_rpc(&computer, "/api/readSessionChat", chat_params)
        })
        .await
        .ok()
        .flatten(),
    };
    // A round of quiet is dropped when the thread wrote a row since it was judged.
    let stall_due = match &report.kind {
        RemoteReport::Stalled { due } | RemoteReport::TurnEnded { due } => Some(*due),
        _ => None,
    };
    if let Some(due) = stall_due {
        if remote_chat_reading(chat.as_ref()).rows != due.rows {
            if let Ok(mut memory) = memory.lock() {
                memory.in_flight.remove(&key);
            }
            return;
        }
    }
    let mut message_id = None;
    let (body, prompt_key, excerpt) = match report.kind {
        RemoteReport::Stalled { due } => {
            let nudge = if due.round == 1 {
                let (computer, thread) = (report.computer.clone(), report.thread.clone());
                let sent = tokio::task::spawn_blocking(move || {
                    nudge_remote_thread(&computer, &thread, &due)
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
                if sent.is_ok() {
                    if let Ok(mut memory) = memory.lock() {
                        if let Some(watch) = memory.stalls.get_mut(&key) {
                            watch.note_nudged(now_ms());
                        }
                    }
                }
                Some(sent)
            } else {
                None
            };
            let last_row = chat.as_ref().and_then(remote_chat_last_row);
            (
                report_body(
                    &ThreadReport::Stalled {
                        // Whole rounds, so a retried report is the same text (and send).
                        quiet_minutes: crate::coordinators::minutes_of(
                            i64::from(due.round) * crate::coordinators::quiet_round_ms(),
                        ),
                        quiet_since: &crate::coordinators::iso_from_ms(due.quiet_since)
                            .unwrap_or_default(),
                        last_row: last_row.as_deref(),
                        nudge: match &nudge {
                            Some(Ok(())) => StallNudge::Sent,
                            Some(Err(error)) => StallNudge::Failed(error),
                            None => StallNudge::Earlier,
                        },
                        // No red dot or notification here: its row is on the other computer.
                        stuck: false,
                    },
                    &thread_ref,
                ),
                None,
                String::new(),
            )
        }
        RemoteReport::Finished | RemoteReport::TurnEnded { .. } => {
            // This activity time is covered now, reported or not: its chat is not read again.
            if let Some(marker) = report.marker {
                let checked_state = state.clone();
                let checked_key = key.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(db) = open_gxserver_database(&checked_state.paths) {
                        let _ = crate::coordinators::record_remote_thread_checked(
                            &db,
                            &checked_key,
                            marker,
                        );
                    }
                })
                .await;
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
            let mut body = report_body(
                &ThreadReport::Finished {
                    message: message.as_deref(),
                },
                &thread_ref,
            );
            if let Some(due) = stall_due {
                body.push_str(&format!(
                    "\n\n(Its computer still showed it working: Ghostex read the end of this turn from its transcript after {} minutes without new output. Its hooks there may be missing; `ghostex agent-hooks status --computer {}` shows them.)",
                    crate::coordinators::minutes_of(
                        i64::from(due.round) * crate::coordinators::quiet_round_ms()
                    ),
                    report.computer.name
                ));
            }
            (body, None, message.unwrap_or_default())
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
        RemoteReport::Undelivered(ref row) => (
            report_body(
                &ThreadReport::Undelivered {
                    excerpt: &row.excerpt,
                    evidence: Some(&format!(
                        "the message was marked Not sent on {}: {}",
                        report.computer.name,
                        row.reason.trim()
                    )),
                },
                &thread_ref,
            ),
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
        match (&report.kind, stall_due) {
            (RemoteReport::Stalled { .. }, Some(due)) =>
                format!("stall:{}:{}", due.quiet_since, due.round),
            (RemoteReport::Undelivered(row), _) => format!("undelivered:{}", row.row_id),
            _ => prompt_key
                .clone()
                .or_else(|| message_id.clone())
                .or_else(|| report.thread.reported_at.clone())
                .unwrap_or_else(|| "first".to_string()),
        }
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
    let stalled = matches!(report.kind, RemoteReport::Stalled { .. });
    let undelivered = match &report.kind {
        RemoteReport::Undelivered(row) => Some(row.row_id.clone()),
        _ => None,
    };
    if let (true, Some(row_id), Ok(mut memory)) = (sent, undelivered.clone(), memory.lock()) {
        memory.undelivered_reported.insert((key.clone(), row_id));
    }
    if sent {
        if let (Some(due), Ok(mut memory)) = (stall_due, memory.lock()) {
            if let Some(watch) = memory.stalls.get_mut(&key) {
                watch.mark_reported(due.quiet_since, due.round);
                if !stalled {
                    watch.mark_finished(due.rows);
                }
            }
        }
    }
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
        } else if stalled || undelivered.is_some() {
            // A stall report clears nothing the next finished report needs, and a message that
            // was not delivered leaves the thread as it was.
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
    fn an_open_question_card_keeps_a_remote_thread_waiting() {
        for activity in ["working", "attention", "idle"] {
            let session = json!({
                "lifecycleState": "running", "activity": activity,
                "pendingQuestionCount": 0, "awaitingAnswer": true,
            });
            assert_eq!(
                observe_remote_session(Some(&session)),
                RemoteObservation::Waiting
            );
            assert_eq!(session["pendingQuestionCount"], 0);
        }
        assert_eq!(
            observe_remote_session(Some(&json!({
                "lifecycleState": "sleeping", "awaitingAnswer": true,
            }))),
            RemoteObservation::Sleeping
        );
    }

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
