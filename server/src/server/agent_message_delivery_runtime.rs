//! The delivery watch: settles each message `ghostex agents send` left waiting against its
//! recipient, and republishes the sender's chat when one settles. See the CDXC:SessionChat
//! 2026-10-14 note in server/src/agent_message_deliveries.rs.
//!
//! CDXC:SessionChat 2026-10-14 WHY:
//! "Not sent" follows the rule the orchestrator's "did not reach it" report uses (coordinator_runtime.rs `pending_delivery`, the same constants): only once the message is a minute old and the recipient has sat idle without it for twenty seconds, never while it works, waits on a question or holds the message in its queue, and never before its transcript can be read. A queue row that failed is Not sent with that row's reason.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (the orchestrator's report), server/src/coordinators/delivery.rs (the transcript match).

use super::*;

use crate::agent_message_deliveries::{
    expire_unwatched_sends, load_settled_deliveries, settle_send, waiting_sends, WatchedSend,
    DELIVERY_DELIVERED, DELIVERY_FAILED,
};
use crate::coordinators::{self, classify_thread_session, SessionKey, ThreadProgress, ThreadState};

use super::coordinator_runtime::{
    republish_session_chat, UNDELIVERED_IDLE_MS, UNDELIVERED_MIN_AGE_MS,
};

const TICK: Duration = Duration::from_secs(3);
/// How often one waiting send's recipient transcript is read.
const CHECK_EVERY_MS: i64 = 4_000;

const SESSION_REMOVED: &str = "The session was removed before it took the message.";
const WENT_IDLE: &str =
    "Its transcript does not show the message, and it went idle without taking it.";
const QUEUE_FAILED: &str = "Ghostex could not type the message into the session.";

#[derive(Default)]
struct WatchMemory {
    checked_at: HashMap<(SessionKey, String), i64>,
    /// Since when the recipient has been idle without the message.
    idle_since: HashMap<(SessionKey, String), i64>,
    /// Each recipient's transcript file, resolved once.
    transcript_paths: HashMap<SessionKey, std::path::PathBuf>,
}

pub(crate) fn start_agent_message_delivery_runtime(state: Arc<AppState>) {
    let memory = Arc::new(Mutex::new(WatchMemory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let loading = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(db) = open_gxserver_database(&loading.paths) {
                load_settled_deliveries(&db);
            }
        })
        .await;
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let ticking = state.clone();
            let ticking_memory = memory.clone();
            let senders = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for (project_id, session_id) in senders {
                republish_session_chat(&state, &project_id, &session_id);
            }
        }
    });
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// One pass over the waiting sends; returns the senders whose chat must be republished.
fn tick(state: &AppState, memory: &Mutex<WatchMemory>) -> Vec<SessionKey> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let mut republish = expire_unwatched_sends(&db);
    let sends = waiting_sends(&db);
    let Ok(mut memory) = memory.lock() else {
        return republish;
    };
    let watched: HashSet<(SessionKey, String)> = sends.iter().map(WatchedSend::key).collect();
    memory.checked_at.retain(|key, _| watched.contains(key));
    memory.idle_since.retain(|key, _| watched.contains(key));
    let recipients: HashSet<&SessionKey> = sends.iter().map(|send| &send.recipient).collect();
    memory
        .transcript_paths
        .retain(|key, _| recipients.contains(key));
    if sends.is_empty() {
        return republish;
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    for send in &sends {
        let key = send.key();
        if memory
            .checked_at
            .get(&key)
            .is_some_and(|checked| now - checked < CHECK_EVERY_MS)
        {
            continue;
        }
        memory.checked_at.insert(key.clone(), now);
        let (state_name, failure) =
            match verdict(&db, &repository, &mut memory, send, now, &now_iso) {
                Some(verdict) => verdict,
                None => continue,
            };
        if settle_send(&db, send, state_name, failure.as_deref())
            && !republish.contains(&send.sender)
        {
            republish.push(send.sender.clone());
        }
    }
    republish
}

/// How a waiting send ended, or `None` while it is still on its way.
fn verdict(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    memory: &mut WatchMemory,
    send: &WatchedSend,
    now: i64,
    now_iso: &str,
) -> Option<(&'static str, Option<String>)> {
    let key = send.key();
    let (project_id, session_id) = &send.recipient;
    let Some(session) = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten()
    else {
        return Some((DELIVERY_FAILED, Some(SESSION_REMOVED.to_string())));
    };
    let mut path = memory.transcript_paths.remove(&send.recipient);
    let recorded = coordinators::transcript_records_message(
        &session,
        &send.needles,
        send.sent_at_ms,
        &mut path,
    );
    if let Some(path) = path {
        memory.transcript_paths.insert(send.recipient.clone(), path);
    }
    match recorded {
        Some(true) => return Some((DELIVERY_DELIVERED, None)),
        // No transcript to judge by yet: a missing message cannot be told from a slow agent.
        None => {
            memory.idle_since.remove(&key);
            return None;
        }
        Some(false) => {}
    }
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        db, project_id, session_id,
    );
    if let Some(row) = queue.queue.iter().find(|row| {
        coordinators::holds_message(
            &coordinators::normalize_delivery_text(&row.text),
            &send.needles,
        )
    }) {
        memory.idle_since.remove(&key);
        if row.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_FAILED {
            let reason = row
                .error_message
                .as_deref()
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .unwrap_or(QUEUE_FAILED);
            return Some((DELIVERY_FAILED, Some(reason.to_string())));
        }
        return None;
    }
    let as_run = ThreadProgress {
        resolved: false,
        has_run: true,
    };
    if matches!(
        classify_thread_session(Some(&session), as_run, now_iso, false),
        ThreadState::Working | ThreadState::Waiting
    ) {
        memory.idle_since.remove(&key);
        return None;
    }
    let idle_since = *memory.idle_since.entry(key).or_insert(now);
    if now - send.sent_at_ms < UNDELIVERED_MIN_AGE_MS || now - idle_since < UNDELIVERED_IDLE_MS {
        return None;
    }
    Some((DELIVERY_FAILED, Some(WENT_IDLE.to_string())))
}
