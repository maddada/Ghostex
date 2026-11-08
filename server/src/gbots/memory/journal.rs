//! The memory journal, `<app data>/gbots/journal/<botId>.ndjson`: one line per change to a bot's
//! memory files, with who made it and the text before and after, so any change can be undone. It
//! sits outside the bot folder so the bot can't edit its own history.
//!
//! An undo is itself a change (`undoes` names the one it reverses). "Undo last change" reverses
//! the newest change not yet undone, so pressing it again steps further back. A change is undone
//! only while its files still read as it left them; a later edit (by anyone, through Ghostex or
//! not) makes undoing it refused instead of losing the newer text.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::gbots::folder::gbots_dir;
use crate::paths::GxserverPaths;

/// The journal keeps about this many changes; older ones drop off (they can no longer be undone).
const KEEP: usize = 200;
/// Rewritten down to [`KEEP`] once it grows past this.
const TRIM_FROM_BYTES: u64 = 4 * 1024 * 1024;

/// One file a change touched, relative to the bot folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileChange {
    pub file: String,
    /// The whole text before (`None`: the file did not exist) and after. Absent for an append.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Text added to the end of the file (the archive), instead of before and after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appended: Option<String>,
    /// Text taken off the end of the file (an append undone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trimmed: Option<String>,
}

impl FileChange {
    pub fn replaced(file: &str, before: Option<String>, after: String) -> Self {
        Self {
            file: file.to_string(),
            before: Some(before),
            after: Some(after),
            appended: None,
            trimmed: None,
        }
    }

    pub fn appended(file: &str, text: String) -> Self {
        Self {
            file: file.to_string(),
            before: None,
            after: None,
            appended: Some(text),
            trimmed: None,
        }
    }

    /// The change that reverses this one.
    fn reversed(&self) -> Self {
        Self {
            file: self.file.clone(),
            before: self.after.clone().map(Some),
            after: self.before.clone().map(Option::unwrap_or_default),
            appended: self.trimmed.clone(),
            trimmed: self.appended.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalEntry {
    pub id: String,
    pub at: String,
    /// `user`, `chief`, `self` (the bot itself) or `agent`.
    pub by: String,
    /// What happened, in a few words: "Added a note", "Edited MEMORY.md".
    pub what: String,
    pub files: Vec<FileChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undoes: Option<String>,
}

pub(crate) fn journal_path(paths: &GxserverPaths, bot_id: &str) -> PathBuf {
    gbots_dir(paths)
        .join("journal")
        .join(format!("{bot_id}.ndjson"))
}

/// Every change, oldest first.
pub(crate) fn read(paths: &GxserverPaths, bot_id: &str) -> Vec<JournalEntry> {
    std::fs::read_to_string(journal_path(paths, bot_id))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn next_id(entries: &[JournalEntry]) -> String {
    let last = entries
        .iter()
        .filter_map(|entry| entry.id.strip_prefix('m')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("m{}", last + 1)
}

/// Adds a change and answers it with its id.
pub(crate) fn append(
    paths: &GxserverPaths,
    bot_id: &str,
    by: &str,
    what: &str,
    files: Vec<FileChange>,
    undoes: Option<String>,
) -> std::io::Result<JournalEntry> {
    let path = journal_path(paths, bot_id);
    let mut entries = read(paths, bot_id);
    let entry = JournalEntry {
        id: next_id(&entries),
        at: crate::coordinators::now_iso(),
        by: by.to_string(),
        what: what.to_string(),
        files,
        undoes,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    let too_big = std::fs::metadata(&path)
        .map(|meta| meta.len() > TRIM_FROM_BYTES)
        .unwrap_or(false);
    if too_big {
        entries.push(entry.clone());
        let keep = entries.split_off(entries.len().saturating_sub(KEEP));
        let text = keep
            .iter()
            .filter_map(|entry| serde_json::to_string(entry).ok())
            .collect::<Vec<_>>()
            .join("\n");
        crate::gbots::folder::write_atomic(&path, &format!("{text}\n"))?;
        return Ok(entry);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    writeln!(file, "{line}")?;
    Ok(entry)
}

/// The ids of changes already undone.
fn undone_ids(entries: &[JournalEntry]) -> Vec<&str> {
    entries
        .iter()
        .filter_map(|entry| entry.undoes.as_deref())
        .collect()
}

/// The newest change that is not an undo and not undone yet.
pub(crate) fn last_undoable(entries: &[JournalEntry]) -> Option<&JournalEntry> {
    let undone = undone_ids(entries);
    entries
        .iter()
        .rev()
        .find(|entry| entry.undoes.is_none() && !undone.contains(&entry.id.as_str()))
}

/// Whether `entry`'s files still read as it left them; the first file that does not otherwise.
pub(crate) fn still_as_left(folder: &Path, entry: &JournalEntry) -> Result<(), String> {
    for change in &entry.files {
        let now = std::fs::read_to_string(folder.join(&change.file)).ok();
        let same = match (&change.appended, &change.after) {
            (Some(text), _) => now
                .as_deref()
                .is_some_and(|now| now.ends_with(text.as_str())),
            (None, Some(after)) => now.as_deref().unwrap_or_default() == after,
            _ => true,
        };
        if !same {
            return Err(change.file.clone());
        }
    }
    Ok(())
}

/// The file changes that undo `entry` (to write, then journal with `undoes`).
pub(crate) fn reversal(entry: &JournalEntry) -> Vec<FileChange> {
    entry.files.iter().map(FileChange::reversed).collect()
}

/// Who made a change, as the Memory tab says it.
pub(crate) fn by_label(by: &str, bot_name: &str) -> String {
    match by {
        "user" => "You".to_string(),
        "chief" => "Chief of Staff".to_string(),
        "self" => bot_name.to_string(),
        _ => "Another agent".to_string(),
    }
}

/// The newest `limit` changes for a client, newest first, each saying whether it can be undone now.
pub(crate) fn summaries(
    folder: &Path,
    entries: &[JournalEntry],
    bot_name: &str,
    limit: usize,
) -> Vec<Value> {
    let undone = undone_ids(entries);
    let next = last_undoable(entries).map(|entry| entry.id.clone());
    entries
        .iter()
        .rev()
        .take(limit)
        .map(|entry| {
            let is_next = next.as_deref() == Some(entry.id.as_str());
            let blocked = if is_next {
                still_as_left(folder, entry).err()
            } else {
                None
            };
            json!({
                "id": entry.id,
                "at": entry.at,
                "by": entry.by,
                "byLabel": by_label(&entry.by, bot_name),
                "what": entry.what,
                "files": entry.files.iter().map(|file| file.file.clone()).collect::<Vec<_>>(),
                "undoes": entry.undoes,
                "undone": undone.contains(&entry.id.as_str()),
                "undoable": is_next && blocked.is_none(),
                "changedSinceIn": blocked,
            })
        })
        .collect()
}
