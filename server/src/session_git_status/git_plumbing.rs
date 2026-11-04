use super::*;

// ---------------------------------------------------------------------------
// pure git plumbing
// ---------------------------------------------------------------------------

/// The repository's default branch and the ref to diff against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultBranch {
    pub name: String,
    pub git_ref: String,
}

/*
Default-branch resolution, most authoritative first:

1. `origin/HEAD` — what the remote itself says its default branch is.
2. `origin/main` then `origin/master` — repositories cloned before git started
   writing `origin/HEAD`, which is most of them.
3. local `main` then `master` — repositories with no remote at all.

`None` means "this repository has no recognizable default branch" (a fresh repo
whose only branch is the feature branch, say), and the caller then reports the
working tree against HEAD instead of inventing a base.
*/
pub fn resolve_default_branch(run: &dyn Fn(&[&str]) -> Option<String>) -> Option<DefaultBranch> {
    if let Some(value) = run(&[
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ]) {
        if let Some(name) = value
            .trim()
            .strip_prefix("origin/")
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref: format!("refs/remotes/origin/{name}"),
            });
        }
    }
    for name in ["main", "master"] {
        let git_ref = format!("refs/remotes/origin/{name}");
        if git_ref_exists(run, &git_ref) {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref,
            });
        }
    }
    for name in ["main", "master"] {
        let git_ref = format!("refs/heads/{name}");
        if git_ref_exists(run, &git_ref) {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref,
            });
        }
    }
    None
}

fn git_ref_exists(run: &dyn Fn(&[&str]) -> Option<String>, git_ref: &str) -> bool {
    run(&["rev-parse", "--verify", "--quiet", git_ref])
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/*
The whole per-cwd git answer, expressed over a command runner so the plumbing is
testable against fakes as well as real repositories.

Diff base:
- On a feature branch, the base is `merge-base(default, HEAD)`, so the counts are
  "everything this branch did": commits on the branch PLUS staged PLUS unstaged.
  `git diff <base>` compares a commit against the WORKING TREE, so one command
  covers all three.
- On the default branch itself (or with no default branch, or with unrelated
  histories) the base is HEAD, so the counts are the uncommitted work.

Untracked files are deliberately not counted: they are not part of any diff and
counting them would need a per-file `--no-index` pass whose cost scales with
whatever build output happens to be lying around.
*/
pub fn probe_git_status_with(run: &dyn Fn(&[&str]) -> Option<String>) -> Option<SessionGitProbe> {
    let is_inside_work_tree = run(&["rev-parse", "--is-inside-work-tree"])
        .map(|value| value.trim() == "true")
        .unwrap_or(false);
    if !is_inside_work_tree {
        return None;
    }
    let branch = run(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map(|value| value.trim().to_string())
        .filter(|branch| !branch.is_empty());
    let default_branch = resolve_default_branch(run);
    let base = match &default_branch {
        Some(default_branch) if branch.as_deref() != Some(default_branch.name.as_str()) => {
            run(&["merge-base", default_branch.git_ref.as_str(), "HEAD"])
                .map(|value| value.trim().to_string())
                .filter(|base| !base.is_empty())
        }
        _ => None,
    };
    let base = base.unwrap_or_else(|| "HEAD".to_string());
    // A submodule with only uncommitted work inside it adds `0 0 <path>`, never a line, but git
    // checks every submodule's work tree to find that out: about three times the CPU of the diff
    // itself in a repository with nine submodules (measured 2026-11-04, issues-1027/macos-perf).
    // A submodule moved to another commit still counts.
    let numstat = run(&[
        "diff",
        "--numstat",
        "--ignore-submodules=dirty",
        base.as_str(),
        "--",
    ])
    .unwrap_or_default();
    let (additions, deletions) = parse_git_numstat(&numstat);
    Some(SessionGitProbe {
        branch,
        additions,
        deletions,
    })
}

/// `git diff --numstat` is machine output ("adds<TAB>dels<TAB>path"), unlike
/// `--shortstat`, so it survives locales and rename lines. Binary files report
/// `-` and contribute nothing.
pub fn parse_git_numstat(output: &str) -> (i64, i64) {
    let mut additions = 0_i64;
    let mut deletions = 0_i64;
    for line in output.lines() {
        let mut columns = line.split('\t');
        let Some(added) = columns.next() else {
            continue;
        };
        let Some(removed) = columns.next() else {
            continue;
        };
        additions += added.trim().parse::<i64>().unwrap_or(0);
        deletions += removed.trim().parse::<i64>().unwrap_or(0);
    }
    (additions, deletions)
}

/*
`gh pr view --json number,state,url,isDraft`. gh's `state` is OPEN/CLOSED/MERGED
and draft-ness is a separate flag, so a draft is an OPEN PR with `isDraft: true`
while a merged or closed PR keeps its terminal state even if it was a draft. A
PR with no usable number or an unrecognized state is dropped rather than guessed
at: no badge beats a wrong badge, and the auto-settle rule reads the same value.
*/
pub fn parse_gh_pull_request_json(output: &str) -> Option<SessionPullRequest> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    let number = value
        .get("number")
        .and_then(Value::as_i64)
        .filter(|number| *number > 0)?;
    let raw_state = value.get("state").and_then(Value::as_str)?;
    let is_draft = value
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state = parse_gh_pull_request_state(raw_state, is_draft)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    Some(SessionPullRequest { number, state, url })
}

pub fn parse_gh_pull_request_state(raw_state: &str, is_draft: bool) -> Option<PullRequestState> {
    match raw_state.trim().to_ascii_uppercase().as_str() {
        "MERGED" => Some(PullRequestState::Merged),
        "CLOSED" => Some(PullRequestState::Closed),
        "OPEN" => Some(if is_draft {
            PullRequestState::Draft
        } else {
            PullRequestState::Open
        }),
        _ => None,
    }
}
