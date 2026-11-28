//! Provider layer behind pull/merge request create, view, and status.
//!
//! Each supported host (`github`, `gitlab`, and later `bitbucket`, `azure-devops`, `forgejo`)
//! implements [`PullRequestProvider`]. The provider for a repository is chosen from its `origin`
//! remote host via [`resolve_repo_provider`] (async) or [`resolve_repo_provider_sync`] (blocking).
//!
//! Extending with new PR actions (merge, mark ready, checks, watch): add methods to
//! [`PullRequestProvider`] with default unsupported errors where needed so every host stays
//! decoupled behind the same resolution entry point.

mod github;
mod gitlab;

use ghostex_gx_protocol::git_provider::{
    detect_provider_from_remote_url, extract_remote_host, GitHostProvider, GitHostTerms,
};
use serde_json::{json, Map, Value};

use crate::session_git_status::{run_git_probe_command, SessionPullRequest};
use crate::typed_operations::values::{run_process_command, ProcessCommand, TypedOperationContext};

pub(crate) use github::GITHUB_PROVIDER;
pub(crate) use gitlab::GITLAB_PROVIDER;

/// Normalized state for a single PR/MR view (`createPullRequest`, `readProjectGitState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PullRequestViewState {
    Open,
    Closed,
    Merged,
}

impl PullRequestViewState {
    pub(crate) const fn as_wire(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }
}

/// Verified pull/merge request metadata returned by a provider's view command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PullRequestView {
    pub(crate) number: Option<u64>,
    pub(crate) state: PullRequestViewState,
    pub(crate) title: Option<String>,
    pub(crate) url: String,
    pub(crate) is_draft: bool,
    pub(crate) provider: GitHostProvider,
}

impl PullRequestView {
    /// Summary shape for `/api/createPullRequest` (`{ number?, provider, state, url }`).
    pub(crate) fn to_summary_json(&self) -> Value {
        let mut summary = Map::new();
        if let Some(number) = self.number {
            summary.insert("number".to_string(), json!(number));
        }
        summary.insert("provider".to_string(), json!(self.provider.wire_name()));
        summary.insert("state".to_string(), json!(self.state.as_wire()));
        summary.insert("url".to_string(), json!(self.url));
        Value::Object(summary)
    }

    /// Project Git state shape for `/api/readProjectGitState` (`{ number?, provider, state, title, url }`).
    /// Returns `Value::Null` when no title is available, matching the existing GitHub parser.
    pub(crate) fn to_project_git_json(&self) -> Value {
        let Some(title) = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
        else {
            return Value::Null;
        };
        let mut pr = json!({
            "provider": self.provider.wire_name(),
            "state": self.state.as_wire(),
            "title": title,
            "url": self.url,
        });
        if let Some(number) = self.number {
            pr["number"] = json!(number);
        }
        pr
    }
}

/// Options for creating a pull/merge request. Kept as a struct so future flags (draft, title,
/// body, target branch) can be added without breaking provider signatures.
#[derive(Clone, Debug, Default)]
pub(crate) struct CreatePullRequestOptions {
    pub(crate) fill: bool,
}

/// The resolved provider for a repository, alongside its detected `origin` host (when known) for
/// validating self-hosted URLs.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedRepoProvider {
    pub(crate) kind: GitHostProvider,
    pub(crate) remote_host: Option<String>,
}

impl ResolvedRepoProvider {
    pub(crate) fn provider(&self) -> &'static dyn PullRequestProvider {
        provider_for(self.kind)
    }

    pub(crate) fn expected_host(&self) -> Option<&str> {
        self.remote_host.as_deref()
    }
}

/// Provider abstraction behind pull/merge request create, view, and status.
///
/// Future PR action buttons (merge, mark ready, check status / fix checks, watch) extend this
/// trait with additional command builders or probes while reusing [`resolve_repo_provider`].
pub(crate) trait PullRequestProvider: Send + Sync {
    #[allow(dead_code)]
    fn kind(&self) -> GitHostProvider;

    #[allow(dead_code)]
    fn terms(&self) -> GitHostTerms {
        self.kind().terms()
    }

    /// Build a typed-operation command to check whether the provider CLI is installed.
    fn build_version_command(&self, cwd: &str) -> ProcessCommand;

    /// Build a typed-operation command to create a PR/MR for the current branch.
    fn build_create_command(&self, cwd: &str, options: &CreatePullRequestOptions)
        -> ProcessCommand;

    /// Build a typed-operation command to view the current branch's PR/MR as JSON.
    fn build_view_command(&self, cwd: &str, include_title: bool) -> ProcessCommand;

    /// Parse stdout from [`Self::build_view_command`] into a verified [`PullRequestView`].
    fn parse_view_output(
        &self,
        stdout: &str,
        expected_host: Option<&str>,
    ) -> Option<PullRequestView>;

    /// Validate that a candidate URL is a trusted HTTPS PR/MR URL for this provider.
    fn validate_pr_url(&self, candidate: &str, expected_host: Option<&str>) -> Option<String>;

