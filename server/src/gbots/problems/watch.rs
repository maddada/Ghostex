//! The GBot problem watch: every ten seconds it looks at every open chat of every GBot that is not
//! already a coordinator's thread (a chat the user opened, a routine run, one Chief of Staff
//! opened with New Chat) and opens a `gbots_problems` row, reported to Chief of Staff, when one
//! could not start, failed, stalled, or (opened by anyone but the user) waits on a question or an
//! approval. It also closes rows on their own once the chat works again or stops waiting.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A chat handed over with `ghostex gbots ask` is its sender's thread, so the coordinator supervisor already reports its waits, stalls, restarts and closing to that sender (server/coordinator_runtime.rs); this watch covers the chats nobody supervises, with the same signals: Ghostex's own restarts giving up (session_self_heal.rs), the start, sign-in and limit cards the chat shows (session_chat_notice/), the thread stall clock (coordinators/stall.rs) at a 20-minute round, and the thread wait test (coordinators/state.rs). A plain start failure is started once more by Ghostex before Chief of Staff hears of it; a sign-in, a missing agent or every account at its limit goes straight to Chief of Staff marked as the user's to fix.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::Value;

use super::store::{
    insert_problem, list_problems, live_problem_for_chat, new_problem, save_problem, Problem,
    COULD_NOT_START, FAILED, NEEDS_YOU, STALLED,
};
use crate::coordinators::{self, SessionKey, StallWatch, ThreadProgress, ThreadState};
use crate::domain::DomainRepository;
use crate::gbots::session::chat_of;
use crate::presentation::session_attributes::effective_lifecycle_state;
use crate::server::AppState;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT,
    SESSION_CHAT_NOTICE_AGENT_EXITED, SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
    SESSION_CHAT_NOTICE_USAGE_LIMIT,
};
use crate::storage::open_gxserver_database;

const TICK: Duration = Duration::from_secs(10);
/// A chat working with no new output this long is a problem.
const STALL_MS: i64 = 20 * 60_000;
/// After a retry the chat gets this long to come back before the same trouble counts again.
const RETRY_SETTLE_MS: i64 = 2 * 60_000;
/// A wait must hold this long before it is escalated (a card can flash between tool calls).
const WAIT_STABLE_MS: i64 = 8_000;

#[derive(Default)]
struct WatchMemory {
    stalls: HashMap<SessionKey, StallWatch>,
    /// The quiet stretch each stalled chat was last reported for (its start).
    stall_reported: HashMap<SessionKey, i64>,
    /// What each waiting chat waits on, and since when.
    waiting: HashMap<SessionKey, (String, i64)>,
}

/// A restart the watch asks for (Ghostex's own one retry of a plain start failure).
pub(super) struct Restart {
    pub key: SessionKey,
}

pub(crate) fn start_gbots_problem_watch(state: Arc<AppState>) {
    let memory = Arc::new(Mutex::new(WatchMemory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            if !crate::gbots::gbots_on(&state.paths) {
                continue;
            }
            let (ticking, ticking_memory) = (state.clone(), memory.clone());
            let restarts = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for restart in restarts {
                let state = state.clone();
                tokio::spawn(async move {
                    let _ = restart_chat(&state, &restart.key.0, &restart.key.1).await;
                });
            }
        }
    });
}

/// Waits between a chat's sleep and its wake, and between wakes that were refused.
const WAKE_PAUSES: [Duration; 3] = [
    Duration::from_millis(1_500),
    Duration::from_secs(3),
    Duration::from_secs(6),
];

