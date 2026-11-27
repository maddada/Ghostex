//! The git runs multi-repo work needs: time-boxed, no terminal prompts, git's own error text kept
//! so a failed step can say why in one line.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::platform::process::{run_bounded, Keep, RunLimits};

/// Local plumbing (worktree add, branch, status, diff, commit).
pub const GIT_TIMEOUT: Duration = Duration::from_secs(60);
/// A commit may run hooks (lint, tests).
pub const COMMIT_TIMEOUT: Duration = Duration::from_secs(300);

/// What one git run did.
#[derive(Clone, Debug)]
pub struct GitRun {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl GitRun {
    /// Git's first meaningful line, for a message that says why (`fatal: ` and `error: ` taken
    /// off), or `fallback`.
    pub fn reason(&self, fallback: &str) -> String {
        let text = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("hint:"))
            .collect();
        // Git's `fatal:` or `error:` line says why; a `warning:` before it only adds detail.
        lines
            .iter()
            .find(|line| line.starts_with("fatal: ") || line.starts_with("error: "))
            .or_else(|| lines.first())
            .map(|line| {
                line.trim_start_matches("fatal: ")
                    .trim_start_matches("error: ")
                    .to_string()
            })
            .unwrap_or_else(|| fallback.to_string())
    }
}

/// Runs `git <args>` in `cwd`.
pub fn run(cwd: &Path, args: &[&str], timeout: Duration) -> GitRun {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    match run_bounded(
        command,
        RunLimits {
            timeout,
            stdin: None,
            stdout: Keep::Head(8 * 1024 * 1024),
            stderr: Keep::Tail(16 * 1024),
        },
    ) {
        Ok(output) => GitRun {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(error) => GitRun {
            ok: false,
            stdout: String::new(),
            stderr: format!("git did not finish: {error:?}"),
        },
    }
}

/// Runs `git <args>` and gives its trimmed stdout when it succeeded.
pub fn read(cwd: &Path, args: &[&str]) -> Option<String> {
    let run = run(cwd, args, GIT_TIMEOUT);
    run.ok.then(|| run.stdout.trim().to_string())
}

/// Whether `refs/heads/<branch>` exists, or a branch it would nest in or under does (git keeps
/// branches as files, so `a/b` and `a` cannot both exist).
pub fn branch_taken(repo: &Path, branch: &str) -> bool {
    let Some(list) = read(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    ) else {
        return false;
    };
    list.lines().map(str::trim).any(|existing| {
        existing == branch
            || existing.starts_with(&format!("{branch}/"))
            || branch.starts_with(&format!("{existing}/"))
    })
}

/// The branch checked out in `worktree`, `None` when detached.
pub fn current_branch(worktree: &Path) -> Option<String> {
    read(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"]).filter(|name| !name.is_empty())
}

/// Whether `rev` names a commit in `repo`.
pub fn has_commit(repo: &Path, rev: &str) -> bool {
    run(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
        GIT_TIMEOUT,
    )
    .ok
}

/// The repo's default branch name (origin's HEAD, else origin/main or master, else local main or
/// master, else what is checked out).
pub fn default_branch(repo: &Path) -> Option<String> {
    let path = repo.to_string_lossy().to_string();
    crate::worktree_sessions::resolve_repository_default_branch(&path)
        .map(|branch| branch.name)
        .or_else(|| current_branch(repo))
}

/// Where a branch for `repo` starts: `wanted` when the repo has it (local first, then origin's),
/// else the repo's default branch, else HEAD. Returns the ref to pass to `worktree add`.
pub fn start_ref(repo: &Path, wanted: Option<&str>) -> String {
    let wanted = wanted
        .map(|name| {
            name.trim_start_matches("refs/heads/")
                .trim_start_matches("origin/")
        })
        .filter(|name| !name.is_empty() && *name != "HEAD");
    for name in wanted
        .into_iter()
        .map(str::to_string)
        .chain(default_branch(repo))
    {
        if has_commit(repo, &format!("refs/heads/{name}")) {
            return name;
        }
        if has_commit(repo, &format!("refs/remotes/origin/{name}")) {
            return format!("origin/{name}");
        }
    }
    "HEAD".to_string()
}
