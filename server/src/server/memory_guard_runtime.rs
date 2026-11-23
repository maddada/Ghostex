//! The memory guard's runtime: every five seconds it reads the system's memory pressure, and once
//! the kernel has said "critical" for ten seconds it takes one step on the agent session using the
//! most memory, then waits 30 seconds before the next (the steps and words: crate::memory_guard).
//!
//! CDXC:MemoryGuard 2026-11-14 WHY:
//! A session is stopped through `/api/sleepSession`, never by ending its daemon directly: self-heal
//! (session_self_heal_runtime.rs) restarts a running session whose daemon vanished, so a direct kill
//! would be healed straight back into the same memory. A slept row is left alone by self-heal, and
//! its Resume is the ordinary wake. zmx's own kill ends the shell's process group; the session's
//! other processes (the agent's own group, a detached dev server) that are still running two
//! seconds later are ended here.
//! SEE-ALSO: server/src/memory_guard/mod.rs (the CDXC:MemoryGuard decision),
//! server/src/session_chat_agent_restart.rs (the card's Resume button).

use super::*;

use crate::memory_guard::{
    self as guard, alive_members, session_usage, signal_processes, snapshot_user_processes,
    ChildGroup, PressureLevel, SessionUsage, Step, StopSignal,
};
use crate::session_chat_notice::{
    set_session_chat_watchdog_notice, SessionChatTerminalNoticeSeverity,
};

type Key = (String, String);

const TICK: Duration = Duration::from_secs(5);
/// How long the kernel must say critical before the first step.
const CRITICAL_HOLD: Duration = Duration::from_secs(10);
/// How long a step gets to free memory before the next one.
const SETTLE: Duration = Duration::from_secs(30);
/// A session's agent is restarted for memory at most once in this window.
const RESTART_WINDOW: Duration = Duration::from_secs(600);
/// How long a stopped command gets to end on SIGTERM before SIGKILL.
const TERM_GRACE: Duration = Duration::from_secs(5);
/// How long a slept session's processes get to end before the leftovers are killed.
const SLEEP_GRACE: Duration = Duration::from_secs(2);
/// Minimum interval between repeated Warning logs and process-tree scans when memory pressure
/// oscillates between Normal and Warning; Critical pressure always logs and acts without delay.
const WARN_COOLDOWN: Duration = Duration::from_secs(60);

/// What the guard remembers between ticks.
#[derive(Default)]
struct Book {
    /// When the current critical stretch began.
    critical_since: Option<Instant>,
    /// The warning (or critical) stretch was logged with the sessions' sizes.
    warned: bool,
    /// When `memoryGuardWarning` was last logged (survives brief dips back to Normal so flapping
    /// pressure does not re-scan every process and flood gxserver.jsonl).
    last_warned: Option<Instant>,
    /// Nothing big enough to stop was logged for this stretch.
    reported_nothing: bool,
    last_step: Option<Instant>,
    /// Sessions the guard already acted on in this stretch: the next step on one is a stop.
    acted: HashSet<Key>,
    restarted: HashMap<Key, Instant>,
    /// Sessions known to carry a `memoryGuardStopped` runtime record, seeded once from SQLite so
    /// the 5-second tick does not full-scan the `sessions` table when no session is guard-stopped.
    stopped_records_seeded: bool,
    stopped_records: HashSet<Key>,
}

static BOOK: std::sync::LazyLock<Mutex<Book>> = std::sync::LazyLock::new(Default::default);

fn book() -> std::sync::MutexGuard<'static, Book> {
    BOOK.lock().unwrap_or_else(|error| error.into_inner())
}

pub(crate) fn start_memory_guard_runtime(state: Arc<AppState>) {
    if !guard::supported() {
        return;
    }
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let planning = state.clone();
            let plan = tokio::task::spawn_blocking(move || plan(&planning))
                .await
                .ok()
                .flatten();
            if let Some(plan) = plan {
                act(&state, plan).await;
            }
        }
    });
}

