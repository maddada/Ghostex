//! Every write to a bot's memory goes through here: one bot's writes wait for each other (several
//! chats of a bot may write at once), the archive is written before `MEMORY.md` so a note is never
//! in neither file, each change is journaled, and the generated `CLAUDE.md` / `AGENTS.md` are
//! rewritten so the next chat starts with it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use rusqlite::Connection;

use super::journal::{self, FileChange, JournalEntry};
use crate::domain::DomainStateError;
use crate::gbots::folder::{bot_folder, write_atomic, MEMORY_FILE};
use crate::gbots::record::Bot;
use crate::paths::GxserverPaths;

static LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The lock a bot's memory writes take, held from reading the files to journaling the change.
pub(crate) fn bot_lock(bot_id: &str) -> Arc<Mutex<()>> {
    let mut locks = LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(bot_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub(crate) fn internal(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("The GBot's memory could not be written: {error}"),
    }
}

pub(crate) fn read_file(folder: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(folder.join(relative)).ok()
}

/// Writes `changes` into the bot folder, appends first.
fn apply(folder: &Path, changes: &[FileChange]) -> std::io::Result<()> {
    let mut ordered: Vec<&FileChange> = changes.iter().collect();
    ordered.sort_by_key(|change| change.appended.is_none());
    for change in ordered {
        let path = folder.join(&change.file);
        let now = std::fs::read_to_string(&path).ok();
        if let Some(text) = &change.appended {
            write_atomic(&path, &format!("{}{text}", now.unwrap_or_default()))?;
        } else if let Some(text) = &change.trimmed {
            let now = now.unwrap_or_default();
            write_atomic(&path, now.strip_suffix(text.as_str()).unwrap_or(&now))?;
        } else if let Some(after) = &change.after {
            // A topic that did not exist before the change it undoes goes away again.
            if after.is_empty() && change.file != MEMORY_FILE {
                let _ = std::fs::remove_file(&path);
            } else {
                write_atomic(&path, after)?;
            }
        }
    }
    Ok(())
}

/// Writes and journals one change to `bot`'s memory, then refreshes its instruction files.
pub(crate) fn commit(
    paths: &GxserverPaths,
    db: &Connection,
    bot: &Bot,
    by: &str,
    what: &str,
    changes: Vec<FileChange>,
    undoes: Option<String>,
) -> Result<JournalEntry, DomainStateError> {
    let folder = bot_folder(paths, &bot.bot_id);
    apply(&folder, &changes).map_err(internal)?;
    let entry = journal::append(paths, &bot.bot_id, by, what, changes, undoes).map_err(internal)?;
    crate::gbots::brief::refresh_instruction_files(paths, db, bot);
    Ok(entry)
}
