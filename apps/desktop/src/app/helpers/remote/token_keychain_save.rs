//! Saving a remote computer's daemon token to secure storage without holding its connect.
//!
//! CDXC:RemoteMachines 2026-11-10 WHY:
//! The coordinator's brief for dependable device connections: "a keychain write must never hold a connect ... the link comes up with the token it already has in memory, and the save is retried later ... do not weaken where the token is stored and do not skip the keychain". On 10 Nov a rebuilt app's first connect to a computer sat in `SecItemUpdate` for as long as nobody answered the macOS prompt that asks whether the new build may change the saved item: the computer stayed on "connecting", nothing was logged and nothing retried, because the attempt never ended. The connect reads the token over SSH each time and opens the tunnel with that copy, and nothing reads the saved one back during a connect, so the write only has to happen, not to happen first. It now runs on one background thread: a save that waits on a prompt holds that thread alone, a newer token for the same computer replaces the one still waiting, and a save that failed is tried again later. The token goes to the same place as before (the system keychain, Secret Service or Credential Manager) and nowhere else; until it is written it lives only in memory, as it already did.
//!
//! No app imports: the lab's harness compiles this file as it is, with a save that blocks.
//! SEE-ALSO: apps/desktop/src/app/helpers/remote/connect.rs (the connect), tunnel_and_auth.rs (the wiring and the log lines).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// After this long without an answer from secure storage the wait is noted once: a system
/// prompt is most likely waiting for the person.
const WAITING_NOTE_AFTER: Duration = Duration::from_secs(10);
/// When a save that failed is tried again; after the last one the next connect saves anyway.
const RETRY_AFTER: [Duration; 3] = [
    Duration::from_secs(60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(15 * 60),
];

/// What secure storage said to one save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenSaveOutcome {
    Saved,
    Failed,
}

/// What the saver reports, for the log.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TokenSaveNote<'a> {
    /// Secure storage has not answered yet.
    Waiting {
        machine_id: &'a str,
        waited: Duration,
    },
    Saved {
        machine_id: &'a str,
        took: Duration,
        tries: u32,
    },
    /// `retry_in` is `None` when this save is not tried again (the next connect saves anew, or a
    /// newer token for the computer is already waiting).
    Failed {
        machine_id: &'a str,
        tries: u32,
        retry_in: Option<Duration>,
    },
}

type SaveFn = Box<dyn Fn(&str, &str) -> TokenSaveOutcome + Send + Sync>;
type NoteFn = Box<dyn Fn(TokenSaveNote<'_>) + Send + Sync>;

struct Pending {
    token: String,
    /// The saves of this token that failed so far.
    failed: u32,
    not_before: Instant,
}

#[derive(Default)]
struct State {
    /// The newest token of each computer that is not saved yet.
    pending: HashMap<String, Pending>,
    worker_started: bool,
}

/// Saves tokens one at a time on a thread of its own.
pub(crate) struct TokenSaver {
    state: Mutex<State>,
    wake: Condvar,
    save: SaveFn,
    note: NoteFn,
}

impl TokenSaver {
    pub(crate) fn new(save: SaveFn, note: NoteFn) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            save,
            note,
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Queues `token` as `machine_id`'s token to save and returns at once.
    pub(crate) fn save_soon(self: &Arc<Self>, machine_id: &str, token: &str) {
        let start_worker = {
            let mut state = self.state();
            state.pending.insert(
                machine_id.to_string(),
                Pending {
                    token: token.to_string(),
                    failed: 0,
                    not_before: Instant::now(),
                },
            );
            !std::mem::replace(&mut state.worker_started, true)
        };
        if start_worker {
            let saver = self.clone();
            std::thread::spawn(move || saver.run());
        }
        self.wake.notify_all();
    }

    /// Takes the save that is due, waiting for one when none is.
    fn next_due(&self) -> (String, Pending) {
        let mut state = self.state();
        loop {
            let now = Instant::now();
            let due = state
                .pending
                .iter()
                .filter(|(_, pending)| pending.not_before <= now)
                .min_by_key(|(_, pending)| pending.not_before)
                .map(|(machine_id, _)| machine_id.clone());
            if let Some(machine_id) = due {
                if let Some(pending) = state.pending.remove(&machine_id) {
                    return (machine_id, pending);
                }
            }
            let next = state
                .pending
                .values()
                .map(|pending| pending.not_before)
                .min();
            state = match next {
                Some(at) => {
                    self.wake
                        .wait_timeout(state, at.saturating_duration_since(now))
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .0
                }
                None => self
                    .wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            };
        }
    }

    fn run(self: Arc<Self>) {
        loop {
            let (machine_id, pending) = self.next_due();
            let started = Instant::now();
            let answered = Arc::new(AtomicBool::new(false));
            {
                let saver = self.clone();
                let answered = answered.clone();
                let machine_id = machine_id.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(WAITING_NOTE_AFTER);
                    if !answered.load(Ordering::SeqCst) {
                        (saver.note)(TokenSaveNote::Waiting {
                            machine_id: &machine_id,
                            waited: started.elapsed(),
                        });
                    }
                });
            }
            let outcome = (self.save)(&machine_id, &pending.token);
            answered.store(true, Ordering::SeqCst);
            let tries = pending.failed + 1;
            match outcome {
                TokenSaveOutcome::Saved => (self.note)(TokenSaveNote::Saved {
                    machine_id: &machine_id,
                    took: started.elapsed(),
                    tries,
                }),
                TokenSaveOutcome::Failed => {
                    let retry_in = {
                        let mut state = self.state();
                        // A newer token for this computer is waiting already: that one is saved.
                        if state.pending.contains_key(&machine_id) {
                            None
                        } else {
                            let retry_in = RETRY_AFTER.get(pending.failed as usize).copied();
                            if let Some(wait) = retry_in {
                                state.pending.insert(
                                    machine_id.clone(),
                                    Pending {
                                        token: pending.token,
                                        failed: tries,
                                        not_before: Instant::now() + wait,
                                    },
                                );
                            }
                            retry_in
                        }
                    };
                    (self.note)(TokenSaveNote::Failed {
                        machine_id: &machine_id,
                        tries,
                        retry_in,
                    });
                }
            }
        }
    }
}
