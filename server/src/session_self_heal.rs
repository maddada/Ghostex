//! Self-heal for agent sessions whose terminal daemon dies under a running agent. The runtime
//! (server/session_self_heal_runtime.rs) looks at every running agent session every few seconds;
//! this file keeps what it learned (the book) and the rules that decide when a session is restarted,
//! when Ghostex gives up, and what the coordinator and the user are told.
//!
//! CDXC:SessionSleep 2026-11-06 DECISION:
//! User, after every session on the computer was killed three times in one night and each thread sat "running" with a dead terminal, red "not moving" dots and stale "no new output" reports until the coordinator reloaded each one by hand: "fix the red dot issue there and make sure it doesn't happen for users". A running agent session whose terminal daemon disappears while its agent was running (not slept, closed or stopped by the user or by Ghostex) is restarted on its own conversation within seconds, the way `ghostex agents reload` does it (resume command, same account, the wake's account rules when that account cannot start). A coordinator's thread, and a coordinator itself, is then told "Your session stopped unexpectedly and was restarted on the same conversation; check where you were and carry on." At most 3 restarts in 10 minutes; after that, or when a restart cannot work, the session shows as needing attention (the red dot and one notification) and the thread's coordinator gets one report. The coordinator hears one "restarted after its session stopped unexpectedly" report instead of "no new output" reports, since both stall watches stop watching a session whose daemon is gone and start over after the restart.
//! CDXC:SessionSleep 2026-11-06 WHY:
//! Nothing noticed a daemon that was SIGKILLed: zmx sends no exit event, the desktop only closes its tab when the attach client ends, and the provider pass that marks a row `missing` (server/session_state_sync.rs) runs only inside client reads and deliberately keeps a restorable agent `running` for wake-on-open, so a dead session waited for someone to open it while its stale "working" hook state kept the stall watches reporting it.
//! Only a daemon this gxserver saw alive is ever healed: daemons a reboot or shutdown ended are resumed by the computer-startup pass (computer_startup/resume_after_restart.rs), and a wire-generation cycle at startup sleeps the sessions it ends. A daemon must stay gone for `GONE_CONFIRM_MS` and the row is read again right before the restart, so a sleep, close or account switch (which kill the daemon a moment before writing the row) is never mistaken for a crash. A shell the user ended with `exit` after the agent quit is left alone: an agent process must have been seen in the session (or its hooks said it was working) when the daemon was last alive. The row is not marked `missing` here: a logout ends the daemons a moment before gxserver, and the computer-startup pass needs those rows to still say `exists`.
//! SEE-ALSO: server/src/server/session_self_heal_runtime.rs, server/src/server/coordinator_runtime.rs (the hold and the report), server/src/server/session_stall_runtime.rs and server/src/coordinators/stall.rs (`QuietOwner::Heal`), server/src/coordinators/playbook.md.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// (projectId, sessionId).
pub type SessionKey = (String, String);

/// How long a daemon must stay gone before its session counts as dead.
pub const GONE_CONFIRM_MS: i64 = 6_000;
/// The window the restart budget counts in.
pub const RESTART_WINDOW_MS: i64 = 10 * 60_000;
/// Restarts allowed per session in `RESTART_WINDOW_MS`; one more death gives up.
pub const MAX_RESTARTS_PER_WINDOW: usize = 3;
/// Waits before trying again after a restart that failed, by consecutive failures.
const RETRY_AFTER_FAILURE_MS: [i64; 2] = [15_000, 45_000];
/// How often a live session's process tree is looked at for its agent.
pub const AGENT_CHECK_EVERY_MS: i64 = 10_000;

/// What a healed thread or coordinator is told once its agent is back.
pub const CARRY_ON_MESSAGE: &str = "Your session stopped unexpectedly and was restarted on the same conversation; check where you were and carry on.";

static SERVER_STOPPING: AtomicBool = AtomicBool::new(false);

