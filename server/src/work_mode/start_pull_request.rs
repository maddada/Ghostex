//! Starting work on a GitHub pull request (`/api/startWorkOnTicket` with `pullRequest`): the
//! session works on the PR's head branch, linked to the PR and to the issues the PR closes.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: a Slack thread that names only a PR starts work on that PR's head branch: a worktree on
//! it, the session linked to the PR and to any issue the PR closes. `startWorkOnTicket` takes
//! `pullRequest` (number or URL) next to `linearIssue` / `githubIssue` for that.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::session_git_status::run_gh_command;
use crate::worktree_sessions::{worktree_git_succeeds, WORKTREE_FETCH_COMMAND_TIMEOUT};

use super::*;

/// What starting work on a PR settled before the session is created.
pub(crate) struct PullRequestPlan {
    pub(crate) number: u64,
    pub(crate) url: String,
    /// The local branch the worktree checks out: the PR's head branch, or `<fork owner>/<head
    /// branch>` for a PR from a fork.
    pub(crate) branch: String,
    /// The `setSessionWorkLinks`-shaped links for the new session.
    pub(crate) links: Map<String, Value>,
    /// A PR from a fork: its head is fetched into `branch` before the worktree is cut.
    cross_repository: bool,
}

impl PullRequestPlan {
    /// Whether a session already works on this PR, by its links or its worktree's branch.
    pub(crate) fn is_linked(&self, targets: &WorkTargets, worktree_branch: &str) -> bool {
        worktree_branch == self.branch
            || targets.pull_request.as_deref().is_some_and(|linked| {
                linked == self.number.to_string() || linked.trim_end_matches('/') == self.url
            })
    }

    /// Makes the branch exist locally (a fork's head is not on origin). A same-repo head branch is
    /// left to the worktree checkout, which takes it from origin.
    pub(crate) fn prepare_branch(&self, project_path: &str) -> Result<(), DomainStateError> {
        if !self.cross_repository {
            return Ok(());
        }
        let refspec = format!("+refs/pull/{}/head:refs/heads/{}", self.number, self.branch);
        if worktree_git_succeeds(
            project_path,
            &["fetch", "origin", &refspec],
            WORKTREE_FETCH_COMMAND_TIMEOUT,
        ) {
            return Ok(());
        }
        Err(DomainStateError::bad_request(format!(
            "Could not fetch PR #{} from origin into {}.",
            self.number, self.branch
        )))
    }
}

/// Reads the PR with `gh` in the project's folder and works out its branch and links.
pub(crate) fn plan_pull_request(
    project_path: &str,
    linear_team_keys: Option<&[String]>,
    selector: &str,
) -> Result<PullRequestPlan, DomainStateError> {
    if let Some(repo) = pull_request_url_repo(selector) {
        if origin_repo(project_path).is_some_and(|origin| origin != repo) {
            return Err(DomainStateError::bad_request(format!(
                "That PR is in {repo}, not in this project's repository."
            )));
        }
    }
    let output = run_gh_command(
        Some(project_path),
        &[
            "pr",
            "view",
            selector,
            "--json",
            "number,url,state,title,body,headRefName,isCrossRepository,headRepositoryOwner",
        ],
    )
    .ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "GitHub has no PR {selector} in this repository (or gh is not signed in)."
        ))
    })?;
    let pr: Value = serde_json::from_str(&output).map_err(|_| {
        DomainStateError::bad_request("gh answered `pr view` with something that is not JSON.")
    })?;
    let number = pr
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| DomainStateError::bad_request("gh did not say which PR this is."))?;
    let url = text(&pr, "url").unwrap_or_default().to_string();
    if text(&pr, "state") == Some("MERGED") {
        return Err(DomainStateError::bad_request(format!(
            "PR #{number} is already merged."
        )));
    }
    let head = text(&pr, "headRefName")
        .ok_or_else(|| DomainStateError::bad_request("gh did not say which branch the PR is."))?;
    let cross_repository = pr.get("isCrossRepository") == Some(&Value::Bool(true));
    let branch = match (cross_repository, pr.pointer("/headRepositoryOwner/login")) {
        (true, Some(Value::String(owner))) => format!("{}/{head}", owner.to_ascii_lowercase()),
        _ => head.to_string(),
    };

    let mut links = Map::new();
    links.insert("pullRequest".to_string(), json!(number));
    let github_issues = closing_issue_numbers(project_path, &url, number);
    if !github_issues.is_empty() {
        links.insert("githubIssues".to_string(), json!(github_issues));
    }
    if let Some(team_keys) = linear_team_keys {
        // Setting `linearIssues` replaces what the branch names, so the branch's own IDs stay in.
        let mut linear = linear_identifiers_in_branch(head, team_keys);
        let words = format!(
            "{}\n{}",
            text(&pr, "title").unwrap_or_default(),
            text(&pr, "body").unwrap_or_default()
        );
        for identifier in linear_issues_closed_by(&words, team_keys) {
            if !linear.contains(&identifier) {
                linear.push(identifier);
            }
        }
        if !linear.is_empty() {
            links.insert("linearIssues".to_string(), json!(linear));
        }
    }
    Ok(PullRequestPlan {
        number,
        url,
        branch,
        links,
        cross_repository,
    })
}

