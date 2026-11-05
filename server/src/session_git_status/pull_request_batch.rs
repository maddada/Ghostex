//! The branch pull requests a git-status pass asks for, read one GraphQL call per repo.
//!
//! CDXC:Git 2026-11-05 WHY:
//! Each checkout's branch PR was its own `gh pr view <branch>` in that checkout, and `gh` resolves the repo from the checkout's remotes with git commands of its own: on Windows each of those went through Git for Windows' `cmd\git.exe` launcher, so one PR read was a `gh` start and about six git starts (measured 2026-11-05, "git under gh" and "git under git under gh"). The repo is already known from the checkout's remotes (`refresh_work_repo`, read at most hourly), so every due branch of a repo is asked in one `gh api graphql` call that starts no git. A checkout whose GitHub repo is not known still asks `gh pr view`. The pick among a branch's PRs follows `gh pr view <branch>`: the newest open one, else the newest; a fork asks its `upstream`, where `gh` looks first.
//! SEE-ALSO: server/src/session_git_status/refresh.rs (the pass), server/src/work_mode/github.rs (the remotes cache).

use super::*;

/// Branches one call asks for.
const BATCH_SIZE: usize = 30;

/// One branch to read in a repo, and the places in the pass's list that asked for it (several
/// checkouts on `main` ask once).
struct Wanted<'a> {
    indexes: Vec<usize>,
    branch: &'a str,
    /// The checkout's own `origin` owner when it asks its `upstream`, whose PRs come from the fork.
    fork_owner: Option<String>,
}

/// The pull request of each `(cwd, branch)`, in order: one GraphQL call per repo (and 30
/// branches), `gh pr view` for a checkout whose repo is not known.
pub(super) fn probe_pull_requests_batched(
    wanted: &[(String, String)],
    probe_alone: &dyn Fn(&str, &str) -> Option<SessionPullRequest>,
) -> Vec<Option<SessionPullRequest>> {
    let mut answers: Vec<Option<SessionPullRequest>> = vec![None; wanted.len()];
    let mut by_repo: Vec<(String, Vec<Wanted>)> = Vec::new();
    for (index, (cwd, branch)) in wanted.iter().enumerate() {
        crate::work_mode::refresh_work_repo(cwd);
        let origin = crate::work_mode::cached_work_repo(cwd);
        let upstream = crate::work_mode::cached_work_upstream_repo(cwd);
        let (repo, fork_owner) = match (&upstream, &origin) {
            (Some(upstream), origin) if Some(upstream) != origin.as_ref() => (
                upstream.clone(),
                origin
                    .as_deref()
                    .and_then(|origin| origin.split_once('/'))
                    .map(|(owner, _)| owner.to_string()),
            ),
            (_, Some(origin)) => (origin.clone(), None),
            _ => {
                answers[index] = probe_alone(cwd, branch);
                continue;
            }
        };
        let list = match by_repo.iter().position(|(known, _)| *known == repo) {
            Some(position) => &mut by_repo[position].1,
            None => {
                by_repo.push((repo, Vec::new()));
                &mut by_repo.last_mut().expect("just pushed").1
            }
        };
        match list
            .iter_mut()
            .find(|known| known.branch == branch && known.fork_owner == fork_owner)
        {
            Some(known) => known.indexes.push(index),
            None => list.push(Wanted {
                indexes: vec![index],
                branch,
                fork_owner,
            }),
        }
    }
    for (repo, list) in &by_repo {
        let Some((owner, name)) = repo.split_once('/') else {
            continue;
        };
        for chunk in list.chunks(BATCH_SIZE) {
            let Some(repository) = read_branches(owner, name, chunk) else {
                continue;
            };
            for (alias, wanted) in chunk.iter().enumerate() {
                let answer = pick_pull_request(
                    &repository,
                    &format!("b{alias}"),
                    wanted.branch,
                    wanted.fork_owner.as_deref(),
                );
                for index in &wanted.indexes {
                    answers[*index] = answer.clone();
                }
            }
        }
    }
    answers
}

/// The query for `count` branches, each passed as a variable (`$b0`, `$b1`, ...).
pub(super) fn branches_query(count: usize) -> String {
    let mut variables = String::from("$owner: String!, $name: String!");
    let mut fields = String::from("defaultBranchRef { name } ");
    for alias in 0..count {
        variables.push_str(&format!(", $b{alias}: String!"));
        fields.push_str(&format!(
            "b{alias}: pullRequests(headRefName: $b{alias}, first: 30, orderBy: {{field: CREATED_AT, direction: DESC}}) {{ nodes {{ number state url isDraft isCrossRepository headRepositoryOwner {{ login }} }} }} "
        ));
    }
    format!("query({variables}) {{ repository(owner: $owner, name: $name) {{ {fields}}} }}")
}