/// Restarts a chat on its own conversation: puts it to sleep, then wakes it, the way
/// `ghostex agents reload` does.
///
/// CDXC:GBots 2026-11-09 WHY:
/// A wake sent right after the sleep was refused once ("macOS refused to start it from this login") while the old terminal was still going away, and worked a moment later (seen on a test server, 2026-11-09). So the wake waits a moment and is tried up to three times; a retry that never got its chat back is not counted against the problem.
pub(super) async fn restart_chat(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let body =
        serde_json::json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let call = |path: &'static str| {
        let body = body.clone();
        async move {
            let routed = crate::server::handle_zmx_lifecycle_http(
                state,
                path.to_string(),
                uuid::Uuid::new_v4().to_string(),
                &body,
            )
            .await;
            crate::session_chat_options::forget_session_chat_options(state, project_id, session_id);
            routed.response.status().is_success()
        }
    };
    if !call("/api/sleepSession").await {
        return Err("it could not be put to sleep".to_string());
    }
    for pause in WAKE_PAUSES {
        tokio::time::sleep(pause).await;
        if call("/api/wakeSession").await {
            crate::session_chat_agent_restart::note_agent_restarted(state, project_id, session_id);
            return Ok(());
        }
    }
    Err("its terminal would not start again".to_string())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn ms_of(iso: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso?)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// What a chat's card says about why its agent is not up: the detail, and whether only the user
/// can fix it. `None` for every other card.
pub(super) fn start_trouble(notice: &SessionChatTerminalNotice) -> Option<(String, bool)> {
    let title = notice.title.trim();
    if notice.missing_login() {
        return Some((
            format!("Its agent has no saved login on this computer, so it needs the user to sign in ({title})."),
            true,
        ));
    }
    if notice.account_disabled() {
        return Some((format!("Its agent's account is disabled ({title})."), true));
    }
    if notice.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED && notice.blocks_input() {
        return Some((
            format!("Its agent waits for the user to sign in ({title})."),
            true,
        ));
    }
    if notice
        .actions
        .iter()
        .any(|action| action.id == SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT)
    {
        return Some((
            format!("Its agent isn't installed on this computer ({title})."),
            true,
        ));
    }
    if notice.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT && notice.account_wait.is_some() {
        return Some((
            format!("Every account it could use hit its usage limit, so it waits ({title})."),
            true,
        ));
    }
    if notice.start_failed() || notice.kind == SESSION_CHAT_NOTICE_AGENT_EXITED {
        let mut detail = format!("Its agent stopped before it came up ({title})");
        if let Some(lines) = notice
            .error_lines
            .as_deref()
            .filter(|lines| !lines.trim().is_empty())
        {
            detail.push_str(&format!(": {}", lines.trim()));
        }
        return Some((detail, false));
    }
    None
}

