//! Coordinator threads a restart cut off: held until the user carries them on or dismisses them.
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! GitHub #608: after Ghostex was closed overnight, launching it set an orchestrator's threads
//! working on their own (the thread watch counts its rounds from each thread's last report, so a
//! night off made every thread that owed work due at the first tick: a carry-on nudge to the
//! thread and a report to its coordinator, both waking their sessions). From the restart page
//! (i608-fix/restart.html) the user picked "Ask on the coordinator" (Q3234): after a restart
//! Ghostex types nothing into a thread a restart cut off; its coordinator's chat shows a card
//! listing them, with Carry on and Not now. Their note: "show red dot on these sessions pls", so
//! each such thread has the red dot, its hover and row saying "Stopped when Ghostex closed"
//! (Q3240), until it is carried on or dismissed. With Settings > General "Resume sessions after a
//! restart" on, nothing is held: the switch opts in to carrying on without the card (Q3235).
//! A cut-off thread is one whose agent was working when the computer shut down and whose terminal
//! is gone when gxserver starts: "Leave them out" (Q67, 2026-11-16, after a card woke four threads
//! that had already reported): "Only list threads that were working when the computer shut down.
//! Finished ones just stay idle." This supersedes the 2026-11-14 rule (Q3237 "Above a size") that
//! showed the Compact first checkbox only when a thread was at or above Compact chats above: the
//! user picked "Always show the checkbox" (Q66): "Offer it whenever a listed thread can be
//! compacted, ticked only for the ones above the limit.", then "A tick per thread" (Q68): the card
//! has one "Compact <thread> first (312K)" checkbox per listed thread whose agent has a compact
//! command, ticked at or above the size and unticked below, and Carry on compacts exactly the
//! ticked ones. Those rows are `checkboxes`, optional, which the chat core draws instead of the one
//! `checkbox` that clients from before them still get ("Compact the ones above 200K first",
//! ticked while any thread is, a click ticking or unticking the threads it covers). Threads the
//! switch carries on compact first by themselves when at or above the size
//! (server/src/carry_on_compaction.rs).
//! The hold is the thread's `runtimeSettings.stoppedByRestart` (`at`, `tokens`, `dismissed`), so it
//! survives another restart; the thread watch skips a held thread until it works again (the user
//! opened it and sent something), which ends the hold.
//! SEE-ALSO: server/src/computer_startup/watch.rs (where cut-off sessions are found),
//! server/src/server/coordinator_runtime.rs (the skip, Carry on), server/src/presentation/attention_alerts.rs
//! (the red dot), server/src/session_chat_notice/watchdog_store.rs (the card),
//! packages/gx-chat-core/src/questions/notice_checkboxes.rs, apps/desktop/src/app/native_chat/notice.rs
//! and apps/mobile/app/src/chat/native/cards/NoticeCard.tsx (the rows).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{Map, Value, json};

use super::records::{SessionKey, list_open_threads, now_iso};
use crate::domain::DomainRepository;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeActionKind, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource,
};

/// The hold a cut-off thread keeps in its runtime settings.
pub const STOPPED_BY_RESTART: &str = "stoppedByRestart";
/// The red dot's hover and the row's words.
pub const STOPPED_BY_RESTART_REASON: &str = "Stopped when Ghostex closed";
/// Who alerted for the red dot (`blockedAlertedBy`): the card on the coordinator says it, so the
/// blocked rule's sound and banner do not repeat it once per thread.
pub const STOPPED_BY_RESTART_ALERTED_BY: &str = "restart";
/// The coordinator's card.
pub const STOPPED_THREADS_NOTICE_KIND: &str = "threadsStoppedByRestart";
/// The card's buttons (answered as `restartAgent` with these ids, which every client sends).
pub const CARRY_ON_STOPPED_THREADS: &str = "carryOnStoppedThreads";
pub const DISMISS_STOPPED_THREADS: &str = "dismissStoppedThreads";
/// What a carried-on thread is told, the same words as the thread watch's carry-on nudge.
pub const CARRY_ON_AFTER_RESTART: &str =
    "Ghostex closed while you were working. Carry on where you left off, or say you're finished.";

