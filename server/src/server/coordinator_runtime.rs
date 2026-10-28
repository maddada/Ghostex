//! The coordinator supervisor: every two seconds it looks at every open thread and tells its
//! coordinator when the thread finished a turn, started waiting on an answer, or was closed.
//! Also the HTTP glue for the coordinator endpoints and coordinator creation.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! firstmate's lesson: supervision must cost the coordinator nothing until something needs it. gxserver already knows every thread's state from agent hooks, so it watches and the coordinator only wakes for a report; the coordinator never polls, sleeps or runs `wait-for-text`. A report is marked delivered only after the send succeeded, so a restart or a coordinator busy with a question card delays a report but never loses or repeats it.
//! SEE-ALSO: server/src/coordinators/ (records, states, report text), server/src/delayed_sends.rs (the same stability-window idea for one watched agent).

use super::*;

use crate::coordinators::{
    self, agent_message, classify_thread_session, clear_thread_pending_message, list_threads,
    record_thread_report, report_body, set_thread_observed_working, set_thread_resolved,
    MessageSender, SessionKey, ThreadProgress, ThreadRecord, ThreadReport, ThreadState,
};
use crate::presentation::effective_lifecycle_state;
use crate::session_chat_queue_runtime::SessionChatTranscriptGate;

const TICK: Duration = Duration::from_secs(2);
/// A thread must stay out of work this long before its turn counts as finished: hooks and title
/// observation can dip between tool calls.
const FINISH_STABILITY_MS: i64 = 4_000;
/// Waits after a failed delivery, doubled per failure up to the last value.
const RETRY_DELAYS_MS: [i64; 4] = [10_000, 20_000, 40_000, 60_000];
/// A message handed to a thread is reported undelivered only once it is this old...
pub(super) const UNDELIVERED_MIN_AGE_MS: i64 = 60_000;
/// ...and the thread has sat idle without it this long: a busy thread takes a message typed during
/// its turn at its next input boundary, and a starting one once its input box appears.
pub(super) const UNDELIVERED_IDLE_MS: i64 = 20_000;
/// How often one thread's transcript is read for a pending message.
const DELIVERY_CHECK_EVERY_MS: i64 = 6_000;

#[derive(Default)]
struct SupervisorMemory {
    /// When each thread was first seen out of work since it last worked.
    not_working_since: HashMap<SessionKey, i64>,
    /// Coordinators with a delivery running right now.
    in_flight: HashSet<SessionKey>,
    /// Coordinator → (consecutive failures, earliest retry in ms).
    retry: HashMap<SessionKey, (usize, i64)>,
    transcript_gates: HashMap<SessionKey, SessionChatTranscriptGate>,
    /// Each thread's transcript file, resolved once for the pending-message check.
    transcript_paths: HashMap<SessionKey, std::path::PathBuf>,
    /// When each thread's pending message was last looked for in its transcript.
    delivery_checked_at: HashMap<SessionKey, i64>,
    /// Since when a thread has been idle without its pending message (sent at, since).
    undelivered_idle_since: HashMap<SessionKey, (String, i64)>,
    /// The `sendRequestId` of each report still waiting to reach its coordinator, by coordinator
    /// and report text, so a retried report is the same send to gxserver's send ledger.
    report_send_ids: HashMap<(SessionKey, String), String>,
    /// Empryo coordinators: when their role was last checked, and how often it was typed again.
    empryo_roles: HashMap<SessionKey, EmpryoRoleCheck>,
}

enum ReportKind {
    Finished,
    Interrupted,
    Waiting {
        key: String,
        summary: String,
    },
    Closed,
    Undelivered {
        sent_at: String,
        excerpt: String,
        evidence: Option<String>,
    },
}

struct PendingReport {
    thread: ThreadRecord,
    kind: ReportKind,
    sender: MessageSender,
    session: Option<Value>,
}

struct Delivery {
    coordinator: SessionKey,
    reports: Vec<PendingReport>,
}

pub(crate) fn start_coordinator_runtime(state: Arc<AppState>) {
    let _ = coordinators::ensure_coordinator_role_file(&state.paths);
    let memory = Arc::new(Mutex::new(SupervisorMemory::default()));
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
            let deliveries = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for delivery in deliveries {
                tokio::spawn(deliver(state.clone(), memory.clone(), delivery));
            }
        }
    });
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// Launcher display names by agent id ("Claude" for a custom Claude launcher), as the sidebar and
/// `ghostex agents types` show them.
fn agent_names(repository: &DomainRepository<'_>) -> HashMap<String, String> {
    let projects = repository.list_projects().unwrap_or_default();
    crate::sidebar_hud::sidebar_agent_buttons_from_projects(&projects)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|agent| {
            let id = text(agent, "agentId");
            let name = text(agent, "name");
            (!id.is_empty() && !name.is_empty()).then_some((id, name))
        })
        .collect()
}

