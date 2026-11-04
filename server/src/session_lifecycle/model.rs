use super::*;

pub(super) const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

/// Inactivity window used when the shared settings file says nothing. Matches
/// the Sidebar V2 spec default and `sidebarAutoSettleAfterDays`'s default.
pub const DEFAULT_AUTO_SETTLE_AFTER_DAYS: f64 = 3.0;

/// The `sidebarVersion` value that opts a machine into the Sidebar V2 inbox.
/// Anything else (including a missing key) means the user is on V1.
pub const SIDEBAR_V2_VERSION: &str = "v2";

/// Sweep cadence. Auto-settle boundaries are days away and snooze wakes are
/// resolved client-side from `snoozedUntil`, so a minute of slack costs nothing
/// and keeps the daemon idle.
pub const SESSION_LIFECYCLE_SWEEP_INTERVAL_SECONDS: u64 = 60;

/*
CDXC:Sessions 2026-09-12 WHY:
Snooze used to keep `snoozedUntil` for a day after the wake time so a Sidebar V2 "Woke" indicator could read it; that
inbox never shipped its UI. The sidebar's Snoozed section now needs a presentation delta when the wake time passes,
because nothing on the client re-partitions sections on a timer: the sweep clearing the spent snooze at the boundary
is that delta. Clients still classify by the clock, so the row leaves the section at the wake time to the millisecond
and the sweep's minute of slack only affects the stored fields.
*/
pub const SNOOZE_WAKE_RETENTION_MS: i64 = 0;

/// Upper bound on lifecycle writes per sweep pass. The first pass after an
/// upgrade can find a large backlog of stale sessions; spreading it over passes
/// keeps the presentation delta stream sane. The rest is picked up next pass.
pub const SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS: usize = 100;

#[derive(Debug)]
pub struct SessionLifecycleOutcome {
    /// False when the command was a no-op (double click, raced clients). The
    /// caller then skips the presentation delta instead of churning revisions.
    pub changed: bool,
    pub session: Value,
}

#[derive(Clone, Debug)]
pub struct SessionLifecycleSweepOptions {
    /// `None` disables inactivity auto-settle entirely.
    pub auto_settle_after_days: Option<f64>,
    /*
    CDXC:Git 2026-07-29-00:00:
    The second auto-settle trigger: a session whose branch's pull request is
    merged or closed is finished work, so it settles IMMEDIATELY instead of
    waiting out the inactivity window. See
    `auto_settle_on_finished_pull_request` for why this rides the same switch as
    the window instead of adding a second setting.
    */
    pub auto_settle_on_finished_pull_request: bool,
    pub max_mutations: usize,
    pub now_iso: String,
}

/*
CDXC:Git 2026-07-29-00:00:
PR-driven auto-settle deliberately has no setting of its own. It is enabled
exactly when the inactivity window is: Sidebar V2 is selected AND the user has
not switched auto-settle off with `sidebarAutoSettleAfterDays: null`. A user who
turned auto-settle off expects rows to stay in their inbox no matter what the
forge says, and a V1 user has no settled shelf to find them on. If the two
triggers ever need to diverge, this is the one place to split them.
*/
pub fn auto_settle_on_finished_pull_request(auto_settle_after_days: Option<f64>) -> bool {
    auto_settle_after_days.is_some()
}

/// Resolver for callers that have no git/PR knowledge (tests, and any future
/// sweep caller running without the git-status cache).
pub fn ignore_pull_requests(_session: &Value) -> PullRequestDisposition {
    PullRequestDisposition::Unknown
}

#[derive(Debug, Default)]
pub struct SessionLifecycleSweepOutcome {
    /// `(projectId, sessionId)` pairs whose lifecycle state changed; the caller
    /// emits one presentation delta per pair.
    pub changed: Vec<(String, String)>,
}