/// The card's words after the names of the threads it lists.
const CARRY_ON_DETAIL: &str = "Ghostex didn't carry them on by itself; Carry on wakes each one and tells it to pick up where it stopped.";

/// One held thread, as its coordinator's card lists it.
#[derive(Clone, Debug)]
pub struct StoppedThread {
    pub key: SessionKey,
    pub title: String,
    pub tokens: Option<u64>,
    pub at: String,
    /// Its agent has a compact command.
    pub compactable: bool,
}

#[derive(Clone, Debug)]
struct HeldThreads {
    threads: Vec<StoppedThread>,
    /// The user's ticks on the card's per-thread Compact first checkboxes; a thread they have not
    /// touched is ticked when it is at or above the size (`ticked`).
    ticks: HashMap<SessionKey, bool>,
}

impl HeldThreads {
    /// Whether a thread is at or above the size, so the checkbox covers only those.
    fn any_above(&self, threshold: u64) -> bool {
        self.threads
            .iter()
            .any(|thread| thread.compactable && is_above(thread, threshold))
    }

    /// Whether the older clients' single checkbox, when ticked, covers this thread: with a thread
    /// above the size only those, otherwise every one that can be.
    fn covered_by_single(&self, thread: &StoppedThread, threshold: u64) -> bool {
        thread.compactable && (!self.any_above(threshold) || is_above(thread, threshold))
    }

    /// Whether Carry on compacts this thread first: its tick.
    fn ticked(&self, thread: &StoppedThread, threshold: u64) -> bool {
        thread.compactable
            && self
                .ticks
                .get(&thread.key)
                .copied()
                .unwrap_or_else(|| is_above(thread, threshold))
    }
}

fn is_above(thread: &StoppedThread, threshold: u64) -> bool {
    thread.tokens.is_some_and(|tokens| tokens >= threshold)
}

/// The threads each coordinator's card lists, by coordinator.
fn held() -> &'static Mutex<HashMap<SessionKey, HeldThreads>> {
    static HELD: OnceLock<Mutex<HashMap<SessionKey, HeldThreads>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Threads the restart switch carries on by itself that are compacted as soon as they are resumed,
/// so whatever carries them on (a nudge, a report's answer) waits behind the compaction.
fn compact_before_nudge() -> &'static Mutex<HashMap<SessionKey, u64>> {
    static KEYS: OnceLock<Mutex<HashMap<SessionKey, u64>>> = OnceLock::new();
    KEYS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn record(session: &Value) -> Option<&Value> {
    session
        .pointer(&format!("/runtimeSettings/{STOPPED_BY_RESTART}"))
        .filter(|record| record.is_object())
}

/// Whether the thread watch leaves this thread alone: a restart cut it off and it has not worked
/// since (dismissed or not).
pub fn held_by_restart(session: &Value) -> bool {
    record(session).is_some()
}

/// Whether a held thread started working after the restart that cut it off (the user opened it and
/// sent something): its hooks say working, changed after the hold began. Hooks still saying
/// working from before the shutdown do not count.
pub fn worked_since_restart(session: &Value) -> bool {
    let Some(at) = record(session).and_then(|record| record["at"].as_str()) else {
        return false;
    };
    let Some(activity) = session.pointer("/runtimeSettings/agentActivity") else {
        return false;
    };
    activity["activity"] == "working"
        && activity["lastChangedAt"]
            .as_str()
            .and_then(iso_ms)
            .zip(iso_ms(at))
            .is_some_and(|(changed, held)| changed > held)
}

fn iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// (since, reason) for the red dot while the thread waits to be carried on or dismissed.
pub fn blocked_by_restart(session: &Value) -> Option<(String, String)> {
    let record = record(session)?;
    if record["dismissed"] == true {
        return None;
    }
    Some((
        record["at"].as_str()?.to_string(),
        STOPPED_BY_RESTART_REASON.to_string(),
    ))
}

/// True once for a thread the restart switch resumed that should be compacted now.
pub fn take_compact_after_resume(key: &SessionKey) -> bool {
    compact_before_nudge()
        .lock()
        .map(|mut keys| keys.remove(key).is_some())
        .unwrap_or(false)
}

