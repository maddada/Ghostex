//! Making, renaming, moving and removing a session folder: one folder next to the project folder
//! (`shop-ab12cd34`) holding a worktree of each picked repo at the same relative place, all on one
//! branch, with everything else in the project folder linked to the original.

use std::fs;
use std::path::{Path, PathBuf};

use super::git::{self, GIT_TIMEOUT};
use super::record::{self, RecordRepo, SessionRecord};
use super::{join_rel, relative_path, RepoEntry, RepoKind, RepoSet};

/// A new worktree session picks every repo by default up to this many; past it, only the
/// project folder's own repo unless the caller picks.
pub const DEFAULT_PICK_MAX: usize = 8;

/// What a session folder needs before anything is made.
pub struct Plan {
    pub project_path: PathBuf,
    pub repos: Vec<RepoEntry>,
    /// The folder that stands in for `anchor`.
    pub anchor: PathBuf,
}

/// A made session folder.
#[derive(Clone, Debug)]
pub struct Made {
    pub root: PathBuf,
    /// Where the project folder sits in it: the session's working folder.
    pub project_dir: PathBuf,
    pub branch: String,
    pub record: SessionRecord,
}

/// The repos a new session gets: the project folder's own repo always, then `picked` (repo
/// `rel`s or paths) when given, else every repo up to [`DEFAULT_PICK_MAX`].
pub fn plan(project_path: &Path, set: &RepoSet, picked: Option<&[String]>) -> Result<Plan, String> {
    let wanted = |repo: &RepoEntry| match picked {
        Some(picked) => picked
            .iter()
            .any(|pick| pick == &repo.rel || pick == &repo.path || pick == &repo.name),
        None => set.repos.len() <= DEFAULT_PICK_MAX,
    };
    let mut repos: Vec<RepoEntry> = set
        .repos
        .iter()
        .filter(|repo| repo.kind == RepoKind::Root || wanted(repo))
        // A repo with no commit has nothing to branch from.
        .filter(|repo| git::has_commit(Path::new(&repo.path), "HEAD"))
        .cloned()
        .collect();
    if repos.is_empty() {
        return Err("Pick at least one repo with a commit for the new worktree.".to_string());
    }
    let mut anchor = project_path.to_path_buf();
    for repo in &repos {
        let path = Path::new(&repo.path);
        while !path.starts_with(&anchor) {
            if !anchor.pop() {
                return Err(format!(
                    "{} shares no folder with the project, so it cannot sit in one worktree folder.",
                    repo.name
                ));
            }
        }
    }
    // Shallowest first, so a repo's worktree exists before a submodule inside it.
    repos.sort_by_key(|repo| Path::new(&repo.path).components().count());
    Ok(Plan {
        project_path: project_path.to_path_buf(),
        repos,
        anchor,
    })
}

/// Where the session folder for `suffix` goes: beside the project folder, named after it.
pub fn folder_for(project_path: &Path, suffix: &str) -> PathBuf {
    let name = project_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "worktree".to_string());
    project_path
        .parent()
        .unwrap_or_else(|| Path::new("/"))
        .join(format!("{name}-{suffix}"))
}

/// Whether `branch` is free in every repo of the plan and `folder` is free on disk.
pub fn is_free(plan: &Plan, branch: &str, folder: &Path) -> bool {
    !folder.exists()
        && plan
            .repos
            .iter()
            .all(|repo| !git::branch_taken(Path::new(&repo.path), branch))
}

/// What a failed make takes back.
struct Undo {
    worktrees: Vec<(PathBuf, PathBuf)>,
    root: PathBuf,
    branch: String,
}