fn sender_for(
    state: &AppState,
    project: Option<&Value>,
    session: &Value,
    thread: &ThreadRecord,
    names: &HashMap<String, String>,
) -> MessageSender {
    let presentation = project.map(|project| {
        crate::presentation::project_presentation_session(
            project,
            &crate::presentation::default_group_id(&thread.project_id),
            session,
            &crate::presentation::now_iso(),
        )
    });
    let presented = presentation.as_ref().unwrap_or(session);
    let title = presented
        .get("displayTitle")
        .or_else(|| presented.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    MessageSender {
        agent_name: names
            .get(&text(session, "agentId"))
            .cloned()
            .unwrap_or_else(|| {
                let name = text(presented, "agentName");
                if name.is_empty() {
                    text(session, "agentId")
                } else {
                    name
                }
            }),
        title,
        session_id: thread.session_id.clone(),
        agent_id: text(session, "agentId"),
        agent_session_id: session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        global_ref: crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &thread.project_id,
            &thread.session_id,
        ),
    }
}

fn closed_sender(state: &AppState, thread: &ThreadRecord) -> MessageSender {
    MessageSender {
        title: thread
            .task
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect(),
        session_id: thread.session_id.clone(),
        global_ref: crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &thread.project_id,
            &thread.session_id,
        ),
        ..MessageSender::default()
    }
}