/// At gxserver start, before any client connects: `cut_off` are the agent sessions whose agent
/// was working when gxserver went down and whose terminal is gone. Held (`hold`) unless the
/// restart switch is carrying them on. Then every held thread, new or from before, is listed on its
/// coordinator's card.
pub fn note_restart(repository: &DomainRepository<'_>, cut_off: &[SessionKey], hold: bool) {
    let Ok(threads) = list_open_threads(repository.db) else {
        return;
    };
    let at = now_iso();
    for thread in threads
        .iter()
        .filter(|thread| cut_off.contains(&thread.key()))
    {
        let Ok(Some(session)) = repository.get_session(&thread.project_id, &thread.session_id)
        else {
            continue;
        };
        // Already held from an earlier start (dismissed or not): it keeps that hold.
        if record(&session).is_some() {
            continue;
        }
        let worth = crate::carry_on_compaction::worth_compacting(repository, &session);
        if !hold {
            if let Some(tokens) = worth {
                if let Ok(mut keys) = compact_before_nudge().lock() {
                    keys.insert(thread.key(), tokens);
                }
            }
            continue;
        }
        let tokens = crate::carry_on_compaction::context_tokens(repository, &session);
        let mut runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        runtime.insert(
            STOPPED_BY_RESTART.to_string(),
            json!({ "at": at, "tokens": tokens }),
        );
        let _ = crate::accounts::endpoint::update_session_members(
            repository,
            &session,
            &runtime,
            &[STOPPED_BY_RESTART],
        );
    }
    let Ok(sessions) = repository.list_sessions_with_runtime_object(STOPPED_BY_RESTART) else {
        return;
    };
    let mut cards: HashMap<SessionKey, HeldThreads> = HashMap::new();
    for session in sessions {
        let Some(record) = record(&session) else {
            continue;
        };
        if record["dismissed"] == true {
            continue;
        }
        let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        let Some(thread) = threads
            .iter()
            .find(|thread| thread.project_id == pid && thread.session_id == sid)
        else {
            continue;
        };
        cards
            .entry(thread.coordinator_key())
            .or_insert_with(|| HeldThreads {
                threads: Vec::new(),
                ticks: HashMap::new(),
            })
            .threads
            .push(StoppedThread {
                key: thread.key(),
                title: session["title"]
                    .as_str()
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .unwrap_or("Thread")
                    .to_string(),
                tokens: record["tokens"].as_u64(),
                at: record["at"].as_str().unwrap_or_default().to_string(),
                compactable: crate::carry_on_compaction::compact_command(&session).is_some(),
            });
    }
    if let Ok(mut held) = held().lock() {
        *held = cards;
    }
}