/// An agent session of this computer the guard may act on, with its usage.
struct Candidate {
    key: Key,
    agent: String,
    session: Value,
    usage: SessionUsage,
}

enum Plan {
    StopCommand(Candidate, ChildGroup),
    RestartAgent(Candidate),
    StopSession(Candidate),
}

fn text_field(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Every running agent session in a zmx daemon here, with what it uses, biggest first. BLOCKING.
fn measure(state: &AppState) -> Vec<Candidate> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.running_sessions() else {
        return Vec::new();
    };
    let sessions = sessions
        .iter()
        .filter_map(|session| {
            if session.get("kind").and_then(Value::as_str) != Some("agent")
                || session_state_sync::read_session_persistence_provider(session).as_deref()
                    != Some("zmx")
                || crate::agentbox::is_agentbox_session(session)
            {
                return None;
            }
            let key = (
                text_field(session, "projectId")?,
                text_field(session, "sessionId")?,
            );
            let zmx_name = crate::zmx::provider_zmx_session_name(session).ok()?;
            Some((key, zmx_name, session.clone()))
        })
        .collect::<Vec<_>>();
    if sessions.is_empty() {
        return Vec::new();
    }
    let names = sessions
        .iter()
        .map(|(_, name, _)| name.clone())
        .collect::<Vec<_>>();
    let agents =
        crate::zmx::read_display_zmx_session_process_identities(&names, &state.paths.home_dir)
            .unwrap_or_default();
    let rows = snapshot_user_processes();
    let mut candidates = sessions
        .into_iter()
        .filter_map(|(key, zmx_name, session)| {
            let agent_pid = agents
                .get(&zmx_name)
                .and_then(|identity| identity.process_id)
                .and_then(|pid| u32::try_from(pid).ok());
            let usage = session_usage(&rows, &zmx_name, agent_pid)?;
            let agent = guard::agent_name(
                crate::session_chat_follower::session_chat_agent_for_session(&session).as_deref(),
            );
            Some(Candidate {
                key,
                agent,
                session,
                usage,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.usage.total.cmp(&a.usage.total));
    candidates
}

fn sizes(candidates: &[Candidate]) -> Value {
    json!(candidates
        .iter()
        .take(5)
        .map(|candidate| json!({
            "projectId": candidate.key.0,
            "sessionId": candidate.key.1,
            "bytes": candidate.usage.total,
            "agentBytes": candidate.usage.agent_bytes,
            "biggestCommand": candidate.usage.groups.first().map(|group| json!({
                "label": group.label,
                "bytes": group.bytes,
            })),
        }))
        .collect::<Vec<_>>())
}

/// One tick's decision. BLOCKING (settings file, SQLite, a process scan while under pressure).
fn plan(state: &AppState) -> Option<Plan> {
    clear_records_of_woken_sessions(state);
    let settings = crate::shared_settings_file::read_shared_settings(&state.paths);
    if !guard::setting_on(settings.as_ref(), guard::ENABLED_SETTING) {
        let mut book = book();
        let stopped_records_seeded = book.stopped_records_seeded;
        let stopped_records = std::mem::take(&mut book.stopped_records);
        *book = Book {
            stopped_records_seeded,
            stopped_records,
            ..Book::default()
        };
        return None;
    }
    let level = guard::read_pressure_level()?;
    let now = Instant::now();
    if level == PressureLevel::Normal {
        let mut book = book();
        if book.warned {
            log(
                state,
                "memoryGuardPressureNormal",
                LogLevel::Info,
                json!({}),
            );
        }
        let restarted = std::mem::take(&mut book.restarted);
        let last_warned = book.last_warned;
        let stopped_records_seeded = book.stopped_records_seeded;
        let stopped_records = std::mem::take(&mut book.stopped_records);
        *book = Book {
            last_warned,
            restarted: restarted
                .into_iter()
                .filter(|(_, at)| now.duration_since(*at) < RESTART_WINDOW)
                .collect(),
            stopped_records_seeded,
            stopped_records,
            ..Book::default()
        };
        return None;
    }
    let (should_log, hold_over, settled) = {
        let mut book = book();
        let critical_first_sight = level == PressureLevel::Critical && book.critical_since.is_none();
        let warning_due = !book.warned
            && book
                .last_warned
                .is_none_or(|at| now.duration_since(at) >= WARN_COOLDOWN);
        let should_log = critical_first_sight || warning_due;
        if should_log {
            book.warned = true;
            if level == PressureLevel::Warning {
                book.last_warned = Some(now);
            }
        }
        if level == PressureLevel::Critical {
            book.critical_since.get_or_insert(now);
        } else {
            book.critical_since = None;
        }
        let hold_over = book
            .critical_since
            .is_some_and(|since| now.duration_since(since) >= CRITICAL_HOLD);
        let settled = book
            .last_step
            .is_none_or(|at| now.duration_since(at) >= SETTLE);
        (should_log, hold_over, settled)
    };
    if should_log {
        let candidates = measure(state);
        log(
            state,
            if level == PressureLevel::Critical {
                "memoryGuardCritical"
            } else {
                "memoryGuardWarning"
            },
            LogLevel::Warn,
            json!({ "level": level.as_str(), "sessions": sizes(&candidates) }),
        );
    }
    if !hold_over || !settled {
        return None;
    }
    let candidates = measure(state);
    let Some(heaviest) = candidates
        .into_iter()
        .find(|candidate| candidate.usage.total >= guard::min_session_bytes())
    else {
        let mut book = book();
        if !book.reported_nothing {
            book.reported_nothing = true;
            drop(book);
            log(
                state,
                "memoryGuardNothingToStop",
                LogLevel::Warn,
                json!({ "minimumBytes": guard::min_session_bytes() }),
            );
        }
        return None;
    };
    let mut book = book();
    let escalate = book.acted.contains(&heaviest.key);
    let restarted_recently = book
        .restarted
        .get(&heaviest.key)
        .is_some_and(|at| now.duration_since(*at) < RESTART_WINDOW);
    let step = heaviest.usage.next_step(escalate, restarted_recently);
    book.last_step = Some(now);
    book.acted.insert(heaviest.key.clone());
    if step == Step::RestartAgent {
        book.restarted.insert(heaviest.key.clone(), now);
    }
    drop(book);
    Some(match step {
        Step::StopCommand(index) => {
            let group = heaviest.usage.groups[index].clone();
            Plan::StopCommand(heaviest, group)
        }
        Step::RestartAgent => Plan::RestartAgent(heaviest),
        Step::StopSession => Plan::StopSession(heaviest),
    })
}

async fn act(state: &Arc<AppState>, plan: Plan) {
    match plan {
        Plan::StopCommand(candidate, group) => stop_command(state, candidate, group).await,
        Plan::RestartAgent(candidate) => restart_agent(state, candidate).await,
        Plan::StopSession(candidate) => stop_session(state, candidate).await,
    }
}

/// SIGTERM, then SIGKILL for what is still running after the grace period. The number of
/// processes that had to be killed.
async fn end_members(members: &[guard::Member]) -> usize {
    let members = members.to_vec();
    let _ = tokio::task::spawn_blocking({
        let members = members.clone();
        move || signal_processes(&members, StopSignal::Term)
    })
    .await;
    tokio::time::sleep(TERM_GRACE).await;
    tokio::task::spawn_blocking(move || {
        signal_processes(&alive_members(&members), StopSignal::Kill)
    })
    .await
    .unwrap_or(0)
}

async fn stop_command(state: &Arc<AppState>, candidate: Candidate, group: ChildGroup) {
    let killed = end_members(&group.members).await;
    log(
        state,
        "memoryGuardStoppedCommand",
        LogLevel::Warn,
        json!({
            "projectId": candidate.key.0,
            "sessionId": candidate.key.1,
            "label": group.label,
            "command": group.command,
            "bytes": group.bytes,
            "sessionBytes": candidate.usage.total,
            "processes": group.members.len(),
            "killedAfterGrace": killed,
        }),
    );
    let words = guard::stopped_command_card(&group, &candidate.agent);
    show_card(
        state,
        &candidate.key,
        &words,
        SessionChatTerminalNoticeSeverity::Info,
        false,
    );
    notify(state, &candidate, &words.detail).await;
    // The note lands in the agent's own chat at once: an interrupt ends the step it was waiting
    // on (the command that just died), then the note starts its next one.
    let request = Uuid::new_v4().to_string();
    let body = json!({ "params": {
        "projectId": candidate.key.0,
        "sessionId": candidate.key.1,
        "sendRequestId": request,
    }});
    let interrupted = crate::session_chat_send::handle_interrupt_session_chat_http(
        state,
        "/api/interruptSessionChat".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    let sent = send(
        state,
        &candidate.key,
        &guard::stopped_command_message(&group),
        &request,
    )
    .await;
    log(
        state,
        "memoryGuardToldAgent",
        LogLevel::Warn,
        json!({
            "projectId": candidate.key.0,
            "sessionId": candidate.key.1,
            "interrupted": interrupted.response.status().is_success(),
            "sent": sent.is_ok(),
            "error": sent.err(),
        }),
    );
}

async fn restart_agent(state: &Arc<AppState>, candidate: Candidate) {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(candidate.key.0));
    params.insert("sessionId".into(), json!(candidate.key.1));
    let reloaded = crate::session_chat_agent_restart::reload(state, &params).await;
    if let Err(error) = &reloaded {
        log(
            state,
            "memoryGuardRestartFailed",
            LogLevel::Warn,
            json!({ "projectId": candidate.key.0, "sessionId": candidate.key.1, "error": error.message }),
        );
        // A restart that could not run frees nothing: the session is stopped instead.
        return stop_session(state, candidate).await;
    }
    tokio::time::sleep(SLEEP_GRACE).await;
    let leftovers = candidate.usage.members.clone();
    let killed = tokio::task::spawn_blocking(move || {
        signal_processes(&alive_members(&leftovers), StopSignal::Kill)
    })
    .await
    .unwrap_or(0);
    log(
        state,
        "memoryGuardRestartedAgent",
        LogLevel::Warn,
        json!({
            "projectId": candidate.key.0,
            "sessionId": candidate.key.1,
            "agentBytes": candidate.usage.agent_bytes,
            "sessionBytes": candidate.usage.total,
            "leftoversKilled": killed,
        }),
    );
    let words = guard::restarted_agent_card(&candidate.agent, &candidate.usage);
    show_card(
        state,
        &candidate.key,
        &words,
        SessionChatTerminalNoticeSeverity::Info,
        false,
    );
    notify(state, &candidate, &words.detail).await;
    let sent = send(
        state,
        &candidate.key,
        &guard::restarted_agent_message(&candidate.usage),
        &Uuid::new_v4().to_string(),
    )
    .await;
    if let Err(error) = sent {
        log(
            state,
            "memoryGuardToldAgent",
            LogLevel::Warn,
            json!({ "projectId": candidate.key.0, "sessionId": candidate.key.1, "sent": false, "error": error }),
        );
    }
}

async fn stop_session(state: &Arc<AppState>, candidate: Candidate) {
    let body = json!({ "params": { "projectId": candidate.key.0, "sessionId": candidate.key.1 } });
    let routed = super::zmx_http::handle_zmx_lifecycle_http(
        state,
        "/api/sleepSession".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    let slept = routed.response.status().is_success();
    tokio::time::sleep(SLEEP_GRACE).await;
    let members = candidate.usage.members.clone();
    let killed = tokio::task::spawn_blocking(move || {
        signal_processes(&alive_members(&members), StopSignal::Kill)
    })
    .await
    .unwrap_or(0);
    log(
        state,
        "memoryGuardStoppedSession",
        LogLevel::Warn,
        json!({
            "projectId": candidate.key.0,
            "sessionId": candidate.key.1,
            "slept": slept,
            "sessionBytes": candidate.usage.total,
            "processes": candidate.usage.members.len(),
            "leftoversKilled": killed,
        }),
    );
    let words = guard::stopped_session_card(&candidate.usage);
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let record = guard::stop_record(&words, &guard::stopped_session_parts(&candidate.usage), &at);
    write_stop_record(state, &candidate.key, Some(record)).await;
    let mut notice = guard::card(&words, SessionChatTerminalNoticeSeverity::Warning, true);
    notice.detected_at = at;
    set_session_chat_watchdog_notice(&candidate.key.0, &candidate.key.1, notice);
    crate::session_chat_options::session_chat_terminal_notice_publisher(
        state,
        &candidate.key.0,
        &candidate.key.1,
    )();
    notify(state, &candidate, &words.detail).await;
}

/// The card in the chat the guard acted on.
fn show_card(
    state: &AppState,
    key: &Key,
    words: &guard::CardWords,
    severity: SessionChatTerminalNoticeSeverity,
    resume: bool,
) {
    set_session_chat_watchdog_notice(&key.0, &key.1, guard::card(words, severity, resume));
    crate::session_chat_options::session_chat_terminal_notice_publisher(state, &key.0, &key.1)();
}

/// Writes (or, with `None`, removes) a session's stop record and refreshes its row, so the red
/// dot shows or clears on every client.
async fn write_stop_record(state: &Arc<AppState>, key: &Key, record: Option<Value>) {
    let state = state.clone();
    let key = key.clone();
    let _ = tokio::task::spawn_blocking(move || store_stop_record(&state, &key, record)).await;
}

/// BLOCKING. `true` when the row changed.
fn store_stop_record(state: &AppState, key: &Key, record: Option<Value>) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(&key.0, &key.1) else {
        return false;
    };
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let has_record = record.is_some();
    match record {
        Some(record) => runtime.insert(guard::STOP_RECORD.to_string(), record),
        None => runtime.remove(guard::STOP_RECORD),
    };
    if crate::accounts::endpoint::update_session_members(
        &repository,
        &session,
        &runtime,
        &[guard::STOP_RECORD],
    )
    .is_err()
    {
        return false;
    }
    {
        let mut book = book();
        if has_record {
            book.stopped_records.insert(key.clone());
        } else {
            book.stopped_records.remove(key);
        }
    }
    let _ =
        crate::server::schedule_presentation_session_delta(state, &db, &repository, &key.0, &key.1);
    true
}

/// Clears the stop record of every session that runs again, however it was woken, so its red dot
/// goes and a later sleep is not taken for the guard's. BLOCKING.
fn clear_records_of_woken_sessions(state: &AppState) {
    let tracked: Vec<Key> = {
        let guard_book = book();
        if !guard_book.stopped_records_seeded {
            drop(guard_book);
            let Ok(db) = open_gxserver_database(&state.paths) else {
                return;
            };
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let Ok(stopped) = repository.list_sessions_with_runtime_object(guard::STOP_RECORD)
            else {
                return;
            };
            let keys: Vec<Key> = stopped
                .iter()
                .filter_map(|session| {
                    Some((
                        text_field(session, "projectId")?,
                        text_field(session, "sessionId")?,
                    ))
                })
                .collect();
            let mut guard_book = book();
            guard_book.stopped_records_seeded = true;
            guard_book.stopped_records.extend(keys);
            guard_book.stopped_records.iter().cloned().collect()
        } else if guard_book.stopped_records.is_empty() {
            return;
        } else {
            guard_book.stopped_records.iter().cloned().collect()
        }
    };
    if tracked.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut to_clear = Vec::new();
    let mut gone = Vec::new();
    for key in tracked {
        match repository.get_session(&key.0, &key.1) {
            Ok(Some(session)) => {
                if session
                    .pointer(&format!("/runtimeSettings/{}", guard::STOP_RECORD))
                    .is_none_or(|value| !value.is_object())
                {
                    gone.push(key);
                } else if crate::presentation::effective_lifecycle_state(&session) != "sleeping" {
                    to_clear.push(key);
                }
            }
            Ok(None) => gone.push(key),
            Err(_) => {}
        }
    }
    drop(repository);
    drop(db);
    if !gone.is_empty() {
        let mut book = book();
        for key in gone {
            book.stopped_records.remove(&key);
        }
    }
    for key in to_clear {
        store_stop_record(state, &key, None);
    }
}

/// The bell row and the phone's push, while the notification switch is on.
async fn notify(state: &Arc<AppState>, candidate: &Candidate, body: &str) {
    let state = state.clone();
    let key = candidate.key.clone();
    let session = candidate.session.clone();
    let body = body.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        let settings = crate::shared_settings_file::read_shared_settings(&state.paths);
        if !guard::setting_on(settings.as_ref(), guard::NOTIFY_SETTING) {
            return;
        }
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        // CDXC:Notifications 2026-11-14 (Q3245 in server/src/worker_alerts.rs): a stop in a
        // coordinator's thread is the coordinator's to handle; a row only while Workers brings
        // them back, a push by the phone's Workers choice.
        let worker = crate::worker_alerts::is_worker(&db, &key.0, &key.1);
        if worker && !crate::worker_alerts::worker_alerts().rows() {
            crate::push::announce_worker_event(
                &state,
                &repository,
                &session,
                crate::push::BlockedWhy::OutOfMemory.kind(),
            );
            return;
        }
        let route = if worker {
            crate::push::PushRoute::worker()
        } else {
            crate::push::PushRoute::default()
        };
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(key.0));
        params.insert("sessionId".to_string(), json!(key.1));
        params.insert("body".to_string(), json!(body));
        let Ok(created) = crate::notification_feed::create_notification_endpoint(
            &state,
            &repository,
            &db,
            &params,
        ) else {
            return;
        };
        if let Some(id) = created.pointer("/notification/id").and_then(Value::as_str) {
            crate::push::announce_blocked(
                &state,
                &repository,
                &session,
                id,
                crate::push::BlockedWhy::OutOfMemory,
                route,
            );
        }
    })
    .await;
}