/// The issues of the PR's own repository that the PR closes (GitHub's "Closes #12" and linked
/// issues). Asked through GraphQL because `gh pr view --json closingIssuesReferences` only exists in
/// newer `gh` releases; a failure means no links, not a failed start.
fn closing_issue_numbers(project_path: &str, url: &str, number: u64) -> Vec<u64> {
    let Some((repo_url, _)) = url.rsplit_once("/pull/") else {
        return Vec::new();
    };
    let Some(repo) = pull_request_url_repo(url) else {
        return Vec::new();
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return Vec::new();
    };
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let number_arg = format!("number={number}");
    let output = run_gh_command(
        Some(project_path),
        &[
            "api",
            "graphql",
            "-f",
            "query=query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){closingIssuesReferences(first:20){nodes{number url}}}}}",
            "-F",
            &owner_arg,
            "-F",
            &name_arg,
            "-F",
            &number_arg,
        ],
    );
    let Some(answer) = output.and_then(|output| serde_json::from_str::<Value>(&output).ok()) else {
        return Vec::new();
    };
    // Closing references can name issues of other repositories; a session's GitHub issues are
    // numbers in its own repository.
    let issues_prefix = format!("{}/issues/", repo_url.to_ascii_lowercase());
    answer
        .pointer("/data/repository/pullRequest/closingIssuesReferences/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|issue| {
            text(issue, "url")
                .is_some_and(|url| url.to_ascii_lowercase().starts_with(&issues_prefix))
        })
        .filter_map(|issue| issue.get("number").and_then(Value::as_u64))
        .collect()
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// The Linear issues a PR's title or description closes with Linear's magic words ("Fixes
/// SPX-1245", "Closes SPX-1, SPX-2", or a Linear issue link after the word). Only real team keys
/// count.
fn linear_issues_closed_by(text: &str, team_keys: &[String]) -> Vec<String> {
    const MAGIC: &[&str] = &[
        "close",
        "closes",
        "closed",
        "closing",
        "fix",
        "fixes",
        "fixed",
        "fixing",
        "resolve",
        "resolves",
        "resolved",
        "resolving",
        "complete",
        "completes",
        "completed",
        "completing",
    ];
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .map(|word| word.trim_matches(|c: char| c == ':' || c == '.' || c == '*' || c == '`'))
        .filter(|word| !word.is_empty())
        .collect();
    let identifier = |word: &str| {
        // A Linear link ends its path in the identifier and a slug: `/issue/SPX-1245/copy-link`.
        let candidate = word
            .split_once("/issue/")
            .map(|(_, rest)| rest.split('/').next().unwrap_or(rest))
            .unwrap_or(word);
        normalize_linear_identifier(candidate).filter(|id| {
            id.split_once('-')
                .is_some_and(|(key, _)| team_keys.iter().any(|team| team.eq_ignore_ascii_case(key)))
        })
    };
    let mut found = Vec::new();
    let mut index = 0;
    while index < words.len() {
        if MAGIC.contains(&words[index].to_ascii_lowercase().as_str()) {
            let mut next = index + 1;
            while next < words.len() {
                if let Some(id) = identifier(words[next]) {
                    if !found.contains(&id) {
                        found.push(id);
                    }
                } else if !words[next].eq_ignore_ascii_case("and") {
                    break;
                }
                next += 1;
            }
            index = next;
        } else {
            index += 1;
        }
    }
    found
}

/// `owner/name` of a `https://github.com/owner/name/pull/12` link, lowercased.
fn pull_request_url_repo(selector: &str) -> Option<String> {
    let rest = selector
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| selector.trim().strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/');
    let (owner, name, kind) = (parts.next()?, parts.next()?, parts.next()?);
    (kind == "pull" && !owner.is_empty() && !name.is_empty())
        .then(|| format!("{owner}/{name}").to_ascii_lowercase())
}

/// `owner/name` of a folder's `origin` remote, lowercased.
pub(crate) fn origin_repo(path: &str) -> Option<String> {
    let output = crate::platform::process::background_command("git")
        .args(["-C", path, "remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = url
        .strip_suffix(".git")
        .unwrap_or(&url)
        .rsplitn(3, ['/', ':'])
        .take(2)
        .collect::<Vec<_>>();
    match path.as_slice() {
        [name, owner] if !name.is_empty() && !owner.is_empty() => {
            Some(format!("{owner}/{name}").to_ascii_lowercase())
        }
        _ => None,
    }
}