fn tick(state: &AppState, memory: &Mutex<SupervisorMemory>) -> Vec<Delivery> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(threads) = list_threads(&db) else {
        return Vec::new();
    };
    refresh_thread_screen_waits(state, &repository, &threads);
    for (project_id, session_id) in
        crate::coordinators::refresh_coordinator_panels(&db, &repository)
    {
        republish_session_chat(state, &project_id, &session_id);
    }
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    repair_empryo_coordinator_roles(state, &db, &repository, &mut memory);
    let open = threads
        .into_iter()
        .filter(|thread| !thread.is_resolved())
        .collect::<Vec<_>>();
    let open_keys = open.iter().map(ThreadRecord::key).collect::<HashSet<_>>();
    memory
        .not_working_since
        .retain(|key, _| open_keys.contains(key));
    memory
        .transcript_gates
        .retain(|key, _| open_keys.contains(key));
    memory
        .transcript_paths
        .retain(|key, _| open_keys.contains(key));
    memory
        .delivery_checked_at
        .retain(|key, _| open_keys.contains(key));
    memory
        .undelivered_idle_since
        .retain(|key, _| open_keys.contains(key));
    if open.is_empty() {
        return Vec::new();
    }
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    let mut coordinator_alive: HashMap<SessionKey, bool> = HashMap::new();
    let mut projects: HashMap<String, Option<Value>> = HashMap::new();
    let mut names: Option<HashMap<String, String>> = None;
    let mut pending: std::collections::BTreeMap<SessionKey, Vec<PendingReport>> =
        std::collections::BTreeMap::new();
    for mut thread in open {
        let coordinator_key = thread.coordinator_key();
        let alive = *coordinator_alive
            .entry(coordinator_key.clone())
            .or_insert_with(|| {
                repository
                    .get_session(&coordinator_key.0, &coordinator_key.1)
                    .ok()
                    .flatten()
                    .is_some_and(|session| {
                        matches!(
                            effective_lifecycle_state(&session).as_str(),
                            "running" | "sleeping"
                        )
                    })
            });
        // A closed coordinator has nobody to report to; its threads wait until it is resumed.
        if !alive {
            continue;
        }
        let key = thread.key();
        let session = repository
            .get_session(&thread.project_id, &thread.session_id)
            .ok()
            .flatten();
        let Some(session) = session else {
            memory.not_working_since.remove(&key);
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: closed_sender(state, &thread),
                    thread,
                    kind: ReportKind::Closed,
                    session: None,
                });
            continue;
        };
        let lifecycle = effective_lifecycle_state(&session);
        if lifecycle == "stopped" {
            memory.not_working_since.remove(&key);
            let project = projects
                .entry(thread.project_id.clone())
                .or_insert_with(|| repository.get_project(&thread.project_id).ok().flatten())
                .clone();
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: sender_for(
                        state,
                        project.as_ref(),
                        &session,
                        &thread,
                        names.get_or_insert_with(|| agent_names(&repository)),
                    ),
                    thread,
                    kind: ReportKind::Closed,
                    session: Some(session),
                });
            continue;
        }
        if !matches!(lifecycle.as_str(), "running" | "sleeping") {
            // `missing` / `unknown`: the provider has not been probed or died; nothing to report yet.
            continue;
        }
        // The supervisor reads the session's own state; "starting" is a presentation notion.
        let as_run = ThreadProgress {
            resolved: false,
            has_run: true,
        };
        let hook_state = classify_thread_session(Some(&session), as_run, &now_iso, false);
        // CDXC:Coordinators 2026-10-26 WHY:
        // A permission prompt leaves its tool call open in the transcript, so the transcript check kept an unwatched thread working while it needed approval (a Claude thread on Claude Code's rm safety check, which asks even in bypass mode, never reached its coordinator). The shared approval rule takes precedence, as a wait read off the screen already does.
        let state_now = if is_running_empryo(&session) {
            empryo_thread_state(&db, &mut memory, &mut thread, &session, hook_state)
        } else if hook_state == ThreadState::Waiting
            && (coordinators::waits_on_screen(&session)
                || crate::session_approval::session_awaits_approval(&session, &now_iso))
        {
            ThreadState::Waiting
        } else if hook_state != ThreadState::Working
            && hook_state != ThreadState::Sleeping
            && thread.observed_working
        {
            // Only a thread about to be reported pays for the transcript read.
            let working = memory
                .transcript_gates
                .entry(key.clone())
                .or_default()
                .is_working(&session);
            if working {
                ThreadState::Working
            } else {
                hook_state
            }
        } else {
            hook_state
        };
        let report = match state_now {
            ThreadState::Working => {
                memory.not_working_since.remove(&key);
                if !thread.observed_working {
                    let _ =
                        set_thread_observed_working(&db, &thread.project_id, &thread.session_id);
                }
                None
            }
            ThreadState::Waiting => {
                memory.not_working_since.remove(&key);
                let prompt = coordinators::waiting_prompt(&session)
                    .map(|prompt| (prompt.key, prompt.summary));
                let (prompt_key, summary) = match prompt {
                    Some(prompt) => prompt,
                    None => (
                        format!(
                            "attention:{}",
                            session
                                .pointer("/runtimeSettings/agentActivity/attentionEventId")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                        ),
                        "It is waiting for someone to look at its screen (an approval or a prompt the chat cannot show).".to_string(),
                    ),
                };
                (thread.reported_prompt_key.as_deref() != Some(prompt_key.as_str())).then_some(
                    ReportKind::Waiting {
                        key: prompt_key,
                        summary,
                    },
                )
            }
            ThreadState::Finished | ThreadState::Sleeping if thread.observed_working => {
                let since = *memory.not_working_since.entry(key.clone()).or_insert(now);
                if now - since < FINISH_STABILITY_MS {
                    None
                } else {
                    // CDXC:Coordinators 2026-10-27 WHY:
                    // A denied or stopped Claude turn has an interruption boundary, not a final answer. Forwarding the last assistant text would present the earlier tool-call preamble as the result, so interrupted turns settle the report without forwarding that text.
                    let lifecycle = memory
                        .transcript_gates
                        .entry(key.clone())
                        .or_default()
                        .lifecycle(&session);
                    match lifecycle.map(|lifecycle| lifecycle.state) {
                        Some(crate::session_chat::SessionChatTurnLifecycleState::Interrupted) => {
                            Some(ReportKind::Interrupted)
                        }
                        Some(crate::session_chat::SessionChatTurnLifecycleState::Working)
                            if state_now != ThreadState::Sleeping =>
                        {
                            memory.not_working_since.remove(&key);
                            None
                        }
                        _ => Some(ReportKind::Finished),
                    }
                }
            }
            _ => None,
        };
        let report = match report {
            Some(report) => Some(report),
            None => pending_delivery(&db, &mut memory, &thread, &session, state_now, now),
        };
        if let Some(kind) = report {
            let project = projects
                .entry(thread.project_id.clone())
                .or_insert_with(|| repository.get_project(&thread.project_id).ok().flatten())
                .clone();
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: sender_for(
                        state,
                        project.as_ref(),
                        &session,
                        &thread,
                        names.get_or_insert_with(|| agent_names(&repository)),
                    ),
                    thread,
                    kind,
                    session: Some(session),
                });
        }
    }
    let mut deliveries = Vec::new();
    for (coordinator, reports) in pending {
        if memory.in_flight.contains(&coordinator) {
            continue;
        }
        if memory
            .retry
            .get(&coordinator)
            .is_some_and(|(_, retry_at)| now < *retry_at)
        {
            continue;
        }
        memory.in_flight.insert(coordinator.clone());
        deliveries.push(Delivery {
            coordinator,
            reports,
        });
    }
    deliveries
}

#[derive(Default)]
struct EmpryoRoleCheck {
    checked_at: i64,
    retyped: usize,
}

/// A running session whose agent is Empryo.
fn is_running_empryo(session: &Value) -> bool {
    effective_lifecycle_state(session) == "running"
        && crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
            == Some("empryo")
}

/// How often an Empryo coordinator's role is checked on its screen, and how often it is typed
/// again before the supervisor gives up on that coordinator.
const EMPRYO_ROLE_CHECK_EVERY_MS: i64 = 30_000;
const EMPRYO_ROLE_RETYPES: usize = 3;

