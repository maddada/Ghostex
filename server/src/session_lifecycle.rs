use serde_json::Value;

use crate::{
    domain::{DomainRepository, DomainStateError, SessionLifecycleFields},
    paths::GxserverPaths,
    presentation::{
        is_active, presentation_activity, session_effective_working_started_at,
        session_meaningful_activity_at,
    },
    session_git_status::PullRequestDisposition,
    session_status::{iso_from_ms, parse_iso_ms},
};

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
Server side of Sidebar V2's settle/snooze inbox. The Ghostex
client twin lives in `packages/shared/sidebar-v2-lifecycle.ts` and this module must agree
with it field for field.

Concept mapping:
- `session.status` starting/running -> gxserver activity "working".
- `hasPendingApprovals` / `hasPendingUserInput` -> gxserver activity "attention".
- `latestTurn` / `latestUserMessageAt` -> gxserver's meaningful-activity clock
  (`meaningfulActivityAt`, falling back to `lastActiveAt`/`createdAt`) combined
  with `workingStartedAt`, which is exactly what the sidebar projection feeds the
  client as `lastInteractionAt`.
- A provider-side queued-turn grace window has no Ghostex twin: text goes
  straight into the terminal and gxserver flips activity to working, so there is
  no "message sent but unadopted" state to protect.

Lifecycle decisions documented at their rule:
- Auto-settle is persisted here. gxserver serves
  GPUI, web, mobile, and the CLI, so one server answer beats four derivations.
- Snooze expiry is NOT an eager clear (see `SNOOZE_WAKE_RETENTION_MS`).
- Snoozing does not clear a settle: the client partition already ranks snoozed
  above settled, and the decider leaves the settle untouched, so a woken
  session returns to whichever shelf it came from.
*/

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

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
A snooze is not cleared the moment it expires. The wake itself is derived —
clients (and `packages/shared/sidebar-v2-lifecycle.ts`) stop classifying a session as
snoozed as soon as `snoozedUntil` is in the past, to the millisecond, and the
retained fields are what drive the "Woke" indicator until the user visits the
row. Clearing at the boundary would erase that signal within one sweep. The
server therefore only garbage-collects spent snooze state a day after the wake
time, once the indicator can no longer be useful.
*/
pub const SNOOZE_WAKE_RETENTION_MS: i64 = DAY_MS;

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
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
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
CDXC:SidebarV2GitStatus 2026-07-29-00:00:
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
file for `debuggingMode` (see `logging.rs`), and both `sidebarVersion` and
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
CDXC:SidebarV2DataGate 2026-07-29:
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
fn read_sidebar_settings(paths: &GxserverPaths) -> Option<Value> {
    let settings_path = paths.app_config_dir.join("native-sidebar-settings.json");
    let text = std::fs::read_to_string(settings_path).ok()?;
    serde_json::from_str::<Value>(&text).ok()
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

/*
Guards. These are the server-side twins of `canSettleSidebarV2Session` /
`canSnoozeSidebarV2Session`: a stale or raced client must not park work that is
blocked on the user (attention) or in motion (working) behind a settle. Snooze
only affects visibility and never pauses the agent, so a WORKING session is
snoozable — only blocked-on-you work is refused.
*/
fn reject_when_settle_is_blocked(
    session: &Value,
    now_iso: &str,
    session_ref: &str,
) -> Result<(), DomainStateError> {
    match session_activity(session, now_iso).as_str() {
        "attention" => Err(DomainStateError::bad_request(format!(
            "Session {session_ref} is waiting on you and cannot be settled."
        ))),
        "working" => Err(DomainStateError::bad_request(format!(
            "Session {session_ref} is working and cannot be settled."
        ))),
        _ => Ok(()),
    }
}

fn reject_when_snooze_is_blocked(
    session: &Value,
    now_iso: &str,
    session_ref: &str,
) -> Result<(), DomainStateError> {
    if session_activity(session, now_iso) == "attention" {
        return Err(DomainStateError::bad_request(format!(
            "Session {session_ref} is waiting on you and cannot be snoozed."
        )));
    }
    Ok(())
}

fn require_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })
}

fn apply(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    session: Value,
    current: SessionLifecycleFields,
    next: SessionLifecycleFields,
    now_iso: &str,
) -> Result<SessionLifecycleOutcome, DomainStateError> {
    if next == current {
        return Ok(SessionLifecycleOutcome {
            changed: false,
            session,
        });
    }
    let session = repository.write_session_lifecycle(project_id, session_id, &next, now_iso)?;
    Ok(SessionLifecycleOutcome {
        changed: true,
        session,
    })
}

/*
Settle. Settling an already-settled session keeps the original `settledAt` so a
double click or a bulk settle is a silent no-op instead of an error or a
reordering churn from redundant settle emissions. Settling a session the
sweep auto-settled DOES stamp `settledAt`: that is the user promoting an
automatic decision into an explicit one, which treats a row
with a null `settledAt` as not-yet-explicitly-settled.
*/
pub fn settle_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    now_iso: &str,
) -> Result<SessionLifecycleOutcome, DomainStateError> {
    let session = require_session(repository, project_id, session_id)?;
    let session_ref = format!("{project_id}/{session_id}");
    reject_when_settle_is_blocked(&session, now_iso, &session_ref)?;
    let current = SessionLifecycleFields::from_session(&session);
    let mut next = current.clone();
    if !(current.is_settled_override() && current.settled_at.is_some()) {
        next.settled_override = Some("settled".to_string());
        next.settled_at = Some(now_iso.to_string());
        next.settled_override_at = Some(now_iso.to_string());
    }
    apply(
        repository, project_id, session_id, session, current, next, now_iso,
    )
}

