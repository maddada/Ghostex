//! Deleting files past an age limit, and the counts each rule reports.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

pub(crate) const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// What one rule deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub files: u64,
    pub bytes: u64,
}

impl Tally {
    pub(crate) fn add(&mut self, other: Tally) {
        self.files += other.files;
        self.bytes += other.bytes;
    }
}

/// Which time an age limit is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Since {
    /// Last written.
    Modified,
    /// Last read (gxserver refreshes it on the files it reads, see `codex_rollout_archive.rs`).
    Accessed,
}

/// True when `metadata`'s time is more than `max_age` before `now`. A time the system cannot
/// report, or one in the future, is never old.
pub(crate) fn is_older(
    metadata: &fs::Metadata,
    since: Since,
    max_age: Duration,
    now: SystemTime,
) -> bool {
    let time = match since {
        Since::Modified => metadata.modified(),
        Since::Accessed => metadata.accessed(),
    };
    time.ok()
        .and_then(|time| now.duration_since(time).ok())
        .is_some_and(|age| age > max_age)
}

/// Deletes one file, counting it when the delete succeeded.
pub(crate) fn remove_file(path: &Path, metadata: &fs::Metadata, tally: &mut Tally) {
    if fs::remove_file(path).is_ok() {
        tally.files += 1;
        tally.bytes += metadata.len();
    }
}

/// Deletes every file under `dir` (at any depth) older than `max_age`, then the folders that
/// leaves empty. `dir` itself is kept. Symbolic links are neither followed nor deleted.
pub(crate) fn prune_tree(dir: &Path, since: Since, max_age: Duration, now: SystemTime) -> Tally {
    let mut tally = Tally::default();
    prune_tree_into(dir, since, max_age, now, &mut tally);
    tally
}

fn prune_tree_into(
    dir: &Path,
    since: Since,
    max_age: Duration,
    now: SystemTime,
    tally: &mut Tally,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            prune_tree_into(&path, since, max_age, now, tally);
            // Fails, as it should, while anything is left inside.
            let _ = fs::remove_dir(&path);
        } else if metadata.is_file() && is_older(&metadata, since, max_age, now) {
            remove_file(&path, &metadata, tally);
        }
    }
}

/// Deletes the files directly inside `dir` older than `max_age`; folders are left alone.
pub(crate) fn prune_files_in(dir: &Path, max_age: Duration, now: SystemTime) -> Tally {
    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return tally;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() && is_older(&metadata, Since::Modified, max_age, now) {
            remove_file(&path, &metadata, &mut tally);
        }
    }
    tally
}