/// CDXC:Coordinators 2026-10-07 WHY:
/// Empryo 3.9.1-beta sets `/agent` on the window's current chat only: a first window draws its input box before its engine has restored the tab, and the restore (or another window joining the engine) writes the tab back without the agent, so a coordinator queued its `/agent ghostex-coordinator` line, saw it accepted, and still ran without its role (seen live 2026-10-07). The engine records the tab's profile in its session's `meta.json` (and logs `"agent":null` after every turn of a tab without one), which a narrow window's border cannot show (it shortens the `as ghostex-coordinator` segment to a glyph or drops it), so the role is read there, not off the border. An idle coordinator without it gets the line again, a few times at most, typed straight into its terminal: never through the chat queue, which piled duplicates behind a busy coordinator and drew a chat row for every one (seen live 2026-10-07).
fn repair_empryo_coordinator_roles(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    memory: &mut SupervisorMemory,
) {
    let Ok(coordinators) = coordinators::list_coordinators(db) else {
        return;
    };
    let now = now_ms();
    let keys: HashSet<SessionKey> = coordinators
        .into_iter()
        .map(|coordinator| (coordinator.project_id, coordinator.session_id))
        .collect();
    memory.empryo_roles.retain(|key, _| keys.contains(key));
    let Some(command) = coordinators::coordinator_role_queued_command("empryo") else {
        return;
    };
    for key in keys {
        let check = memory.empryo_roles.entry(key.clone()).or_default();
        if now - check.checked_at < EMPRYO_ROLE_CHECK_EVERY_MS {
            continue;
        }
        check.checked_at = now;
        let Some(session) = repository.get_session(&key.0, &key.1).ok().flatten() else {
            continue;
        };
        if !is_running_empryo(&session) {
            continue;
        }
        let Some(agent) = crate::session_chat_pi_models::empryo_session_log(repository, &session)
            .and_then(|log| crate::session_chat_empryo_tabs::empryo_tab_agent(&log))
        else {
            continue;
        };
        if agent.as_deref() == Some(coordinators::EMPRYO_COORDINATOR_AGENT_NAME) {
            check.retyped = 0;
            continue;
        }
        if check.retyped >= EMPRYO_ROLE_RETYPES
            || crate::session_chat_queue::session_has_pending_session_chat_queue(db, &key.0, &key.1)
            || memory
                .transcript_gates
                .entry(key.clone())
                .or_default()
                .is_working(&session)
        {
            continue;
        }
        // The send clears the input box first, so a box holding the user's unsent text is left
        // alone until it is empty; one holding only the role command (a send that typed it but
        // was never submitted) is retyped. A capture without a cursor (wmx) cannot tell a tip
        // from typed text, so there only that leftover command counts.
        let idle = crate::zmx::read_zmx_session_history_capture_vt(repository, &key.0, &key.1)
            .is_ok_and(|screen| {
                crate::session_chat_composer::empryo_composer_busy(&screen.text) == Some(false)
                    && crate::session_chat_composer::session_chat_composer_input(
                        "empryo",
                        &screen.text,
                    )
                    .is_some_and(|input| {
                        (input.text_is_empty() && !input.text_unreadable())
                            || input.text.trim() == command.trim()
                    })
            });
        if !idle || coordinators::ensure_empryo_coordinator_agent_file(&state.paths).is_err() {
            continue;
        }
        let steps = crate::session_chat_send::build_session_chat_message_steps(
            Some("empryo"),
            &command,
            &[],
            false,
        );
        if crate::session_chat_send::enqueue_session_write_sequence(
            &session,
            &key.0,
            &key.1,
            "coordinator-role",
            steps,
        )
        .is_ok()
        {
            check.retyped += 1;
        }
    }
}

/// CDXC:Coordinators 2026-10-07 WHY:
/// The coordinator supervisor counts an Empryo thread's turns (working, finished, final message) from its own tab's transcript instead of hooks, scoped to Empryo.
/// Empryo 3.9.1-beta runs every window of a repository on one engine, which runs the hooks of all of them in the first window's process with that window's session id and no tab, so a thread whose window joined another's engine never reported a turn, and the first window's hooks spoke for its neighbours' turns too (seen live 2026-10-07).
/// Its own tab's transcript (session_chat_empryo_mirror.rs) is exact: an open turn is work, and a turn that ended after the thread's last report is a finished turn to report; an older one was reported already.
/// A question or approval panel on its screen reads as waiting even while its turn is open, since the turn that asked stays open until it is answered.
fn empryo_thread_state(
    db: &rusqlite::Connection,
    memory: &mut SupervisorMemory,
    thread: &mut ThreadRecord,
    session: &Value,
    hook_state: ThreadState,
) -> ThreadState {
    if hook_state == ThreadState::Waiting && coordinators::waits_on_screen(session) {
        return ThreadState::Waiting;
    }
    let lifecycle = memory
        .transcript_gates
        .entry(thread.key())
        .or_default()
        .lifecycle(session);
    let Some(lifecycle) = lifecycle else {
        // No turn yet: its brief is still on its way in.
        return ThreadState::Working;
    };
    if lifecycle.state == crate::session_chat::SessionChatTurnLifecycleState::Working {
        return ThreadState::Working;
    }
    if hook_state == ThreadState::Waiting {
        return ThreadState::Waiting;
    }
    let reported = thread.reported_at.as_deref().and_then(parse_iso_ms_opt);
    let new_turn = match (lifecycle.timestamp, reported) {
        (Some(ended), Some(reported)) => ended > reported,
        (_, None) => true,
        (None, Some(_)) => false,
    };
    if new_turn && !thread.observed_working {
        let _ = set_thread_observed_working(db, &thread.project_id, &thread.session_id);
        thread.observed_working = true;
    }
    ThreadState::Finished
}