/*
Un-settle. The "active" pin is the user saying "keep this in my inbox": it
suppresses auto-settle until real activity outruns the pin's stamp, at which
point the sweep clears it and the ordinary rules apply again. This is the
`thread.unsettled` with `reason: "user"` projected to `settledOverride: "active"`
plus its `reason: "activity"` reset, expressed against gxserver's activity clock
instead of an event log.
*/
pub fn unsettle_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    now_iso: &str,
) -> Result<SessionLifecycleOutcome, DomainStateError> {
    let session = require_session(repository, project_id, session_id)?;
    let current = SessionLifecycleFields::from_session(&session);
    let mut next = current.clone();
    if !current.is_active_override() {
        next.settled_override = Some("active".to_string());
        next.settled_at = None;
        next.settled_override_at = Some(now_iso.to_string());
    }
    apply(
        repository, project_id, session_id, session, current, next, now_iso,
    )
}

/*
Snooze. A wake time in the past (or an unparseable one) would produce a session
that is snoozed and awake at once — the row would never leave the inbox but
would still carry snooze state — so it is rejected rather than silently
normalized. Re-snoozing to the same wake time keeps the original `snoozedAt` so
duplicates stay no-ops; a different wake time is a real change and stamps fresh.
*/
pub fn snooze_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    snoozed_until: &str,
    now_iso: &str,
) -> Result<SessionLifecycleOutcome, DomainStateError> {
    let session = require_session(repository, project_id, session_id)?;
    let session_ref = format!("{project_id}/{session_id}");
    reject_when_snooze_is_blocked(&session, now_iso, &session_ref)?;
    let now_ms = parse_iso_ms(now_iso).ok_or_else(|| {
        DomainStateError::corrupt_state(format!("Invalid gxserver timestamp: {now_iso}."))
    })?;
    let wake_at_ms = parse_iso_ms(snoozed_until.trim()).filter(|wake_at_ms| *wake_at_ms > now_ms);
    let Some(wake_at_ms) = wake_at_ms else {
        return Err(DomainStateError::bad_request(format!(
            "snoozedUntil must be an ISO timestamp in the future; got {}.",
            snoozed_until.trim()
        )));
    };
    let normalized_until = iso_from_ms(wake_at_ms);
    let current = SessionLifecycleFields::from_session(&session);
    let mut next = current.clone();
    let is_duplicate = current.snoozed_until.as_deref() == Some(normalized_until.as_str())
        && current.snoozed_at.is_some();
    if !is_duplicate {
        next.snoozed_until = Some(normalized_until);
        next.snoozed_at = Some(now_iso.to_string());
    }
    apply(
        repository, project_id, session_id, session, current, next, now_iso,
    )
}

/// Wake. Clears both snooze fields; waking a session that is not snoozed is a
/// no-op rather than an error.
pub fn unsnooze_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    now_iso: &str,
) -> Result<SessionLifecycleOutcome, DomainStateError> {
    let session = require_session(repository, project_id, session_id)?;
    let current = SessionLifecycleFields::from_session(&session);
    let mut next = current.clone();
    next.clear_snooze();
    apply(
        repository, project_id, session_id, session, current, next, now_iso,
    )
}

/*
The periodic sweep. One pass owns three rules, applied in order to at most one
durable write per session:

1. Spent-snooze collection (see `SNOOZE_WAKE_RETENTION_MS`). Attention NEVER
   clears a snooze here: a snoozed session that raises its hand stays snoozed
   server-side and the client surfaces it, so the user's return ticket is not
   spent by the agent asking a question. This rule (and rule 2) runs for every
   user regardless of `sidebarVersion`: someone can snooze rows in V2 and then
   switch back to V1, and that state still has to be groomed.
2. Activity reset. Real activity newer than the override's stamp clears ANY
   override — a settled session wakes, an "active" pin unpins — mirroring
   activity-driven session unsetting rule.
3. Auto-settle, from either of two triggers: the inactivity window, or a merged
   or closed pull request on the session's branch. Runs only for Sidebar V2
   users (see `read_sweep_auto_settle_after_days`; a V1 machine passes
   `auto_settle_after_days: None` and `auto_settle_on_finished_pull_request:
   false`, which stops at rules 1 and 2). Both triggers share one set of guards:
   skip working/attention sessions (the same guards the settle command
   enforces), skip explicitly snoozed sessions (snoozed is not settled: the wake
   time is a stronger statement about when the row matters), and skip "active"
   pins. Either auto-settle deliberately leaves `settledAt` NULL: the settled
   shelf sorts unstamped rows by the meaningful-activity clock, so an automatic
   settle reads as "work ended then", not "the daemon noticed now".

Only sessions the sidebar can actually show (running/sleeping) are considered, so
a backlog of long-dead stopped rows never produces a delta storm.

`resolve_pull_request` is injected rather than read from the git-status cache
here so the rules stay testable without a probe cache, and so the sweep never
does I/O of its own: gxserver's background pass owns the probing, this pass only
reads its answer.
*/
pub fn run_session_lifecycle_sweep(
    repository: &DomainRepository<'_>,
    options: &SessionLifecycleSweepOptions,
    resolve_pull_request: &dyn Fn(&Value) -> PullRequestDisposition,
) -> Result<SessionLifecycleSweepOutcome, DomainStateError> {
    let now_iso = options.now_iso.as_str();
    let now_ms = parse_iso_ms(now_iso).ok_or_else(|| {
        DomainStateError::corrupt_state(format!("Invalid gxserver timestamp: {now_iso}."))
    })?;
    let mut outcome = SessionLifecycleSweepOutcome::default();
    for session in repository.list_sessions(None)? {
        if outcome.changed.len() >= options.max_mutations {
            break;
        }
        if !is_active(&session) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        let current = SessionLifecycleFields::from_session(&session);
        let pull_request = if options.auto_settle_on_finished_pull_request {
            resolve_pull_request(&session)
        } else {
            PullRequestDisposition::Unknown
        };
        let next = resolve_swept_lifecycle(&session, &current, options, now_ms, pull_request);
        if next == current {
            continue;
        }
        let project_id = project_id.to_string();
        let session_id = session_id.to_string();
        repository.write_session_lifecycle(&project_id, &session_id, &next, now_iso)?;
        outcome.changed.push((project_id, session_id));
    }
    Ok(outcome)
}

