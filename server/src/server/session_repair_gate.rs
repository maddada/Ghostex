//! When a read runs the three session repair passes, per session.
//!
//! CDXC:StateSync 2026-11-10 WHY:
//! The passes (sidecar state, zmx provider existence, live process identity) copy what agents and
//! zmx report outside gxserver into the session rows. They used to run on every listSessions, and
//! the snapshot ran them at most every 5 seconds behind one shared gate (CDXC:StateSync
//! 2026-09-18). A review found that a shared gate lets a narrow read use up the window for every
//! other session: the CLI's session lookup snapshots 1 to 3 rows, and the remote coordinator
//! runtime lists one project every 3 seconds, so under steady CLI traffic a session could list
//! later than 5 seconds. Each session now keeps its own time: a read repairs only those of its
//! rows whose last repair is 5 seconds old or more, and stamps only them. So every listed row is
//! at most 5 seconds behind what agents and zmx last reported, whichever read lists it, and a
//! read that repairs a few rows leaves every other session's window alone. Writes gxserver makes
//! itself are in the rows already and never wait for this.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::*;

const REPAIR_EVERY: Duration = Duration::from_secs(5);

fn repaired_at() -> &'static Mutex<HashMap<(String, String), Instant>> {
    static REPAIRED: OnceLock<Mutex<HashMap<(String, String), Instant>>> = OnceLock::new();
    REPAIRED.get_or_init(Default::default)
}

/// The positions in `sessions` of the rows whose repair is due, each stamped now.
fn take_due(sessions: &[Value]) -> Vec<usize> {
    let Ok(mut repaired) = repaired_at().lock() else {
        return (0..sessions.len()).collect();
    };
    let now = Instant::now();
    // A stamp older than the window means "due" either way, so dropping it changes nothing.
    repaired.retain(|_, at| now.duration_since(*at) < REPAIR_EVERY);
    sessions
        .iter()
        .enumerate()
        .filter_map(|(index, session)| {
            let text = |key: &str| session.get(key).and_then(Value::as_str);
            let key = (
                text("projectId")?.to_string(),
                text("sessionId")?.to_string(),
            );
            if repaired.contains_key(&key) {
                return None;
            }
            repaired.insert(key, now);
            Some(index)
        })
        .collect()
}

/// Runs the three repair passes over the rows of `sessions` that are due (see the module), and
/// says whether any row changed, so the caller reads its list again.
pub(crate) fn run_due_repair_passes(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    sessions: &[Value],
    reason: &str,
) -> std::result::Result<bool, DomainStateError> {
    let due = take_due(sessions);
    if due.is_empty() {
        return Ok(false);
    }
    let subset;
    let rows: &[Value] = if due.len() == sessions.len() {
        sessions
    } else {
        subset = due
            .iter()
            .map(|index| sessions[*index].clone())
            .collect::<Vec<_>>();
        &subset
    };
    let mut changed = sync_session_state_sidecars(state, db, repository, rows, reason)?;
    changed |= sync_zmx_provider_existence(state, db, repository, rows)?;
    changed |= sync_live_zmx_process_identities(state, db, repository, rows, None, reason)?;
    Ok(changed)
}
