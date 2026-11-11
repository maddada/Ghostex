//! How long a dropped remote computer waits for its next automatic try.
//!
//! CDXC:RemoteMachines 2026-11-10 WHY:
//! The coordinator's brief for dependable device connections: "a gone computer is retried every 15 s for the first minutes". The ladder was 2, 5, 15, 30 s and then 60 s after each failed try, and a try at a computer that does not answer takes 6 to 12 s itself (SSH's 8 s connect limit), so from the fifth try on a computer was only looked for every 66 to 72 s. On 10 Nov a PC that was offline for six minutes was connected again 61 s after it came back, on the first try that found it. A restart, an update or a router that drops out lasts a few minutes, so that is exactly when the long gap costs the most. Now a try starts every 15 s while the computer has been gone for less than ten minutes, and every 60 s after that; both are counted from the start of the try before, so a slow try no longer adds to the gap. A try costs one `ssh` that gives up after 8 s and nothing else: Easy Connect's helper stays up between tries (`easy_connect_forward.rs`) and an automatic try shows no toast.
//! A fault on this computer (the keychain did not open, no token yet) keeps the old ladder: nothing suggests the next seconds fix it, and a keychain read every 15 s could mean a system prompt every 15 s.
//!
//! No app imports: the lab's ladder harness compiles this file as it is.
//! SEE-ALSO: apps/desktop/src/app/remote_conn/reconnect_ladder.rs (the bookkeeping and the timer).

use std::time::Duration;

/// The waits before the first two tries after a drop: a link that only blinked is back at once.
const FIRST_WAITS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];
/// How far apart tries start while the computer has not been gone for long.
const QUICK_TRY_EVERY: Duration = Duration::from_secs(15);
/// How long a computer counts as not gone for long.
const QUICK_TRIES_FOR: Duration = Duration::from_secs(10 * 60);
/// How far apart tries start after that.
const SLOW_TRY_EVERY: Duration = Duration::from_secs(60);
/// The shortest pause between two tries, however long the one before took.
const SHORTEST_WAIT: Duration = Duration::from_secs(2);
/// The ladder a fault on this computer is retried on; the last step repeats.
const LOCAL_FAULT_WAITS: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// Why the last try failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetryCause {
    /// The computer, or the way to it, did not answer.
    ComputerGone,
    /// Something this computer needs for the try was missing (the keychain, a token).
    LocalFault,
}

/// The wait before the next automatic try. `tries` counts the automatic tries since the computer
/// was lost, `gone_for` is how long ago that was, and `last_try_took` is how long the try that
/// just failed ran (zero when the link dropped without a try).
pub(crate) fn wait_before_next_try(
    cause: RetryCause,
    tries: usize,
    gone_for: Duration,
    last_try_took: Duration,
) -> Duration {
    if cause == RetryCause::LocalFault {
        return LOCAL_FAULT_WAITS[tries.min(LOCAL_FAULT_WAITS.len() - 1)];
    }
    if let Some(wait) = FIRST_WAITS.get(tries) {
        return *wait;
    }
    let every = if gone_for < QUICK_TRIES_FOR {
        QUICK_TRY_EVERY
    } else {
        SLOW_TRY_EVERY
    };
    every.saturating_sub(last_try_took).max(SHORTEST_WAIT)
}