/// What a waiting chat waits on (its key and words), the way a waiting thread's report names it.
pub(super) fn waits_on(session: &Value, now_iso: &str) -> Option<(String, String)> {
    let as_run = ThreadProgress {
        resolved: false,
        has_run: true,
    };
    if coordinators::classify_thread_session(Some(session), as_run, now_iso, false)
        != ThreadState::Waiting
    {
        return None;
    }
    Some(match coordinators::waiting_prompt(session) {
        Some(prompt) => (prompt.key, prompt.summary),
        None => (
            format!(
                "attention:{}",
                session
                    .pointer("/runtimeSettings/agentActivity/attentionEventId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ),
            "A prompt on its screen (an approval or a question the chat can't show).".to_string(),
        ),
    })
}

/// Whether the chat finished a turn after `since_ms` (BLOCKING: a transcript tail read).
pub(super) fn finished_since(session: &Value, since_ms: i64) -> bool {
    crate::notification_feed::body::last_assistant_message(session)
        .and_then(|(_, at)| at)
        .is_some_and(|at| at > since_ms)
}

/// Opens (or brings back) a problem of `kind` for a chat; returns the row when one is due to be
/// reported. A live row of the same family is reused: the same trouble seen again after a retry
/// re-opens it, so Chief of Staff hears "again after your retry", never a second row.
fn raise(
    db: &Connection,
    bot_id: &str,
    chat: &str,
    kind: &str,
    detail: &str,
    user_only: bool,
    prompt_key: Option<&str>,
    now: i64,
) -> Option<Problem> {
    let escalation = kind == NEEDS_YOU;
    if let Ok(Some(mut live)) = live_problem_for_chat(db, chat, escalation) {
        if escalation && live.source.as_deref() == Some("prompt") {
            if live.prompt_key.as_deref() == prompt_key {
                return None;
            }
            // It waits on something new: the old wait is over.
            live.status = "closed".to_string();
            live.closed_by = Some("ghostex: it stopped waiting".to_string());
            let _ = save_problem(db, &mut live);
        } else if escalation {
            // A flag it raised is still open; a prompt is its own row.
        } else {
            if live.status != "retrying" {
                return None;
            }
            let retried = ms_of(live.retried_at.as_deref()).unwrap_or(0);
            if now - retried < coordinators::test_scaled(RETRY_SETTLE_MS) {
                return None;
            }
            live.status = "open".to_string();
            // What only the user can fix stays theirs: a restart's own error (say, "Couldn't
            // reopen this chat" after a sign-in screen) does not make it Chief of Staff's again.
            if !live.user_only || user_only {
                live.kind = kind.to_string();
                live.detail = super::store::short_detail(detail);
            }
            live.user_only = live.user_only || user_only;
            live.reported_to = None;
            live.reported_at = None;
            live.report_due_at = None;
            let _ = save_problem(db, &mut live);
            return Some(live);
        }
    }
    let mut problem = new_problem(bot_id, Some(chat), kind, detail);
    problem.user_only = user_only;
    if escalation {
        problem.source = Some("prompt".to_string());
        problem.prompt_key = prompt_key.map(str::to_string);
    }
    insert_problem(db, &mut problem).ok()?;
    Some(problem)
}

fn tick(state: &AppState, memory: &Mutex<WatchMemory>) -> Vec<Restart> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    let gave_up = crate::session_self_heal::gave_up_sessions();
    let mut restarts = Vec::new();
    let mut seen = HashSet::new();
    let bots = crate::gbots::record::list_bots(&db).unwrap_or_default();
    for bot in &bots {
        let Ok(chats) = crate::gbots::session::bot_chats(&repository, bot) else {
            continue;
        };
        for session in chats {
            let key = (text(&session, "projectId"), text(&session, "sessionId"));
            let chat = format!("{}:{}", key.0, key.1);
            let linked = coordinators::read_thread(&db, &key.0, &key.1)
                .ok()
                .flatten()
                .is_some_and(|thread| !thread.is_resolved());
            if linked || effective_lifecycle_state(&session) != "running" {
                continue;
            }
            seen.insert(key.clone());
            let opened_by = chat_of(&session)
                .map(|(_, by)| by.to_string())
                .unwrap_or_default();
            if gave_up.contains_key(&key) {
                raise(
                    &db,
                    &bot.bot_id,
                    &chat,
                    FAILED,
                    &format!(
                        "Its terminal kept ending under its agent, and Ghostex's {} restarts in {} minutes did not keep it running.",
                        crate::session_self_heal::MAX_RESTARTS_PER_WINDOW,
                        crate::session_self_heal::RESTART_WINDOW_MS / 60_000
                    ),
                    false,
                    None,
                    now,
                );
                continue;
            }
            if crate::session_self_heal::daemon_lost(&key.0, &key.1) {
                continue;
            }
            let notice = crate::session_chat_options::cached_session_chat_screen_state(
                state, &key.0, &key.1,
            )
            .notice;
            if let Some((detail, user_only)) = notice.as_ref().and_then(start_trouble) {
                let fresh = !matches!(live_problem_for_chat(&db, &chat, false), Ok(Some(_)));
                if fresh && !user_only {
                    // A plain start failure: Ghostex starts it once more before Chief of Staff
                    // hears of it.
                    let mut problem =
                        new_problem(&bot.bot_id, Some(&chat), COULD_NOT_START, &detail);
                    problem.status = "retrying".to_string();
                    problem.ghostex_retried = true;
                    problem.retried_at = Some(now_iso.clone());
                    if insert_problem(&db, &mut problem).is_ok() {
                        restarts.push(Restart { key: key.clone() });
                    }
                } else {
                    raise(
                        &db,
                        &bot.bot_id,
                        &chat,
                        COULD_NOT_START,
                        &detail,
                        user_only,
                        None,
                        now,
                    );
                }
                continue;
            }
            // A chat the user opened is theirs to answer; any other waits on Chief of Staff.
            match waits_on(&session, &now_iso).filter(|_| opened_by != "user") {
                Some((prompt_key, summary)) => {
                    let since = match memory.waiting.get(&key) {
                        Some((known, since)) if *known == prompt_key => *since,
                        _ => {
                            memory
                                .waiting
                                .insert(key.clone(), (prompt_key.clone(), now));
                            now
                        }
                    };
                    if now - since >= WAIT_STABLE_MS {
                        raise(
                            &db,
                            &bot.bot_id,
                            &chat,
                            NEEDS_YOU,
                            &summary,
                            false,
                            Some(&prompt_key),
                            now,
                        );
                    }
                    memory.stalls.remove(&key);
                    continue;
                }
                None => {
                    memory.waiting.remove(&key);
                }
            }
            let working = crate::presentation::presentation_activity(&session, &now_iso)
                == "working"
                && !crate::session_approval::session_has_question(&session);
            if !working {
                memory.stalls.remove(&key);
                continue;
            }
            let watch = memory
                .stalls
                .entry(key.clone())
                .or_insert_with(|| StallWatch::new(now));
            let _ = watch.observe(&session, now);
            let quiet_since = watch.quiet_since();
            let stalled = now - quiet_since >= coordinators::test_scaled(STALL_MS);
            if stalled && memory.stall_reported.get(&key) != Some(&quiet_since) {
                memory.stall_reported.insert(key.clone(), quiet_since);
                let minutes = coordinators::minutes_of(now - quiet_since);
                raise(
                    &db,
                    &bot.bot_id,
                    &chat,
                    STALLED,
                    &format!(
                        "It has been working with no new output for {minutes} minutes (since {}); it may be stuck on a hung command.",
                        crate::network_status::local_clock(quiet_since)
                    ),
                    false,
                    None,
                    now,
                );
            }
        }
    }
    memory.stalls.retain(|key, _| seen.contains(key));
    memory.stall_reported.retain(|key, _| seen.contains(key));
    memory.waiting.retain(|key, _| seen.contains(key));
    settle_live(state, &db, &repository, &memory, &gave_up, now, &now_iso);
    restarts
}

/// Closes live rows whose trouble is over, and sends the reports that are due.
fn settle_live(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    memory: &WatchMemory,
    gave_up: &HashMap<SessionKey, i64>,
    now: i64,
    now_iso: &str,
) {
    let Ok(live) = list_problems(db, true, 500) else {
        return;
    };
    for mut problem in live {
        if let Some(reason) =
            settled_reason(state, repository, memory, gave_up, &problem, now, now_iso)
        {
            problem.status = "closed".to_string();
            problem.closed_by = Some(format!("ghostex: {reason}"));
            let _ = save_problem(db, &mut problem);
            continue;
        }
        let due = problem.status == "open"
            && problem.reported_to.is_none()
            && ms_of(problem.report_due_at.as_deref()).is_none_or(|due| due <= now);
        // Ghostex's own retry that did not help: Chief of Staff hears of it now.
        let retry_failed = problem.status == "retrying"
            && problem.ghostex_retried
            && problem.attempts == 0
            && problem.reported_to.is_none()
            && ms_of(problem.retried_at.as_deref())
                .is_some_and(|at| now - at >= coordinators::test_scaled(RETRY_SETTLE_MS));
        if retry_failed {
            problem.status = "open".to_string();
        }
        if due || retry_failed {
            super::report::send_report(state, db, &mut problem);
        }
    }
}

/// Why a live row is over, if it is.
fn settled_reason(
    state: &AppState,
    repository: &DomainRepository<'_>,
    memory: &WatchMemory,
    gave_up: &HashMap<SessionKey, i64>,
    problem: &Problem,
    now: i64,
    now_iso: &str,
) -> Option<&'static str> {
    let (project, session_id) = problem.chat()?;
    let key = (project.to_string(), session_id.to_string());
    let Some(session) = repository
        .get_session(project, session_id)
        .ok()
        .flatten()
        .filter(crate::presentation::session_attributes::is_active)
    else {
        return Some("its chat was closed");
    };
    if problem.is_escalation() {
        if problem.source.as_deref() != Some("prompt") {
            return None;
        }
        let waiting = waits_on(&session, now_iso).map(|(key, _)| key);
        return (waiting.as_deref() != problem.prompt_key.as_deref())
            .then_some("it stopped waiting");
    }
    if problem.status == "handed" {
        return None;
    }
    let since = ms_of(problem.retried_at.as_deref())
        .or_else(|| ms_of(Some(&problem.created_at)))
        .unwrap_or(now);
    if now - since < coordinators::test_scaled(RETRY_SETTLE_MS) && problem.status == "retrying" {
        return None;
    }
    let running = effective_lifecycle_state(&session) == "running";
    let trouble_now = match problem.kind.as_str() {
        FAILED => gave_up.contains_key(&key),
        COULD_NOT_START => crate::session_chat_options::cached_session_chat_screen_state(
            state, project, session_id,
        )
        .notice
        .as_ref()
        .and_then(start_trouble)
        .is_some(),
        STALLED => memory
            .stalls
            .get(&key)
            .is_some_and(|watch| now - watch.quiet_since() >= coordinators::test_scaled(STALL_MS)),
        _ => return None,
    };
    if trouble_now {
        return None;
    }
    let working =
        running && crate::presentation::presentation_activity(&session, now_iso) == "working";
    let moved = finished_since(&session, since)
        || (problem.kind == STALLED
            && memory
                .stalls
                .get(&key)
                .is_none_or(|watch| watch.quiet_since() > since))
        || (working && problem.kind != STALLED);
    moved.then_some("its chat works again")
}

