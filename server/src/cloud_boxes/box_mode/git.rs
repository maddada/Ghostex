//! Running `git` on the box for prepare-session and work-status.

use std::path::Path;
use std::process::{Command, Stdio};

/// `git -C <dir> <args…>`; stdout on success, the trimmed stderr as the error.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        // A box has nobody to type a password; an unauthenticated fetch must fail, not wait.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("Can't run git ({error}). Is git installed on the box?"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or_default(),
            stderr.trim()
        ))
    }
}

pub(crate) fn is_repository(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--git-dir"]).is_ok()
}

/// The branch `origin/HEAD` points at, else the current branch.
pub(crate) fn default_branch(dir: &Path) -> Option<String> {
    git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .and_then(|name| name.strip_prefix("origin/").map(str::to_string))
    .or_else(|| {
        git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|name| name != "HEAD")
    })
}

pub(crate) fn has_origin(dir: &Path) -> bool {
    git(dir, &["remote", "get-url", "origin"]).is_ok()
}

/// A branch name usable as a folder name.
pub(crate) fn folder_name(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
