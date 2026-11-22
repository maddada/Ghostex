//! The shared logs folder: log files nothing writes any more, and crash reports.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use super::files::{prune_files_in, prune_tree, Since, Tally, DAY};

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: a log file nothing has written for 14 days is deleted, and crash reports are kept for 30 days. Logs still being written are capped by their writers (rotation in `support_logs.rs` and `logging/`), so this only removes files left behind by older builds and areas that were turned off.
const IDLE_LOG_MAX_AGE: Duration = Duration::from_secs(14 * DAY.as_secs());
const CRASH_REPORT_MAX_AGE: Duration = Duration::from_secs(30 * DAY.as_secs());

pub(crate) fn prune_logs(logs_dir: &Path, now: SystemTime) -> Tally {
    let mut tally = prune_files_in(logs_dir, IDLE_LOG_MAX_AGE, now);
    tally.add(prune_tree(
        &logs_dir.join("crashes"),
        Since::Modified,
        CRASH_REPORT_MAX_AGE,
        now,
    ));
    tally.add(prune_tree(
        &logs_dir.join("gxserver"),
        Since::Modified,
        IDLE_LOG_MAX_AGE,
        now,
    ));
    tally
}
