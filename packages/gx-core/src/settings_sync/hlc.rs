//! Hybrid logical clocks as sortable text: `<wall ms, 13 digits>-<counter, 6 digits>-<server id>`.
//! Comparing two stamps as plain strings orders them by time, then counter, then computer, so the
//! phone's JavaScript and gxserver agree without parsing.

/// The next stamp on this computer after `clock` (the newest stamp it has written or received).
///
/// CDXC:SettingsSync 2026-11-06 WHY:
/// A computer whose clock runs behind still writes after what it has seen: the wall part never goes back past the newest stamp received, and the counter breaks ties within one millisecond. Without that, an edit made on a slow clock right after a sync would lose to the value it replaced.
pub fn next_hlc(clock: Option<&str>, now_ms: u64, node: &str) -> String {
    let (last_wall, last_counter) = clock.map(parse).unwrap_or((0, 0));
    let (wall, counter) = if now_ms > last_wall {
        (now_ms, 0)
    } else {
        (last_wall, last_counter + 1)
    };
    format!("{wall:013}-{counter:06}-{node}")
}

/// The wall time of a stamp in milliseconds (0 for an unreadable one).
pub fn hlc_wall_ms(stamp: &str) -> u64 {
    parse(stamp).0
}

fn parse(stamp: &str) -> (u64, u64) {
    let mut parts = stamp.splitn(3, '-');
    let wall = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let counter = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    (wall, counter)
}