async fn deliver(state: Arc<AppState>, memory: Arc<Mutex<SupervisorMemory>>, delivery: Delivery) {
    let coordinator = delivery.coordinator.clone();
    // CDXC:Coordinators 2026-10-04 WHY: closing a thread and then its coordinator a second apart let a tick queue the thread's "closed" report while the coordinator was still open, and sending it woke the closed coordinator back up. The coordinator is checked again right before sending; a report for a closed one waits until it is resumed.
    let open_state = state.clone();
    let open_key = coordinator.clone();
    let coordinator_open =
        tokio::task::spawn_blocking(move || coordinator_is_open(&open_state, &open_key))
            .await
            .unwrap_or(false);
    let mut failed = false;
    let reports = if coordinator_open {
        delivery.reports
    } else {
        Vec::new()
    };
    for report in reports {
        let thread_ref = report.sender.global_ref.clone();
        let (body, finished_text) = match &report.kind {
            ReportKind::Finished => {
                let session = report.session.clone();
                let reported_at = report.thread.reported_at.clone();
                let message = tokio::task::spawn_blocking(move || {
                    session
                        .as_ref()
                        .and_then(crate::notification_feed::body::last_assistant_message)
                        .filter(|(_, timestamp)| {
                            // A reply we already forwarded is not this turn's report.
                            match (timestamp, reported_at.as_deref().and_then(parse_iso_ms_opt)) {
                                (Some(timestamp), Some(reported)) => *timestamp > reported,
                                _ => true,
                            }
                        })
                        .map(|(text, _)| text)
                })
                .await
                .ok()
                .flatten();
                let body = report_body(
                    &ThreadReport::Finished {
                        message: message.as_deref(),
                    },
                    &thread_ref,
                );
                let body = match report
                    .session
                    .as_ref()
                    .and_then(crate::accounts::api_errors::slow_retry_note)
                {
                    Some(note) => format!("{body}\n\n{note}"),
                    None => body,
                };
                (body, Some(message.unwrap_or_default()))
            }
            ReportKind::Interrupted => (
                report_body(&ThreadReport::Interrupted, &thread_ref),
                Some("(turn interrupted)".to_string()),
            ),
            ReportKind::Waiting { summary, .. } => (
                report_body(&ThreadReport::Waiting { prompt: summary }, &thread_ref),
                None,
            ),
            ReportKind::Closed => (report_body(&ThreadReport::Closed, &thread_ref), None),
            ReportKind::Undelivered {
                excerpt, evidence, ..
            } => (
                report_body(
                    &ThreadReport::Undelivered {
                        excerpt,
                        evidence: evidence.as_deref(),
                    },
                    &thread_ref,
                ),
                None,
            ),
        };
        let message = agent_message(&report.sender, &body);
        let report_key = (coordinator.clone(), message.clone());
        let send_request_id = memory
            .lock()
            .map(|mut memory| {
                memory
                    .report_send_ids
                    .entry(report_key.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            })
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        match send_to_coordinator(&state, &coordinator, &message, &send_request_id).await {
            Ok(()) => {
                if let Ok(mut memory) = memory.lock() {
                    memory.report_send_ids.remove(&report_key);
                }
                let settle_state = state.clone();
                let thread = report.thread.clone();
                let kind_key = match &report.kind {
                    ReportKind::Waiting { key, .. } => Some(key.clone()),
                    _ => None,
                };
                let closed = matches!(report.kind, ReportKind::Closed);
                let undelivered_sent_at = match &report.kind {
                    ReportKind::Undelivered { sent_at, .. } => Some(sent_at.clone()),
                    _ => None,
                };
                let _ = tokio::task::spawn_blocking(move || match undelivered_sent_at {
                    Some(sent_at) => settle_undelivered(&settle_state, &thread, &sent_at),
                    None => settle_report(
                        &settle_state,
                        &thread,
                        finished_text.as_deref(),
                        kind_key.as_deref(),
                        closed,
                    ),
                })
                .await;
            }
            Err(error) => {
                let _ = state.logger.log_routine(
                    crate::logging::DiagnosticLogScenario::ServerLifecycle,
                    GxserverLogInput {
                        level: LogLevel::Warn,
                        event: "coordinatorReportDeferred".to_string(),
                        server_id: Some(state.metadata.server_id.clone()),
                        request_id: None,
                        client: None,
                        duration_ms: None,
                        error: Some(error.chars().take(300).collect()),
                        details: Some(json!({
                            "coordinatorProjectId": coordinator.0,
                            "coordinatorSessionId": coordinator.1,
                            "threadSessionId": report.thread.session_id,
                            "sendRequestId": send_request_id,
                        })),
                    },
                );
                failed = true;
                break;
            }
        }
    }
    if let Ok(mut memory) = memory.lock() {
        memory.in_flight.remove(&coordinator);
        if failed {
            let failures = memory
                .retry
                .get(&coordinator)
                .map(|(count, _)| *count)
                .unwrap_or(0);
            let delay = RETRY_DELAYS_MS[failures.min(RETRY_DELAYS_MS.len() - 1)];
            memory
                .retry
                .insert(coordinator, (failures + 1, now_ms() + delay));
        } else {
            memory.retry.remove(&coordinator);
            memory
                .report_send_ids
                .retain(|(owner, _), _| owner != &coordinator);
        }
    }
}

fn parse_iso_ms_opt(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn settle_report(
    state: &AppState,
    thread: &ThreadRecord,
    finished_text: Option<&str>,
    prompt_key: Option<&str>,
    closed: bool,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if closed {
        let _ = set_thread_resolved(&db, &thread.project_id, &thread.session_id, true);
    } else {
        let _ = record_thread_report(
            &db,
            &thread.project_id,
            &thread.session_id,
            finished_text.map(|text| {
                if text.is_empty() {
                    "(no final message)"
                } else {
                    text
                }
            }),
            prompt_key,
        );
    }
    let _ = schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        &thread.project_id,
        &thread.session_id,
    );
}

/// The thread's pending message, checked against its transcript: cleared once it shows up, and
/// reported to the coordinator when the thread sits idle without it. See the CDXC:Coordinators
/// 2026-10-04 note on `watch_pending_message` in server/src/coordinators/endpoint.rs.
fn pending_delivery(
    db: &rusqlite::Connection,
    memory: &mut SupervisorMemory,
    thread: &ThreadRecord,
    session: &Value,
    state_now: ThreadState,
    now: i64,
) -> Option<ReportKind> {
    let key = thread.key();
    let (Some(excerpt), Some(sent_at)) = (
        thread.pending_message.as_deref(),
        thread.pending_message_at.as_deref(),
    ) else {
        memory.undelivered_idle_since.remove(&key);
        return None;
    };
    // A working thread is either on the message or takes it at its next input boundary, so it is
    // never reported; its transcript is still read, so a message it took clears while it works.
    let working = state_now == ThreadState::Working;
    if working {
        memory.undelivered_idle_since.remove(&key);
    }
    if memory
        .delivery_checked_at
        .get(&key)
        .is_some_and(|checked| now - checked < DELIVERY_CHECK_EVERY_MS)
    {
        return None;
    }
    memory.delivery_checked_at.insert(key.clone(), now);
    let sent_ms = parse_iso_ms_opt(sent_at).unwrap_or(now);
    let needles = coordinators::delivery_needles(excerpt);
    let mut path = memory.transcript_paths.remove(&key);
    let recorded = coordinators::transcript_records_message(session, &needles, sent_ms, &mut path);
    if let Some(path) = path {
        memory.transcript_paths.insert(key.clone(), path);
    }
    match recorded {
        Some(true) => {
            memory.undelivered_idle_since.remove(&key);
            let _ =
                clear_thread_pending_message(db, &thread.project_id, &thread.session_id, sent_at);
            // A turn too short for a tick to see it working still gets its report.
            if !thread.observed_working {
                let _ = set_thread_observed_working(db, &thread.project_id, &thread.session_id);
            }
            return None;
        }
        // No transcript to judge by yet: a missing message cannot be told from a slow agent.
        None => {
            memory.undelivered_idle_since.remove(&key);
            return None;
        }
        Some(false) => {}
    }
    // A question or a blocking screen is reported as waiting; the message follows its answer.
    if working || state_now == ThreadState::Waiting {
        memory.undelivered_idle_since.remove(&key);
        return None;
    }
    // Held in the thread's own queue: it goes out once the input box is ready, and a queued
    // message from another agent that fails already tells its sender.
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        db,
        &thread.project_id,
        &thread.session_id,
    );
    if let Some(row) = queue.queue.iter().find(|row| {
        coordinators::holds_message(&coordinators::normalize_delivery_text(&row.text), &needles)
    }) {
        memory.undelivered_idle_since.remove(&key);
        if row.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_FAILED {
            let _ =
                clear_thread_pending_message(db, &thread.project_id, &thread.session_id, sent_at);
        }
        return None;
    }
    let idle = memory
        .undelivered_idle_since
        .entry(key)
        .or_insert_with(|| (sent_at.to_string(), now));
    if idle.0 != sent_at {
        *idle = (sent_at.to_string(), now);
    }
    if now - sent_ms < UNDELIVERED_MIN_AGE_MS || now - idle.1 < UNDELIVERED_IDLE_MS {
        return None;
    }
    let evidence = crate::session_chat_notice::session_chat_watchdog_notice(
        &thread.project_id,
        &thread.session_id,
    )
    .map(|notice| notice.title);
    Some(ReportKind::Undelivered {
        sent_at: sent_at.to_string(),
        excerpt: excerpt.to_string(),
        evidence,
    })
}