/// Called when gxserver starts to stop (`broadcast_server_stopping`): from then on nothing is
/// restarted, since a stop-all ends every daemon on purpose.
pub fn note_server_stopping() {
    SERVER_STOPPING.store(true, Ordering::SeqCst);
}

pub fn server_stopping() -> bool {
    SERVER_STOPPING.load(Ordering::SeqCst)
}

/// What one tick saw of one running agent session.
pub struct Sighting {
    pub key: SessionKey,
    pub daemon_alive: bool,
    /// Whether an agent process was found under its daemon, when this tick looked.
    pub agent_running: Option<bool>,
    /// Its hooks say it is working or waiting on someone, which only a running agent reports.
    pub working: bool,
}

/// A restart the runtime should run now.
#[derive(Clone, Debug)]
pub struct HealDue {
    pub key: SessionKey,
    /// 1 for the first restart in the window.
    pub attempt: usize,
    /// It was working when its daemon was last seen alive.
    pub was_working: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GiveUpReason {
    /// It died again after `MAX_RESTARTS_PER_WINDOW` restarts in the window.
    StoppedRepeatedly,
    /// The restarts themselves failed; the last error.
    RestartFailed(String),
    /// It never had a conversation to come back to.
    NothingToResume,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealReportKind {
    Restarted {
        attempt: usize,
        /// Why "carry on" could not be sent, when it could not.
        carry_on_error: Option<String>,
    },
    GaveUp {
        reason: GiveUpReason,
    },
}

/// A report waiting for a thread's coordinator; `id` lets the delivery acknowledge exactly it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealReport {
    pub id: u64,
    pub kind: HealReportKind,
}

#[derive(Default)]
struct Watched {
    /// Its daemon was seen alive by this gxserver.
    seen_alive: bool,
    /// Whether its agent was running when its daemon was last seen alive.
    agent_running: Option<bool>,
    agent_checked_at: Option<i64>,
    was_working: bool,
    /// Since when its daemon has been missing.
    gone_since: Option<i64>,
    /// Restart attempts (successful or not), pruned to the window.
    restarts: Vec<i64>,
    failures: usize,
    next_try_at: i64,
    in_flight: bool,
    gave_up_at: Option<i64>,
}

impl Watched {
    fn lost(&self) -> bool {
        self.in_flight
            || self.gave_up_at.is_some()
            || (self.gone_since.is_some() && self.seen_alive && self.agent_running == Some(true))
    }
}

#[derive(Default)]
struct HealBook {
    watched: HashMap<SessionKey, Watched>,
    reports: HashMap<SessionKey, HealReport>,
}

fn book() -> &'static Mutex<HealBook> {
    static BOOK: OnceLock<Mutex<HealBook>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(HealBook::default()))
}

fn next_report_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// The keys among `alive` whose agent check is due.
pub fn agent_check_due(alive: &[SessionKey], now: i64) -> Vec<SessionKey> {
    let Ok(book) = book().lock() else {
        return Vec::new();
    };
    alive
        .iter()
        .filter(|key| {
            book.watched
                .get(*key)
                .and_then(|watched| watched.agent_checked_at)
                .is_none_or(|at| now - at >= AGENT_CHECK_EVERY_MS)
        })
        .cloned()
        .collect()
}

/// What one tick decided: the restarts to run now and the sessions it gave up on.
#[derive(Default)]
pub struct TickOutcome {
    pub heals: Vec<HealDue>,
    pub gave_up: Vec<(SessionKey, GiveUpReason)>,
}

