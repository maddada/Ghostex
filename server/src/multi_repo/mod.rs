//! Projects that span several git repositories: a folder of repos, a `.code-workspace` file's
//! folders, or a repo with checked-out submodules, and the session folders a worktree session of
//! such a project works in.
//!
//! CDXC:MultiRepo 2026-11-27 DECISION:
//! User (Q5267 to Q5270, from the "Projects that span several repos" page): one project can cover
//! a folder of repos, a `.code-workspace` file and a repo with submodules (not repos added by
//! hand); a worktree session gets ONE session folder next to the project folder
//! (`shop-ab12cd34`) with a worktree of each picked repo inside, all on one branch name; Changes
//! lists one stream grouped by repo and one Commit makes a commit in each repo with changes; the
//! phone does the same.
//!
//! What a project covers is found on disk ([`discover`]), never stored: a project row keeps its
//! one folder, and presentation publishes `repos` beside it only when the folder holds several
//! repos, so a plain single-repo project, and every older phone and CLI, sees nothing new. A
//! session folder carries a small record ([`record`]) naming the repos it holds and the branch,
//! which every later step (Changes, Commit, diff counts, Rename, Delete, Merge, the agent's
//! note) reads instead of guessing.
//!
//! Plain files: inside a session folder, everything in the project folder that is not a picked
//! repo is a symbolic link to the original (unpicked repos too), so nothing is copied and those
//! files stay shared with the project folder and other sessions. Folders on the way to a picked
//! repo are real folders, so the worktree can sit at the same relative place.
//! SEE-ALSO: server/src/multi_repo/session_folder.rs (making and removing one),
//! server/src/typed_operations/git_changes.rs (Changes across repos).

pub mod cache;
pub mod changes;
pub mod discover;
pub mod git;
pub mod note;
pub mod record;
pub mod session_folder;

use serde::{Deserialize, Serialize};

/// How a repo became part of a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepoKind {
    /// The project folder itself is the repo.
    Root,
    /// A repo inside a project folder that is not one (one level down, or two inside a plain
    /// folder).
    Folder,
    /// A folder a `.code-workspace` file in the project folder lists.
    Workspace,
    /// A checked-out submodule of the project's own repo.
    Submodule,
}

impl RepoKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Folder => "folder",
            Self::Workspace => "workspace",
            Self::Submodule => "submodule",
        }
    }
}

/// One repo of a set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoEntry {
    /// What the user sees: the path below the set's folder (`api`, `packages/ui`), the folder's
    /// own name for the root repo, or the last part for a workspace folder outside it.
    pub name: String,
    /// The repo's place relative to the set's folder, `/`-separated: empty for the folder
    /// itself, `../billing` for a workspace folder beside it.
    pub rel: String,
    /// Where it is on disk.
    pub path: String,
    pub kind: RepoKind,
}

/// The repos one folder covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoSet {
    /// The folder asked about.
    pub root: String,
    pub root_is_repo: bool,
    pub repos: Vec<RepoEntry>,
    /// Set when the folder is (inside) a session folder Ghostex made.
    pub session: Option<SessionFolder>,
}

/// A session folder a folder sits in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFolder {
    /// The session folder itself (`shop-ab12cd34`).
    pub root: String,
    pub record: record::SessionRecord,
}

impl RepoSet {
    /// More than one repo, or a folder that is not a repo holding at least one: the projects
    /// Ghostex treats differently from today's single repo.
    pub fn is_multi(&self) -> bool {
        if self.root_is_repo {
            self.repos.len() >= 2
        } else {
            !self.repos.is_empty()
        }
    }

    /// The repo a path below the set's folder belongs to (the deepest one holding it), with the
    /// rest of the path inside that repo.
    pub fn repo_for_rel_path<'s, 'p>(
        &'s self,
        rel_path: &'p str,
    ) -> Option<(&'s RepoEntry, &'p str)> {
        self.repos
            .iter()
            .filter_map(|repo| {
                if repo.rel.is_empty() {
                    return Some((repo, rel_path));
                }
                let rest = rel_path.strip_prefix(repo.rel.as_str())?;
                if rest.is_empty() {
                    return Some((repo, rest));
                }
                rest.strip_prefix('/').map(|rest| (repo, rest))
            })
            .max_by_key(|(repo, _)| repo.rel.len())
    }
}

/// The repos `path` covers when it covers several, else `None` (a plain repo, a monorepo
/// subfolder, or a folder with no repo in it).
pub fn multi_repo_set(path: &str) -> Option<std::sync::Arc<RepoSet>> {
    cache::repo_set(path).filter(|set| set.is_multi())
}

/// `[{name, rel, kind}]` for presentation and the CLI, from the cache only (never the disk).
pub fn published_repos(path: &str) -> Option<serde_json::Value> {
    let set = cache::cached_repo_set(path)?;
    if !set.is_multi() {
        return None;
    }
    Some(serde_json::Value::Array(
        set.repos
            .iter()
            .map(|repo| {
                serde_json::json!({
                    "name": repo.name,
                    "rel": repo.rel,
                    "kind": repo.kind.as_str(),
                })
            })
            .collect(),
    ))
}

/// `/`-separated path of `to` relative to `from`, with `..` steps when `to` is not below it.
pub(crate) fn relative_path(from: &std::path::Path, to: &std::path::Path) -> Option<String> {
    use std::path::Component;
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let common = from
        .iter()
        .zip(to.iter())
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..from.len() {
        parts.push("..".to_string());
    }
    for component in &to[common..] {
        parts.push(component.as_os_str().to_string_lossy().to_string());
    }
    Some(parts.join("/"))
}

/// `base` joined with a `/`-separated relative path that may hold `..` steps, without touching
/// the disk.
pub(crate) fn join_rel(base: &std::path::Path, rel: &str) -> std::path::PathBuf {
    let mut path = base.to_path_buf();
    for part in rel
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        if part == ".." {
            path.pop();
        } else {
            path.push(part);
        }
    }
    path
}