fn coordinator_is_open(state: &AppState, coordinator: &SessionKey) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(&coordinator.0, &coordinator.1)
        .ok()
        .flatten()
        .is_some_and(|session| {
            matches!(
                effective_lifecycle_state(&session).as_str(),
                "running" | "sleeping"
            )
        })
}

fn settle_undelivered(state: &AppState, thread: &ThreadRecord, sent_at: &str) {
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let _ = clear_thread_pending_message(&db, &thread.project_id, &thread.session_id, sent_at);
    }
}

/// The same default delivery `ghostex agents send` uses: typed now, picked up by a busy agent at
/// its next input boundary, and a sleeping coordinator is woken for it. A report retried after a
/// failure keeps its `sendRequestId`, so one that did arrive is never typed again.
async fn send_to_coordinator(
    state: &AppState,
    coordinator: &SessionKey,
    message: &str,
    send_request_id: &str,
) -> std::result::Result<(), String> {
    let body = json!({
        "params": {
            "projectId": coordinator.0,
            "sessionId": coordinator.1,
            "text": message,
            "sendRequestId": send_request_id,
        }
    });
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if routed.response.status().is_success() {
        return Ok(());
    }
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    Err(String::from_utf8_lossy(&bytes).to_string())
}

/// Re-sends an open chat its current state, now carrying a refreshed Threads panel (or, for the
/// delivery watch, its settled agent messages); the same frame a queue change sends, without its
/// presentation delta.
pub(super) fn republish_session_chat(state: &AppState, project_id: &str, session_id: &str) {
    let key = session_observer_key(project_id, session_id);
    let options = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).map(|entry| entry.value.options.clone()))
        .unwrap_or_default();
    let screen = crate::session_chat_options::cached_session_chat_screen_state(
        state, project_id, session_id,
    );
    crate::session_chat_options::emit_session_chat_options_state_frame(
        &state.session_chat_followers,
        &state.event_hub,
        &state.paths,
        &state.metadata.server_id,
        project_id,
        session_id,
        options.as_ref(),
        screen.borrow(),
    );
}