/// Ends a thread's hold: it worked again, was carried on, or was closed.
pub fn release_thread(repository: &DomainRepository<'_>, session: &Value) {
    if record(session).is_none() {
        return;
    }
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.remove(STOPPED_BY_RESTART);
    let _ = crate::accounts::endpoint::update_session_members(
        repository,
        session,
        &runtime,
        &[STOPPED_BY_RESTART],
    );
    let key = (
        session["projectId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        session["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    );
    if let Ok(mut held) = held().lock() {
        for card in held.values_mut() {
            card.threads.retain(|thread| thread.key != key);
        }
        held.retain(|_, card| !card.threads.is_empty());
    }
}

/// The card on a coordinator whose threads a restart cut off.
pub fn stopped_threads_card(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatTerminalNotice> {
    let key = (project_id.to_string(), session_id.to_string());
    let card = held().lock().ok()?.get(&key)?.clone();
    let count = card.threads.len();
    let title = if count == 1 {
        "1 thread stopped when Ghostex closed".to_string()
    } else {
        format!("{count} threads stopped when Ghostex closed")
    };
    let threshold = crate::carry_on_compaction::threshold();
    let names: Vec<String> = card
        .threads
        .iter()
        .map(|thread| match thread.tokens {
            Some(tokens) => format!(
                "{} ({})",
                thread.title,
                crate::carry_on_compaction::short_tokens(tokens)
            ),
            None => thread.title.clone(),
        })
        .collect();
    let detail = format!("{}. {CARRY_ON_DETAIL}", names.join(", "));
    let action = |id: &str, label: &str| SessionChatTerminalNoticeAction {
        id: id.to_string(),
        label: label.to_string(),
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    };
    let mut notice = SessionChatTerminalNotice::new(
        STOPPED_THREADS_NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Watchdog,
        title,
    )
    .with_detail(detail)
    .with_actions(vec![
        action(CARRY_ON_STOPPED_THREADS, "Carry on"),
        action(DISMISS_STOPPED_THREADS, "Not now"),
    ])
    .with_input_blocking(false);
    if card.threads.iter().any(|thread| thread.compactable) {
        // Clients older than the per-thread rows draw this one checkbox (Q66): ticked while any
        // thread is, and a click ticks or unticks all it covers.
        let label = if card.any_above(threshold) {
            format!(
                "Compact the ones above {} first",
                crate::carry_on_compaction::short_tokens(threshold)
            )
        } else if card.threads.len() == 1 {
            "Compact it first".to_string()
        } else {
            "Compact them first".to_string()
        };
        let any_ticked = card
            .threads
            .iter()
            .any(|thread| card.ticked(thread, threshold));
        notice.checkbox = Some(json!({
            "id": "compactFirst",
            "label": label,
            "checked": any_ticked,
            "hint": crate::carry_on_compaction::COMPACT_HINT,
            "answer": { "kind": "compactFirst", "checked": !any_ticked },
        }));
        // Q68: one tick per thread that can be compacted, the (i) on the first.
        let rows: Vec<Value> = card
            .threads
            .iter()
            .filter(|thread| thread.compactable)
            .enumerate()
            .map(|(index, thread)| {
                let ticked = card.ticked(thread, threshold);
                let label = match thread.tokens {
                    Some(tokens) => format!(
                        "Compact {} first ({})",
                        thread.title,
                        crate::carry_on_compaction::short_tokens(tokens)
                    ),
                    None => format!("Compact {} first", thread.title),
                };
                json!({
                    "id": format!("compactFirst:{}:{}", thread.key.0, thread.key.1),
                    "label": label,
                    "checked": ticked,
                    "hint": (index == 0).then_some(crate::carry_on_compaction::COMPACT_HINT),
                    "answer": {
                        "kind": "compactFirst",
                        "checked": !ticked,
                        "threadProjectId": thread.key.0,
                        "threadSessionId": thread.key.1,
                    },
                })
            })
            .collect();
        notice.checkboxes = Some(Value::Array(rows));
        // The rows name the threads they cover; the words name only the others.
        let others: Vec<&str> = card
            .threads
            .iter()
            .zip(&names)
            .filter(|(thread, _)| !thread.compactable)
            .map(|(_, name)| name.as_str())
            .collect();
        notice.checkboxes_detail = Some(if others.is_empty() {
            CARRY_ON_DETAIL.to_string()
        } else {
            format!("{}. {CARRY_ON_DETAIL}", others.join(", "))
        });
    }
    notice.detected_at = card
        .threads
        .iter()
        .map(|thread| thread.at.clone())
        .min()
        .unwrap_or_default();
    Some(notice)
}

/// A Compact first tick on this coordinator's card: one thread's (`thread`), or, from the single
/// checkbox older clients draw, every thread it covers. False when it shows no card.
pub fn set_stopped_threads_compact(
    coordinator: &SessionKey,
    thread: Option<&SessionKey>,
    checked: bool,
) -> bool {
    let Ok(mut held) = held().lock() else {
        return false;
    };
    let Some(card) = held.get_mut(coordinator) else {
        return false;
    };
    let threshold = crate::carry_on_compaction::threshold();
    let ticks: Vec<(SessionKey, bool)> = card
        .threads
        .iter()
        .filter_map(|listed| match thread {
            Some(key) => (&listed.key == key).then(|| (listed.key.clone(), checked)),
            None => Some((
                listed.key.clone(),
                checked && card.covered_by_single(listed, threshold),
            )),
        })
        .collect();
    card.ticks.extend(ticks);
    true
}

/// Takes the coordinator's held threads for Carry on: each, and whether to compact it first.
pub fn take_stopped_threads(coordinator: &SessionKey) -> Vec<(StoppedThread, bool)> {
    let Some(card) = held()
        .lock()
        .ok()
        .and_then(|mut held| held.remove(coordinator))
    else {
        return Vec::new();
    };
    let threshold = crate::carry_on_compaction::threshold();
    card.threads
        .iter()
        .map(|thread| {
            let compact = card.ticked(thread, threshold);
            (thread.clone(), compact)
        })
        .collect()
}

/// Not now: the threads lose the red dot and the card goes, but the thread watch still leaves them
/// alone until they work again.
pub fn dismiss_stopped_threads(repository: &DomainRepository<'_>, coordinator: &SessionKey) {
    let Some(card) = held()
        .lock()
        .ok()
        .and_then(|mut held| held.remove(coordinator))
    else {
        return;
    };
    for thread in card.threads {
        let Ok(Some(session)) = repository.get_session(&thread.key.0, &thread.key.1) else {
            continue;
        };
        let mut runtime: Map<String, Value> = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(record) = runtime
            .get_mut(STOPPED_BY_RESTART)
            .and_then(Value::as_object_mut)
        {
            record.insert("dismissed".to_string(), json!(true));
        }
        let _ = crate::accounts::endpoint::update_session_members(
            repository,
            &session,
            &runtime,
            &[STOPPED_BY_RESTART],
        );
    }
}

/// The card's Carry on: each held thread loses its hold, is compacted first when its checkbox is
/// ticked, and is told to carry on (which wakes it). Not now dismisses.
pub async fn answer_stopped_threads(
    state: &crate::server::AppState,
    params: &Map<String, Value>,
    carry_on: bool,
) -> Result<(), crate::domain::DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| crate::domain::DomainStateError::bad_request("Missing session."))
    };
    let coordinator = (text("projectId")?, text("sessionId")?);
    // Every database step before the first send: a connection cannot be held across an await.
    let sends = {
        let db = crate::storage::open_gxserver_database(&state.paths)
            .map_err(|error| crate::domain::DomainStateError::bad_request(error.to_string()))?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        if !carry_on {
            dismiss_stopped_threads(&repository, &coordinator);
            republish_card(state, &repository, &coordinator);
            return Ok(());
        }
        let mut sends = Vec::new();
        for (thread, compact) in take_stopped_threads(&coordinator) {
            let Ok(Some(session)) = repository.get_session(&thread.key.0, &thread.key.1) else {
                continue;
            };
            release_thread(&repository, &session);
            let command = compact
                .then(|| crate::carry_on_compaction::compact_command(&session))
                .flatten();
            sends.push((thread.key, command));
        }
        republish_card(state, &repository, &coordinator);
        sends
    };
    let mut failed = Vec::new();
    for (key, command) in sends {
        if let Some(command) = command {
            if let Err(error) =
                crate::carry_on_compaction::send_like_coordinator(state, &key.0, &key.1, command)
                    .await
            {
                failed.push(error);
                continue;
            }
        }
        if let Err(error) = crate::carry_on_compaction::send_like_coordinator(
            state,
            &key.0,
            &key.1,
            CARRY_ON_AFTER_RESTART,
        )
        .await
        {
            failed.push(error);
        }
    }
    match failed.first() {
        Some(error) => Err(crate::domain::DomainStateError::bad_request(format!(
            "Some threads could not be carried on: {error}"
        ))),
        None => Ok(()),
    }
}

/// Re-reads the coordinator's chat, so its open chats drop (or redraw) the card.
fn republish_card(
    state: &crate::server::AppState,
    repository: &DomainRepository<'_>,
    coordinator: &SessionKey,
) {
    let Ok(Some(session)) = repository.get_session(&coordinator.0, &coordinator.1) else {
        return;
    };
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(&session);
    crate::session_chat_options::schedule_session_chat_option_redetect(
        state,
        &coordinator.0,
        &coordinator.1,
        agent.as_deref(),
    );
}
