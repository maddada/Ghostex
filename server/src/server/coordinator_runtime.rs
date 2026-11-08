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
    /// Each working thread's output clock, for its "no new output" report (coordinators/stall.rs).
    stalls: HashMap<SessionKey, coordinators::StallWatch>,
    /// Each idle thread's clock, for its "idle though a background job waits" report
    /// (coordinators/idle_wait.rs).
    idle_waits: HashMap<SessionKey, coordinators::IdleWatch>,
    /// When each working thread's transcript was last read for the line it is on.
    activity_read_at: HashMap<SessionKey, i64>,
    /// What each open thread's transcript last said about its newest turn.
    turns: HashMap<SessionKey, TurnReading>,
    /// The outage the supervisor is holding through (since when this computer is offline).
    offline_since: Option<i64>,
    /// Coordinators whose threads were working, or whose reports waited, during that outage.
    held_coordinators: HashSet<SessionKey>,
    /// The one line each of them gets once the network is back, until it is delivered.
    back_lines: HashMap<SessionKey, String>,
}

/// Follows the network for the supervisor: true while this computer is offline. When it comes
/// back, every coordinator held meanwhile is owed one line saying so.
///
/// CDXC:OfflineHold 2026-11-07 WHY:
/// During the 2026-11-07 outage one thread was nudged and reported to its coordinator 9 times, none of which could help. While offline, the threads' clocks are held (stall.rs, idle_wait.rs) and nothing is typed into a coordinator, whose agent could not answer either; reports are worked out from the threads' states on every tick, so a held one goes out once the network is back. Each held coordinator then hears once, not once per thread per round.
fn follow_network(memory: &mut SupervisorMemory, offline_since: Option<i64>, now: i64) -> bool {
    match (memory.offline_since, offline_since) {
        (_, Some(since)) => {
            memory.offline_since = Some(since);
            true
        }
        (Some(since), None) => {
            memory.offline_since = None;
            let line = back_line(since, now);
            for coordinator in std::mem::take(&mut memory.held_coordinators) {
                memory.back_lines.insert(coordinator, line.clone());
            }
            false
        }
        (None, None) => false,
    }
}

/// The line a held coordinator gets when the network is back.
fn back_line(offline_since: i64, back_at: i64) -> String {
    format!(
        "Ghostex: this computer was offline from {} to {} ({}). Nudges and not-moving reports for your threads were held meanwhile; their clocks start again now, and any held reports follow.",
        crate::network_status::local_clock(offline_since),
        crate::network_status::local_clock(back_at),
        crate::network_status::outage_length(back_at - offline_since),
    )
}

/// How often a working thread's transcript is read for the line it is on (the board's Moving row).
const ACTIVITY_READ_EVERY_MS: i64 = 6_000;
/// How many of a transcript's newest messages are read for a thread's newest turn and words.
const TURN_TAIL_MESSAGES: usize = 24;

/// What a thread's transcript says about its newest turn.
#[derive(Clone, Debug, Default)]
struct TurnReading {
    /// The newest turn has not ended.
    open: bool,
    /// When the newest turn started (ms), while it is open.
    started_ms: Option<i64>,
    /// Its newest assistant words in that turn (the last turn's when it ended).
    last_words: Option<String>,
    /// What started the open turn, in a few words, for a turn with no words of its own yet.
    started_by: Option<String>,
}

/// BLOCKING (a transcript tail read, only when the file changed since the last one).
fn read_turn(gate: &mut SessionChatTranscriptGate, session: &Value) -> TurnReading {
    use crate::session_chat::{SessionChatBlock, SessionChatRole, SessionChatTurnLifecycleState};
    let Some(tail) = gate.tail(session, TURN_TAIL_MESSAGES) else {
        return TurnReading::default();
    };
    let open = tail
        .lifecycle
        .as_ref()
        .is_some_and(|lifecycle| lifecycle.state == SessionChatTurnLifecycleState::Working);
    let started_ms = tail
        .lifecycle
        .as_ref()
        .filter(|_| open)
        .and_then(|lifecycle| lifecycle.timestamp);
    let last_words = tail
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .filter(|message| {
            started_ms.is_none_or(|started| message.timestamp.is_none_or(|at| at >= started))
        })
        .find_map(|message| {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    SessionChatBlock::Text { text } => Some(text.trim()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (!text.is_empty()).then_some(text)
        });
    let started_by = started_ms.and_then(|started| {
        let prompt = tail
            .messages
            .iter()
            .rev()
            .filter(|message| message.role == SessionChatRole::User)
            .find(|message| message.timestamp.is_some_and(|at| at >= started))?;
        let text = prompt.blocks.iter().find_map(|block| match block {
            SessionChatBlock::Text { text } if !text.trim().is_empty() => Some(text.trim()),
            _ => None,
        })?;
        Some(if text.contains("<task-notification>") {
            "Reading a background job's result".to_string()
        } else {
            let line = text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('<'))
                .unwrap_or_default();
            format!("Picked up: {}", coordinators::report_headline(line, 120))
        })
    });
    TurnReading {
        open,
        started_ms,
        last_words,
        started_by,
    }
}