/// The `repository` object of one call, or `None` when the call failed.
fn read_branches(owner: &str, name: &str, chunk: &[Wanted]) -> Option<Value> {
    let mut args: Vec<String> = vec![
        "api".into(),
        "graphql".into(),
        "-f".into(),
        format!("query={}", branches_query(chunk.len())),
        "-f".into(),
        format!("owner={owner}"),
        "-f".into(),
        format!("name={name}"),
    ];
    for (alias, wanted) in chunk.iter().enumerate() {
        args.push("-f".into());
        args.push(format!("b{alias}={}", wanted.branch));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_gh_command_any_status(None, &args)?;
    let answer: Value = serde_json::from_str(output.trim()).ok()?;
    answer
        .pointer("/data/repository")
        .filter(|repository| repository.is_object())
        .cloned()
}

/// What `gh pr view <branch>` would show from one alias of the answer: the newest open PR from
/// that branch (from the fork when the checkout is one), else the newest, and no closed one for
/// the default branch itself.
pub(super) fn pick_pull_request(
    repository: &Value,
    alias: &str,
    branch: &str,
    fork_owner: Option<&str>,
) -> Option<SessionPullRequest> {
    let nodes = repository.get(alias)?.get("nodes")?.as_array()?;
    let is_default_branch = repository
        .pointer("/defaultBranchRef/name")
        .and_then(Value::as_str)
        == Some(branch);
    let from_this_checkout = |node: &&Value| {
        let cross = node
            .get("isCrossRepository")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match fork_owner {
            Some(owner) => {
                !cross
                    || node
                        .pointer("/headRepositoryOwner/login")
                        .and_then(Value::as_str)
                        .is_some_and(|login| login.eq_ignore_ascii_case(owner))
            }
            None => !cross,
        }
    };
    let candidates: Vec<&Value> = nodes.iter().filter(from_this_checkout).collect();
    let node = candidates
        .iter()
        .find(|node| node.get("state").and_then(Value::as_str) == Some("OPEN"))
        .or_else(|| (!is_default_branch).then(|| candidates.first()).flatten())?;
    parse_gh_pull_request_json(&node.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(number: i64, state: &str, cross: bool, owner: &str) -> Value {
        json!({
            "number": number,
            "state": state,
            "url": format!("https://github.com/o/r/pull/{number}"),
            "isDraft": false,
            "isCrossRepository": cross,
            "headRepositoryOwner": { "login": owner },
        })
    }

    fn repository(nodes: Vec<Value>) -> Value {
        json!({ "defaultBranchRef": { "name": "main" }, "b0": { "nodes": nodes } })
    }

    /// Like `gh pr view <branch>`: the newest open PR, else the newest, from this checkout's own
    /// branch (the fork's, when it asks its upstream), and no finished one for the default branch.
    #[test]
    fn the_pick_follows_gh_pr_view() {
        let answer = repository(vec![
            node(9, "CLOSED", false, "o"),
            node(7, "OPEN", false, "o"),
        ]);
        assert_eq!(
            pick_pull_request(&answer, "b0", "feature", None).map(|pr| pr.number),
            Some(7)
        );
        let answer = repository(vec![
            node(9, "MERGED", false, "o"),
            node(7, "CLOSED", false, "o"),
        ]);
        assert_eq!(
            pick_pull_request(&answer, "b0", "feature", None).map(|pr| pr.number),
            Some(9)
        );
        assert_eq!(pick_pull_request(&answer, "b0", "main", None), None);
        let answer = repository(vec![
            node(5, "OPEN", true, "someone"),
            node(4, "OPEN", true, "me"),
        ]);
        assert_eq!(pick_pull_request(&answer, "b0", "feature", None), None);
        assert_eq!(
            pick_pull_request(&answer, "b0", "feature", Some("me")).map(|pr| pr.number),
            Some(4)
        );
        assert_eq!(pick_pull_request(&json!({}), "b0", "feature", None), None);
    }

    #[test]
    fn every_branch_is_a_variable_of_one_query() {
        let query = branches_query(2);
        assert!(
            query.starts_with("query($owner: String!, $name: String!, $b0: String!, $b1: String!)")
        );
        assert!(query.contains("b1: pullRequests(headRefName: $b1"));
    }
}
