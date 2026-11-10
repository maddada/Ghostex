//! How much the socket threads may have read that the host has not handled yet.
//!
//! CDXC:SessionChat 2026-11-10 WHY:
//! The socket threads handed every frame to the host over an unbounded channel, each one already
//! parsed into a `serde_json::Value` (a coordinator chat's state frame is about 490 KB of wire text
//! and 3.4 MB parsed). While the one host thread was saturated the queue grew without limit, one
//! suspect for the app's 2.8 GB memory peaks. A frame is now handed over only when what is queued
//! plus the frame fits the budget; until then the socket thread holds it, and what it reads after
//! it, as raw text (`native.rs`, `READ_AHEAD_BYTES`), and only when that is full too does it wait
//! here and leave the rest to the kernel's socket buffer and gxserver. Nothing is dropped and
//! nothing is reordered: the frame still travels on the host's one channel, behind and ahead of the
//! same commands as before, and the views' own commands never pass through this gate.
//!
//! The rules that keep it from stalling:
//!
//! - One frame is always let in while nothing is queued, so a frame larger than the whole budget
//!   (a long conversation's snapshot) still arrives.
//! - Admission is first come, first served across every machine's thread, so a stream of small
//!   frames from one machine cannot keep a large one from another waiting for ever. A thread takes
//!   its place in line ([`Place`]) the first time its frame is refused, without waiting, and keeps
//!   it while it reads ahead and offers the frame again; every later arrival, waiting or not, gives
//!   way to it. A place whose thread drops it (its socket ended, or the thread died) is skipped, so
//!   the line never stops on a thread that left it.
//! - The host only ever releases (`handled`) and never waits here, and it never waits on a socket
//!   thread, so a socket thread blocked here cannot block the host, even while the host sends that
//!   thread a command (`ChatStreams` commands are an unbounded channel the thread reads once it is
//!   let through).
//! - `close` opens the gate for good when the host thread ends, so no socket thread waits on a
//!   reader that is gone; what it then hands over is refused by the closed channel, as before.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// The wait between the socket threads and the host thread, in bytes of wire text.
pub struct InboundBacklog {
    budget: usize,
    state: Mutex<State>,
    room: Condvar,
}

#[derive(Default)]
struct State {
    /// Wire bytes handed over and not yet handled.
    queued: usize,
    /// The size of each frame handed over, oldest first. The host handles them in the order they
    /// were handed over, because they share one FIFO channel and are handed over under this lock.
    sizes: VecDeque<usize>,
    /// The next ticket to give a thread whose frame was refused, and the one allowed in next.
    next_ticket: u64,
    serving: u64,
    /// Tickets given up before their turn came, skipped when it does.
    abandoned: BTreeSet<u64>,
    /// The host is gone: nobody waits any more.
    closed: bool,
}

impl State {
    fn fits(&self, budget: usize, bytes: usize) -> bool {
        self.queued == 0 || self.queued.saturating_add(bytes) <= budget
    }

    /// Hands the frame over and passes the turn on.
    fn take(&mut self, bytes: usize, ticket: Option<u64>) {
        if self.closed {
            return;
        }
        self.queued += bytes;
        self.sizes.push_back(bytes);
        if let Some(ticket) = ticket {
            self.serving = ticket + 1;
            self.skip_abandoned();
        }
    }

    fn skip_abandoned(&mut self) {
        while self.abandoned.remove(&self.serving) {
            self.serving += 1;
        }
    }
}

impl InboundBacklog {
    /// A gate that holds at most `budget` bytes of wire text queued, plus the one frame that is
    /// always let in while nothing is queued.
    pub fn new(budget: usize) -> Arc<Self> {
        Arc::new(Self {
            budget,
            state: Mutex::new(State::default()),
            room: Condvar::new(),
        })
    }

    /// The host has handled the oldest frame it was handed. Called on the host thread once per
    /// inbound command it took off its channel.
    pub fn handled(&self) {
        let mut state = self.lock();
        if let Some(bytes) = state.sizes.pop_front() {
            state.queued -= bytes;
        }
        drop(state);
        self.room.notify_all();
    }

    /// The host thread has ended: let every waiting and future frame through at once.
    pub fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        state.queued = 0;
        state.sizes.clear();
        drop(state);
        self.room.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One socket thread's place in line: none while its frames go straight in, a ticket from the
/// first refusal until that frame is handed over. A thread offers its frames oldest first, so one
/// place is all it needs.
///
/// Both ways in run `deliver` under the gate's lock, which is what keeps the gate's sizes in the
/// channel's order, so `deliver` must only queue the frame and return.
pub(crate) struct Place {
    backlog: Arc<InboundBacklog>,
    ticket: Option<u64>,
}

impl Place {
    pub(crate) fn new(backlog: Arc<InboundBacklog>) -> Self {
        Self {
            backlog,
            ticket: None,
        }
    }

    /// Hands `item` over with `deliver` if it is this thread's turn and a frame of `bytes` fits
    /// now. Otherwise gives `item` back untouched, without waiting, and keeps (or takes) this
    /// thread's place, so frames that arrive later wait behind it.
    pub(crate) fn try_admit<T>(
        &mut self,
        bytes: usize,
        item: T,
        deliver: impl FnOnce(T),
    ) -> Result<(), T> {
        let mut state = self.backlog.lock();
        if !state.closed {
            // With no place yet, it is this thread's turn only if nobody is in line.
            let turn = match self.ticket {
                Some(ticket) => state.serving == ticket,
                None => state.serving == state.next_ticket,
            };
            if !turn || !state.fits(self.backlog.budget, bytes) {
                if self.ticket.is_none() {
                    self.ticket = Some(state.next_ticket);
                    state.next_ticket += 1;
                }
                return Err(item);
            }
        }
        state.take(bytes, self.ticket.take());
        deliver(item);
        drop(state);
        // The next in line may fit now.
        self.backlog.room.notify_all();
        Ok(())
    }

    /// Waits for this thread's turn and room for a frame of `bytes`, then hands it over with
    /// `deliver`. Keeps the place this thread already holds, or takes the last one.
    pub(crate) fn admit(&mut self, bytes: usize, deliver: impl FnOnce()) {
        let backlog = &self.backlog;
        let mut state = backlog.lock();
        let ticket = *self.ticket.get_or_insert_with(|| {
            let ticket = state.next_ticket;
            state.next_ticket += 1;
            ticket
        });
        while !state.closed && (state.serving != ticket || !state.fits(backlog.budget, bytes)) {
            state = backlog
                .room
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.take(bytes, self.ticket.take());
        deliver();
        drop(state);
        backlog.room.notify_all();
    }
}

impl Drop for Place {
    /// A thread that leaves the line (its socket ended with the frame still held, or it died)
    /// gives its turn up, so the threads behind it are not left waiting on it.
    fn drop(&mut self) {
        let Some(ticket) = self.ticket.take() else {
            return;
        };
        let mut state = self.backlog.lock();
        if state.serving == ticket {
            state.serving += 1;
            state.skip_abandoned();
        } else if ticket > state.serving {
            state.abandoned.insert(ticket);
        }
        drop(state);
        self.backlog.room.notify_all();
    }
}
