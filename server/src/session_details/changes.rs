//! The Changes section: the lines the session's branch changed against the default branch,
//! uncommitted edits included, and the branch's pull request.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q12): Changes counts "the branch against main, uncommitted included", the number the sidebar's git status already keeps (`probe_git_status_with`: `git diff --numstat <merge-base>` against the working tree; on the default branch that is the uncommitted work), with Create PR, View PR, or Commit on the default branch. The probe is the sidebar's own, but its answers stay in this module's cache: the sidebar's cache publishes a change only when its own pass sees one, so writing into it from here would hide that change from Sidebar V2.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::session_git_status::{
    PullRequestState, SessionGitStatusProber, SessionPullRequest, SystemSessionGitStatusProber,
};

/// How long a git answer is reused while the panel keeps reading.
const GIT_FRESH_FOR: Duration = Duration::from_secs(20);
/// How long a pull request answer (a network call) is reused.
const PULL_REQUEST_FRESH_FOR: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
struct Cached {
    at: Instant,
    value: Option<Value>,
    branch: Option<String>,
    pull_request: Option<(Instant, Option<SessionPullRequest>)>,
}

fn cache() -> &'static Mutex<HashMap<String, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

fn pull_request_value(pull_request: &SessionPullRequest) -> Value {
    json!({
        "number": pull_request.number,
        "state": pull_request.state.as_wire(),
        "url": pull_request.url,
        "open": matches!(pull_request.state, PullRequestState::Open | PullRequestState::Draft),
    })
}

/// `{branch, defaultBranch, additions, deletions, pullRequest}` for `cwd`, or `None` outside a git
/// checkout. Blocking.
pub(crate) fn read_changes(cwd: &str) -> Option<Value> {
    let now = Instant::now();
    let previous = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(cwd).cloned());
    if let Some(previous) = previous
        .as_ref()
        .filter(|previous| now.duration_since(previous.at) < GIT_FRESH_FOR)
    {
        return previous.value.clone();
    }
    let prober = SystemSessionGitStatusProber;
    let probe = prober.probe_git(cwd);
    let Some(probe) = probe else {
        if let Ok(mut cache) = cache().lock() {
            cache.insert(
                cwd.to_string(),
                Cached {
                    at: now,
                    value: None,
                    branch: None,
                    pull_request: None,
                },
            );
        }
        return None;
    };
    let default_branch = crate::session_git_status::run_git_probe_command(
        cwd,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .map(|value| value.trim().trim_start_matches("origin/").to_string())
    .filter(|name| !name.is_empty());
    let on_default = match (&probe.branch, &default_branch) {
        (Some(branch), Some(default)) => branch == default,
        (Some(branch), None) => matches!(branch.as_str(), "main" | "master"),
        (None, _) => false,
    };
    // The pull request is asked again only when it is old or the branch moved.
    let pull_request = previous
        .as_ref()
        .and_then(|previous| {
            previous
                .pull_request
                .clone()
                .filter(|(at, _)| now.duration_since(*at) < PULL_REQUEST_FRESH_FOR)
                .filter(|_| previous.branch == probe.branch)
        })
        .or_else(|| {
            let branch = probe.branch.as_deref().filter(|_| !on_default)?;
            if !prober.supports_pull_requests() {
                return Some((now, None));
            }
            Some((now, prober.probe_pull_request(cwd, branch)))
        });
    let value = json!({
        "branch": probe.branch,
        "defaultBranch": default_branch,
        "onDefaultBranch": on_default,
        "additions": probe.additions,
        "deletions": probe.deletions,
        "pullRequest": pull_request
            .as_ref()
            .and_then(|(_, pull_request)| pull_request.as_ref())
            .map(pull_request_value),
    });
    if let Ok(mut cache) = cache().lock() {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(
            cwd.to_string(),
            Cached {
                at: now,
                value: Some(value.clone()),
                branch: probe.branch.clone(),
                pull_request,
            },
        );
    }
    Some(value)
}
