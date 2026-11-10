//! CDXC:SessionStatus 2026-11-04 WHY:
//! OpenCode's working and idle reach Ghostex only from its TUI plugin (server/src/agent_hooks/install/opencode_tui.js), which polls the conversation the TUI shows. OpenCode 2 runs the conversation in its background service, so a turn ends whether or not the TUI is still on it: once the TUI moves to another conversation, closes, or its `Stop` post fails, the last `SessionBusy` stands, and the sidebar, the chat's Stop square and its "Cogitating…" card stay working after the reply and every subagent finished (GitHub #403, Windows, OpenCode with subagents). The service is the record: this observer watches OpenCode sessions that still show working and sends the plugin's own `Stop` once the service says the lead conversation went idle after that working began, with no running shell, no question or approval open and no child conversation running, on two passes in a row so a turn that just began is never cut short.
//! CDXC:SessionStatus 2026-11-10 WHY:
//! The same gap runs the other way: the plugin reports `SessionBusy` only while the TUI shows that conversation and its notifier runs (on Windows it spawns gxserver for each report), so a long turn the TUI is not showing, or a report that failed, left the session idle while OpenCode's service ran it, and the chat showed Send with no working word until something made the plugin report again (GitHub #529, Windows, OpenCode). The observer now also sends the plugin's own `SessionBusy` when the service says the lead conversation runs, with no question or approval open, on two passes in a row while Ghostex shows it not working; the `Stop` above settles that stint once the service goes idle, so a turn the service ended never stays working (#403).
//! SEE-ALSO: server/src/zcode_turn_activity.rs (the same observer shape), server/src/session_chat_opencode/mirror.rs (the cached conversation read and its active check), server/src/session_chat_opencode/status.rs (`fleet`, the children's working state).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{read_session_text, session_observer_key, AppState};
use crate::storage::open_gxserver_database;

/// How far OpenCode's idle stamp may sit before the hook's working stamp and still end that
/// stint: the plugin's `SessionBusy` is stamped by a process it spawns after reading "running",
/// so it can land a moment after a turn that ended right then.
const IDLE_STAMP_SLACK_MS: i64 = 2_000;

/// What OpenCode's service says about the lead conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LeadState {
    /// In the service's active set, or running a shell command.
    pub(crate) active: bool,
    /// A question or an approval is waiting for the user.
    pub(crate) waiting_on_user: bool,
    /// `time.idle` of the conversation (ms): when its last run ended.
    pub(crate) idle_at: Option<i64>,
}

/// What one pass decided for a session, kept for the next pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    /// The turn that ended at this idle stamp was seen over once; the next pass confirms it.
    Ended(i64),
    /// `Stop` was sent for the turn that ended at this idle stamp.
    Reported(i64),
    /// The service said the lead runs while Ghostex showed it not working; the next pass confirms it.
    Running,
}

/// The idle stamp of the run that ended the working stint begun at `working_since`, when the
/// lead and all its children are done.
pub(crate) fn ended_turn(
    working_since: i64,
    lead: &LeadState,
    children_working: bool,
) -> Option<i64> {
    if lead.active || lead.waiting_on_user || children_working {
        return None;
    }
    lead.idle_at
        .filter(|idle_at| idle_at + IDLE_STAMP_SLACK_MS >= working_since)
}

/// Whether to send `Stop` now, and what to remember for the next pass.
pub(crate) fn next_step(previous: Option<Seen>, ended: Option<i64>) -> (bool, Option<Seen>) {
    match (previous, ended) {
        (_, None) => (false, None),
        (Some(Seen::Reported(at)), Some(ended)) if at == ended => (false, previous),
        (Some(Seen::Ended(at)), Some(ended)) if at == ended => (true, Some(Seen::Reported(ended))),
        (_, Some(ended)) => (false, Some(Seen::Ended(ended))),
    }
}