/// Types `text` into the session's agent the way `ghostex agents send` does (a sleeping session
/// is woken for it).
async fn send(
    state: &AppState,
    key: &Key,
    text: &str,
    request: &str,
) -> std::result::Result<(), String> {
    let body = json!({ "params": {
        "projectId": key.0,
        "sessionId": key.1,
        "text": text,
        "sendRequestId": request,
    }});
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
    let bytes = axum::body::to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    Err(String::from_utf8_lossy(&bytes).chars().take(300).collect())
}

/// The card's Resume: wakes the session the guard stopped and tells its agent what was running.
pub(crate) async fn resume_after_memory_stop(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<(), crate::domain::DomainStateError> {
    let read = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default()
    };
    let key = (read("projectId"), read("sessionId"));
    let reader = (Arc::new(state.clone()), key.clone());
    let parts = tokio::task::spawn_blocking(move || {
        let (state, key) = reader;
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository.get_session(&key.0, &key.1).ok()??;
        let parts = guard::stopped_parts(&session);
        drop(repository);
        drop(db);
        store_stop_record(&state, &key, None);
        Some(parts)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    crate::session_chat_notice::clear_session_chat_watchdog_notice(&key.0, &key.1);
    crate::session_chat_options::session_chat_terminal_notice_publisher(state, &key.0, &key.1)();
    send(
        state,
        &key,
        &guard::resumed_message(&parts),
        &Uuid::new_v4().to_string(),
    )
    .await
    .map_err(|message| crate::domain::DomainStateError {
        code: "invalidState",
        message,
    })
}

fn log(state: &AppState, event: &str, level: LogLevel, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}