enum ReportKind {
    /// Its turn ended; `cut_off` when it went to sleep (or its agent stopped) mid-turn instead.
    Finished {
        cut_off: bool,
    },
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
    /// Working with no new output (coordinators/stall.rs): the first round nudges the thread.
    Stalled {
        due: coordinators::StallDue,
    },
    /// Idle although a background job waits for it (coordinators/idle_wait.rs): the first round
    /// nudges the thread.
    IdleWaiting {
        due: coordinators::IdleDue,
    },
    /// Its session stopped unexpectedly and was restarted, or could not be (session_self_heal.rs).
    SelfHeal {
        report: crate::session_self_heal::HealReport,
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
    /// The network is back: said first (`follow_network`).
    back_line: Option<String>,
}

pub(crate) fn start_coordinator_runtime(state: Arc<AppState>) {
    let _ = coordinators::ensure_coordinator_role_file(&state.paths);
    let memory = Arc::new(Mutex::new(SupervisorMemory::default()));
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

/// The key and words of what a waiting thread waits on, as its "waiting for an answer" report
/// names it.
fn waiting_report_prompt(session: &Value) -> (String, String) {
    match coordinators::waiting_prompt(session) {
        Some(prompt) => (prompt.key, prompt.summary),
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
    }
}

/// Whether the thread still waits on the prompt a report about it was worded from, read again
/// right before the report goes out.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User (Q1293, "all three"): a "waiting for an answer" report reached the coordinator after the
/// user had already answered the card in the thread, and the coordinator asked them again. The
/// report is worded on the tick and sent later (after the reports before it, or a busy
/// coordinator's retries), so it is checked again just before sending and dropped when the thread
/// stopped waiting or waits on something else by then; that something else gets its own report on
/// the next tick. The relayed card (coordinators/relay.rs) and the report's note cover a report
/// that still lands late.
fn still_waits_on(state: &AppState, thread: &ThreadRecord, key: &str) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(session) = repository
        .get_session(&thread.project_id, &thread.session_id)
        .ok()
        .flatten()
    else {
        return false;
    };
    if effective_lifecycle_state(&session) != "running" {
        return false;
    }
    let now_iso = crate::presentation::now_iso();
    let as_run = ThreadProgress {
        resolved: false,
        has_run: true,
    };
    let waiting = classify_thread_session(Some(&session), as_run, &now_iso, false)
        == ThreadState::Waiting
        || coordinators::waits_on_screen(&session);
    waiting && waiting_report_prompt(&session).0 == key
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
    crate::coordinators::load_audit(&state.paths);
    super::side_chat_http::refresh_side_chats(state, &db, &repository);
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let deliveries = supervise(state, &db, &repository, &mut memory, threads);
    drop(memory);
    // CDXC:Coordinators 2026-11-06 WHY: the Workers rows and the board are built after the
    // supervisor's verdicts for this tick are published (coordinators/live.rs), so a thread's
    // state reaches the board on the tick it changed, from the same reading its reports use.
    for (project_id, session_id) in
        crate::coordinators::refresh_coordinator_panels(&db, &repository)
    {
        republish_session_chat(state, &project_id, &session_id);
    }
    deliveries
}

/// One tick of supervision over every thread: their states, their reports, and the verdicts the
/// other surfaces read (coordinators/live.rs).
fn supervise(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    memory: &mut SupervisorMemory,
    threads: Vec<ThreadRecord>,
) -> Vec<Delivery> {
    repair_empryo_coordinator_roles(state, db, repository, memory);
    // A thread its coordinator's close took with it waits for the coordinator's restore, which
    // brings it back; reporting it closed meanwhile would mark it done.
    let open = threads
        .into_iter()
        .filter(|thread| !thread.is_resolved() && thread.closed_with_coordinator_at.is_none())
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
    memory.stalls.retain(|key, _| open_keys.contains(key));
    memory.idle_waits.retain(|key, _| open_keys.contains(key));
    memory
        .activity_read_at
        .retain(|key, _| open_keys.contains(key));
    memory.turns.retain(|key, _| open_keys.contains(key));
    let mut live: HashMap<SessionKey, coordinators::ThreadLive> = HashMap::new();
    if open.is_empty() {
        coordinators::replace_thread_live(live);
        let changes =
            coordinators::replace_session_quiet(coordinators::QuietOwner::Threads, HashMap::new());
        super::session_stall_runtime::publish_stuck_changes(
            state,
            db,
            repository,
            coordinators::QuietOwner::Threads,
            changes,
        );
        return Vec::new();
    }
    let now = now_ms();
    let offline = follow_network(memory, crate::network_status::offline_since(), now);
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
                        names.get_or_insert_with(|| agent_names(repository)),
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
        // Its terminal ended under it: the self-heal restarts it and sends the one report about it,
        // so its stale "working" hooks neither count as quiet nor end as a finished turn.
        let heal_report = crate::session_self_heal::pending_report(&key);
        if heal_report.is_some()
            || crate::session_self_heal::daemon_lost(&thread.project_id, &thread.session_id)
        {
            memory.not_working_since.remove(&key);
            memory.stalls.remove(&key);
            memory.idle_waits.remove(&key);
            if let Some(report) = heal_report {
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
                            names.get_or_insert_with(|| agent_names(repository)),
                        ),
                        thread,
                        kind: ReportKind::SelfHeal { report },
                        session: Some(session),
                    });
            }
            continue;
        }
        // The supervisor reads the session's own state; "starting" is a presentation notion.
        let as_run = ThreadProgress {
            resolved: false,
            has_run: true,
        };
        let hook_state = classify_thread_session(Some(&session), as_run, &now_iso, false);
        let reported_ms = thread.reported_at.as_deref().and_then(parse_iso_ms_opt);
        let state_now = if is_running_empryo(&session) {
            empryo_thread_state(db, memory, &mut thread, &session, hook_state)
        } else if hook_state == ThreadState::Waiting && coordinators::waits_on_screen(&session) {
            ThreadState::Waiting
        } else if hook_state != ThreadState::Working
            && hook_state != ThreadState::Sleeping
            && thread.observed_working
            && coordinators::thread_prompt(&session).is_none()
        {
            // CDXC:Coordinators 2026-11-02 WHY: A question or permission prompt keeps the asking turn open in the transcript, so the transcript check read a Claude thread stuck on a permission prompt as working and its orchestrator got no "waiting for an answer" until someone's screen read recorded a screen wait (tester report, 2026-11-02). The stored card the hook wrote is the stronger evidence, so the transcript is not asked while one is shown.
            // Only a thread about to be reported pays for the transcript read.
            let reading = read_turn(
                memory.transcript_gates.entry(key.clone()).or_default(),
                &session,
            );
            let working = reading.open;
            memory.turns.insert(key.clone(), reading);
            if working {
                ThreadState::Working
            } else {
                hook_state
            }
        } else if hook_state == ThreadState::Finished
            && coordinators::thread_prompt(&session).is_none()
        {
            // CDXC:Coordinators 2026-11-06 WHY: a turn a Ghostex nudge, a coordinator's message or a
            // queued task notification starts after the thread's last report can run without its
            // hooks saying so (they were reset to idle by a restart or a wake, or the harness turn
            // fires none), so the board called it parked and its end was never reported. Its
            // transcript says whether a turn newer than that report is open; the read is cached
            // until the file changes, so an idle thread costs a `stat` a tick.
            let reading = read_turn(
                memory.transcript_gates.entry(key.clone()).or_default(),
                &session,
            );
            let new_turn = reading.open
                && match (reading.started_ms, reported_ms) {
                    (Some(started), Some(reported)) => started > reported,
                    (_, None) => true,
                    (None, Some(_)) => false,
                };
            memory.turns.insert(key.clone(), reading);
            if new_turn {
                ThreadState::Working
            } else {
                hook_state
            }
        } else {
            hook_state
        };
        // A working thread's line, and whether a sleeping thread's turn never ended, come from its
        // transcript too; a working one is read at most every `ACTIVITY_READ_EVERY_MS`.
        let read_due = match state_now {
            ThreadState::Working => memory
                .activity_read_at
                .get(&key)
                .is_none_or(|at| now - at >= ACTIVITY_READ_EVERY_MS),
            ThreadState::Sleeping => true,
            _ => false,
        };
        if read_due {
            memory.activity_read_at.insert(key.clone(), now);
            let reading = read_turn(
                memory.transcript_gates.entry(key.clone()).or_default(),
                &session,
            );
            memory.turns.insert(key.clone(), reading);
        }
        let turn_open = state_now != ThreadState::Working
            && memory.turns.get(&key).is_some_and(|turn| turn.open);
        if state_now != ThreadState::Working {
            memory.stalls.remove(&key);
        }
        if !matches!(state_now, ThreadState::Finished | ThreadState::Sleeping) {
            memory.idle_waits.remove(&key);
        }
        let report = match state_now {
            ThreadState::Working => {
                memory.not_working_since.remove(&key);
                if offline {
                    memory.held_coordinators.insert(coordinator_key.clone());
                }
                if !thread.observed_working {
                    let _ = set_thread_observed_working(db, &thread.project_id, &thread.session_id);
                }
                memory
                    .stalls
                    .entry(key.clone())
                    .or_insert_with(|| coordinators::StallWatch::new(now))
                    .observe(&session, now)
                    .map(|due| ReportKind::Stalled { due })
            }
            ThreadState::Waiting => {
                memory.not_working_since.remove(&key);
                let (prompt_key, mut summary) = waiting_report_prompt(&session);
                // A card the coordinator's chat relays is the user's to answer there
                // (coordinators/relay.rs).
                if coordinators::thread_needs_user_approval(&session)
                    && coordinators::relay_card(&session).is_some()
                {
                    summary = format!("{summary}\n\n{}", coordinators::RELAYED_REPORT_NOTE);
                }
                (thread.reported_prompt_key.as_deref() != Some(prompt_key.as_str())).then_some(
                    ReportKind::Waiting {
                        key: prompt_key,
                        summary,
                    },
                )
            }
            ThreadState::Finished | ThreadState::Sleeping if thread.observed_working => {
                let since = *memory.not_working_since.entry(key.clone()).or_insert(now);
                (now - since >= FINISH_STABILITY_MS).then_some(ReportKind::Finished {
                    cut_off: state_now == ThreadState::Sleeping && turn_open,
                })
            }
            ThreadState::Finished | ThreadState::Sleeping => {
                idle_wait_report(memory, &thread, &session, turn_open, now)
            }
            _ => None,
        };
        let turn = memory.turns.get(&key);
        live.insert(
            key.clone(),
            coordinators::ThreadLive {
                working: state_now == ThreadState::Working,
                activity: (state_now == ThreadState::Working)
                    .then(|| {
                        turn.and_then(|turn| turn.last_words.clone().or(turn.started_by.clone()))
                    })
                    .flatten()
                    .map(|words| coordinators::report_headline(&words, 140)),
                turn_started_ms: (state_now == ThreadState::Working)
                    .then(|| turn.and_then(|turn| turn.started_ms))
                    .flatten(),
                unfinished: matches!(state_now, ThreadState::Finished | ThreadState::Sleeping)
                    .then(|| {
                        memory
                            .idle_waits
                            .get(&key)
                            .and_then(|watch| watch.reason().map(|reason| (reason, watch.since())))
                    })
                    .flatten(),
            },
        );
        let report = match report {
            Some(report) => Some(report),
            None => pending_delivery(db, memory, &thread, &session, state_now, now),
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
                        names.get_or_insert_with(|| agent_names(repository)),
                    ),
                    thread,
                    kind,
                    session: Some(session),
                });
        }
    }
    // A thread whose working verdict changed republishes its session, so the sidebar's
    // `coordinatorThreadState` (coordinators/presentation.rs) follows without waiting for a hook.
    let flipped: Vec<SessionKey> = live
        .iter()
        .filter(|(key, entry)| {
            coordinators::thread_live(&key.0, &key.1).map(|previous| previous.working)
                != Some(entry.working)
        })
        .map(|(key, _)| key.clone())
        .collect();
    coordinators::replace_thread_live(live);
    for (project_id, session_id) in flipped {
        let _ =
            schedule_presentation_session_delta(state, db, repository, &project_id, &session_id);
    }
    let changes = coordinators::replace_session_quiet(
        coordinators::QuietOwner::Threads,
        memory
            .stalls
            .iter()
            .map(|(key, watch)| {
                (
                    key.clone(),
                    watch.view(now, coordinators::THREAD_STUCK_ROUNDS),
                )
            })
            .chain(
                memory
                    .idle_waits
                    .iter()
                    .filter_map(|(key, watch)| Some((key.clone(), watch.view(now)?))),
            )
            .collect(),
    );
    super::session_stall_runtime::publish_stuck_changes(
        state,
        db,
        repository,
        coordinators::QuietOwner::Threads,
        changes,
    );
    let mut deliveries = Vec::new();
    if offline {
        memory.held_coordinators.extend(pending.into_keys());
        return deliveries;
    }
    let mut pending = pending;
    for coordinator in memory.back_lines.keys() {
        pending.entry(coordinator.clone()).or_default();
    }
    for (coordinator, reports) in pending {
        if memory.in_flight.contains(&coordinator) {
            continue;
        }
        // A coordinator whose own terminal ended hears once the self-heal brought it back.
        if crate::session_self_heal::daemon_lost(&coordinator.0, &coordinator.1) {
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
        let back_line = memory.back_lines.get(&coordinator).cloned();
        deliveries.push(Delivery {
            coordinator,
            reports,
            back_line,
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
    if !coordinator_open && delivery.back_line.is_some() {
        // A closed coordinator hears its threads' reports when it is resumed; news of an outage
        // that ended is stale by then.
        if let Ok(mut memory) = memory.lock() {
            memory.back_lines.remove(&coordinator);
        }
    }
    if let Some(line) = delivery.back_line.filter(|_| coordinator_open) {
        let ledger_key = (coordinator.clone(), line.clone());
        let send_request_id = memory
            .lock()
            .map(|mut memory| {
                memory
                    .report_send_ids
                    .entry(ledger_key.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            })
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        match send_to_coordinator(&state, &coordinator, &line, &send_request_id).await {
            Ok(()) => {
                if let Ok(mut memory) = memory.lock() {
                    memory.report_send_ids.remove(&ledger_key);
                    memory.back_lines.remove(&coordinator);
                }
            }
            Err(_) => failed = true,
        }
    }
    let reports = if failed { Vec::new() } else { reports };
    for report in reports {
        if let ReportKind::Waiting { key, .. } = &report.kind {
            let (check_state, thread, key) = (state.clone(), report.thread.clone(), key.clone());
            let waits =
                tokio::task::spawn_blocking(move || still_waits_on(&check_state, &thread, &key))
                    .await
                    .unwrap_or(true);
            if !waits {
                continue;
            }
        }
        // A stall report worded on the tick is dropped when the transcript moved since; its first
        // round nudges the thread before the coordinator hears of it.
        let mut stall_last_row = None;
        let mut stall_nudge = None;
        if let ReportKind::Stalled { due } = &report.kind {
            if crate::session_self_heal::daemon_lost(
                &report.thread.project_id,
                &report.thread.session_id,
            ) {
                continue;
            }
            let (check_state, thread) = (state.clone(), report.thread.clone());
            let (seen, quiet_since) = (due.transcript_seen, due.quiet_since);
            // The session as it is now, not as the tick saw it: a reload in between changes its
            // transcript and its launch stamp.
            let row = tokio::task::spawn_blocking(move || {
                fresh_session(&check_state, &thread)
                    .and_then(|session| coordinators::stall_last_row(&session, seen, quiet_since))
            })
            .await
            .ok()
            .flatten();
            match row {
                Some(row) => stall_last_row = row,
                None => continue,
            }
            if due.round == 1 {
                stall_nudge = Some(nudge_thread(&state, &memory, &report.thread, due).await);
            }
        }
        // An idle report is dropped when the thread started a turn since; its first round nudges
        // the thread (which wakes a sleeping one) before the coordinator hears of it.
        if let ReportKind::IdleWaiting { due } = &report.kind {
            let (check_state, thread) = (state.clone(), report.thread.clone());
            let idle = tokio::task::spawn_blocking(move || still_idle(&check_state, &thread))
                .await
                .unwrap_or(true);
            if !idle {
                continue;
            }
            if due.round == 1 {
                stall_nudge = Some(
                    send_nudge(
                        &state,
                        &memory,
                        &report.thread.key(),
                        due.reason.nudge_message(),
                        idle_nudge_send_key(due),
                    )
                    .await,
                );
            }
        }
        let thread_ref = report.sender.global_ref.clone();
        let (body, finished_text) = match &report.kind {
            ReportKind::Finished { cut_off } => {
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
                    &if *cut_off {
                        ThreadReport::CutOff {
                            message: message.as_deref(),
                        }
                    } else {
                        ThreadReport::Finished {
                            message: message.as_deref(),
                        }
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
            ReportKind::IdleWaiting { due } => (
                report_body(
                    &ThreadReport::IdleWaiting {
                        // Whole rounds, so a retried report is the same text (and send).
                        idle_minutes: coordinators::minutes_of(
                            i64::from(due.round) * coordinators::idle_round_ms(),
                        ),
                        idle_since: &coordinators::iso_from_ms(due.since).unwrap_or_default(),
                        reason: due.reason.into(),
                        nudge: match &stall_nudge {
                            Some(Ok(())) => coordinators::StallNudge::Sent,
                            Some(Err(error)) => coordinators::StallNudge::Failed(error),
                            None => coordinators::StallNudge::Earlier,
                        },
                        stuck: due.round >= coordinators::IDLE_STUCK_ROUNDS,
                    },
                    &thread_ref,
                ),
                None,
            ),
            ReportKind::SelfHeal { report } => (
                crate::session_self_heal::report_text(report, &thread_ref),
                None,
            ),
            ReportKind::Stalled { due } => (
                report_body(
                    &ThreadReport::Stalled {
                        // Whole rounds, so a retried report is the same text (and send).
                        quiet_minutes: coordinators::minutes_of(
                            i64::from(due.round) * coordinators::quiet_round_ms(),
                        ),
                        quiet_since: &coordinators::iso_from_ms(due.quiet_since)
                            .unwrap_or_default(),
                        last_row: stall_last_row.as_deref(),
                        nudge: match &stall_nudge {
                            Some(Ok(())) => coordinators::StallNudge::Sent,
                            Some(Err(error)) => coordinators::StallNudge::Failed(error),
                            None => coordinators::StallNudge::Earlier,
                        },
                        stuck: due.round >= coordinators::THREAD_STUCK_ROUNDS,
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
                    if let ReportKind::Stalled { due } = &report.kind {
                        memory
                            .report_send_ids
                            .remove(&(report.thread.key(), nudge_send_key(due)));
                        if let Some(watch) = memory.stalls.get_mut(&report.thread.key()) {
                            watch.mark_reported(due.quiet_since, due.round);
                        }
                    }
                    if let ReportKind::IdleWaiting { due } = &report.kind {
                        memory
                            .report_send_ids
                            .remove(&(report.thread.key(), idle_nudge_send_key(due)));
                        if let Some(watch) = memory.idle_waits.get_mut(&report.thread.key()) {
                            watch.mark_reported(due.round);
                        }
                    }
                }
                if let ReportKind::SelfHeal { report: heal } = &report.kind {
                    crate::session_self_heal::ack_report(&report.thread.key(), heal.id);
                }
                // A stall, idle or self-heal report changes nothing on the thread's record.
                if matches!(
                    report.kind,
                    ReportKind::Stalled { .. }
                        | ReportKind::IdleWaiting { .. }
                        | ReportKind::SelfHeal { .. }
                ) {
                    continue;
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

/// The send ledger key of a quiet stretch's nudge, so a report retried after a failure does not
/// type the nudge again.
fn nudge_send_key(due: &coordinators::StallDue) -> String {
    format!("nudge:{}", due.quiet_since)
}

/// The send ledger key of an idle stretch's nudge.
fn idle_nudge_send_key(due: &coordinators::IdleDue) -> String {
    format!("idle-nudge:{}", due.since)
}

/// Sends a thread a nudge the way a coordinator's message goes (typed now, taken by a busy agent
/// at its next input boundary, a sleeping one woken for it), under a stable `sendRequestId` so a
/// retried report never types it twice.
async fn send_nudge(
    state: &Arc<AppState>,
    memory: &Arc<Mutex<SupervisorMemory>>,
    key: &SessionKey,
    text: &str,
    ledger_key: String,
) -> std::result::Result<(), String> {
    // A bot's conversation (Hermes) takes no nudge (CDXC:Bots 2026-11-08, bot_projects.rs).
    let (checking, project_id) = (state.clone(), key.0.clone());
    let bot = tokio::task::spawn_blocking(move || {
        open_gxserver_database(&checking.paths).is_ok_and(|db| {
            crate::bot_projects::is_bot_conversation(
                &DomainRepository::new(&db, checking.metadata.server_id.as_str()),
                &project_id,
            )
        })
    })
    .await
    .unwrap_or(false);
    if bot {
        return Err("it is a bot's conversation, which Ghostex never nudges".to_string());
    }
    let send_request_id = memory
        .lock()
        .map(|mut memory| {
            memory
                .report_send_ids
                .entry((key.clone(), ledger_key))
                .or_insert_with(|| Uuid::new_v4().to_string())
                .clone()
        })
        .unwrap_or_else(|_| Uuid::new_v4().to_string());
    send_to_coordinator(state, key, text, &send_request_id)
        .await
        .map_err(|error| error.chars().take(200).collect())
}

/// Sends a quiet thread `nudge_message()` and starts the watch's grace for it.
async fn nudge_thread(
    state: &Arc<AppState>,
    memory: &Arc<Mutex<SupervisorMemory>>,
    thread: &ThreadRecord,
    due: &coordinators::StallDue,
) -> std::result::Result<(), String> {
    let key = thread.key();
    let sent = send_nudge(
        state,
        memory,
        &key,
        &coordinators::nudge_message(),
        nudge_send_key(due),
    )
    .await;
    if sent.is_ok() {
        if let Ok(mut memory) = memory.lock() {
            if let Some(watch) = memory.stalls.get_mut(&key) {
                watch.note_nudged(now_ms());
            }
        }
    }
    sent
}

/// The idle watch of a thread whose turn ended and was reported: its report when one is due.
fn idle_wait_report(
    memory: &mut SupervisorMemory,
    thread: &ThreadRecord,
    session: &Value,
    turn_open: bool,
    now: i64,
) -> Option<ReportKind> {
    let since = thread.reported_at.as_deref().and_then(parse_iso_ms_opt)?;
    let watch = memory
        .idle_waits
        .entry(thread.key())
        .or_insert_with(|| coordinators::IdleWatch::new(since));
    // A later report means a later turn ended: its idle clock starts there.
    if watch.since() != since {
        *watch = coordinators::IdleWatch::new(since);
    }
    watch
        .observe(session, thread, turn_open, now)
        .map(|due| ReportKind::IdleWaiting { due })
}

/// BLOCKING. A thread's session row as it is now.
fn fresh_session(state: &AppState, thread: &ThreadRecord) -> Option<Value> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .get_session(&thread.project_id, &thread.session_id)
        .ok()
        .flatten()
}

/// Whether a thread still sits between turns (not working, not waiting on someone), read again
/// right before its idle report goes out.
fn still_idle(state: &AppState, thread: &ThreadRecord) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(session) = repository
        .get_session(&thread.project_id, &thread.session_id)
        .ok()
        .flatten()
    else {
        return false;
    };
    let as_run = ThreadProgress {
        resolved: false,
        has_run: true,
    };
    matches!(
        // The supervisor's own transcript reading counts: a turn its hooks missed is a new turn.
        coordinators::classify_thread_live(Some(&session), as_run, &crate::presentation::now_iso(),),
        ThreadState::Finished | ThreadState::Sleeping
    )
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

pub(super) fn coordinator_is_open(state: &AppState, coordinator: &SessionKey) -> bool {
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
pub(super) async fn send_to_coordinator(
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
pub(crate) fn republish_session_chat(state: &AppState, project_id: &str, session_id: &str) {
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
    let skill_only_start = coordinators::coordinator_create_request(&params)?
        .is_some_and(|request| request.first_request.is_empty());
    let runtime = params
        .entry("runtimeSettings".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(runtime) = runtime.as_object_mut() {
        runtime.insert(
            coordinators::COORDINATOR_ROLE_SETTING.to_string(),
            json!(coordinators::COORDINATOR_ROLE_SKILL),
        );
        if skill_only_start {
            runtime.insert(
                coordinators::COORDINATOR_SKILL_ONLY_START_SETTING.to_string(),
                json!(true),
            );
        }
    }
    Ok(params)
}

/// Hands a just-created orchestrator its role: installs the `ghostex-orchestrator` skill for its
/// agent and queues its first message (the skill line, then the create's first request) as a
/// startup send. A ZCode or OpenCode orchestrator's chosen model goes first as a `/model` line
/// (OpenCode's with the chosen variant after it), because neither has a launch model flag and the
/// first message must run on the model the user picked.
pub(crate) fn start_orchestrator_role(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    family: &str,
    params: &Map<String, Value>,
) -> std::result::Result<(), DomainStateError> {
    coordinators::ensure_orchestrator_skill_installed(&state.paths, family)?;
    if let Some(line) = coordinator_model_line(family, params) {
        queue_session_chat_prompt(state, project_id, session_id, &line, true)?;
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

/// The `/model` line a new ZCode or OpenCode orchestrator's chosen model goes in ahead of its first
/// message: `/model <model>` for ZCode, `/model <provider/model> [<variant>]` for OpenCode (its send
/// path sets both through OpenCode's server, `session_chat_opencode::Client::send`).
fn coordinator_model_line(family: &str, params: &Map<String, Value>) -> Option<String> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let model = text("agentModel")?;
    match family {
        "zcode" => Some(format!("/model {model}")),
        "opencode" => Some(match text("agentEffort") {
            Some(effort) => format!("/model {model} {effort}"),
            None => format!("/model {model}"),
        }),
        _ => None,
    }
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
    // A board card's tag on a thread is its session's tag, so its sidebar row follows at once.
    if endpoint_path == "/api/coordinatorBoard"
        && params.get("op").and_then(Value::as_str) == Some("tag")
    {
        let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default();
        if text("threadServerId").is_empty() && !text("threadSessionId").is_empty() {
            schedule_presentation_session_delta(
                state,
                db,
                repository,
                text("threadProjectId"),
                text("threadSessionId"),
            )?;
        }
    }
    // The orchestrator's Threads panel is a cache the supervisor rebuilds every tick; an unlinked
    // thread must leave it now, not on the next tick.
    // A board change shows on the coordinator's chat now too (board.rs).
    let board_changed = endpoint_path == "/api/coordinatorBoard"
        && params
            .get("op")
            .and_then(Value::as_str)
            .is_some_and(|op| op != "read");
    if endpoint_path == "/api/unlinkCoordinatorThread" || board_changed {
        for (project_id, session_id) in
            crate::coordinators::refresh_coordinator_panels(db, repository)
        {
            republish_session_chat(state, &project_id, &session_id);
        }
    }
    Ok(output.result)
}

#[cfg(test)]
mod skill_only_start_tests {
    use super::prepare_coordinator_create_params;
    use serde_json::{json, Map, Value};

    fn prepared(coordinator: Value) -> Value {
        let params = json!({ "coordinator": coordinator });
        let params: Map<String, Value> = params.as_object().unwrap().clone();
        Value::Object(prepare_coordinator_create_params(&params).unwrap())
    }

    #[test]
    fn a_coordinator_without_a_first_request_is_marked_skill_only() {
        let params = prepared(json!({ "goal": "Ship it" }));
        assert_eq!(
            params["runtimeSettings"]["coordinatorSkillOnlyStart"],
            json!(true)
        );
    }

    #[test]
    fn a_coordinator_with_a_first_request_is_not() {
        let params = prepared(json!({ "firstRequest": "Fix the build" }));
        assert!(params["runtimeSettings"]
            .get("coordinatorSkillOnlyStart")
            .is_none());
    }
}

#[cfg(test)]
mod offline_tests {
    use super::*;

    fn key(id: &str) -> SessionKey {
        ("p".to_string(), id.to_string())
    }

    #[test]
    fn each_held_coordinator_hears_once_when_the_network_is_back() {
        let mut memory = SupervisorMemory::default();
        assert!(!follow_network(&mut memory, None, 0));
        assert!(follow_network(&mut memory, Some(1_000), 2_000));
        memory.held_coordinators.insert(key("c1"));
        memory.held_coordinators.insert(key("c2"));
        // Many offline ticks, still one line each.
        for now in (4_000..60_000).step_by(2_000) {
            assert!(follow_network(&mut memory, Some(1_000), now));
            memory.held_coordinators.insert(key("c1"));
        }
        assert!(
            memory.back_lines.is_empty(),
            "nothing is said while offline"
        );
        assert!(!follow_network(&mut memory, None, 106 * 60_000 + 1_000));
        assert_eq!(memory.back_lines.len(), 2);
        assert!(memory.held_coordinators.is_empty());
        let line = &memory.back_lines[&key("c1")];
        assert!(line.starts_with("Ghostex: this computer was offline from "));
        assert!(line.contains("(1 hour 46 minutes)"), "{line}");
        // Online ticks after that owe nothing more.
        assert!(!follow_network(&mut memory, None, 107 * 60_000));
        assert_eq!(memory.back_lines.len(), 2);
    }
}
