//! The move's journal: one file per project in `<gxserver state>/folder-moves/`, written durably
//! before each step, so a gxserver that dies part-way finishes or undoes the move at its next start
//! (recovery.rs) instead of leaving the database pointing at a folder that is gone (risk 2).

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::path_match::MovedRoot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    /// Checks passed; the folder has not moved yet.
    Planned,
    /// The folder moved; the database still names the old one.
    Moved,
    /// The database and files name the new folder; git is not repaired yet.
    Recorded,
    /// Git repaired; agent history not carried yet.
    Repaired,
}

impl Phase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Phase::Planned => "planned",
            Phase::Moved => "moved",
            Phase::Recorded => "recorded",
            Phase::Repaired => "repaired",
        }
    }
}

/// A linked worktree of the moved checkout, as `git worktree list` gave it before the move.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalWorktree {
    /// The checkout it belongs to, by its old path.
    pub(crate) checkout: String,
    /// Its folder before the move (inside the moved folder for a nested worktree).
    pub(crate) path: String,
    pub(crate) has_submodules: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Journal {
    pub(crate) version: u32,
    pub(crate) project_id: String,
    pub(crate) old_path: String,
    pub(crate) new_path: String,
    pub(crate) old_spellings: Vec<String>,
    pub(crate) ignore_case: bool,
    /// Only the letter case of the folder's name changes (the same folder on disk).
    pub(crate) case_only: bool,
    /// Every project the move touches: the project, its worktree projects, and projects at
    /// folders inside it.
    pub(crate) project_ids: Vec<String>,
    /// The git checkouts inside the moved folder (its own, and any project's below it) whose
    /// `.git` is a folder, by their old path.
    pub(crate) checkouts: Vec<String>,
    pub(crate) worktrees: Vec<JournalWorktree>,
    /// Claude's folder names for the old path in its `projects/` folders, worked out before the
    /// move (they resolve the path on disk).
    pub(crate) claude_old_keys: Vec<String>,
    /// The agents (`claude`, `codex`) some profile of which trusted the old folder.
    #[serde(default)]
    pub(crate) trusted_by: Vec<String>,
    pub(crate) phase: Phase,
    pub(crate) started_at: String,
    /// What later steps reported, kept so a move finished at startup still says it.
    pub(crate) notes: Vec<String>,
}

impl Journal {
    pub(crate) fn root(&self) -> MovedRoot {
        MovedRoot {
            old_spellings: self.old_spellings.clone(),
            new_path: self.new_path.clone(),
            ignore_case: self.ignore_case,
        }
    }
}

pub(crate) fn journal_file(root_dir: &Path, project_id: &str) -> PathBuf {
    super::registry::moves_dir(root_dir).join(format!("{project_id}.json"))
}

pub(crate) fn read_journal(root_dir: &Path, project_id: &str) -> Option<Journal> {
    let text = std::fs::read_to_string(journal_file(root_dir, project_id)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every unfinished move's journal (recovery at startup).
pub(crate) fn read_all_journals(root_dir: &Path) -> Vec<Journal> {
    let Ok(entries) = std::fs::read_dir(super::registry::moves_dir(root_dir)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.ends_with(".json") && name != "former-paths.json"
        })
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            serde_json::from_str(&text).ok()
        })
        .collect()
}

pub(crate) fn write_journal(root_dir: &Path, journal: &Journal) -> std::io::Result<()> {
    let text = serde_json::to_vec_pretty(journal).map_err(std::io::Error::other)?;
    write_durably(&journal_file(root_dir, &journal.project_id), &text)
}

/// The move's first journal write: refused when a journal for the project is already there, so
/// a second move can never overwrite (and later delete) the journal of one still running.
pub(crate) fn create_journal(root_dir: &Path, journal: &Journal) -> std::io::Result<()> {
    let file = journal_file(root_dir, &journal.project_id);
    let folder = file
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    std::fs::create_dir_all(folder)?;
    let text = serde_json::to_vec_pretty(journal).map_err(std::io::Error::other)?;
    let written = (|| {
        let mut handle = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)?;
        handle.write_all(&text)?;
        handle.sync_all()
    })();
    match written {
        Ok(()) => {
            sync_folder(folder);
            Ok(())
        }
        Err(error) => {
            // A half-written journal of ours goes; another move's file is never touched.
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                let _ = std::fs::remove_file(&file);
            }
            Err(error)
        }
    }
}

pub(crate) fn remove_journal(root_dir: &Path, project_id: &str) {
    let file = journal_file(root_dir, project_id);
    let _ = std::fs::remove_file(&file);
    if let Some(folder) = file.parent() {
        sync_folder(folder);
    }
}

/// Writes `bytes` to a temporary file beside `path`, flushes it to disk, renames it over `path`
/// and flushes the folder, so a crash leaves either the old file or the new one, never half.
pub(crate) fn write_durably(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    std::fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        std::process::id()
    ));
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)?;
    sync_folder(folder);
    Ok(())
}

fn sync_folder(folder: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = std::fs::File::open(folder) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = folder;
}

/// `GX_TEST_FOLDER_MOVE_PAUSE=<phase>`: after the journal reaches that phase the move waits
/// ten minutes, so a test can kill gxserver there and check the startup recovery. Unset in normal
/// use.
pub(crate) fn pause_for_test(phase: Phase) {
    if std::env::var("GX_TEST_FOLDER_MOVE_PAUSE").ok().as_deref() == Some(phase.name()) {
        eprintln!(
            "[gxserver] folder move paused at {} for a test",
            phase.name()
        );
        std::thread::sleep(std::time::Duration::from_secs(600));
    }
}