/// Whether to send `SessionBusy` now, for a session Ghostex shows not working, and what to remember
/// for the next pass: the service must say the lead runs, waiting on nobody, twice in a row.
pub(crate) fn busy_step(previous: Option<Seen>, lead: &LeadState) -> (bool, Option<Seen>) {
    match (previous, lead.active && !lead.waiting_on_user) {
        (_, false) => (false, None),
        (Some(Seen::Running), true) => (true, None),
        (_, true) => (false, Some(Seen::Running)),
    }
}

pub(crate) fn spawn_opencode_turn_activity_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut seen = HashMap::new();
        loop {
            let state = state.clone();
            seen = tokio::task::spawn_blocking(move || {
                refresh(&state, &mut seen);
                seen
            })
            .await
            .unwrap_or_default();
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        }
    })
}

fn lead_state(snapshot: &crate::session_chat_opencode::Snapshot) -> LeadState {
    LeadState {
        active: snapshot.working,
        waiting_on_user: !snapshot.permissions.is_empty() || !snapshot.forms.is_empty(),
        idle_at: snapshot.info["time"]["idle"].as_i64(),
    }
}

fn refresh(state: &AppState, seen: &mut HashMap<String, Seen>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    // Every two seconds: with no OpenCode session running there is nothing to watch.
    match repository.has_running_session_with_agent("opencode") {
        Ok(true) => {}
        Ok(false) => {
            seen.clear();
            return;
        }
        Err(_) => return,
    }
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return;
    };
    let mut watched = HashSet::new();
    for session in &sessions {
        if read_session_text(session, "agentId").as_deref() != Some("opencode") {
            continue;
        }
        let (Some(project_id), Some(session_id), Ok(conversation)) = (
            read_session_text(session, "projectId"),
            read_session_text(session, "sessionId"),
            crate::session_chat_opencode::session_id(session),
        ) else {
            continue;
        };
        let key = session_observer_key(&project_id, &session_id);
        watched.insert(key.clone());
        // The conversation read is cached and refetched only when OpenCode reports a change.
        let Ok(snapshot) = crate::session_chat_opencode::snapshot(&conversation) else {
            continue;
        };
        let lead = lead_state(&snapshot);
        let Some(working_since) = crate::zcode_turn_activity::working_since_ms(session) else {
            let (send, next) = busy_step(seen.get(&key).copied(), &lead);
            match next {
                Some(next) => seen.insert(key.clone(), next),
                None => seen.remove(&key),
            };
            // CDXC:SessionStatus 2026-11-10 WHY:
            // The read above can be up to 15 seconds old (mirror.rs `freshness`). A `SessionBusy` sent from a read that predates the turn's end would start a working stint after OpenCode's idle time, which the `Stop` above never ends (it only ends a stint the idle time is not older than), and hold a finished thread as working. So the service is asked again, uncached, right before it is sent.
            let still_running = send && {
                crate::session_chat_opencode::invalidate(&conversation);
                crate::session_chat_opencode::snapshot(&conversation).is_ok_and(|fresh| {
                    let fresh = lead_state(&fresh);
                    fresh.active && !fresh.waiting_on_user
                })
            };
            if still_running {
                report(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                    &conversation,
                    "SessionBusy",
                );
            }
            continue;
        };
        // Children are asked only once the lead itself is done; an unreadable roster proves nothing.
        let ended = match ended_turn(working_since, &lead, false) {
            Some(_) => match crate::session_chat_opencode::fleet(&conversation, &snapshot.messages)
            {
                Ok(fleet) => ended_turn(
                    working_since,
                    &lead,
                    fleet.is_some_and(|fleet| fleet.is_working()),
                ),
                Err(_) => continue,
            },
            None => None,
        };
        let (send, next) = next_step(seen.get(&key).copied(), ended);
        match next {
            Some(next) => seen.insert(key.clone(), next),
            None => seen.remove(&key),
        };
        if send {
            report(
                state,
                &db,
                &repository,
                &project_id,
                &session_id,
                &conversation,
                "Stop",
            );
        }
    }
    seen.retain(|key, _| watched.contains(key));
}