impl Undo {
    fn run(&self) {
        for (source, target) in self.worktrees.iter().rev() {
            let target = target.to_string_lossy().to_string();
            let _ = git::run(
                source,
                &["worktree", "remove", "--force", &target],
                GIT_TIMEOUT,
            );
            let _ = git::run(source, &["worktree", "prune"], GIT_TIMEOUT);
            let _ = git::run(source, &["branch", "-D", &self.branch], GIT_TIMEOUT);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Makes the session folder `root` for `plan` on `branch`, starting each repo from `base` when it
/// has it, else from its own default branch (a submodule from the commit its parent's base
/// records). Everything made is taken back when a step fails.
pub fn make(plan: &Plan, root: &Path, branch: &str, base: Option<&str>) -> Result<Made, String> {
    if root.exists() {
        return Err("The worktree folder already exists.".to_string());
    }
    let mut undo = Undo {
        worktrees: Vec::new(),
        root: root.to_path_buf(),
        branch: branch.to_string(),
    };
    let result = make_inner(plan, root, branch, base, &mut undo);
    if result.is_err() {
        undo.run();
    }
    result
}

fn make_inner(
    plan: &Plan,
    root: &Path,
    branch: &str,
    base: Option<&str>,
    undo: &mut Undo,
) -> Result<Made, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not make the worktree folder: {error}"))?;
    let project_rel = relative_path(&plan.anchor, &plan.project_path)
        .filter(|rel| rel != ".")
        .unwrap_or_default();
    let project_dir = join_rel(root, &project_rel);
    let root_repo = plan.repos.iter().find(|repo| repo.kind == RepoKind::Root);
    let mut records = Vec::new();
    for repo in &plan.repos {
        let source = PathBuf::from(&repo.path);
        let rel = relative_path(&plan.anchor, &source)
            .filter(|rel| rel != ".")
            .unwrap_or_default();
        let target = join_rel(root, &rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not make a folder for {}: {error}", repo.name))?;
        }
        if target.exists()
            && fs::read_dir(&target)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
        {
            return Err(format!(
                "{} already has files in the worktree folder.",
                repo.name
            ));
        }
        let start = match (repo.kind, root_repo) {
            (RepoKind::Submodule, Some(parent)) => {
                // The commit the parent's starting point records for this submodule.
                let parent_path = Path::new(&parent.path);
                let parent_start = git::start_ref(parent_path, base);
                let sub_rel = relative_path(parent_path, &source).unwrap_or_default();
                git::read(
                    parent_path,
                    &["rev-parse", &format!("{parent_start}:{sub_rel}")],
                )
                .filter(|sha| git::has_commit(&source, sha))
                .unwrap_or_else(|| "HEAD".to_string())
            }
            _ => git::start_ref(&source, base),
        };
        let target_text = target.to_string_lossy().to_string();
        let add = git::run(
            &source,
            &["worktree", "add", "-b", branch, &target_text, &start],
            GIT_TIMEOUT,
        );
        if !add.ok {
            return Err(format!(
                "{}: {}",
                repo.name,
                add.reason("git worktree add failed.")
            ));
        }
        undo.worktrees.push((source.clone(), target.clone()));
        records.push(RecordRepo {
            name: repo.name.clone(),
            rel,
            source: repo.path.clone(),
            kind: repo.kind,
        });
    }
    if root_repo.is_none() {
        let picked: Vec<PathBuf> = plan
            .repos
            .iter()
            .map(|repo| PathBuf::from(&repo.path))
            .collect();
        link_plain(&plan.project_path, &project_dir, &picked)
            .map_err(|error| format!("Could not link the project's files: {error}"))?;
    }
    let record = SessionRecord::new(
        plan.project_path.to_string_lossy().to_string(),
        project_rel,
        branch.to_string(),
        base.map(str::to_string),
        records,
    );
    record::write(root, &record)
        .map_err(|error| format!("Could not write the worktree folder's record: {error}"))?;
    super::cache::forget(&project_dir.to_string_lossy());
    Ok(Made {
        root: root.to_path_buf(),
        project_dir,
        branch: branch.to_string(),
        record,
    })
}

/// Fills `target` with links to `source`'s entries, except picked repos (their worktrees are
/// there already) and the folders that lead to one, which become real folders filled the same
/// way.
fn link_plain(source: &Path, target: &Path, picked: &[PathBuf]) -> std::io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = target.join(&name);
        if picked.iter().any(|repo| repo == &from) {
            continue;
        }
        let leads_to_repo = picked
            .iter()
            .any(|repo| repo.starts_with(&from) && repo != &from);
        if leads_to_repo {
            link_plain(&from, &to, picked)?;
        } else if fs::symlink_metadata(&to).is_err() {
            symlink(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

#[cfg(windows)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::os::windows::fs::symlink_dir(from, to)
    } else {
        std::os::windows::fs::symlink_file(from, to)
    }
}

/// One repo's state in a session folder, for Delete Worktree and the hidden-work check.
#[derive(Clone, Debug)]
pub struct RepoState {
    pub name: String,
    pub path: PathBuf,
    pub source: PathBuf,
    pub uncommitted: usize,
    pub exists: bool,
}

pub fn repo_states(root: &Path, record: &SessionRecord) -> Vec<RepoState> {
    record
        .repos
        .iter()
        .map(|repo| {
            let path = join_rel(root, &repo.rel);
            let exists = path.is_dir();
            let uncommitted = if exists {
                git::read(
                    &path,
                    &["status", "--porcelain", "--ignore-submodules=dirty"],
                )
                .map(|status| {
                    status
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .count()
                })
                .unwrap_or(0)
            } else {
                0
            };
            RepoState {
                name: repo.name.clone(),
                path,
                source: PathBuf::from(&repo.source),
                uncommitted,
                exists,
            }
        })
        .collect()
}

/// Removes the session folder `root`: every repo's worktree (deepest first), each repo's branch
/// when `delete_branches`, then the links and the folder. `force` removes worktrees with
/// uncommitted files and the files made outside the repos; without it a worktree with work stops
/// the removal and says which repo.
pub fn remove(root: &Path, delete_branches: bool, force: bool) -> Result<(), String> {
    let record = record::read(root).ok_or("This worktree folder has no Ghostex record.")?;
    let mut repos = record.repos.clone();
    repos.sort_by_key(|repo| std::cmp::Reverse(repo.rel.split('/').count()));
    for repo in &repos {
        let source = PathBuf::from(&repo.source);
        let target = join_rel(root, &repo.rel);
        if target.exists() {
            let target_text = target.to_string_lossy().to_string();
            let mut args = vec!["worktree", "remove"];
            if force {
                args.push("--force");
            }
            args.push(&target_text);
            let removed = git::run(&source, &args, GIT_TIMEOUT);
            if !removed.ok {
                return Err(format!(
                    "{}: {}",
                    repo.name,
                    removed.reason("git worktree remove failed.")
                ));
            }
        }
        let _ = git::run(&source, &["worktree", "prune"], GIT_TIMEOUT);
        if delete_branches {
            let _ = git::run(&source, &["branch", "-D", &record.branch], GIT_TIMEOUT);
        }
    }
    if root.exists() {
        if force {
            fs::remove_dir_all(root)
                .map_err(|error| format!("Could not remove the worktree folder: {error}"))?;
        } else {
            remove_links(root);
            let _ = fs::remove_file(root.join(record::RECORD_FILE));
            remove_empty_dirs(root);
            if root.exists() {
                return Err(
                    "Files made outside the repos are still in the worktree folder.".to_string(),
                );
            }
        }
    }
    Ok(())
}

fn remove_links(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let _ = fs::remove_file(&path);
            }
            Ok(meta) if meta.is_dir() => remove_links(&path),
            _ => {}
        }
    }
}

fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                remove_empty_dirs(&path);
            }
        }
    }
    let _ = fs::remove_dir(dir);
}

/// Renames the shared branch in every repo; a repo that refuses undoes the ones already renamed.
pub fn rename_branch(root: &Path, new_branch: &str) -> Result<SessionRecord, String> {
    let mut record = record::read(root).ok_or("This worktree folder has no Ghostex record.")?;
    let old = record.branch.clone();
    if old == new_branch {
        return Ok(record);
    }
    for repo in &record.repos {
        if git::branch_taken(Path::new(&repo.source), new_branch) {
            return Err(format!(
                "{}: a branch named {new_branch} already exists.",
                repo.name
            ));
        }
    }
    let mut done: Vec<PathBuf> = Vec::new();
    for repo in &record.repos {
        let target = join_rel(root, &repo.rel);
        let renamed = git::run(&target, &["branch", "-m", &old, new_branch], GIT_TIMEOUT);
        if !renamed.ok {
            for path in done.iter().rev() {
                let _ = git::run(path, &["branch", "-m", new_branch, &old], GIT_TIMEOUT);
            }
            return Err(format!(
                "{}: {}",
                repo.name,
                renamed.reason("git branch -m failed.")
            ));
        }
        done.push(target);
    }
    record.branch = new_branch.to_string();
    record::write(root, &record).map_err(|error| error.to_string())?;
    Ok(record)
}

/// Moves the session folder to `new_root` and tells each repo where its worktree went.
pub fn move_folder(root: &Path, new_root: &Path) -> Result<(), String> {
    let record = record::read(root).ok_or("This worktree folder has no Ghostex record.")?;
    if new_root.exists() {
        return Err("A folder with that name already exists.".to_string());
    }
    fs::rename(root, new_root)
        .map_err(|error| format!("Could not move the worktree folder: {error}"))?;
    for repo in &record.repos {
        let target = join_rel(new_root, &repo.rel).to_string_lossy().to_string();
        let _ = git::run(
            Path::new(&repo.source),
            &["worktree", "repair", &target],
            GIT_TIMEOUT,
        );
    }
    Ok(())
}
