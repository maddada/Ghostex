//! Safety copies taken before a one-off database or settings migration.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use super::files::{is_older, prune_tree, remove_file, Since, Tally, DAY};

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: a backup taken before a migration is kept for 30 days. They sit in `<gxserver state>/backups`, `<state>/backups`, or beside the database as `state.db.bak-*` / `state.db.before-*`.
const BACKUP_MAX_AGE: Duration = Duration::from_secs(30 * DAY.as_secs());

const DATABASE_BACKUP_PREFIXES: &[&str] = &["state.db.bak", "state.db.before-"];

pub(crate) fn prune_backups(
    gxserver_state_dir: &Path,
    app_state_dir: &Path,
    now: SystemTime,
) -> Tally {
    let mut tally = prune_tree(
        &gxserver_state_dir.join("backups"),
        Since::Modified,
        BACKUP_MAX_AGE,
        now,
    );
    tally.add(prune_tree(
        &app_state_dir.join("backups"),
        Since::Modified,
        BACKUP_MAX_AGE,
        now,
    ));
    let Ok(entries) = fs::read_dir(gxserver_state_dir) else {
        return tally;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !DATABASE_BACKUP_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let path = entry.path();
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.is_file() && is_older(&metadata, Since::Modified, BACKUP_MAX_AGE, now) {
                remove_file(&path, &metadata, &mut tally);
            }
        }
    }
    tally
}