/// A routine run that failed (gbots/routine_runs.rs): a problem of its own unless its chat already
/// has one; a run of that routine that worked closes the ones it left open.
pub(crate) fn routine_run_settled(
    db: &Connection,
    bot_id: &str,
    automation_id: &str,
    failed: bool,
    summary: &str,
    chat: Option<&str>,
) {
    if !failed {
        for mut problem in
            super::store::live_routine_problems(db, automation_id).unwrap_or_default()
        {
            if problem.chat_session_key.is_none() {
                problem.status = "closed".to_string();
                problem.closed_by = Some("ghostex: a later run worked".to_string());
                let _ = save_problem(db, &mut problem);
            }
        }
        return;
    }
    if let Some(chat) = chat {
        if matches!(live_problem_for_chat(db, chat, false), Ok(Some(_))) {
            return;
        }
    }
    let detail = if summary.trim().is_empty() {
        "The run failed and said nothing about why.".to_string()
    } else {
        summary.trim().to_string()
    };
    let mut problem = new_problem(bot_id, chat, super::store::ROUTINE_FAILED, &detail);
    problem.automation_id = Some(automation_id.to_string());
    let _ = insert_problem(db, &mut problem);
}

/// A chat whose agent did not start even after Ghostex tried twice (gbots/session.rs).
pub(crate) fn chat_did_not_start(
    db: &Connection,
    bot_id: &str,
    project_id: &str,
    session_id: &str,
) {
    let chat = format!("{project_id}:{session_id}");
    let mut problem = new_problem(
        bot_id,
        Some(&chat),
        COULD_NOT_START,
        "Ghostex could not start its agent's terminal.",
    );
    problem.ghostex_retried = true;
    let _ = insert_problem(db, &mut problem);
}
