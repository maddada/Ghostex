//! Answers read from a file, kept until the file's length or modification time moves.
//!
//! CDXC:ServerDaemon 2026-11-05 WHY:
//! Background passes read the same transcript tails again on every tick to answer the same question (the recovery pass every 30 seconds asked each running Claude session for its pending question and its last API failure, parsing a tail page each time): about a tenth of gxserver's CPU on a Mac with a dozen sessions (measured 2026-11-05, issues-1027/macos-perf), while most transcripts had not changed. An answer is kept against the file's length and modification time, read before the file is, so a kept answer is never older than the stamp it is kept under.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

/// Files whose answers one memo keeps; past this the memo starts over.
const KEPT_FILES: usize = 512;

pub(crate) struct FileMemo<T> {
    entries: Mutex<HashMap<PathBuf, (u64, Option<SystemTime>, T)>>,
}

impl<T: Clone> FileMemo<T> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The answer `read` gave for `path` while its length and modification time are what they were,
    /// or `read`'s new answer. A file that cannot be stat'ed is read every time.
    pub(crate) fn get_or_read(&self, path: &Path, read: impl FnOnce() -> T) -> T {
        let Ok(metadata) = std::fs::metadata(path) else {
            return read();
        };
        let stamp = (metadata.len(), metadata.modified().ok());
        if let Ok(entries) = self.entries.lock() {
            if let Some((len, modified, answer)) = entries.get(path) {
                if (*len, *modified) == stamp {
                    return answer.clone();
                }
            }
        }
        let answer = read();
        if let Ok(mut entries) = self.entries.lock() {
            if entries.len() >= KEPT_FILES {
                entries.clear();
            }
            entries.insert(path.to_path_buf(), (stamp.0, stamp.1, answer.clone()));
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn an_answer_is_kept_until_the_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, "one\n").unwrap();
        let memo = FileMemo::new();
        let reads = Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            std::fs::read_to_string(&path).unwrap()
        };
        assert_eq!(memo.get_or_read(&path, read), "one\n");
        assert_eq!(memo.get_or_read(&path, read), "one\n");
        assert_eq!(reads.get(), 1);
        std::fs::write(&path, "one\ntwo\n").unwrap();
        assert_eq!(memo.get_or_read(&path, read), "one\ntwo\n");
        assert_eq!(reads.get(), 2);
        // A missing file is never kept.
        let gone = dir.path().join("gone");
        assert_eq!(memo.get_or_read(&gone, || "x".to_string()), "x");
        assert_eq!(memo.get_or_read(&gone, || "y".to_string()), "y");
    }
}