/*
The configuration channel. gxserver already reads the shared sidebar settings
file for `debuggingMode` (see `logging/logger.rs`), and both `sidebarVersion` and
`sidebarAutoSettleAfterDays` ride the same settings pipeline the Sidebar V2
toggle uses, so there is no new transport, no new endpoint, and no duplicated
source of truth. One read serves both keys.

Automatic settling is a Sidebar V2 concept: a V1 user has no settled shelf, so
a session the sweep parks would simply vanish from their sidebar with no way to
see or undo it. The window is therefore gated on `sidebarVersion == "v2"`;
missing file, missing key, or any other value means V1 and disables the pass.
The settle/snooze RPCs stay open regardless of version — remote machines and
mobile clients drive those explicitly, and only the AUTOMATIC pass is gated.

Within V2: missing key / unparseable value -> the default window. Explicit
`null` or a non-positive number -> auto-settle disabled, matching the client
predicate's `autoSettleAfterDays: null`.
*/
pub fn read_sweep_auto_settle_after_days(paths: &GxserverPaths) -> Option<f64> {
    resolve_sweep_auto_settle_after_days(read_sidebar_settings(paths).as_ref())
}

/*
CDXC:StateSync 2026-07-29:
The same settings read, answering the other question every Sidebar V2 server-side
data pass has to ask first: is this machine on V2 at all?

gxserver's git-status and `origin`-remote passes exist ONLY to fill Sidebar V2
surfaces (cards' branch/±/PR badge and cross-machine grouping). Ungated they cost
a V1 user permanent ambient load: git spawns per live session cwd and up to a
dozen `gh` NETWORK calls every minute, forever, for data nothing renders. So
they run exactly when auto-settle's window does: `sidebarVersion == "v2"`,
missing file / missing key / any other value means V1.

Callers must resolve this ONCE PER PASS rather than at task spawn, so flipping
the setting takes effect within one pass (~60s) without restarting the daemon.
*/
pub fn read_sidebar_v2_selected(paths: &GxserverPaths) -> bool {
    is_sidebar_v2_selected(
        read_sidebar_settings(paths)
            .as_ref()
            .and_then(|settings| settings.get("sidebarVersion")),
    )
}

/// The single read of the shared sidebar settings file. `None` when it is
/// missing or unparseable, which every caller treats as "V1, nothing
/// configured" — never as a reason to fall back to a different rule.
pub(crate) fn read_sidebar_settings(paths: &GxserverPaths) -> Option<Value> {
    crate::shared_settings_file::read_shared_settings(paths)
}

/// Pure twin of `read_sweep_auto_settle_after_days`, split out so the version
/// gate is testable without a settings file.
pub fn resolve_sweep_auto_settle_after_days(settings: Option<&Value>) -> Option<f64> {
    if !is_sidebar_v2_selected(settings.and_then(|settings| settings.get("sidebarVersion"))) {
        return None;
    }
    normalize_auto_settle_after_days(
        settings.and_then(|settings| settings.get("sidebarAutoSettleAfterDays")),
    )
}

pub fn is_sidebar_v2_selected(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .map(|version| version.trim().eq_ignore_ascii_case(SIDEBAR_V2_VERSION))
        .unwrap_or(false)
}

pub fn normalize_auto_settle_after_days(value: Option<&Value>) -> Option<f64> {
    match value {
        None => Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS),
        Some(Value::Null) => None,
        Some(value) => match value.as_f64() {
            Some(days) if days.is_finite() && days > 0.0 => Some(days),
            Some(_) => None,
            None => Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS),
        },
    }
}

/// gxserver's three-state activity for one session, resolved the same way
/// presentation resolves it so guards and published rows can never disagree.
pub fn session_activity(session: &Value, now_iso: &str) -> String {
    presentation_activity(session, now_iso)
}

/// The meaningful-activity clock a session's settle window counts from — the
/// server twin of `sessionLastActivityAtMs` in `packages/shared/sidebar-v2-lifecycle.ts`.
pub fn session_last_activity_ms(session: &Value, now_iso: &str) -> Option<i64> {
    let meaningful = parse_iso_ms(&session_meaningful_activity_at(session, now_iso));
    let working_started = session_effective_working_started_at(session, now_iso)
        .and_then(|value| parse_iso_ms(&value));
    match (meaningful, working_started) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (value, None) => value,
        (None, value) => value,
    }
}

/// True while an explicit snooze is still in force by the clock. Malformed or
/// absent wake times never hide a session.
pub fn is_snoozed_by_clock(lifecycle: &SessionLifecycleFields, now_ms: i64) -> bool {
    lifecycle
        .snoozed_until
        .as_deref()
        .and_then(parse_iso_ms)
        .map(|wake_at_ms| wake_at_ms > now_ms)
        .unwrap_or(false)
}
