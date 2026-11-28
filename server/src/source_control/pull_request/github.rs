use ghostex_gx_protocol::git_provider::{
    pull_request_number_from_url, validate_github_pull_request_url, GitHostProvider,
};
use serde_json::Value;

use super::{CreatePullRequestOptions, PullRequestProvider, PullRequestView, PullRequestViewState};
use crate::session_git_status::{
    cached_gh_cli_available, gh_cli_is_available, parse_gh_pull_request_json,
    probe_pull_requests_batched, run_gh_command, SessionPullRequest,
};
use crate::typed_operations::values::ProcessCommand;

pub(crate) struct GitHubPullRequestProvider;

pub(crate) static GITHUB_PROVIDER: GitHubPullRequestProvider = GitHubPullRequestProvider;

impl PullRequestProvider for GitHubPullRequestProvider {
    fn kind(&self) -> GitHostProvider {
        GitHostProvider::GitHub
    }

    fn build_version_command(&self, cwd: &str) -> ProcessCommand {
        ProcessCommand::new("gh", vec!["--version"], cwd)
    }

    fn build_create_command(
        &self,
        cwd: &str,
        options: &CreatePullRequestOptions,
    ) -> ProcessCommand {
        let mut args = vec!["pr", "create"];
        if options.fill {
            args.push("--fill");
        }
        ProcessCommand::new("gh", args, cwd)
    }

    fn build_view_command(&self, cwd: &str, include_title: bool) -> ProcessCommand {
        let fields = if include_title {
            "number,state,title,url"
        } else {
            "number,state,url"
        };
        ProcessCommand::new("gh", vec!["pr", "view", "--json", fields], cwd)
    }

    fn parse_view_output(
        &self,
        stdout: &str,
        _expected_host: Option<&str>,
    ) -> Option<PullRequestView> {
        let value = serde_json::from_str::<Value>(stdout.trim()).ok()?;
        let object = value.as_object()?;
        let state = match object.get("state")?.as_str()?.to_ascii_lowercase().as_str() {
            "open" => PullRequestViewState::Open,
            "closed" => PullRequestViewState::Closed,
            "merged" => PullRequestViewState::Merged,
            _ => return None,
        };
        let url = validate_github_pull_request_url(object.get("url")?.as_str()?)?;
        let number = object
            .get("number")
            .and_then(Value::as_u64)
            .or_else(|| pull_request_number_from_url(GitHostProvider::GitHub, &url));
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string);
        let is_draft = object
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(PullRequestView {
            number,
            state,
            title,
            url,
            is_draft,
            provider: GitHostProvider::GitHub,
        })
    }

    fn validate_pr_url(&self, candidate: &str, _expected_host: Option<&str>) -> Option<String> {
        validate_github_pull_request_url(candidate)
    }

    fn cli_unavailable_reason(&self) -> &'static str {
        "githubCliUnavailable"
    }

    fn is_cli_authenticated(&self) -> bool {
        gh_cli_is_available()
    }

    fn cached_cli_authenticated(&self) -> bool {
        cached_gh_cli_available()
    }

    fn probe_branch_status(&self, cwd: &str, branch: &str) -> Option<SessionPullRequest> {
        let output = run_gh_command(
            Some(cwd),
            &["pr", "view", branch, "--json", "number,state,url,isDraft"],
        )?;
        parse_gh_pull_request_json(&output)
    }

    fn probe_branch_statuses(
        &self,
        wanted: &[(String, String)],
    ) -> Vec<Option<SessionPullRequest>> {
        probe_pull_requests_batched(wanted, &|cwd, branch| self.probe_branch_status(cwd, branch))
    }
}
