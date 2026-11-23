//! The Changes section: the lines the session's branch changed against the default branch,
//! uncommitted edits included, and the branch's pull request.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q12): Changes counts "the branch against main, uncommitted included", the number the sidebar's git status already keeps (`probe_git_status_with`: `git diff --numstat <merge-base>` against the working tree; on the default branch that is the uncommitted work), with Create PR, View PR, or Commit on the default branch. The probe is the sidebar's own, but its answers stay in this module's cache: the sidebar's cache publishes a change only when its own pass sees one, so writing into it from here would hide that change from Sidebar V2.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Value, json};

use crate::session_git_status::{
    PullRequestState, SessionGitStatusProber, SessionPullRequest, SystemSessionGitStatusProber,
    cached_gh_cli_available, cached_session_git_status, parse_git_numstat, resolve_default_branch,
    run_git_probe_command,
};

use super::async_cache::{AsyncSectionCache, SectionLoad};

/// How long a git answer is reused on platforms without a folder watch (or as a safety cap).
#[cfg(any(windows, target_os = "macos"))]
const GIT_FRESH_FOR: Duration = Duration::from_secs(60);
#[cfg(not(any(windows, target_os = "macos")))]
const GIT_FRESH_FOR: Duration = Duration::from_secs(20);
/// Hard cap for a synchronous (non-progressive) git wait.
const GIT_SYNC_WAIT: Duration = Duration::from_secs(4);
/// How long a pull request answer (a network call) is reused.
const PULL_REQUEST_FRESH_FOR: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug)]
struct GitChangesSnapshot {
    branch: Option<String>,
    default_branch: Option<String>,
    on_default: bool,
    additions: i64,
    deletions: i64,
}

fn git_cache() -> &'static AsyncSectionCache<String, Option<GitChangesSnapshot>> {
    static CACHE: OnceLock<AsyncSectionCache<String, Option<GitChangesSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| AsyncSectionCache::new(64))
}

fn pr_cache() -> &'static AsyncSectionCache<(String, String), Option<SessionPullRequest>> {
    static CACHE: OnceLock<AsyncSectionCache<(String, String), Option<SessionPullRequest>>> =
        OnceLock::new();
    CACHE.get_or_init(|| AsyncSectionCache::new(64))
}

#[cfg(any(windows, target_os = "macos"))]
fn folder_watch_dirty(cwd: &str) -> bool {
    use crate::platform::folder_watch::{ChangeGatedCache, GitFolders};
    static WATCH: OnceLock<ChangeGatedCache<()>> = OnceLock::new();
    let watch = WATCH.get_or_init(ChangeGatedCache::new);
    let folder = Path::new(cwd);
    if watch
        .unchanged(cwd, folder, GitFolders::WithSharedRefs)
        .is_some()
    {
        return false;
    }
    watch.remember(cwd, ());
    true
}

#[cfg(not(any(windows, target_os = "macos")))]
fn folder_watch_dirty(_cwd: &str) -> bool {
    false
}

/// Resolves `(git_dir, common_dir)` for `cwd` by walking ancestors without spawning `git`.
/// Returns `None` if `cwd` is not inside a git work tree.
fn resolve_git_dirs(cwd: &Path) -> Option<(PathBuf, PathBuf)> {
    for dir in cwd.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some((dot_git.clone(), dot_git));
        }
        if dot_git.is_file() {
            let text = fs::read_to_string(&dot_git).ok()?;
            let raw = text.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
            if raw.is_empty() {
                return None;
            }
            let git_dir = if Path::new(raw).is_absolute() {
                PathBuf::from(raw)
            } else {
                dir.join(raw)
            };
            let common_dir = fs::read_to_string(git_dir.join("commondir"))
                .ok()
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .map(|c| {
                    if Path::new(&c).is_absolute() {
                        PathBuf::from(c)
                    } else {
                        git_dir.join(c)
                    }
                })
                .unwrap_or_else(|| git_dir.clone());
            return Some((git_dir, common_dir));
        }
    }
    None
}

/// Reads the current local branch from `.git/HEAD` without spawning `git`.
/// Returns `Some(Some(branch))` on a branch, `Some(None)` on detached HEAD, or `None` if unreadable.
fn read_head_branch_fast(git_dir: &Path) -> Option<Option<String>> {
    let text = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let line = text.lines().next()?.trim();
    if let Some(target) = line.strip_prefix("ref:") {
        let target = target.trim();
        return Some(target.strip_prefix("refs/heads/").map(str::to_string));
    }
    if line.len() >= 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(None);
    }
    None
}

