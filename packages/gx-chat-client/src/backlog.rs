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
//!   frames from one machine cannot keep a large one from another waiting for ever.
//! - The host only ever releases (`handled`) and never waits here, and it never waits on a socket
//!   thread, so a socket thread blocked here cannot block the host, even while the host sends that
//!   thread a command (`ChatStreams` commands are an unbounded channel the thread reads once it is
//!   let through).
//! - `close` opens the gate for good when the host thread ends, so no socket thread waits on a
//!   reader that is gone; what it then hands over is refused by the closed channel, as before.

use std::collections::VecDeque;
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
    /// The next ticket to give a waiting thread, and the one allowed in next.
    next_ticket: u64,
    serving: u64,
    /// The host is gone: nobody waits any more.
    closed: bool,
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

    /// Waits until a frame of `bytes` fits, then hands it over with `deliver`.
    ///
    /// `deliver` runs under the gate's lock, which is what keeps `sizes` in the channel's order, so
    /// it must only queue the frame and return.
    pub(crate) fn admit(&self, bytes: usize, deliver: impl FnOnce()) {
        let mut state = self.lock();
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        while !state.closed
            && (state.serving != ticket
                || (state.queued > 0 && state.queued.saturating_add(bytes) > self.budget))
        {
            state = self
                .room
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.serving = state.serving.max(ticket + 1);
        if !state.closed {
            state.queued += bytes;
            state.sizes.push_back(bytes);
        }
        deliver();
        drop(state);
        // The next ticket holder may fit now.
        self.room.notify_all();
    }

    /// Hands `item` over with `deliver` if a frame of `bytes` fits now and nobody is waiting ahead
    /// of it; otherwise gives `item` back untouched, without waiting.
    pub(crate) fn try_admit<T>(
        &self,
        bytes: usize,
        item: T,
        deliver: impl FnOnce(T),
    ) -> Result<(), T> {
        let mut state = self.lock();
        let fits = state.closed
            || (state.serving == state.next_ticket
                && (state.queued == 0 || state.queued.saturating_add(bytes) <= self.budget));
        if !fits {
            return Err(item);
        }
        if !state.closed {
            state.queued += bytes;
            state.sizes.push_back(bytes);
        }
        deliver(item);
        Ok(())
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
