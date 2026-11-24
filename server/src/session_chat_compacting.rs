/*
CDXC:AgentScreenDetection 2026-08-31:
The zmx screen classifier in session_chat_terminal_activity/ is the source of
truth for Claude Code's live `/compact` state. This module projects that
ephemeral screen fact into the session's effective activity without rewriting
agentActivity, which remains owned by hooks and terminal-title transitions.

Only a stable detected-at marker is persisted. Progress details remain in the
screen cache and chat frames. The marker makes presentation, lifecycle guards,
and the daemon-owned prompt queue agree even when no client is connected.
*/

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::events::GxserverEventHub;
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Durable runtime-settings key for the currently observed compaction run.
/// The value is the activity's stable `detectedAt`, so presentation can expose
/// a real working-start boundary without copying progress details into the
/// domain session.
pub const SESSION_CHAT_COMPACTING_DETECTED_AT_KEY: &str = "sessionChatCompactingDetectedAt";

pub(crate) const SESSION_CHAT_FLEET_DETECTED_AT_KEY: &str = "sessionChatFleetDetectedAt";
pub(crate) const SESSION_CHAT_MONITOR_DETECTED_AT_KEY: &str = "sessionChatMonitorDetectedAt";

pub(crate) fn session_chat_monitor_detected_at(session: &Value) -> Option<String> {
    session
        .pointer("/runtimeSettings/sessionChatMonitorDetectedAt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn session_chat_fleet_detected_at(session: &Value) -> Option<String> {
    session
        .pointer("/runtimeSettings/sessionChatFleetDetectedAt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn session_chat_compacting_detected_at(session: &Value) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(SESSION_CHAT_COMPACTING_DETECTED_AT_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[derive(Clone)]
pub(crate) struct SessionChatCompactingPublisher {
    event_hub: GxserverEventHub,
    paths: GxserverPaths,
    presentation_event_sequence: Arc<Mutex<()>>,
    server_id: String,
}

impl SessionChatCompactingPublisher {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            event_hub: state.event_hub.clone(),
            paths: state.paths.clone(),
            presentation_event_sequence: state.presentation_event_sequence.clone(),
            server_id: state.metadata.server_id.clone(),
        }
    }

    pub(crate) fn publish(&self, project_id: &str, session_id: &str, detected_at: Option<&str>) {
        self.publish_marker(
            project_id,
            session_id,
            SESSION_CHAT_COMPACTING_DETECTED_AT_KEY,
            detected_at,
        );
    }

    pub(crate) fn publish_fleet(
        &self,
        project_id: &str,
        session_id: &str,
        detected_at: Option<&str>,
    ) {
        self.publish_marker(
            project_id,
            session_id,
            SESSION_CHAT_FLEET_DETECTED_AT_KEY,
            detected_at,
        );
    }

    pub(crate) fn publish_monitor(
        &self,
        project_id: &str,
        session_id: &str,
        detected_at: Option<&str>,
    ) {
        self.publish_marker(
            project_id,
            session_id,
            SESSION_CHAT_MONITOR_DETECTED_AT_KEY,
            detected_at,
        );
    }

    /// Returns `false` when the database could not be opened or the statement failed. A statement
    /// that changed no row succeeded: the stored value already matches, or the session is gone.
    pub(crate) fn publish_marker(
        &self,
        project_id: &str,
        session_id: &str,
        key: &str,
        detected_at: Option<&str>,
    ) -> bool {
        let Ok(db) = open_gxserver_database(&self.paths) else {
            return false;
        };
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        let json_path = format!("$.{key}");
        /*
        Activity hooks and screen detection can land together at both ends of
        a compaction. Mutate only this JSON member in SQLite, atomically, so a
        stale read-modify-write cannot erase a concurrent agentActivity update
        (or vice versa).
        */
        let changed = match detected_at {
            Some(detected_at) => db.execute(
                r#"
                UPDATE sessions
                SET runtimeSettingsJson = json_set(runtimeSettingsJson, ?3, ?4),
                    updatedAt = ?5
                WHERE projectId = ?1 AND sessionId = ?2
                  AND json_extract(runtimeSettingsJson, ?3) IS NOT ?4
                  AND (?6 = 0 OR json_type(runtimeSettingsJson, ?3) IS NULL)
                "#,
                rusqlite::params![
                    project_id,
                    session_id,
                    json_path,
                    detected_at,
                    crate::domain::now_iso(),
                    key != SESSION_CHAT_COMPACTING_DETECTED_AT_KEY,
                ],
            ),
            None => db.execute(
                r#"
                UPDATE sessions
                SET runtimeSettingsJson = json_remove(runtimeSettingsJson, ?3),
                    updatedAt = ?4
                WHERE projectId = ?1 AND sessionId = ?2
                  AND json_type(runtimeSettingsJson, ?3) IS NOT NULL
                "#,
                rusqlite::params![project_id, session_id, json_path, crate::domain::now_iso(),],
            ),
        };
        if !matches!(changed, Ok(1)) {
            return changed.is_ok();
        }
        let Ok(_event_sequence) = self.presentation_event_sequence.lock() else {
            return true;
        };
        let Ok(delta) = crate::presentation::build_presentation_session_delta(
            &db,
            &repository,
            project_id,
            session_id,
        ) else {
            return true;
        };
        let Ok(revision) = crate::presentation::increment_presentation_revision(&db) else {
            return true;
        };
        self.event_hub.broadcast(json!({
            "delta": delta,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": self.server_id,
            "type": "presentationDelta",
        }));
        true
    }
}

/// One screen-approval marker write at a time, across every session (see
/// `publish_screen_approval_marker`). It holds the clears still owed to sessions whose reading was
/// dropped (`forget_session_chat_options`) while the database refused the drop's clear, keyed like
/// the option cache: with the reading gone, no cache entry is left to owe them, and every writer of
/// the marker already holds this lock when it decides what to write.
static SCREEN_APPROVAL_MARKER_WRITES: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Holds off every other screen-approval marker write until the guard is dropped. Take it before
/// the session option cache, never while holding the cache.
pub(crate) fn lock_screen_approval_marker_writes(
) -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    SCREEN_APPROVAL_MARKER_WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether a screen-approval marker write is under way, for tests that check who holds it.
#[cfg(test)]
pub(crate) fn screen_approval_marker_writes_held() -> bool {
    matches!(
        SCREEN_APPROVAL_MARKER_WRITES.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    )
}

/// Writes the screen-approval marker the session's latest committed verdict still owes
/// (`SessionChatOptionCacheEntry::unpublished_screen_approval`), or clears it when the session has
/// no reading, or when its readings since a drop whose clear was refused are all failed captures.
/// Every writer of the marker goes through here: the detector after each capture that owes a write
/// or is the first since the session's reading was dropped, and the stall sweep's refresh;
/// `forget_session_chat_options` clears it through `clear_screen_approval_marker` under the same
/// lock.
///
/// CDXC:SessionStatus 2026-11-23 WHY:
/// Several readers capture one session's screen at once (a chat read, the probes after a send, the follower, the stall sweep), and each used to write the marker it had just decided after leaving the cache lock. A reader that saw the screen clear could then land its clear after a newer reader's set, and the cache, already holding "approval", wrote nothing again, so the session stopped reading as needing the user. A write that failed (the database busy or unopenable) was swallowed the same way. So the write is taken from the cache under one lock at the moment it is made, always the latest verdict's, and a failed one stays owed for the next capture to retry. The lock is one for all sessions because these writes happen only when the verdict changes or a retry is owed; the cache lock is never held across the database write, so detection of other sessions is not held up by SQLite. Dropping a reading takes this lock before it removes the cache entry and clears the marker while still holding it, without asking the cache: a capture that failed in between commits an entry owing no write and would otherwise keep the dropped reading's mark, while a whole capture committed after the removal writes its own verdict once the lock is released. A drop's clear that the database refused has no cache entry left to owe it, so the lock itself keeps it until the next capture of that session (a failed one too) or the stall sweep's refresh makes it, or a whole verdict committed after the drop writes its own mark; it used to be lost, and an old approval stayed marked after an agent switch or restart until the screen could be read whole again.
pub(crate) fn publish_screen_approval_marker(state: &AppState, project_id: &str, session_id: &str) {
    let mut writing = lock_screen_approval_marker_writes();
    let key = crate::server::session_observer_key(project_id, session_id);
    let owed = match state.session_chat_option_cache.lock() {
        Ok(mut cache) => match cache.get_mut(&key) {
            Some(entry) => {
                // A whole verdict committed after the drop writes its own mark instead.
                if entry.projected_screen_approval.is_some() {
                    writing.remove(&key);
                }
                entry
                    .unpublished_screen_approval
                    .take()
                    .or_else(|| writing.contains(&key).then_some(None))
            }
            None => Some(None),
        },
        Err(_) => return,
    };
    let Some(detected_at) = owed else {
        return;
    };
    write_screen_approval_marker(&mut writing, state, project_id, session_id, detected_at);
}

/// Clears the screen-approval marker of a session whose reading the caller has just removed while
/// holding `writing` (see `publish_screen_approval_marker`).
pub(crate) fn clear_screen_approval_marker(
    writing: &mut std::sync::MutexGuard<'static, BTreeSet<String>>,
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    write_screen_approval_marker(writing, state, project_id, session_id, None);
}

fn write_screen_approval_marker(
    writing: &mut std::sync::MutexGuard<'static, BTreeSet<String>>,
    state: &AppState,
    project_id: &str,
    session_id: &str,
    detected_at: Option<String>,
) {
    let key = crate::server::session_observer_key(project_id, session_id);
    if SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        crate::session_approval::SESSION_CHAT_SCREEN_APPROVAL_DETECTED_AT_KEY,
        detected_at.as_deref(),
    ) {
        writing.remove(&key);
        return;
    }
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        match cache.get_mut(&key) {
            // A verdict committed meanwhile owes its own write, which replaces this one.
            Some(entry) => {
                if entry.unpublished_screen_approval.is_none() {
                    entry.unpublished_screen_approval = Some(detected_at);
                }
            }
            // No reading is left to owe the clear; the next capture of the session or the sweep makes it.
            None => {
                if detected_at.is_none() {
                    writing.insert(key);
                }
            }
        }
    }
}

