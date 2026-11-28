use ghostex_gx_protocol::git_provider::{
    pull_request_number_from_url, validate_gitlab_merge_request_url, GitHostProvider,
};
use serde_json::Value;

use super::{
    resolve_repo_provider_sync, CreatePullRequestOptions, PullRequestProvider, PullRequestView,
    PullRequestViewState,
};
use crate::session_git_status::{
    cached_glab_cli_available, glab_cli_is_available, resolve_default_branch,
    run_git_probe_command, run_glab_command, PullRequestState, SessionPullRequest,
};
use crate::typed_operations::values::ProcessCommand;

pub(crate) struct GitLabPullRequestProvider;

pub(crate) static GITLAB_PROVIDER: GitLabPullRequestProvider = GitLabPullRequestProvider;

fn glab_command(args: Vec<&str>, cwd: &str) -> ProcessCommand {
    ProcessCommand::new("glab", args, cwd)
        .with_env("NO_PROMPT", "true")
        .with_env("GLAB_NO_PROMPT", "1")
        .with_env("GLAB_CHECK_UPDATE", "false")
        .with_env("GIT_TERMINAL_PROMPT", "0")
}

impl PullRequestProvider for GitLabPullRequestProvider {
    fn kind(&self) -> GitHostProvider {
        GitHostProvider::GitLab
    }

    fn build_version_command(&self, cwd: &str) -> ProcessCommand {
        glab_command(vec!["--version"], cwd)
    }

    fn build_create_command(
        &self,
        cwd: &str,
        options: &CreatePullRequestOptions,
    ) -> ProcessCommand {
        let mut args = vec!["mr", "create"];
        if options.fill {
            args.push("--fill");
        }
        args.push("--yes");
        glab_command(args, cwd)
    }

    fn build_view_command(&self, cwd: &str, _include_title: bool) -> ProcessCommand {
        glab_command(vec!["mr", "view", "--output", "json"], cwd)
    }

    fn parse_view_output(
        &self,
        stdout: &str,
        expected_host: Option<&str>,
    ) -> Option<PullRequestView> {
        let value = serde_json::from_str::<Value>(stdout.trim()).ok()?;
        parse_gitlab_mr_object(&value, expected_host)
    }

    fn validate_pr_url(&self, candidate: &str, expected_host: Option<&str>) -> Option<String> {
        validate_gitlab_merge_request_url(candidate, expected_host)
    }

    fn is_cli_authenticated(&self) -> bool {
        glab_cli_is_available()
    }

    fn cached_cli_authenticated(&self) -> bool {
        cached_glab_cli_available()
    }

    fn probe_branch_status(&self, cwd: &str, branch: &str) -> Option<SessionPullRequest> {
        let output = run_glab_command(
            Some(cwd),
            &[
                "mr",
                "list",
                "--source-branch",
                branch,
                "--all",
                "--output",
                "json",
            ],
        )?;
        let expected_host = resolve_repo_provider_sync(cwd).remote_host;
        let is_default_branch = resolve_default_branch(&|args| run_git_probe_command(cwd, args))
            .is_some_and(|default| default.name == branch);
        parse_gitlab_mr_list_json(&output, expected_host.as_deref(), is_default_branch)
    }
}

fn parse_gitlab_mr_object(value: &Value, expected_host: Option<&str>) -> Option<PullRequestView> {
    let object = value.as_object()?;
    let state = match object.get("state")?.as_str()?.to_ascii_lowercase().as_str() {
        "opened" | "open" => PullRequestViewState::Open,
        "closed" => PullRequestViewState::Closed,
        "merged" => PullRequestViewState::Merged,
        _ => return None,
    };
    let raw_url = object
        .get("web_url")
        .or_else(|| object.get("url"))
        .and_then(Value::as_str)?;
    let url = validate_gitlab_merge_request_url(raw_url, expected_host)?;
    let number = object
        .get("iid")
        .or_else(|| object.get("number"))
        .and_then(Value::as_u64)
        .filter(|number| *number > 0)
        .or_else(|| pull_request_number_from_url(GitHostProvider::GitLab, &url));
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    let is_draft = object
        .get("draft")
        .and_then(Value::as_bool)
        .or_else(|| object.get("work_in_progress").and_then(Value::as_bool))
        .unwrap_or(false);
    Some(PullRequestView {
        number,
        state,
        title,
        url,
        is_draft,
        provider: GitHostProvider::GitLab,
    })
}

/// Picks the branch's merge request from `glab mr list --source-branch <branch> --all --output json`,
/// matching GitHub's selection: newest open MR first, else newest merged/closed MR (never a
/// finished MR on the default branch).
fn parse_gitlab_mr_list_json(
    output: &str,
    expected_host: Option<&str>,
    is_default_branch: bool,
) -> Option<SessionPullRequest> {
    let items = serde_json::from_str::<Value>(output.trim()).ok()?;
    let list = items.as_array()?;
    let parsed: Vec<PullRequestView> = list
        .iter()
        .filter_map(|item| parse_gitlab_mr_object(item, expected_host))
        .collect();
    let picked = parsed
        .iter()
        .find(|mr| mr.state == PullRequestViewState::Open)
        .or_else(|| (!is_default_branch).then(|| parsed.first()).flatten())?;
    let number = i64::try_from(picked.number?).ok().filter(|n| *n > 0)?;
    let state = match picked.state {
        PullRequestViewState::Merged => PullRequestState::Merged,
        PullRequestViewState::Closed => PullRequestState::Closed,
        PullRequestViewState::Open => {
            if picked.is_draft {
                PullRequestState::Draft
            } else {
                PullRequestState::Open
            }
        }
    };
    Some(SessionPullRequest {
        number,
        state,
        url: Some(picked.url.clone()),
    })
}
