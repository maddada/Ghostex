//! A cloud session's branch and pull request on GitHub: the branch a copy or a Work page ticket
//! names in the task, and what `gh` says about it.

use serde_json::Value;

use crate::session_git_status::run_gh_command_any_status;

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

/// The one GraphQL query `read_branch` asks: the branch's head, how far it is ahead of the default
/// branch, and the newest pull request from it.
const BRANCH_QUERY: &str = "query($owner: String!, $name: String!, $branch: String!, $ref: String!) { repository(owner: $owner, name: $name) { ref(qualifiedName: $ref) { target { oid } } defaultBranchRef { compare(headRef: $branch) { aheadBy } } pullRequests(headRefName: $branch, first: 1, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { number url state } } } }";

/// Reads the branch and its pull request with `gh` (blocking). `None` when `gh` is missing, signed
/// out or GitHub did not answer, so the caller keeps what it had.
///
/// CDXC:ClaudeCloud 2026-11-05 WHY:
/// The branch, the newest pull request, the default branch and the comparison were four `gh` starts per cloud session per look (`gh api` twice more, `gh pr list`), about once a minute each; one GraphQL call answers all four with the same values.
pub(crate) fn read_branch(repo: &str, branch: &str) -> Option<BranchReport> {
    let (owner, name) = repo.split_once('/')?;
    let output = run_gh_command_any_status(
        None,
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={BRANCH_QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
            "-f",
            &format!("branch={branch}"),
            "-f",
            &format!("ref=refs/heads/{branch}"),
        ],
    )?;
    parse_branch_report(&output)
}

/// What the GraphQL answer says, in the shape the four REST reads gave. A branch that does not
/// exist yet answers with an error beside its data, which is still a report.
fn parse_branch_report(output: &str) -> Option<BranchReport> {
    let answer = serde_json::from_str::<Value>(output.trim()).ok()?;
    let repository = answer
        .pointer("/data/repository")
        .filter(|repository| repository.is_object())?;
    let head = repository
        .pointer("/ref/target/oid")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|sha| sha.len() >= 7);
    let pull_request = repository
        .pointer("/pullRequests/nodes/0")
        .and_then(|pull| {
            Some((
                pull.get("number")?.as_u64()?,
                pull.get("url")?.as_str()?.to_string(),
                pull.get("state")?.as_str()?.to_string(),
            ))
        });
    let ahead_by = head.as_ref().and_then(|_| {
        repository
            .pointer("/defaultBranchRef/compare/aheadBy")
            .and_then(Value::as_u64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_graphql_answer_gives_the_report_the_rest_reads_gave() {
        let answer = r#"{"data":{"repository":{"ref":{"target":{"oid":"0123456789abcdef"}},"defaultBranchRef":{"compare":{"aheadBy":3}},"pullRequests":{"nodes":[{"number":12,"url":"https://github.com/o/r/pull/12","state":"OPEN"}]}}}}"#;
        assert_eq!(
            parse_branch_report(answer),
            Some(BranchReport {
                head: Some("0123456789abcdef".to_string()),
                ahead_by: Some(3),
                pull_request: Some((
                    12,
                    "https://github.com/o/r/pull/12".to_string(),
                    "OPEN".to_string()
                )),
            })
        );
    }

    #[test]
    fn a_branch_not_pushed_yet_is_a_report_without_head_and_a_refused_call_is_none() {
        let answer = r#"{"data":{"repository":{"ref":null,"defaultBranchRef":{"compare":null},"pullRequests":{"nodes":[]}}},"errors":[{"message":"No commit found for the ref"}]}"#;
        assert_eq!(parse_branch_report(answer), Some(BranchReport::default()));
        assert_eq!(
            parse_branch_report(r#"{"errors":[{"message":"Bad credentials"}]}"#),
            None
        );
        assert_eq!(parse_branch_report("gh: not logged in"), None);
    }
}