/// BLOCKING (at most one screen capture, through the detector's own cache lifetime). A running
/// session that still carries the compacting marker has its screen read again, and the marker
/// follows what that whole capture shows.
///
/// CDXC:Coordinators 2026-11-18 WHY:
/// The marker is the screen's to set and clear (CDXC:AgentScreenDetection 2026-09-02 in session_chat_options/detector.rs), but the screen is only read while something follows the session. Claude Code 2.1.326 compacts an idle session on its own shortly before its prompt cache expires; a read during that compaction set the marker, nothing read the screen of the idle, unwatched session again, and the marker kept a finished session "working" for good: its sidebar dot, and on 2026-11-17 the stall nudges to three finished coordinator threads (coordinators/idle_compaction.rs). So the sidebar-wide stall tick reads the screen of every session still marked compacting until a capture says otherwise. A compaction that is still running keeps its marker.
pub(crate) fn refresh_compacting_marker_from_screen(state: &AppState, session: &Value) {
    if session_chat_compacting_detected_at(session).is_none() {
        return;
    }
    let field = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id)) = (field("projectId"), field("sessionId")) else {
        return;
    };
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    let detection = crate::session_chat_options::SessionChatOptionDetector::new(state)
        .detect_blocking(&project_id, &session_id, agent.as_deref(), false);
    // The detector publishes a change it sees itself; a cache that already agreed with the screen
    // while the stored marker did not (a write that lost a race) is put right here.
    if detection.captured
        && !crate::session_chat_terminal_activity::is_session_chat_compacting_activity(
            detection.activity.as_ref(),
        )
    {
        SessionChatCompactingPublisher::new(state).publish(&project_id, &session_id, None);
    }
}

