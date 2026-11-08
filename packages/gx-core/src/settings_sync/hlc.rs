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

/// The stamp of a value this computer has without knowing when it was set: what it holds when it
/// starts syncing a category. It sorts before every real edit, so turning sync on never overrides
/// another computer's value; it only fills in what nobody synced yet.
pub fn seed_hlc(node: &str) -> String {
    format!("{:013}-{:06}-{node}", 0, 0)
}

/// Stamps below this wall time (2001) are seeds, never real edits.
const SEED_CEILING_MS: u64 = 1_000_000_000_000;

/// A seed (see [`seed_hlc`]) ranked by when this computer started syncing (`since_ms`): it still
/// sorts before every real edit, and between two computers' seeds the one that started first
/// wins, the same on every computer.
///
/// CDXC:SettingsSync 2026-11-08 WHY:
/// #488's follow-up: a computer's first Spaces sync wrote its whole layout (each project "in no Space" too) as real edits, so a newcomer whose first sync came after another computer's arrangement replaced it instead of adopting it. Its first export is now seeds, and plain seeds (`seed_hlc`) would tie between two computers and be decided by server id; ranking by start time lets the shared layout stand and the newcomer fill in only what nobody filed.
pub fn seed_hlc_since(node: &str, since_ms: u64) -> String {
    let wall = SEED_CEILING_MS - 1 - (since_ms / 1000).min(SEED_CEILING_MS - 1);
    format!("{wall:013}-{:06}-{node}", 0)
}

/// Whether a stamp is a seed rather than an edit someone made.
pub fn is_seed_hlc(stamp: &str) -> bool {
    hlc_wall_ms(stamp) < SEED_CEILING_MS
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