/// Records one tick's sightings of every running agent session and decides what to restart.
/// Sessions missing from `sightings` left `running` (slept, closed, removed) and are forgotten.
pub fn observe(sightings: Vec<Sighting>, now: i64) -> TickOutcome {
    let mut outcome = TickOutcome::default();
    let Ok(mut book) = book().lock() else {
        return outcome;
    };
    let seen: std::collections::HashSet<SessionKey> = sightings
        .iter()
        .map(|sighting| sighting.key.clone())
        .collect();
    book.watched
        .retain(|key, watched| watched.in_flight || seen.contains(key));
    for sighting in sightings {
        let watched = book.watched.entry(sighting.key.clone()).or_default();
        if sighting.daemon_alive {
            watched.seen_alive = true;
            watched.gone_since = None;
            watched.was_working = sighting.working;
            match sighting.agent_running {
                Some(running) => {
                    watched.agent_running = Some(running);
                    watched.agent_checked_at = Some(now);
                }
                None if sighting.working => watched.agent_running = Some(true),
                None => {}
            }
            // Open again (the user opened it, which wakes it): it no longer needs attention.
            watched.gave_up_at = None;
            if !watched.in_flight {
                watched.failures = 0;
            }
            continue;
        }
        if !watched.seen_alive || watched.in_flight || watched.gave_up_at.is_some() {
            continue;
        }
        let gone_since = *watched.gone_since.get_or_insert(now);
        if now - gone_since < GONE_CONFIRM_MS {
            continue;
        }
        if watched.agent_running != Some(true) {
            // Its shell ended with no agent in it (the user typed `exit`): not a crash. Forget it
            // until its daemon is seen alive again.
            watched.seen_alive = false;
            watched.gone_since = None;
            continue;
        }
        if now < watched.next_try_at {
            continue;
        }
        watched.restarts.retain(|at| now - at < RESTART_WINDOW_MS);
        if watched.restarts.len() >= MAX_RESTARTS_PER_WINDOW {
            watched.gave_up_at = Some(now);
            outcome
                .gave_up
                .push((sighting.key, GiveUpReason::StoppedRepeatedly));
            continue;
        }
        watched.in_flight = true;
        watched.restarts.push(now);
        outcome.heals.push(HealDue {
            key: sighting.key,
            attempt: watched.restarts.len(),
            was_working: watched.was_working,
        });
    }
    outcome
}

/// How a restart ended.
pub enum HealOutcome {
    Restarted,
    /// It was not needed after all (slept, closed or back meanwhile, or gxserver is stopping);
    /// the attempt does not count.
    Cancelled,
    Failed(String),
    /// It cannot work at all; gives up at once.
    Hopeless(GiveUpReason),
}

/// Records how a restart ended; returns the reason when Ghostex now gives up on the session.
pub fn finish_heal(key: &SessionKey, now: i64, outcome: HealOutcome) -> Option<GiveUpReason> {
    let Ok(mut book) = book().lock() else {
        return None;
    };
    let watched = book.watched.get_mut(key)?;
    watched.in_flight = false;
    match outcome {
        HealOutcome::Restarted => {
            watched.failures = 0;
            watched.gone_since = None;
            // The agent it just started counts as running until a check says otherwise, so a
            // daemon that dies again at once is healed (and counted) too.
            watched.agent_running = Some(true);
            watched.agent_checked_at = Some(now);
            None
        }
        HealOutcome::Cancelled => {
            watched.restarts.pop();
            watched.gone_since = None;
            None
        }
        HealOutcome::Failed(error) => {
            watched.failures += 1;
            watched.next_try_at = now
                + RETRY_AFTER_FAILURE_MS
                    [(watched.failures - 1).min(RETRY_AFTER_FAILURE_MS.len() - 1)];
            if watched.restarts.len() >= MAX_RESTARTS_PER_WINDOW {
                watched.gave_up_at = Some(now);
                return Some(GiveUpReason::RestartFailed(error));
            }
            None
        }
        HealOutcome::Hopeless(reason) => {
            watched.gave_up_at = Some(now);
            Some(reason)
        }
    }
}

