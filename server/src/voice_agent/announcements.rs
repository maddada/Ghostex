//! Spoken announcements: while any voice conversation is open, every session's state (this
//! computer's and the connected computers', `computers.rs`) is read every two seconds and a session that finishes its turn or starts waiting on a question or an approval
//! is announced to every open conversation; a call about one session keeps only that session's
//! (`focus.rs`). Each pass also ends every open call once the voice agent is turned off
//! (`feature.rs`).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: while a voice conversation is open it speaks up on its own with short announcements when a session finishes its turn or starts waiting on a question or an approval ("Sidebar fix finished").
//! CDXC:VoiceMode 2026-10-20 WHY:
//! Polling the session rows (the coordinator supervisor's approach, `classify_thread_session`) instead of hooking the activity endpoints: a question card can arrive through a hook that changes no activity. A finished turn is the Stop hook's new turn-complete attention event, so a turn shorter than the poll is still announced (live test: a one-word reply finished between two reads and was missed); agents without hooks fall back to a working state followed by an idle one, each held for two reads so a turn that flickers idle between tool calls is not called finished.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::conversation::{Announcement, Input};
use super::registry;
use super::sessions::{self, VoiceState};
use crate::server::AppState;

const POLL: Duration = Duration::from_secs(2);

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts the watcher if it is not running; it stops by itself once no conversation is open.
pub(super) fn ensure_running(state: &Arc<AppState>) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        // By session ref, which names the computer too (ids are only unique per computer).
        let mut stable: HashMap<String, VoiceState> = HashMap::new();
        let mut seen: HashMap<String, VoiceState> = HashMap::new();
        let mut turn_events: HashMap<String, Option<String>> = HashMap::new();
        let mut seeded = false;
        loop {
            if registry::count() == 0 {
                RUNNING.store(false, Ordering::SeqCst);
                // A conversation opened between the check and the store restarts the watcher.
                if registry::count() == 0 || RUNNING.swap(true, Ordering::SeqCst) {
                    return;
                }
            }
            // The voice agent was turned off on Settings > Extensions: every open call ends, and
            // the watcher stops once they are gone (`feature.rs`).
            let paths = state.paths.clone();
            let on = tokio::task::spawn_blocking(move || super::feature::voice_agent_on(&paths))
                .await
                .unwrap_or(true);
            if !on {
                for tx in registry::senders() {
                    let _ = tx.send(Input::TurnedOff);
                }
                tokio::time::sleep(POLL).await;
                continue;
            }
            if let Ok(rows) = sessions::list_sessions_async(&state).await {
                let mut lines: Vec<Announcement> = Vec::new();
                let mut present: HashSet<String> = HashSet::new();
                for row in &rows {
                    let key = row.reference();
                    present.insert(key.clone());
                    let known = turn_events.contains_key(&key);
                    let previous_event = turn_events
                        .insert(key.clone(), row.turn_event.clone())
                        .flatten();
                    let new_turn = seeded
                        && known
                        && row.turn_event.is_some()
                        && row.turn_event != previous_event
                        && row.state == VoiceState::Finished;
                    if new_turn {
                        stable.insert(key.clone(), row.state);
                        seen.insert(key.clone(), row.state);
                        lines.push(Announcement {
                            text: format!("{} finished its turn.", row.spoken_title()),
                            kind: "turnFinished",
                            project_id: row.project_id.clone(),
                            session_id: row.session_id.clone(),
                            server_id: row.computer.as_ref().map(|c| c.server_id.clone()),
                            model_text: None,
                        });
                        continue;
                    }
                    let previous_seen = seen.insert(key.clone(), row.state);
                    if previous_seen != Some(row.state) {
                        continue;
                    }
                    let before = stable.insert(key.clone(), row.state);
                    if !seeded || before == Some(row.state) {
                        continue;
                    }
                    // A session first seen now (just opened, or just started by the voice agent) is
                    // announced from its next change on.
                    let was_busy = matches!(before, Some(VoiceState::Working));
                    let was_awake = !matches!(before, None | Some(VoiceState::Sleeping));
                    let (text, kind) = match row.state {
                        VoiceState::Finished if was_busy && row.turn_event.is_none() => (
                            format!("{} finished its turn.", row.spoken_title()),
                            "turnFinished",
                        ),
                        VoiceState::WaitingOnQuestion if was_awake => (
                            format!("{} is waiting on a question.", row.spoken_title()),
                            "waitingOnQuestion",
                        ),
                        VoiceState::WaitingOnApproval if was_awake => (
                            format!("{} is waiting for your approval.", row.spoken_title()),
                            "waitingOnApproval",
                        ),
                        _ => continue,
                    };
                    lines.push(Announcement {
                        text,
                        kind,
                        project_id: row.project_id.clone(),
                        session_id: row.session_id.clone(),
                        server_id: row.computer.as_ref().map(|c| c.server_id.clone()),
                        model_text: None,
                    });
                }
                stable.retain(|key, _| present.contains(key));
                turn_events.retain(|key, _| present.contains(key));
                seen.retain(|key, _| present.contains(key));
                seeded = true;
                // Each call keeps its own scope: a focused one learns from the list whether its
                // session was renamed or closed, and drops the announcements it does not want.
                let rows = Arc::new(rows);
                for tx in registry::senders() {
                    let _ = tx.send(Input::Sessions(rows.clone()));
                    if !lines.is_empty() {
                        let _ = tx.send(Input::Announce(lines.clone()));
                    }
                }
            }
            tokio::time::sleep(POLL).await;
        }
    });
}