    /// Failure reason returned by `/api/createPullRequest` when this provider's CLI cannot be run.
    /// GitHub keeps `"githubCliUnavailable"` for wire compatibility with existing clients.
    fn cli_unavailable_reason(&self) -> &'static str {
        "cliUnavailable"
    }

    /// Probe whether this provider's CLI is installed and authenticated (process-wide TTL cache).
    fn is_cli_authenticated(&self) -> bool;

    /// Fast non-spawning cached read of [`Self::is_cli_authenticated`].
    fn cached_cli_authenticated(&self) -> bool;

    /// Blocking background probe of a branch's PR/MR status (sidebar badges, auto-settle,
    /// Session details).
    fn probe_branch_status(&self, cwd: &str, branch: &str) -> Option<SessionPullRequest>;

    /// Batched background probe for `(cwd, branch)` pairs belonging to this provider.
    fn probe_branch_statuses(
        &self,
        wanted: &[(String, String)],
    ) -> Vec<Option<SessionPullRequest>> {
        wanted
            .iter()
            .map(|(cwd, branch)| self.probe_branch_status(cwd, branch))
            .collect()
    }
}

/// Returns the static provider implementation for `kind`.
pub(crate) fn provider_for(kind: GitHostProvider) -> &'static dyn PullRequestProvider {
    match kind {
        GitHostProvider::GitHub => &GITHUB_PROVIDER,
        GitHostProvider::GitLab => &GITLAB_PROVIDER,
    }
}

/// All registered PR/MR providers, in lookup order.
const ALL_PROVIDERS: [&'static dyn PullRequestProvider; 2] = [&GITHUB_PROVIDER, &GITLAB_PROVIDER];

/// Whether at least one supported provider CLI is installed and authenticated.
pub(crate) fn any_provider_cli_authenticated() -> bool {
    ALL_PROVIDERS
        .iter()
        .any(|provider| provider.is_cli_authenticated())
}

/// Resolves the provider from a raw `origin` remote URL (defaulting to GitHub when absent or
/// unrecognized, preserving existing behavior).
pub(crate) fn resolve_repo_provider_from_origin(origin_url: Option<&str>) -> ResolvedRepoProvider {
    let remote_host = origin_url.and_then(extract_remote_host);
    let kind = origin_url
        .and_then(detect_provider_from_remote_url)
        .unwrap_or(GitHostProvider::GitHub);
    ResolvedRepoProvider { kind, remote_host }
}

/// Resolves the provider for a repository's `cwd` asynchronously via `git remote get-url origin`.
pub(crate) async fn resolve_repo_provider(context: &TypedOperationContext) -> ResolvedRepoProvider {
    let command = ProcessCommand::new(
        crate::platform::git_program::git_for_reads(),
        vec!["remote", "get-url", "origin"],
        context.cwd.as_str(),
    );
    let origin = match run_process_command(&command, context).await {
        Ok(output) if output.exit_code == 0 && output.error.is_none() => {
            let trimmed = output.stdout.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        _ => None,
    };
    resolve_repo_provider_from_origin(origin.as_deref())
}

/// Resolves the provider for a repository's `cwd` synchronously (for background polling and
/// Session details), reusing the cached `project_git_remote` entry when available.
pub(crate) fn resolve_repo_provider_sync(cwd: &str) -> ResolvedRepoProvider {
    let cached = crate::project_git_remote::cached_project_git_remote(cwd)
        .and_then(|remote| remote.origin_url);
    let origin = cached.or_else(|| {
        run_git_probe_command(cwd, &["remote", "get-url", "origin"])
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
    });
    resolve_repo_provider_from_origin(origin.as_deref())
}

/// Probes `(cwd, branch)` pull/merge request statuses by grouping checkouts per provider and
/// running each provider's batch probe (GitHub GraphQL, GitLab `glab mr list`, etc.).
pub(crate) fn probe_pull_requests_by_provider(
    wanted: &[(String, String)],
) -> Vec<Option<SessionPullRequest>> {
    let mut answers: Vec<Option<SessionPullRequest>> = vec![None; wanted.len()];
    let mut by_provider: Vec<(GitHostProvider, Vec<(usize, (String, String))>)> = Vec::new();
    for (index, (cwd, branch)) in wanted.iter().enumerate() {
        let resolved = resolve_repo_provider_sync(cwd);
        match by_provider
            .iter_mut()
            .find(|(kind, _)| *kind == resolved.kind)
        {
            Some((_, bucket)) => bucket.push((index, (cwd.clone(), branch.clone()))),
            None => by_provider.push((resolved.kind, vec![(index, (cwd.clone(), branch.clone()))])),
        }
    }
    for (kind, bucket) in by_provider {
        let provider = provider_for(kind);
        if !provider.is_cli_authenticated() {
            continue;
        }
        let pairs: Vec<(String, String)> = bucket.iter().map(|(_, pair)| pair.clone()).collect();
        let results = provider.probe_branch_statuses(&pairs);
        for ((index, _), result) in bucket.into_iter().zip(results.into_iter()) {
            answers[index] = result;
        }
    }
    answers
}