/// Pure sweep decision for one session, split out so the rules are testable
/// without a database.
pub fn resolve_swept_lifecycle(
    session: &Value,
    current: &SessionLifecycleFields,
    options: &SessionLifecycleSweepOptions,
    now_ms: i64,
    pull_request: PullRequestDisposition,
) -> SessionLifecycleFields {
    let now_iso = options.now_iso.as_str();
    let mut next = current.clone();

    // 1. Spent-snooze collection.
    match next.snoozed_until.as_deref().map(str::trim) {
        None => {
            // An orphan `snoozedAt` without a wake time is unreachable state.
            next.snoozed_at = None;
        }
        Some(until) => match parse_iso_ms(until) {
            // Malformed data never hides a session, so it never lingers either.
            None => next.clear_snooze(),
            Some(wake_at_ms) if now_ms >= wake_at_ms + SNOOZE_WAKE_RETENTION_MS => {
                next.clear_snooze()
            }
            Some(_) => {}
        },
    }

    let activity = session_activity(session, now_iso);
    let last_activity_ms = session_last_activity_ms(session, now_iso);

    // 2. Activity reset of the settle override.
    if next.settled_override.is_some() || next.settled_at.is_some() {
        let stamped_ms = next.settled_override_at.as_deref().and_then(parse_iso_ms);
        if let (Some(stamped_ms), Some(last_activity_ms)) = (stamped_ms, last_activity_ms) {
            if last_activity_ms > stamped_ms {
                next.clear_settle();
            }
        }
    }

    // 3. Auto-settle: a finished pull request, or the inactivity window.
    if options.auto_settle_after_days.is_none() && !options.auto_settle_on_finished_pull_request {
        return next;
    }
    if next.settled_override.is_some() || next.settled_at.is_some() {
        return next;
    }
    if activity != "idle" || is_snoozed_by_clock(&next, now_ms) {
        return next;
    }
    /*
    A merged or closed pull request is a completion signal, not a staleness one,
    so it does not wait out the inactivity window — the branch's work is over the
    moment the forge says so. It still passes through every guard above: a
    session that is working or waiting on the user is never parked behind a
    settle, a snooze outranks it, and an "active" pin suppresses it until real
    activity clears the pin (rule 2).
    */
    let has_finished_pull_request = options.auto_settle_on_finished_pull_request
        && pull_request == PullRequestDisposition::Finished;
    let is_inactive = match (options.auto_settle_after_days, last_activity_ms) {
        (Some(auto_settle_after_days), Some(last_activity_ms)) => {
            let window_ms = (auto_settle_after_days * DAY_MS as f64) as i64;
            last_activity_ms < now_ms - window_ms
        }
        _ => false,
    };
    if has_finished_pull_request || is_inactive {
        next.settled_override = Some("settled".to_string());
        next.settled_override_at = Some(now_iso.to_string());
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };
    use rusqlite::Connection;
    use serde_json::{json, Map};

    const NOW: &str = "2026-07-29T12:00:00.000Z";

    fn now_ms() -> i64 {
        parse_iso_ms(NOW).expect("now")
    }

    fn ago(days: f64) -> String {
        iso_from_ms(now_ms() - (days * DAY_MS as f64) as i64)
    }

    fn ahead(hours: f64) -> String {
        iso_from_ms(now_ms() + (hours * 60.0 * 60.0 * 1_000.0) as i64)
    }

    struct TestDb {
        _temp: tempfile::TempDir,
        db: Connection,
    }

    fn test_db() -> TestDb {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        TestDb { _temp: temp, db }
    }

    fn repository(handle: &TestDb) -> DomainRepository<'_> {
        DomainRepository::new(&handle.db, "S1")
    }

    fn params(entries: Value) -> Map<String, Value> {
        entries.as_object().cloned().expect("params object")
    }

    /// A running session whose meaningful-activity clock reads `activity_at`.
    fn create_session(
        repository: &DomainRepository<'_>,
        project_id: &str,
        activity: &str,
        activity_at: &str,
    ) -> (String, String) {
        let mut agent_activity = params(json!({
            "activity": activity,
            "agentName": "codex",
            "lastChangedAt": activity_at,
            "lastMeaningfulActivityAt": activity_at,
            "updatedAt": activity_at,
        }));
        if activity == "working" {
            // An explicit working stint never expires on read, which is what a
            // real agent-hook working session looks like.
            agent_activity.insert("workingSource".to_string(), json!("explicit"));
            agent_activity.insert("workingStartedAt".to_string(), json!(activity_at));
            agent_activity.insert("hasSeenWorking".to_string(), json!(true));
        }
        let session = repository
            .create_session(
                &params(json!({
                    "projectId": project_id,
                    "kind": "agent",
                    "agentId": "codex",
                    "title": "Lifecycle session",
                    "lifecycleState": "running",
                    "lastActiveAt": activity_at,
                    "providerState": { "lifecycleState": "exists" },
                    "runtimeSettings": { "agentActivity": Value::Object(agent_activity) },
                })),
                false,
            )
            .expect("create session");
        (
            session
                .get("projectId")
                .and_then(Value::as_str)
                .expect("projectId")
                .to_string(),
            session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("sessionId")
                .to_string(),
        )
    }

    fn create_project(repository: &DomainRepository<'_>) -> String {
        let project = repository
            .create_project(&params(
                json!({ "name": "Lifecycle", "path": "/tmp/lifecycle" }),
            ))
            .expect("create project");
        project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("projectId")
            .to_string()
    }

    fn lifecycle_of(
        repository: &DomainRepository<'_>,
        project_id: &str,
        session_id: &str,
    ) -> SessionLifecycleFields {
        let session = repository
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session exists");
        SessionLifecycleFields::from_session(&session)
    }

    fn sweep_options(auto_settle_after_days: Option<f64>) -> SessionLifecycleSweepOptions {
        SessionLifecycleSweepOptions {
            auto_settle_on_finished_pull_request: auto_settle_on_finished_pull_request(
                auto_settle_after_days,
            ),
            auto_settle_after_days,
            max_mutations: SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS,
            now_iso: NOW.to_string(),
        }
    }

    /// Resolves every session's pull request to `disposition`, which is enough
    /// for the guard tests: they only ever have one candidate session.
    fn every_pull_request(
        disposition: PullRequestDisposition,
    ) -> impl Fn(&Value) -> PullRequestDisposition {
        move |_session: &Value| disposition
    }

    /// Resolves the pull request of one session id, and nothing else.
    fn pull_request_of(
        session_id: &str,
        disposition: PullRequestDisposition,
    ) -> impl Fn(&Value) -> PullRequestDisposition + '_ {
        move |session: &Value| {
            if session.get("sessionId").and_then(Value::as_str) == Some(session_id) {
                disposition
            } else {
                PullRequestDisposition::Unknown
            }
        }
    }

    #[test]
    fn old_state_rows_hydrate_without_lifecycle_fields() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

        let session = repository
            .get_session(&project_id, &session_id)
            .expect("get session")
            .expect("session exists");
        for key in [
            "settledAt",
            "settledOverride",
            "settledOverrideAt",
            "snoozedAt",
            "snoozedUntil",
        ] {
            assert!(session.get(key).is_none(), "{key} should be absent");
        }
        assert_eq!(
            SessionLifecycleFields::from_session(&session),
            SessionLifecycleFields::default()
        );
    }

    #[test]
    fn settle_rejects_working_and_attention_sessions() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        for activity in ["working", "attention"] {
            let (project_id, session_id) = create_session(&repository, &project_id, activity, NOW);
            let error = settle_session(&repository, &project_id, &session_id, NOW)
                .expect_err("blocked settle");
            assert_eq!(error.code, "badRequest");
            assert_eq!(
                lifecycle_of(&repository, &project_id, &session_id),
                SessionLifecycleFields::default()
            );
        }
    }

    #[test]
    fn snooze_rejects_attention_but_allows_working() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        let (project_id, attention_id) = create_session(&repository, &project_id, "attention", NOW);
        let error = snooze_session(&repository, &project_id, &attention_id, &ahead(1.0), NOW)
            .expect_err("blocked snooze");
        assert_eq!(error.code, "badRequest");

        let (project_id, working_id) = create_session(&repository, &project_id, "working", NOW);
        let outcome = snooze_session(&repository, &project_id, &working_id, &ahead(1.0), NOW)
            .expect("working sessions are snoozable");
        assert!(outcome.changed);
        assert_eq!(
            lifecycle_of(&repository, &project_id, &working_id).snoozed_until,
            Some(ahead(1.0))
        );
    }

    #[test]
    fn snooze_rejects_wake_times_that_are_not_in_the_future() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", NOW);

        for wake_at in [ago(1.0), NOW.to_string(), "not-a-timestamp".to_string()] {
            let error = snooze_session(&repository, &project_id, &session_id, &wake_at, NOW)
                .expect_err("rejected wake time");
            assert_eq!(error.code, "badRequest");
        }
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id),
            SessionLifecycleFields::default()
        );
    }

    #[test]
    fn settle_and_unsettle_are_idempotent_and_carry_override_semantics() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

        let settled = settle_session(&repository, &project_id, &session_id, NOW).expect("settle");
        assert!(settled.changed);
        let after_settle = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(after_settle.settled_override.as_deref(), Some("settled"));
        assert_eq!(after_settle.settled_at.as_deref(), Some(NOW));

        let later = "2026-07-29T13:00:00.000Z";
        let repeat = settle_session(&repository, &project_id, &session_id, later).expect("settle");
        assert!(!repeat.changed, "duplicate settle must be a no-op");
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id).settled_at,
            after_settle.settled_at
        );

        let unsettled =
            unsettle_session(&repository, &project_id, &session_id, later).expect("unsettle");
        assert!(unsettled.changed);
        let after_unsettle = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(after_unsettle.settled_override.as_deref(), Some("active"));
        assert_eq!(after_unsettle.settled_at, None);
        assert_eq!(after_unsettle.settled_override_at.as_deref(), Some(later));

        let repeat =
            unsettle_session(&repository, &project_id, &session_id, NOW).expect("unsettle");
        assert!(!repeat.changed, "duplicate unsettle must be a no-op");
    }

    #[test]
    fn snooze_is_idempotent_and_unsnooze_clears_both_fields() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
        let wake_at = ahead(3.0);

        let snoozed =
            snooze_session(&repository, &project_id, &session_id, &wake_at, NOW).expect("snooze");
        assert!(snoozed.changed);
        let after_snooze = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(
            after_snooze.snoozed_until.as_deref(),
            Some(wake_at.as_str())
        );
        assert_eq!(after_snooze.snoozed_at.as_deref(), Some(NOW));

        let later = "2026-07-29T12:30:00.000Z";
        let repeat =
            snooze_session(&repository, &project_id, &session_id, &wake_at, later).expect("snooze");
        assert!(!repeat.changed, "same wake time must be a no-op");
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id).snoozed_at,
            after_snooze.snoozed_at
        );

        let rescheduled = snooze_session(&repository, &project_id, &session_id, &ahead(9.0), later)
            .expect("snooze");
        assert!(rescheduled.changed, "a new wake time is a real change");
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id).snoozed_at,
            Some(later.to_string())
        );

        let woken =
            unsnooze_session(&repository, &project_id, &session_id, later).expect("unsnooze");
        assert!(woken.changed);
        let after_wake = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(after_wake.snoozed_until, None);
        assert_eq!(after_wake.snoozed_at, None);

        let repeat =
            unsnooze_session(&repository, &project_id, &session_id, later).expect("unsnooze");
        assert!(!repeat.changed, "waking an awake session must be a no-op");
    }

    #[test]
    fn snoozing_a_settled_session_leaves_the_settle_alone() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));

        settle_session(&repository, &project_id, &session_id, NOW).expect("settle");
        snooze_session(&repository, &project_id, &session_id, &ahead(2.0), NOW).expect("snooze");

        let lifecycle = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(lifecycle.settled_override.as_deref(), Some("settled"));
        assert_eq!(lifecycle.settled_at.as_deref(), Some(NOW));
        assert_eq!(
            lifecycle.snoozed_until.as_deref(),
            Some(ahead(2.0).as_str())
        );
    }

    #[test]
    fn generic_session_updates_can_neither_set_nor_lose_lifecycle_state() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
        settle_session(&repository, &project_id, &session_id, NOW).expect("settle");

        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert("title".to_string(), json!("Renamed"));
        update.insert("settledOverride".to_string(), json!("active"));
        update.insert("snoozedUntil".to_string(), json!(ahead(4.0)));
        let updated = repository.update_session(&update).expect("update session");

        assert_eq!(updated.get("title"), Some(&json!("Renamed")));
        let lifecycle = lifecycle_of(&repository, &project_id, &session_id);
        assert_eq!(
            lifecycle.settled_override.as_deref(),
            Some("settled"),
            "a generic update must not smuggle a lifecycle change past the guards"
        );
        assert_eq!(lifecycle.settled_at.as_deref(), Some(NOW));
        assert_eq!(lifecycle.snoozed_until, None);
    }

    #[test]
    fn sweep_auto_settles_only_stale_idle_unpinned_sessions() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        let (project_id, stale_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
        let (project_id, fresh_id) = create_session(&repository, &project_id, "idle", &ago(1.0));
        let (project_id, working_id) =
            create_session(&repository, &project_id, "working", &ago(5.0));
        let (project_id, attention_id) =
            create_session(&repository, &project_id, "attention", &ago(5.0));
        let (project_id, snoozed_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
        snooze_session(&repository, &project_id, &snoozed_id, &ahead(4.0), NOW).expect("snooze");
        let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
        unsettle_session(&repository, &project_id, &pinned_id, NOW).expect("unsettle");

        let outcome = run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &ignore_pull_requests,
        )
        .expect("sweep");
        assert_eq!(
            outcome.changed,
            vec![(project_id.clone(), stale_id.clone())],
            "only the stale idle session may auto-settle"
        );

        let settled = lifecycle_of(&repository, &project_id, &stale_id);
        assert_eq!(settled.settled_override.as_deref(), Some("settled"));
        assert_eq!(
            settled.settled_at, None,
            "an inactivity settle is not an explicit stamp"
        );
        for session_id in [&fresh_id, &working_id, &attention_id] {
            assert_eq!(
                lifecycle_of(&repository, &project_id, session_id).settled_override,
                None
            );
        }
        assert_eq!(
            lifecycle_of(&repository, &project_id, &snoozed_id).settled_override,
            None,
            "snoozed is not settled"
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &pinned_id)
                .settled_override
                .as_deref(),
            Some("active"),
            "a manual un-settle pin suppresses auto-settle"
        );
    }

    #[test]
    fn sweep_does_not_auto_settle_when_the_window_is_disabled() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(90.0));

        let outcome =
            run_session_lifecycle_sweep(&repository, &sweep_options(None), &ignore_pull_requests)
                .expect("sweep");
        assert!(outcome.changed.is_empty());
        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id).settled_override,
            None
        );
    }

    #[test]
    fn sweep_clears_any_override_once_activity_outruns_its_stamp() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        // Settled a day ago, then worked an hour ago.
        let (project_id, settled_id) = create_session(&repository, &project_id, "idle", &ago(0.04));
        settle_session(&repository, &project_id, &settled_id, &ago(1.0)).expect("settle");
        // Pinned active a day ago, then worked an hour ago.
        let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(0.04));
        unsettle_session(&repository, &project_id, &pinned_id, &ago(1.0)).expect("unsettle");
        // Settled after its last activity: the settle still stands.
        let (project_id, stable_id) = create_session(&repository, &project_id, "idle", &ago(5.0));
        settle_session(&repository, &project_id, &stable_id, &ago(1.0)).expect("settle");

        run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &ignore_pull_requests,
        )
        .expect("sweep");

        assert_eq!(
            lifecycle_of(&repository, &project_id, &settled_id),
            SessionLifecycleFields::default()
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &pinned_id),
            SessionLifecycleFields::default()
        );
        let stable = lifecycle_of(&repository, &project_id, &stable_id);
        assert_eq!(stable.settled_override.as_deref(), Some("settled"));
        assert_eq!(stable.settled_at.as_deref(), Some(ago(1.0).as_str()));
    }

    #[test]
    fn sweep_collects_spent_snooze_state_only_after_the_woke_window() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        let (project_id, just_woke_id) =
            create_session(&repository, &project_id, "idle", &ago(0.1));
        let (project_id, long_woke_id) =
            create_session(&repository, &project_id, "idle", &ago(0.1));
        let (project_id, still_snoozed_id) =
            create_session(&repository, &project_id, "idle", &ago(0.1));

        let base = iso_from_ms(now_ms() - 10 * DAY_MS);
        snooze_session(&repository, &project_id, &just_woke_id, &ago(0.2), &base).expect("snooze");
        snooze_session(&repository, &project_id, &long_woke_id, &ago(3.0), &base).expect("snooze");
        snooze_session(
            &repository,
            &project_id,
            &still_snoozed_id,
            &ahead(6.0),
            &base,
        )
        .expect("snooze");

        run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &ignore_pull_requests,
        )
        .expect("sweep");

        assert!(
            lifecycle_of(&repository, &project_id, &just_woke_id)
                .snoozed_until
                .is_some(),
            "a freshly woken session keeps its Woke signal"
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &long_woke_id).snoozed_until,
            None
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &still_snoozed_id).snoozed_until,
            Some(ahead(6.0))
        );
    }

    #[test]
    fn sweep_never_clears_a_snooze_because_a_session_wants_attention() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, session_id) = create_session(&repository, &project_id, "idle", &ago(0.1));
        snooze_session(&repository, &project_id, &session_id, &ahead(5.0), NOW).expect("snooze");

        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert(
            "runtimeSettings".to_string(),
            json!({
                "agentActivity": {
                    "activity": "attention",
                    "agentName": "codex",
                    "updatedAt": NOW,
                },
            }),
        );
        repository.update_session(&update).expect("update session");

        run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &ignore_pull_requests,
        )
        .expect("sweep");

        assert_eq!(
            lifecycle_of(&repository, &project_id, &session_id).snoozed_until,
            Some(ahead(5.0)),
            "the client surfaces a raised hand; the server keeps the return ticket"
        );
    }

    #[test]
    fn auto_settle_window_reads_the_shared_sidebar_settings_file() {
        assert_eq!(
            normalize_auto_settle_after_days(None),
            Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS)
        );
        assert_eq!(normalize_auto_settle_after_days(Some(&json!(null))), None);
        assert_eq!(normalize_auto_settle_after_days(Some(&json!(0))), None);
        assert_eq!(normalize_auto_settle_after_days(Some(&json!(-2))), None);
        assert_eq!(normalize_auto_settle_after_days(Some(&json!(7))), Some(7.0));
        assert_eq!(
            normalize_auto_settle_after_days(Some(&json!("nonsense"))),
            Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS)
        );

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        assert_eq!(
            read_sweep_auto_settle_after_days(&paths),
            None,
            "a machine with no settings file is a V1 machine and never auto-settles"
        );

        let settings_dir = paths.app_config_dir.clone();
        std::fs::create_dir_all(&settings_dir).expect("settings dir");
        let settings_file = settings_dir.join("native-sidebar-settings.json");
        std::fs::write(
            &settings_file,
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 10 }).to_string(),
        )
        .expect("settings file");
        assert_eq!(read_sweep_auto_settle_after_days(&paths), Some(10.0));

        std::fs::write(
            &settings_file,
            json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 10 }).to_string(),
        )
        .expect("settings file");
        assert_eq!(
            read_sweep_auto_settle_after_days(&paths),
            None,
            "switching back to V1 stops the automatic pass without touching the window setting"
        );
    }

    /*
    CDXC:SidebarV2DataGate 2026-07-29:
    The gate the Sidebar V2 data passes (git status, `origin` remote, project
    icons) read once per pass. It answers from the SAME settings file and the
    SAME `is_sidebar_v2_selected` rule as the auto-settle window above, so a
    machine can never be V2 for one of them and V1 for the other, and a flip is
    picked up by the next pass rather than the next daemon start.
    */
    #[test]
    fn the_sidebar_v2_data_gate_reads_the_shared_settings_file_each_time() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        assert!(
            !read_sidebar_v2_selected(&paths),
            "a machine with no settings file is a V1 machine and probes nothing"
        );

        let settings_dir = paths.app_config_dir.clone();
        std::fs::create_dir_all(&settings_dir).expect("settings dir");
        let settings_file = settings_dir.join("native-sidebar-settings.json");

        std::fs::write(&settings_file, "{ not json").expect("settings file");
        assert!(
            !read_sidebar_v2_selected(&paths),
            "an unreadable settings file is V1, never a reason to probe anyway"
        );

        for (settings, expected) in [
            (json!({}), false),
            (json!({ "sidebarVersion": "v1" }), false),
            (json!({ "sidebarVersion": "v3" }), false),
            (json!({ "sidebarVersion": "v2" }), true),
            (json!({ "sidebarVersion": " V2 " }), true),
        ] {
            std::fs::write(&settings_file, settings.to_string()).expect("settings file");
            assert_eq!(
                read_sidebar_v2_selected(&paths),
                expected,
                "settings {settings} must gate the data passes to {expected}"
            );
            assert_eq!(
                read_sweep_auto_settle_after_days(&paths).is_some(),
                expected,
                "the data gate and the auto-settle gate must never disagree"
            );
        }
    }

    #[test]
    fn auto_settle_window_is_gated_on_the_sidebar_version() {
        assert!(!is_sidebar_v2_selected(None));
        assert!(!is_sidebar_v2_selected(Some(&json!("v1"))));
        assert!(!is_sidebar_v2_selected(Some(&json!("v3"))));
        assert!(!is_sidebar_v2_selected(Some(&json!(2))));
        assert!(is_sidebar_v2_selected(Some(&json!("v2"))));
        assert!(is_sidebar_v2_selected(Some(&json!(" v2 "))));

        assert_eq!(resolve_sweep_auto_settle_after_days(None), None);
        assert_eq!(
            resolve_sweep_auto_settle_after_days(Some(&json!({ "sidebarAutoSettleAfterDays": 7 }))),
            None,
            "a missing sidebarVersion means V1"
        );
        assert_eq!(
            resolve_sweep_auto_settle_after_days(Some(
                &json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 7 })
            )),
            None
        );
        assert_eq!(
            resolve_sweep_auto_settle_after_days(Some(
                &json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 7 })
            )),
            Some(7.0)
        );
        assert_eq!(
            resolve_sweep_auto_settle_after_days(Some(&json!({ "sidebarVersion": "v2" }))),
            Some(DEFAULT_AUTO_SETTLE_AFTER_DAYS),
            "V2 without an explicit window uses the default"
        );
        assert_eq!(
            resolve_sweep_auto_settle_after_days(Some(
                &json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": null })
            )),
            None,
            "an explicit null still disables auto-settle inside V2"
        );
    }

    /// The version gate reaches the sweep itself: a V1 machine keeps every stale
    /// row in its inbox, a V2 machine settles it, and spent-snooze collection
    /// grooms both so a user who flips back to V1 does not strand snooze state.
    #[test]
    fn sweep_auto_settles_only_for_v2_while_snooze_gc_runs_for_both_versions() {
        for settings in [
            json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 3 }),
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 3 }),
        ] {
            let is_v2 = settings.get("sidebarVersion") == Some(&json!("v2"));
            let handle = test_db();
            let repository = repository(&handle);
            let project_id = create_project(&repository);

            let (project_id, stale_id) =
                create_session(&repository, &project_id, "idle", &ago(5.0));
            let (project_id, long_woke_id) =
                create_session(&repository, &project_id, "idle", &ago(0.1));
            let base = iso_from_ms(now_ms() - 10 * DAY_MS);
            snooze_session(&repository, &project_id, &long_woke_id, &ago(3.0), &base)
                .expect("snooze");

            let options = sweep_options(resolve_sweep_auto_settle_after_days(Some(&settings)));
            run_session_lifecycle_sweep(&repository, &options, &ignore_pull_requests)
                .expect("sweep");

            assert_eq!(
                lifecycle_of(&repository, &project_id, &stale_id)
                    .settled_override
                    .as_deref(),
                if is_v2 { Some("settled") } else { None },
                "auto-settle must follow sidebarVersion (v2: {is_v2})"
            );
            assert_eq!(
                lifecycle_of(&repository, &project_id, &long_woke_id).snoozed_until,
                None,
                "spent snooze state is collected regardless of sidebarVersion (v2: {is_v2})"
            );
        }
    }

    /*
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
    The pull-request trigger settles finished work the moment the forge says it
    is finished — no inactivity wait — while an open (or draft, which resolves to
    open) pull request and an unknown one leave the row exactly where it is.
    */
    #[test]
    fn sweep_settles_a_finished_pull_request_immediately_without_waiting_out_the_window() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        // All three worked minutes ago, far inside the three-day window.
        let (project_id, finished_id) =
            create_session(&repository, &project_id, "idle", &ago(0.01));
        let (project_id, open_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
        let (project_id, unknown_id) = create_session(&repository, &project_id, "idle", &ago(0.01));

        let finished_ref = finished_id.clone();
        let open_ref = open_id.clone();
        let outcome = run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &move |session: &Value| match session.get("sessionId").and_then(Value::as_str) {
                Some(session_id) if session_id == finished_ref => PullRequestDisposition::Finished,
                Some(session_id) if session_id == open_ref => PullRequestDisposition::Open,
                _ => PullRequestDisposition::Unknown,
            },
        )
        .expect("sweep");

        assert_eq!(
            outcome.changed,
            vec![(project_id.clone(), finished_id.clone())],
            "only the merged/closed pull request settles"
        );
        let settled = lifecycle_of(&repository, &project_id, &finished_id);
        assert_eq!(settled.settled_override.as_deref(), Some("settled"));
        assert_eq!(
            settled.settled_at, None,
            "an automatic settle is not an explicit stamp, whatever triggered it"
        );
        assert_eq!(settled.settled_override_at.as_deref(), Some(NOW));
        for session_id in [&open_id, &unknown_id] {
            assert_eq!(
                lifecycle_of(&repository, &project_id, session_id).settled_override,
                None
            );
        }
    }

    #[test]
    fn a_finished_pull_request_still_passes_through_every_settle_guard() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);

        let (project_id, working_id) =
            create_session(&repository, &project_id, "working", &ago(0.01));
        let (project_id, attention_id) =
            create_session(&repository, &project_id, "attention", &ago(0.01));
        let (project_id, snoozed_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
        snooze_session(&repository, &project_id, &snoozed_id, &ahead(4.0), NOW).expect("snooze");
        let (project_id, pinned_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
        unsettle_session(&repository, &project_id, &pinned_id, NOW).expect("unsettle");

        let outcome = run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &every_pull_request(PullRequestDisposition::Finished),
        )
        .expect("sweep");
        assert!(
            outcome.changed.is_empty(),
            "a merged pull request never parks work that is in motion, blocked on the user, snoozed, or pinned active"
        );

        for session_id in [&working_id, &attention_id, &snoozed_id] {
            assert_eq!(
                lifecycle_of(&repository, &project_id, session_id).settled_override,
                None
            );
        }
        assert_eq!(
            lifecycle_of(&repository, &project_id, &pinned_id)
                .settled_override
                .as_deref(),
            Some("active"),
            "the active pin survives until real activity clears it (rule 2)"
        );
    }

    /*
    The activity-reset half of the pin's semantics. An "active" pin means "keep
    this in my inbox" only until real activity outruns its stamp; after that the
    pin is spent and the ordinary rules apply again, in the same pass — exactly
    how the inactivity trigger already behaves when a long-dormant pin is reset
    by activity that is itself older than the window.
    */
    #[test]
    fn a_pin_outrun_by_activity_stops_protecting_a_finished_pull_request() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        // Pinned a day ago, then worked an hour ago: the pin is spent.
        let (project_id, spent_pin_id) =
            create_session(&repository, &project_id, "idle", &ago(0.04));
        unsettle_session(&repository, &project_id, &spent_pin_id, &ago(1.0)).expect("unsettle");
        // Pinned after its last activity: the pin still stands.
        let (project_id, live_pin_id) = create_session(&repository, &project_id, "idle", &ago(1.0));
        unsettle_session(&repository, &project_id, &live_pin_id, &ago(0.04)).expect("unsettle");

        run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &every_pull_request(PullRequestDisposition::Finished),
        )
        .expect("sweep");

        let spent = lifecycle_of(&repository, &project_id, &spent_pin_id);
        assert_eq!(
            spent.settled_override.as_deref(),
            Some("settled"),
            "the reset clears the pin and the merged pull request then settles the row"
        );
        assert_eq!(spent.settled_at, None);
        assert_eq!(
            lifecycle_of(&repository, &project_id, &live_pin_id)
                .settled_override
                .as_deref(),
            Some("active"),
            "a live pin still outranks the pull request"
        );
    }

    /// The pull-request trigger rides the same switch as the inactivity window:
    /// V1 machines and users who turned auto-settle off never see it.
    #[test]
    fn pull_request_auto_settle_follows_the_inactivity_window_switch() {
        assert!(!auto_settle_on_finished_pull_request(None));
        assert!(auto_settle_on_finished_pull_request(Some(
            DEFAULT_AUTO_SETTLE_AFTER_DAYS
        )));

        for (settings, expected) in [
            (
                json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 3 }),
                None,
            ),
            (json!({ "sidebarVersion": "v2" }), Some("settled")),
            (
                json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": null }),
                None,
            ),
        ] {
            let handle = test_db();
            let repository = repository(&handle);
            let project_id = create_project(&repository);
            let (project_id, session_id) =
                create_session(&repository, &project_id, "idle", &ago(0.01));

            let options = sweep_options(resolve_sweep_auto_settle_after_days(Some(&settings)));
            run_session_lifecycle_sweep(
                &repository,
                &options,
                &every_pull_request(PullRequestDisposition::Finished),
            )
            .expect("sweep");

            assert_eq!(
                lifecycle_of(&repository, &project_id, &session_id)
                    .settled_override
                    .as_deref(),
                expected,
                "settings {settings} must decide the pull-request trigger too"
            );
        }
    }

    /// The resolver is only consulted for machines the trigger is enabled on, so
    /// a V1 daemon never pays for a git-status lookup it would ignore.
    #[test]
    fn the_pull_request_resolver_is_skipped_when_the_trigger_is_off() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        create_session(&repository, &project_id, "idle", &ago(0.01));

        let calls = std::cell::Cell::new(0_usize);
        let options = sweep_options(None);
        run_session_lifecycle_sweep(&repository, &options, &|_session: &Value| {
            calls.set(calls.get() + 1);
            PullRequestDisposition::Finished
        })
        .expect("sweep");
        assert_eq!(calls.get(), 0);

        let options = sweep_options(Some(3.0));
        run_session_lifecycle_sweep(&repository, &options, &|_session: &Value| {
            calls.set(calls.get() + 1);
            PullRequestDisposition::Unknown
        })
        .expect("sweep");
        assert_eq!(calls.get(), 1);
    }

    /// One session id, so `pull_request_of` earns its keep next to
    /// `every_pull_request`.
    #[test]
    fn only_the_session_whose_pull_request_finished_is_settled() {
        let handle = test_db();
        let repository = repository(&handle);
        let project_id = create_project(&repository);
        let (project_id, settled_id) = create_session(&repository, &project_id, "idle", &ago(0.01));
        let (project_id, untouched_id) =
            create_session(&repository, &project_id, "idle", &ago(0.01));

        run_session_lifecycle_sweep(
            &repository,
            &sweep_options(Some(3.0)),
            &pull_request_of(&settled_id, PullRequestDisposition::Finished),
        )
        .expect("sweep");

        assert_eq!(
            lifecycle_of(&repository, &project_id, &settled_id)
                .settled_override
                .as_deref(),
            Some("settled")
        );
        assert_eq!(
            lifecycle_of(&repository, &project_id, &untouched_id).settled_override,
            None
        );
    }
}
