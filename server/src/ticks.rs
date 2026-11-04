//! The clock every repeating gxserver task waits on.
//!
//! CDXC:ServerDaemon 2026-11-05 WHY:
//! `tokio::time::interval` catches up on ticks it missed by firing them back to back (`MissedTickBehavior::Burst`). A pass that ran long because the machine was busy (a coordinator tick, the recovery pass, the chat queue) was then followed at once by the ticks it overran, so gxserver did more work exactly when the machine had the least to spare: the pattern behind gxserver climbing to 3 to 5 cores on a loaded Windows machine (docs/note.md, issues-1027/macos-perf). Every repeating task waits a full period after a late tick instead.

use std::time::Duration;

/// `tokio::time::interval(period)` that, after a late tick, waits a whole `period` again rather than firing the ticks it missed.
pub(crate) fn steady_interval(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_late_tick_is_not_followed_by_the_ticks_it_missed() {
        let mut clock = steady_interval(Duration::from_millis(50));
        clock.tick().await;
        // A pass that overran six periods.
        std::thread::sleep(Duration::from_millis(300));
        clock.tick().await;
        let started = std::time::Instant::now();
        clock.tick().await;
        // `Burst` would answer at once, five times over.
        assert!(started.elapsed() >= Duration::from_millis(40));
    }
}
