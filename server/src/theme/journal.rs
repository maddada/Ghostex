//! The theme history: the last 20 look changes made with `ghostex theme set`, each with when, who
//! (an agent or you) and what changed, and whether it was undone. Kept in `theme-history.json`
//! beside gxserver's state, so it survives a restart.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3116, agent theming page): a bad theme is undone from a history of "the last 20 theme changes": Undo on the "Theme changed" card and `ghostex theme undo`, with redo. Undo puts back what one change replaced and is refused once one of its settings was changed again since, so it never throws away a later change; redo puts the change back while those settings still hold what Undo left.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How many changes the history keeps.
pub(crate) const KEPT_CHANGES: usize = 20;

const FILE_NAME: &str = "theme-history.json";

/// Who made, undid or redid a change.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Who {
    /// `agent` or `you`.
    pub(crate) kind: String,
    /// The agent's id (`claude`, `codex`), for an agent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) agent_id: String,
    /// What the history calls it: the agent's name, or "You".
    pub(crate) name: String,
    /// The session it ran in (`<machine>:<project>:<session>`), for an agent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) session: String,
}

/// One setting a change set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) before: Value,
    pub(crate) after: Value,
    /// `before` and `after` in the words Settings shows them in.
    pub(crate) before_shown: String,
    pub(crate) after_shown: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThemeChange {
    pub(crate) change_id: String,
    pub(crate) at: String,
    pub(crate) by: Who,
    pub(crate) items: Vec<Item>,
    /// What the change made hard to read when it was made (readability.rs).
    #[serde(default)]
    pub(crate) warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) undone_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) undone_by: Option<Who>,
}

#[derive(Default, Serialize, Deserialize)]
struct HistoryFile {
    /// Oldest first.
    #[serde(default)]
    changes: Vec<ThemeChange>,
}

/// One read-change-write of the history at a time in this gxserver.
static LOCK: Mutex<()> = Mutex::new(());

fn path(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE_NAME)
}

fn read(state_dir: &Path) -> HistoryFile {
    std::fs::read(path(state_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(state_dir: &Path, history: &HistoryFile) -> Result<(), String> {
    std::fs::create_dir_all(state_dir)
        .map_err(|error| format!("Could not create {}: {error}", state_dir.display()))?;
    let bytes = serde_json::to_vec_pretty(history)
        .map_err(|error| format!("Could not encode the theme history: {error}"))?;
    let target = path(state_dir);
    let staging = target.with_extension("json.tmp");
    std::fs::write(&staging, bytes)
        .and_then(|()| std::fs::rename(&staging, &target))
        .map_err(|error| format!("Could not save {}: {error}", target.display()))
}

/// Every change kept, newest first.
pub(crate) fn list(state_dir: &Path) -> Vec<ThemeChange> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut changes = read(state_dir).changes;
    changes.reverse();
    changes
}

pub(crate) fn find(state_dir: &Path, change_id: &str) -> Option<ThemeChange> {
    list(state_dir)
        .into_iter()
        .find(|change| change.change_id == change_id.trim())
}

/// Adds `change`, dropping the oldest past [`KEPT_CHANGES`].
pub(crate) fn record(state_dir: &Path, change: ThemeChange) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut history = read(state_dir);
    history.changes.push(change);
    let extra = history.changes.len().saturating_sub(KEPT_CHANGES);
    history.changes.drain(..extra);
    write(state_dir, &history)
}

/// Marks `change_id` undone (`Some(by)`) or redone (`None`).
pub(crate) fn mark(
    state_dir: &Path,
    change_id: &str,
    undone: Option<(String, Who)>,
) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut history = read(state_dir);
    let Some(change) = history
        .changes
        .iter_mut()
        .find(|change| change.change_id == change_id)
    else {
        return Err(format!("There is no theme change {change_id}."));
    };
    match undone {
        Some((at, by)) => {
            change.undone_at = Some(at);
            change.undone_by = Some(by);
        }
        None => {
            change.undone_at = None;
            change.undone_by = None;
        }
    }
    write(state_dir, &history)
}