/// BLOCKING (at most one screen capture, through the detector's own cache lifetime). A running
/// session marked as showing an approval on its screen has its screen read again, and the marker
/// follows the latest verdict, for the reason the compaction marker above is: an approval answered
/// in a terminal nothing else reads would otherwise keep its red dot (the
/// CDXC:SessionStatus 2026-11-21 WHY in session_approval.rs).
pub(crate) fn refresh_screen_approval_marker_from_screen(state: &AppState, session: &Value) {
    if !crate::session_approval::screen_shows_approval(session) {
        return;
    }
    let field = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id)) = (field("projectId"), field("sessionId")) else {
        return;
    };
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    crate::session_chat_options::SessionChatOptionDetector::new(state).detect_blocking(
        &project_id,
        &session_id,
        agent.as_deref(),
        false,
    );
    // The detector writes what a capture owes; a stored marker the latest verdict, already
    // written, says is gone (a write made outside this path) is owed its clear here.
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        if let Some(entry) = cache.get_mut(&crate::server::session_observer_key(
            &project_id,
            &session_id,
        )) {
            if entry.projected_screen_approval == Some(false)
                && entry.unpublished_screen_approval.is_none()
            {
                entry.unpublished_screen_approval = Some(None);
            }
        }
    }
    publish_screen_approval_marker(state, &project_id, &session_id);
}