/// Records what the chat's cached screen reading of every running open thread waits on, for
/// every surface that classifies threads (see `ThreadScreenWait`).
///
/// CDXC:Coordinators 2026-10-06 WHY:
/// The screen reading is refreshed only while something follows the session (a viewer, a send, a chat read), so an Empryo thread nobody watched showed its approval panel for over a minute while its coordinator still saw it working (live check, card agent-bo-95422941). For a working thread whose agent asks only on its screen (Empryo, Cursor, Freebuff), the tick refreshes that reading through the detector's own cache lifetime, at most one capture per thread every few seconds; every other thread is read from the cache alone, as before.
fn refresh_thread_screen_waits(
    state: &AppState,
    repository: &DomainRepository<'_>,
    threads: &[ThreadRecord],
) {
    let generated_at = crate::presentation::now_iso();
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(state);
    let waits = threads
        .iter()
        .filter(|thread| !thread.is_resolved())
        .filter(|thread| {
            let Some(session) = repository
                .get_session(&thread.project_id, &thread.session_id)
                .ok()
                .flatten()
                .filter(|session| effective_lifecycle_state(session) == "running")
            else {
                return false;
            };
            let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
            if crate::session_chat_options::session_chat_questions_only_on_screen(agent.as_deref())
                && crate::presentation::presentation_activity(&session, &generated_at) == "working"
            {
                detector.detect_blocking(
                    &thread.project_id,
                    &thread.session_id,
                    agent.as_deref(),
                    false,
                );
            }
            true
        })
        .filter_map(|thread| {
            let screen = crate::session_chat_options::cached_session_chat_screen_state(
                state,
                &thread.project_id,
                &thread.session_id,
            );
            coordinators::ThreadScreenWait::from_screen(
                screen.notice.as_ref(),
                screen.prompt.as_ref(),
                &crate::ids::create_global_session_ref(
                    state.metadata.server_id.as_str(),
                    &thread.project_id,
                    &thread.session_id,
                ),
            )
            .map(|wait| (thread.key(), wait))
        })
        .collect();
    coordinators::replace_thread_screen_waits(waits);
}