/// Reads the default branch (`(short_name, full_ref)`) directly from `.git` refs when possible,
/// avoiding up to 5 `git` subprocess spawns on the common case.
fn read_default_branch_fast(common_dir: &Path) -> Option<(String, String)> {
    if let Ok(text) = fs::read_to_string(common_dir.join("refs/remotes/origin/HEAD")) {
        if let Some(target) = text
            .lines()
            .next()
            .and_then(|line| line.trim().strip_prefix("ref:"))
            .map(str::trim)
        {
            if let Some(name) = target
                .strip_prefix("refs/remotes/origin/")
                .filter(|name| !name.is_empty())
            {
                return Some((name.to_string(), target.to_string()));
            }
        }
    }
    for name in ["main", "master"] {
        let full = format!("refs/remotes/origin/{name}");
        if common_dir.join(&full).is_file() {
            return Some((name.to_string(), full));
        }
    }
    for name in ["main", "master"] {
        let full = format!("refs/heads/{name}");
        if common_dir.join(&full).is_file() {
            return Some((name.to_string(), full));
        }
    }
    None
}

/// Fast local git probe for the Overview panel:
/// - Reads `.git/HEAD` and default branch directly from `.git` (0 spawns on the common path).
/// - Skips submodule working-tree recursion (`--ignore-submodules=dirty`), rename detection
///   (`--no-renames`), external diff drivers (`--no-ext-diff`) and textconv (`--no-textconv`).
/// - Never runs `gh` (pull request lookup runs independently in `pr_cache`).
fn probe_git_fast(cwd: &str) -> Option<GitChangesSnapshot> {
    let cwd_path = Path::new(cwd);
    if !cwd_path.is_dir() {
        return None;
    }
    let (git_dir, common_dir) = resolve_git_dirs(cwd_path)?;
    // A bare repository's folder has no working tree; confirm via git only if HEAD wasn't readable.
    let branch = match read_head_branch_fast(&git_dir) {
        Some(branch) => branch,
        None => {
            let inside = run_git_probe_command(cwd, &["rev-parse", "--is-inside-work-tree"])
                .is_some_and(|value| value.trim() == "true");
            if !inside {
                return None;
            }
            run_git_probe_command(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
                .map(|value| value.trim().to_string())
                .filter(|branch| !branch.is_empty())
        }
    };
    let default = read_default_branch_fast(&common_dir).or_else(|| {
        resolve_default_branch(&|args| run_git_probe_command(cwd, args))
            .map(|branch| (branch.name, branch.git_ref))
    });
    let default_branch = default.as_ref().map(|(name, _)| name.clone());
    let on_default = match (&branch, &default_branch) {
        (Some(branch), Some(default)) => branch == default,
        (Some(branch), None) => matches!(branch.as_str(), "main" | "master"),
        (None, _) => false,
    };
    let base = match (&default, on_default) {
        (Some((_, git_ref)), false) => {
            run_git_probe_command(cwd, &["merge-base", git_ref.as_str(), "HEAD"])
                .map(|value| value.trim().to_string())
                .filter(|base| !base.is_empty())
        }
        _ => None,
    };
    let base = base.unwrap_or_else(|| "HEAD".to_string());
    let numstat = run_git_probe_command(
        cwd,
        &[
            "diff",
            "--numstat",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=dirty",
            base.as_str(),
            "--",
        ],
    )
    .unwrap_or_default();
    let (additions, deletions) = parse_git_numstat(&numstat);
    Some(GitChangesSnapshot {
        branch,
        default_branch,
        on_default,
        additions,
        deletions,
    })
}

fn pull_request_value(pull_request: &SessionPullRequest) -> Value {
    json!({
        "number": pull_request.number,
        "state": pull_request.state.as_wire(),
        "url": pull_request.url,
        "open": matches!(pull_request.state, PullRequestState::Open | PullRequestState::Draft),
    })
}

/// Seeds `git_cache` (as stale so a fresh read still happens in the background) from Sidebar V2's
/// already-probed read-only cache, so opening the Overview panel on a known repo shows Changes in < 1 ms.
fn seed_from_sidebar_cache(cwd: &str) {
    let Some(status) = cached_session_git_status(cwd) else {
        return;
    };
    let default_branch = resolve_git_dirs(Path::new(cwd))
        .and_then(|(_, common)| read_default_branch_fast(&common))
        .map(|(name, _)| name);
    let on_default = match (&status.branch, &default_branch) {
        (Some(branch), Some(default)) => branch == default,
        (Some(branch), None) => matches!(branch.as_str(), "main" | "master"),
        (None, _) => false,
    };
    if let (Some(branch), Some(pr)) = (status.branch.clone(), status.pull_request.clone()) {
        pr_cache().seed_if_empty((cwd.to_string(), branch), Some(pr), Duration::ZERO);
    }
    git_cache().seed_if_empty(
        cwd.to_string(),
        Some(GitChangesSnapshot {
            branch: status.branch,
            default_branch,
            on_default,
            additions: status.additions,
            deletions: status.deletions,
        }),
        GIT_FRESH_FOR,
    );
}

/// Non-blocking pull request lookup: never holds up git diff numbers on `gh auth status` or
/// `gh pr view`. When a PR lookup finishes in the background it bumps the revision so the panel's
/// button updates to "View PR #N" automatically.
fn read_pull_request_nonblocking(cwd: &str, snapshot: &GitChangesSnapshot) -> Option<SessionPullRequest> {
    if snapshot.on_default {
        return None;
    }
    let branch = snapshot.branch.as_deref()?;
    let key = (cwd.to_string(), branch.to_string());
    let cwd_owned = cwd.to_string();
    let branch_owned = branch.to_string();
    match pr_cache().get_or_start(key, PULL_REQUEST_FRESH_FOR, false, move || {
        let prober = SystemSessionGitStatusProber;
        if !cached_gh_cli_available() && !prober.supports_pull_requests() {
            return None;
        }
        prober.probe_pull_request(&cwd_owned, &branch_owned)
    }) {
        SectionLoad::Ready(pr) => pr,
        SectionLoad::Pending => None,
    }
}

fn format_changes(cwd: &str, snapshot: &GitChangesSnapshot) -> Value {
    let pull_request = read_pull_request_nonblocking(cwd, snapshot);
    json!({
        "branch": snapshot.branch,
        "defaultBranch": snapshot.default_branch,
        "onDefaultBranch": snapshot.on_default,
        "additions": snapshot.additions,
        "deletions": snapshot.deletions,
        "pullRequest": pull_request.as_ref().map(pull_request_value),
    })
}

/// Non-blocking progressive Changes lookup for `/api/readSessionDetails { progressive: true }`.
/// Returns `SectionLoad::Ready(Some(value))` when cached (or seeded from Sidebar V2),
/// `SectionLoad::Ready(None)` outside a git repo, or `SectionLoad::Pending` while a deduplicated
/// background git probe runs.
pub(super) fn read_changes_progressive(cwd: &str) -> SectionLoad<Option<Value>> {
    seed_from_sidebar_cache(cwd);
    let dirty = folder_watch_dirty(cwd);
    let cwd_owned = cwd.to_string();
    match git_cache().get_or_start(cwd.to_string(), GIT_FRESH_FOR, dirty, move || {
        probe_git_fast(&cwd_owned)
    }) {
        SectionLoad::Ready(Some(snapshot)) => SectionLoad::Ready(Some(format_changes(cwd, &snapshot))),
        SectionLoad::Ready(None) => SectionLoad::Ready(None),
        SectionLoad::Pending => SectionLoad::Pending,
    }
}

/// `{branch, defaultBranch, additions, deletions, pullRequest}` for `cwd`, or `None` outside a git
/// checkout. Cached with folder-watch invalidation + stale-while-revalidate and time-boxed; never
/// blocks on `gh pr view`.
pub(crate) fn read_changes(cwd: &str) -> Option<Value> {
    seed_from_sidebar_cache(cwd);
    let dirty = folder_watch_dirty(cwd);
    let cwd_owned = cwd.to_string();
    let snapshot = git_cache().get_or_wait(
        cwd.to_string(),
        GIT_FRESH_FOR,
        dirty,
        GIT_SYNC_WAIT,
        move || probe_git_fast(&cwd_owned),
    )??;
    Some(format_changes(cwd, &snapshot))
}
