//! SQLite's busy wait, taken in turn: writers that find the database locked wait in a first-come
//! queue, and only the one at its head spends the busy budget.
//!
//! CDXC:ServerApi 2026-11-18 WHY:
//! GitHub #666: the domain routes ran SQLite on the async workers, so writers waiting out a locked
//! database (up to 5 s each) filled every worker and stalled the whole runtime. Moving the routes
//! off the workers (server/http_dispatch.rs `blocking_section`) let a whole burst start SQLite's
//! busy wait at once, and that wait is first-come only by luck: every waiter's 5 s ran together,
//! so writes that used to queue on the workers and then succeed failed (review of 76a9d9f13). A
//! permit around whole handlers fixed that but held it across loopback calls, subprocesses and
//! file scans, queued reads behind writes and froze every route (review of 799beeae8, reverted).
//!
//! So the turn is taken only where SQLite itself reports the database busy: a write that finds it
//! locked takes a ticket, waits until it is the oldest waiting ticket (at most `QUEUE_WAIT`), and
//! only then spends the busy budget (`HEAD_BUDGET`, the 5 s `DEFAULT_BUSY_TIMEOUT` it had). WAL
//! reads are never told the database is busy, so they never queue, and nothing is held outside
//! SQLite's own busy wait. A ticket ends when its write commits or rolls back (the connection's
//! hooks), when the connection goes back to the pool, or when its holder has not retried for
//! `HOLDER_GONE` (it got the lock, or gave up, without either hook). Only threads that left their
//! async worker for this (`set_off_worker`) wait in the queue; anywhere else the wait is SQLite's
//! own, bounded by the same budget, as before.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// The busy budget of the waiter at the head of the queue (`storage/pool.rs` `DEFAULT_BUSY_TIMEOUT`).
const HEAD_BUDGET: Duration = super::pool::DEFAULT_BUSY_TIMEOUT;
/// The longest a write waits for its turn before it gives up as busy.
const QUEUE_WAIT: Duration = Duration::from_secs(30);
/// A holder that has not retried for this long is taken to be done.
const HOLDER_GONE: Duration = Duration::from_millis(500);
/// The longest pause between two tries of the head (SQLite's own busy wait tops out at 100 ms).
const MAX_RETRY_PAUSE: Duration = Duration::from_millis(25);

struct Waiter {
    /// When its holder last tried or waited.
    seen: Instant,
}

#[derive(Default)]
struct Queue {
    next: u64,
    waiting: BTreeMap<u64, Waiter>,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    next: 0,
    waiting: BTreeMap::new(),
});
static TURN: Condvar = Condvar::new();

thread_local! {
    /// This thread's ticket, when it waits or holds the head.
    static TICKET: Cell<Option<u64>> = const { Cell::new(None) };
    /// When this thread's current busy wait began, and when it reached the head.
    static SINCE: Cell<Option<(Instant, Option<Instant>)>> = const { Cell::new(None) };
    static OFF_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Marks the running thread as one that left its async worker (or not), so its busy waits queue.
/// Returns what it was, to put back.
pub(crate) fn set_off_worker(off: bool) -> bool {
    OFF_WORKER.replace(off)
}

fn lock() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Ends this thread's ticket, if it holds one: its write committed or rolled back, or its
/// connection went back to the pool.
pub(crate) fn release_current_thread() {
    SINCE.set(None);
    if let Some(ticket) = TICKET.take() {
        lock().waiting.remove(&ticket);
        TURN.notify_all();
    }
}

/// The oldest ticket whose holder is still around, dropping the ones whose holder left.
fn head(queue: &mut Queue, now: Instant) -> Option<u64> {
    queue
        .waiting
        .retain(|_, waiter| now.duration_since(waiter.seen) < HOLDER_GONE);
    queue.waiting.keys().next().copied()
}

/// The connection busy handler (`Connection::busy_handler`): `true` to try again.
pub(crate) fn busy(count: i32) -> bool {
    let now = Instant::now();
    if count == 0 || SINCE.get().is_none() {
        SINCE.set(Some((now, None)));
    }
    let Some((since, head_since)) = SINCE.get() else {
        return false;
    };
    if !OFF_WORKER.get() {
        // SQLite's own wait, as before: try again with a growing pause until the budget is spent.
        if now.duration_since(since) >= HEAD_BUDGET {
            SINCE.set(None);
            return false;
        }
        std::thread::sleep(retry_pause(count));
        return true;
    }
    let ticket = match TICKET.get() {
        Some(ticket) => ticket,
        None => {
            let mut queue = lock();
            let ticket = queue.next;
            queue.next += 1;
            queue.waiting.insert(ticket, Waiter { seen: now });
            TICKET.set(Some(ticket));
            ticket
        }
    };
    let mut queue = lock();
    loop {
        let now = Instant::now();
        match queue.waiting.get_mut(&ticket) {
            Some(waiter) => waiter.seen = now,
            // Dropped as gone (a long pause between tries): back in line where it was.
            None => {
                queue.waiting.insert(ticket, Waiter { seen: now });
            }
        }
        if head(&mut queue, now) == Some(ticket) {
            break;
        }
        if now.duration_since(since) >= QUEUE_WAIT {
            queue.waiting.remove(&ticket);
            drop(queue);
            TICKET.set(None);
            SINCE.set(None);
            TURN.notify_all();
            return false;
        }
        // Wakes for a release, or to stay seen and to notice a holder that left.
        queue = TURN
            .wait_timeout(queue, HOLDER_GONE / 4)
            .map(|(queue, _)| queue)
            .unwrap_or_else(|poisoned| poisoned.into_inner().0);
    }
    drop(queue);
    let head_since = head_since.unwrap_or_else(Instant::now);
    SINCE.set(Some((since, Some(head_since))));
    if head_since.elapsed() >= HEAD_BUDGET {
        release_current_thread();
        return false;
    }
    std::thread::sleep(retry_pause(count));
    true
}

fn retry_pause(count: i32) -> Duration {
    let millis = match count {
        0 => 1,
        1 => 2,
        2 => 5,
        3 => 10,
        _ => 25,
    };
    Duration::from_millis(millis).min(MAX_RETRY_PAUSE)
}