/// `/api/createAgentSession` with a `coordinator` object: marks the session as one that takes its
/// role from the `ghostex-orchestrator` skill, so the older role channels (ZCode's SessionStart
/// pointer) leave it alone.
pub(crate) fn prepare_coordinator_create_params(
    params: &Map<String, Value>,
) -> std::result::Result<Map<String, Value>, DomainStateError> {
    let mut params = params.clone();
    if !params.get("coordinator").is_some_and(Value::is_object) {
        return Ok(params);
    }
    let runtime = params
        .entry("runtimeSettings".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(runtime) = runtime.as_object_mut() {
        runtime.insert(
            coordinators::COORDINATOR_ROLE_SETTING.to_string(),
            json!(coordinators::COORDINATOR_ROLE_SKILL),
        );
    }
    Ok(params)
}

/// Hands a just-created orchestrator its role: installs the `ghostex-orchestrator` skill for its
/// agent and queues its first message (the skill line, then the create's first request) as a
/// startup send. A ZCode orchestrator's chosen model goes first as a `/model` line, because ZCode
/// has no launch model flag and the first message must run on the model the user picked.
pub(crate) fn start_orchestrator_role(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    family: &str,
    params: &Map<String, Value>,
) -> std::result::Result<(), DomainStateError> {
    coordinators::ensure_orchestrator_skill_installed(&state.paths, family)?;
    if family == "zcode" {
        if let Some(model) = params
            .get("agentModel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
        {
            queue_session_chat_prompt(
                state,
                project_id,
                session_id,
                &format!("/model {model}"),
                true,
            )?;
        }
    }
    let first_request = coordinators::coordinator_create_request(params)?
        .map(|request| request.first_request)
        .unwrap_or_default();
    queue_session_chat_prompt(
        state,
        project_id,
        session_id,
        &coordinators::orchestrator_first_message(family, &first_request),
        true,
    )
}

/// Queues a message in a session's chat queue and tells its viewers. A `startup_send` is a new
/// session's first message: the queue holds it until the agent's input box is up (the feedback
/// agent's brief, server/src/feedback/agent.rs, goes this way too).
pub(crate) fn queue_session_chat_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    startup_send: bool,
) -> std::result::Result<(), DomainStateError> {
    let mut queue_params = Map::new();
    queue_params.insert("projectId".to_string(), json!(project_id));
    queue_params.insert("sessionId".to_string(), json!(session_id));
    queue_params.insert("text".to_string(), json!(text));
    queue_params.insert("startupSend".to_string(), json!(startup_send));
    let result = crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &queue_params,
    )?;
    if result.broadcast {
        crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
            state, project_id, session_id,
        );
    }
    Ok(())
}

/// `/api/promoteCoordinator`: makes an existing session a coordinator, then queues its playbook
/// in the session's chat queue, which hands it over only once the agent is idle (never mid-turn).
/// See the CDXC:Coordinators decision on `promote_session_to_coordinator`.
pub(crate) fn promote_coordinator(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let promotion = coordinators::promote_session_to_coordinator(
        db,
        state.metadata.server_id.as_str(),
        params,
    )?;
    let (project_id, session_id) = &promotion.key;
    schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
    // The coordinator record is committed, so a failure from here on is reported, not raised.
    let playbook_error =
        coordinators::ensure_orchestrator_skill_installed(&state.paths, &promotion.family)
            .and_then(|_| {
                queue_session_chat_prompt(
                    state,
                    project_id,
                    session_id,
                    &promotion.playbook_message,
                    false,
                )
            })
            .err()
            .map(|error| error.message);
    Ok(json!({
        "ok": true,
        "globalRef": crate::ids::create_global_session_ref(state.metadata.server_id.as_str(), project_id, session_id),
        "title": promotion.title,
        "playbookQueued": playbook_error.is_none(),
        "playbookError": playbook_error,
    }))
}

pub(crate) fn handle_coordinator_http(
    state: &AppState,
    endpoint_path: &str,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let output = coordinators::handle_coordinator_endpoint(endpoint_path, db, repository, params)?;
    for (project_id, session_id) in &output.changed_sessions {
        schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
    }
    // The orchestrator's Threads panel is a cache the supervisor rebuilds every tick; an unlinked
    // thread must leave it now, not on the next tick.
    if endpoint_path == "/api/unlinkCoordinatorThread" {
        for (project_id, session_id) in
            crate::coordinators::refresh_coordinator_panels(db, repository)
        {
            republish_session_chat(state, &project_id, &session_id);
        }
    }
    Ok(output.result)
}
