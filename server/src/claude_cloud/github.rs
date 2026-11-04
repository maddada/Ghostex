//! A cloud session's branch and pull request on GitHub: the branch a copy or a Work page ticket
//! names in the task, and what `gh` says about it.

use serde_json::Value;

use crate::session_git_status::run_gh_command;

/// The task's line about the branch, added unless the task already names it.
///
/// CDXC:ClaudeCloud 2026-10-26 WHY: a new cloud session is no longer told a branch Ghostex made up: claude.ai checks out a branch of its own before the first turn, so the made-up one made Claude stop and point out the mismatch (the SPX-123 session), and claude.ai's sessions API now reports the real branch (remote.rs). Only a branch that must be used (a copy carrying on its original's branch, a ticket's branch) is named.
pub(crate) fn task_with_branch(task: &str, branch: &str) -> String {
    if task.contains(branch) {
        return task.to_string();
    }
    format!(
        "{}\n\nCreate the branch `{branch}` from the default branch, work on it and push your commits there.",
        task.trim_end()
    )
}

/// What GitHub shows for a cloud session's branch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BranchReport {
    /// The branch's head commit, when the branch exists on GitHub.
    pub(crate) head: Option<String>,
    /// Commits on the branch that the default branch does not have.
    pub(crate) ahead_by: Option<u64>,
    /// The newest pull request from the branch: number, URL and state (`OPEN`, `MERGED`, `CLOSED`).
    pub(crate) pull_request: Option<(u64, String, String)>,
}

/// Reads the branch and its pull request with `gh` (blocking). `None` when `gh` is missing, signed
/// out or GitHub did not answer, so the caller keeps what it had.
pub(crate) fn read_branch(repo: &str, branch: &str) -> Option<BranchReport> {
    let head = run_gh_command(
        None,
        &[
            "api",
            &format!("repos/{repo}/branches/{branch}"),
            "--jq",
            ".commit.sha",
        ],
    )
    .map(|sha| sha.trim().to_string())
    .filter(|sha| sha.len() >= 7);
    let pulls = run_gh_command(
        None,
        &[
            "pr",
            "list",
            "--repo",
            repo,
            "--head",
            branch,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,url,state",
        ],
    )?;
    let pull_request = serde_json::from_str::<Value>(pulls.trim())
        .ok()
        .and_then(|value| value.as_array()?.first().cloned())
        .and_then(|pull| {
            Some((
                pull.get("number")?.as_u64()?,
                pull.get("url")?.as_str()?.to_string(),
                pull.get("state")?.as_str()?.to_string(),
            ))
        });
    let ahead_by = head.as_ref().and_then(|_| {
        let default_branch = run_gh_command(
            None,
            &["api", &format!("repos/{repo}"), "--jq", ".default_branch"],
        )?;
        run_gh_command(
            None,
            &[
                "api",
                &format!("repos/{repo}/compare/{}...{branch}", default_branch.trim()),
                "--jq",
                ".ahead_by",
            ],
        )?
        .trim()
        .parse()
        .ok()
    });
    Some(BranchReport {
        head,
        ahead_by,
        pull_request,
    })
}

/// The row's status line for a report, or `None` while the branch has no work yet.
pub(crate) fn status_line(branch: &str, report: &BranchReport) -> Option<String> {
    if let Some((number, _, state)) = &report.pull_request {
        return Some(match state.as_str() {
            "MERGED" => format!("Pull request #{number} merged"),
            "CLOSED" => format!("Pull request #{number} closed"),
            _ => format!("Pull request #{number} opened"),
        });
    }
    report.head.as_ref()?;
    Some(match report.ahead_by {
        Some(1) => format!("Pushed 1 commit to {branch}"),
        Some(count) if count > 1 => format!("Pushed {count} commits to {branch}"),
        _ => format!("Pushed to {branch}"),
    })
}