/// True while a session's daemon is gone under a running agent, its restart runs, or Ghostex gave
/// up on it: it cannot show output, so no stall watch may count its quiet, and no report about it
/// goes to its coordinator but this module's own.
pub fn daemon_lost(project_id: &str, session_id: &str) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    book()
        .lock()
        .ok()
        .and_then(|book| book.watched.get(&key).map(Watched::lost))
        .unwrap_or(false)
}

/// Every session Ghostex gave up on, with when: the red dot's "needs attention".
pub fn gave_up_sessions() -> HashMap<SessionKey, i64> {
    book()
        .lock()
        .map(|book| {
            book.watched
                .iter()
                .filter_map(|(key, watched)| Some((key.clone(), watched.gave_up_at?)))
                .collect()
        })
        .unwrap_or_default()
}

/// What a panel row says about a session Ghostex gave up on.
pub fn attention_detail(project_id: &str, session_id: &str) -> Option<&'static str> {
    let key = (project_id.to_string(), session_id.to_string());
    book()
        .lock()
        .ok()?
        .watched
        .get(&key)?
        .gave_up_at
        .map(|_| "Stopped unexpectedly and could not be restarted")
}

/// Queues a report for a thread's coordinator; a newer one replaces one not yet delivered.
pub fn queue_report(key: &SessionKey, kind: HealReportKind) {
    if let Ok(mut book) = book().lock() {
        book.reports.insert(
            key.clone(),
            HealReport {
                id: next_report_id(),
                kind,
            },
        );
    }
}

pub fn pending_report(key: &SessionKey) -> Option<HealReport> {
    book().lock().ok()?.reports.get(key).cloned()
}

/// Drops a delivered report, unless a newer one replaced it meanwhile.
pub fn ack_report(key: &SessionKey, id: u64) {
    if let Ok(mut book) = book().lock() {
        if book.reports.get(key).is_some_and(|report| report.id == id) {
            book.reports.remove(key);
        }
    }
}

/// The coordinator report for a thread's heal.
pub fn report_text(report: &HealReport, thread_ref: &str) -> String {
    match &report.kind {
        HealReportKind::Restarted {
            attempt,
            carry_on_error,
        } => {
            let count = if *attempt > 1 {
                format!(
                    " (restart {attempt} of the {MAX_RESTARTS_PER_WINDOW} Ghostex allows in {} minutes)",
                    RESTART_WINDOW_MS / 60_000
                )
            } else {
                String::new()
            };
            let mut body = format!(
                "Ghostex thread report: restarted after its session stopped unexpectedly. Its terminal ended under it while its agent was running (not closed or slept by you, the user or Ghostex), so Ghostex started its agent again on the same conversation{count}."
            );
            match carry_on_error {
                None => body.push_str(&format!(
                    " Ghostex told it: \"{CARRY_ON_MESSAGE}\" Nothing to do: you hear from it when it finishes, as usual."
                )),
                Some(error) => body.push_str(&format!(
                    " Ghostex could not tell it to carry on ({}), so tell it yourself: ghostex agents send {thread_ref} \"carry on\".",
                    error.trim()
                )),
            }
            body
        }
        HealReportKind::GaveUp { reason } => {
            let why = match reason {
                GiveUpReason::StoppedRepeatedly => format!(
                    "it stopped again after {MAX_RESTARTS_PER_WINDOW} restarts in {} minutes",
                    RESTART_WINDOW_MS / 60_000
                ),
                GiveUpReason::RestartFailed(error) => {
                    format!("restarting it failed ({})", error.trim())
                }
                GiveUpReason::NothingToResume => {
                    "it had no conversation to come back to yet".to_string()
                }
            };
            format!(
                "Ghostex thread report: its session stopped unexpectedly and Ghostex could not keep it running: {why}. The user now sees it as needing attention (a red dot) and was notified. Try once yourself: ghostex agents reload {thread_ref}, then ghostex agents send {thread_ref} \"carry on\". If that fails too, tell the user."
            )
        }
    }
}