/// Sends `event` as the OpenCode plugin would for this session's conversation.
fn report(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    conversation: &str,
    event: &str,
) {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("agentName".to_string(), json!("opencode"));
    params.insert("agentSessionId".to_string(), json!(conversation));
    params.insert("eventName".to_string(), json!(event));
    let Ok(output) = dispatch_agent_endpoint(
        repository,
        db,
        &state.paths.home_dir,
        "/api/ingestAgentHookEvent",
        &params,
        None,
    ) else {
        return;
    };
    if let Some((project_id, session_id)) = output.presentation_session {
        let _ = crate::server::schedule_presentation_session_delta(
            state,
            db,
            repository,
            &project_id,
            &session_id,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUSY_AT: i64 = 1_800_000_000_000;

    fn lead(active: bool, idle_at: Option<i64>) -> LeadState {
        LeadState {
            active,
            waiting_on_user: false,
            idle_at,
        }
    }

    /// Runs the passes in order and returns the pass numbers that sent `Stop`.
    fn passes(steps: &[(LeadState, bool)]) -> Vec<usize> {
        let mut seen = None;
        let mut sent = Vec::new();
        for (index, (lead, children_working)) in steps.iter().enumerate() {
            let (send, next) = next_step(seen, ended_turn(BUSY_AT, lead, *children_working));
            seen = next;
            if send {
                sent.push(index);
            }
        }
        sent
    }

    #[test]
    fn a_turn_whose_stop_never_arrived_settles_on_the_second_idle_pass() {
        let ended = BUSY_AT + 60_000;
        assert_eq!(
            passes(&[
                (lead(true, Some(BUSY_AT - 300_000)), false),
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), false),
            ]),
            vec![2],
            "one Stop, after two passes agree, never repeated"
        );
    }

    #[test]
    fn a_child_still_running_after_the_lead_went_idle_holds_the_session_until_it_finishes() {
        let ended = BUSY_AT + 60_000;
        assert_eq!(
            passes(&[
                (lead(false, Some(ended)), true),
                (lead(false, Some(ended)), true),
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), false),
            ]),
            vec![3]
        );
    }

    #[test]
    fn a_child_that_starts_between_the_two_passes_restarts_the_count() {
        let ended = BUSY_AT + 60_000;
        assert_eq!(
            passes(&[
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), true),
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), false),
            ]),
            vec![3]
        );
    }

    #[test]
    fn the_previous_turns_idle_stamp_never_ends_a_new_turn() {
        // The plugin said busy for a new prompt the service has not listed as active yet.
        let previous = BUSY_AT - 30_000;
        assert_eq!(
            passes(&[
                (lead(false, Some(previous)), false),
                (lead(false, Some(previous)), false),
                (lead(false, None), false),
            ]),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn a_turn_that_ended_as_the_busy_report_was_stamped_still_settles() {
        let ended = BUSY_AT - 400;
        assert_eq!(
            passes(&[
                (lead(false, Some(ended)), false),
                (lead(false, Some(ended)), false),
            ]),
            vec![1]
        );
    }

    #[test]
    fn a_new_turn_starting_between_the_passes_is_not_cut_short() {
        let ended = BUSY_AT + 60_000;
        assert_eq!(
            passes(&[
                (lead(false, Some(ended)), false),
                (lead(true, Some(ended)), false),
                (lead(false, Some(ended + 90_000)), false),
                (lead(false, Some(ended + 90_000)), false),
            ]),
            vec![3]
        );
    }

    #[test]
    fn a_question_or_approval_waiting_keeps_the_session_out_of_idle() {
        let waiting = LeadState {
            active: false,
            waiting_on_user: true,
            idle_at: Some(BUSY_AT + 60_000),
        };
        assert_eq!(
            passes(&[(waiting, false), (waiting, false)]),
            Vec::<usize>::new()
        );
    }
}
