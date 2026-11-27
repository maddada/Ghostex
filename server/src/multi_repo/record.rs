//! The record a session folder carries: which repos it holds, where each came from, the branch
//! they share and where the project folder sits inside it.
//!
//! Kept where no repo sees it: in the session folder itself (`.ghostex-worktree.json`) when that
//! folder is not a repo, else in the private git folder of its worktree
//! (`.git/worktrees/<name>/ghostex-worktree.json`), which git never lists, commits or shares and
//! removes with the worktree.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::discover::{git_mark, linked_git_dir, GitMark};
use super::RepoKind;

pub const RECORD_FILE: &str = ".ghostex-worktree.json";
const GIT_DIR_RECORD_FILE: &str = "ghostex-worktree.json";
const RECORD_KIND: &str = "ghostexMultiRepoWorktree";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordRepo {
    pub name: String,
    /// Its place inside the session folder, `/`-separated (empty for the session folder itself).
    pub rel: String,
    /// The repo it is a worktree of.
    pub source: String,
    pub kind: RepoKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub kind: String,
    pub version: u32,
    /// The project folder the session folder was laid out from.
    pub project_path: String,
    /// Where the project folder sits inside the session folder (empty when it is the session
    /// folder itself).
    pub project_rel: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub repos: Vec<RecordRepo>,
    pub created_at: String,
}

impl SessionRecord {
    pub fn new(
        project_path: String,
        project_rel: String,
        branch: String,
        base_branch: Option<String>,
        repos: Vec<RecordRepo>,
    ) -> Self {
        Self {
            kind: RECORD_KIND.to_string(),
            version: 1,
            project_path,
            project_rel,
            branch,
            base_branch,
            repos,
            created_at: crate::domain::now_iso(),
        }
    }
}

/// Where the record of the session folder `root` lives.
pub fn record_path(root: &Path) -> PathBuf {
    match git_mark(root) {
        GitMark::None => root.join(RECORD_FILE),
        _ => linked_git_dir(root)
            .map(|git_dir| git_dir.join(GIT_DIR_RECORD_FILE))
            .unwrap_or_else(|| root.join(".git").join(GIT_DIR_RECORD_FILE)),
    }
}

pub fn read(root: &Path) -> Option<SessionRecord> {
    let text = fs::read_to_string(record_path(root)).ok()?;
    let record: SessionRecord = serde_json::from_str(&text).ok()?;
    (record.kind == RECORD_KIND).then_some(record)
}

pub fn write(root: &Path, record: &SessionRecord) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    let path = record_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

/// The session folder `folder` is, or sits in (up to 8 levels up), with its record.
pub fn find(folder: &Path) -> Option<(PathBuf, SessionRecord)> {
    for candidate in folder.ancestors().take(8) {
        if fs::symlink_metadata(candidate.join(RECORD_FILE)).is_ok() {
            if let Some(record) = read(candidate) {
                return Some((candidate.to_path_buf(), record));
            }
        }
        if let Some(git_dir) = linked_git_dir(candidate) {
            if git_dir.join(GIT_DIR_RECORD_FILE).is_file() {
                if let Some(record) = read(candidate) {
                    return Some((candidate.to_path_buf(), record));
                }
            }
        }
    }
    None
}
