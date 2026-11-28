//! Hosting provider identity, remote-host detection, PR/MR URL validation, and host-specific
//! user-facing wording shared by gxserver, gx-core, gx-chat-core, desktop and web.
//!
//! Kept intentionally lean so additional hosts (Bitbucket, Azure DevOps, Forgejo) can be added
//! one by one later without reshaping callers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A git hosting provider with pull/merge request support.
///
/// New hosts (`bitbucket`, `azure-devops`, `forgejo`) plug in as additional variants with their
/// own [`GitHostTerms`] and server-side provider implementation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GitHostProvider {
    #[default]
    GitHub,
    GitLab,
}

impl GitHostProvider {
    /// Canonical wire name shared with `/api/discoverSourceControl` (`"github"`, `"gitlab"`).
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }

    pub fn from_wire_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            _ => None,
        }
    }

    /// Host-specific user-facing wording for existing buttons, menus, toasts and prompts.
    pub const fn terms(self) -> GitHostTerms {
        match self {
            Self::GitHub => GITHUB_TERMS,
            Self::GitLab => GITLAB_TERMS,
        }
    }
}

impl Serialize for GitHostProvider {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.wire_name())
    }
}

impl<'de> Deserialize<'de> for GitHostProvider {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_wire_name(&value).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown git host provider: {value}"))
        })
    }
}

/// Lenient deserializer for optional wire `provider` / `gitProvider` fields: an unknown future
/// provider name (e.g. from a newer remote gxserver) reads as `None` instead of failing the frame.
pub fn deserialize_optional_git_host_provider<'de, D>(
    deserializer: D,
) -> Result<Option<GitHostProvider>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    Ok(value.as_deref().and_then(GitHostProvider::from_wire_name))
}

/// Host-specific words used in existing buttons, menu rows, disabled reasons, toasts and prompts.
///
/// Every GitHub field keeps the exact wording Ghostex already shipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitHostTerms {
    /// Host name: `"GitHub"`, `"GitLab"`.
    pub host_name: &'static str,
    /// CLI display name: `"GitHub CLI"`, `"GitLab CLI"`.
    pub cli_name: &'static str,
    /// CLI executable name: `"gh"`, `"glab"`.
    pub cli_executable: &'static str,
    /// Short abbreviation: `"PR"`, `"MR"`.
    pub short: &'static str,
    /// Indefinite short phrase: `"a PR"`, `"an MR"`.
    pub a_short: &'static str,
    /// Lowercase noun: `"pull request"`, `"merge request"`.
    pub singular: &'static str,
    /// Title-cased noun: `"Pull request"`, `"Merge request"`.
    pub singular_title: &'static str,
    /// Indefinite noun: `"a pull request"`, `"a merge request"`.
    pub a_singular: &'static str,
    /// Plural lowercase noun: `"pull requests"`, `"merge requests"`.
    pub plural: &'static str,
    /// Reference prefix before numbers: `"#"`, `"!"`.
    pub number_prefix: &'static str,
    /// Non-interactive CLI create command shown in agent prompts.
    pub create_command: &'static str,
    /// `"Create PR"` / `"Create MR"`.
    pub create_short_label: &'static str,
    /// `"View PR"` / `"View MR"`.
    pub view_short_label: &'static str,
    /// `"Push & Create PR"` / `"Push & Create MR"`.
    pub push_create_short_label: &'static str,
    /// `"Push & View PR"` / `"Push & View MR"`.
    pub push_view_short_label: &'static str,
    /// `"Commit, Push & PR"` / `"Commit, Push & MR"`.
    pub commit_push_short_label: &'static str,
    /// `"Commit, Push & Create PR"` / `"Commit, Push & Create MR"`.
    pub commit_push_create_label: &'static str,
    /// `"Commit, Push & Create PR on a new branch"` / `"Commit, Push & Create MR on a new branch"`.
    pub commit_push_create_new_branch_label: &'static str,
    /// `"Commit, push, then open a pull request."` / `"Commit, push, then open a merge request."`.
    pub commit_push_create_detail: &'static str,
    /// `"Create or open a pull request."` / `"Create or open a merge request."`.
    pub prompt_description: &'static str,
    /// `"Committing, pushing, and creating PR…"` / `"Committing, pushing, and creating MR…"`.
    pub started_commit_title: &'static str,
    /// `"Pushing and creating PR…"` / `"Pushing and creating MR…"`.
    pub started_push_title: &'static str,
    /// `"Pull request ready"` / `"Merge request ready"`.
    pub finished_title: &'static str,
    /// `"GitHub CLI unavailable"` / `"GitLab CLI unavailable"`.
    pub cli_unavailable_title: &'static str,
    /// `"Remote GitHub CLI unavailable"` / `"Remote GitLab CLI unavailable"`.
    pub remote_cli_unavailable_title: &'static str,
    /// `"Install GitHub CLI before creating a pull request."` / `"Install GitLab CLI before creating a merge request."`.
    pub cli_unavailable_description: &'static str,
    /// `"Install GitHub CLI on the remote machine before creating a pull request."` / ...
    pub remote_cli_unavailable_description: &'static str,
    /// `"Install GitHub CLI to create or view pull requests."` / ...
    pub cli_missing_disabled_reason: &'static str,
    /// `"Create and checkout a branch before pushing or creating a PR."` / ...
    pub branch_required_disabled_reason: &'static str,
    /// `"Add an \"origin\" remote before creating a PR."` / ...
    pub origin_required_disabled_reason: &'static str,
    /// `"No branch state available for PR creation."` / ...
    pub no_branch_state_disabled_reason: &'static str,
    /// `"GitHub CLI could not create or find an open pull request."` / ...
    pub cli_create_or_find_failed: &'static str,
    /// `"No open pull request is available for this project."` / ...
    pub no_open_for_project: &'static str,
    /// `"No open remote pull request is available for this project."` / ...
    pub no_open_remote_for_project: &'static str,
    /// `"Ghostex couldn't open the pull request."` / ...
    pub could_not_open: &'static str,
    /// `"Ghostex couldn't open the remote pull request."` / ...
    pub could_not_open_remote: &'static str,
    /// `"No pull request was found for this project."` / ...
    pub not_found_for_project: &'static str,
    /// `"Pull request workflow started"` / `"Merge request workflow started"`.
    pub workflow_started_title: &'static str,
    /// `"Remote pull request workflow started"` / `"Remote merge request workflow started"`.
    pub remote_workflow_started_title: &'static str,
    /// `"Pull request workflow failed"` / `"Merge request workflow failed"`.
    pub workflow_failed_title: &'static str,
    /// `"Remote pull request workflow failed"` / `"Remote merge request workflow failed"`.
    pub remote_workflow_failed_title: &'static str,
    /// `"Choose a configured prompt agent before creating a pull request."` / ...
    pub agent_required_description: &'static str,
    /// `"Choose files from the current Git review before creating a pull request."` / ...
    pub invalid_files_description: &'static str,
    /// `"Choose files from the current remote Git review before creating a pull request."` / ...
    pub remote_invalid_files_description: &'static str,
    /// `"Pull request creation did not complete."` / `"Merge request creation did not complete."`.
    pub creation_incomplete_description: &'static str,
}

const GITHUB_TERMS: GitHostTerms = GitHostTerms {
    host_name: "GitHub",
    cli_name: "GitHub CLI",
    cli_executable: "gh",
    short: "PR",
    a_short: "a PR",
    singular: "pull request",
    singular_title: "Pull request",
    a_singular: "a pull request",
    plural: "pull requests",
    number_prefix: "#",
    create_command: "gh pr create --fill",
    create_short_label: "Create PR",
    view_short_label: "View PR",
    push_create_short_label: "Push & Create PR",
    push_view_short_label: "Push & View PR",
    commit_push_short_label: "Commit, Push & PR",
    commit_push_create_label: "Commit, Push & Create PR",
    commit_push_create_new_branch_label: "Commit, Push & Create PR on a new branch",
    commit_push_create_detail: "Commit, push, then open a pull request.",
    prompt_description: "Create or open a pull request.",
    started_commit_title: "Committing, pushing, and creating PR\u{2026}",
    started_push_title: "Pushing and creating PR\u{2026}",
    finished_title: "Pull request ready",
    cli_unavailable_title: "GitHub CLI unavailable",
    remote_cli_unavailable_title: "Remote GitHub CLI unavailable",
    cli_unavailable_description: "Install GitHub CLI before creating a pull request.",
    remote_cli_unavailable_description:
        "Install GitHub CLI on the remote machine before creating a pull request.",
    cli_missing_disabled_reason: "Install GitHub CLI to create or view pull requests.",
    branch_required_disabled_reason:
        "Create and checkout a branch before pushing or creating a PR.",
    origin_required_disabled_reason: "Add an \"origin\" remote before creating a PR.",
    no_branch_state_disabled_reason: "No branch state available for PR creation.",
    cli_create_or_find_failed: "GitHub CLI could not create or find an open pull request.",
    no_open_for_project: "No open pull request is available for this project.",
    no_open_remote_for_project: "No open remote pull request is available for this project.",
    could_not_open: "Ghostex couldn't open the pull request.",
    could_not_open_remote: "Ghostex couldn't open the remote pull request.",
    not_found_for_project: "No pull request was found for this project.",
    workflow_started_title: "Pull request workflow started",
    remote_workflow_started_title: "Remote pull request workflow started",
    workflow_failed_title: "Pull request workflow failed",
    remote_workflow_failed_title: "Remote pull request workflow failed",
    agent_required_description: "Choose a configured prompt agent before creating a pull request.",
    invalid_files_description:
        "Choose files from the current Git review before creating a pull request.",
    remote_invalid_files_description:
        "Choose files from the current remote Git review before creating a pull request.",
    creation_incomplete_description: "Pull request creation did not complete.",
};

const GITLAB_TERMS: GitHostTerms = GitHostTerms {
    host_name: "GitLab",
    cli_name: "GitLab CLI",
    cli_executable: "glab",
    short: "MR",
    a_short: "an MR",
    singular: "merge request",
    singular_title: "Merge request",
    a_singular: "a merge request",
    plural: "merge requests",
    number_prefix: "!",
    create_command: "glab mr create --fill --yes",
    create_short_label: "Create MR",
    view_short_label: "View MR",
    push_create_short_label: "Push & Create MR",
    push_view_short_label: "Push & View MR",
    commit_push_short_label: "Commit, Push & MR",
    commit_push_create_label: "Commit, Push & Create MR",
    commit_push_create_new_branch_label: "Commit, Push & Create MR on a new branch",
    commit_push_create_detail: "Commit, push, then open a merge request.",
    prompt_description: "Create or open a merge request.",
    started_commit_title: "Committing, pushing, and creating MR\u{2026}",
    started_push_title: "Pushing and creating MR\u{2026}",
    finished_title: "Merge request ready",
    cli_unavailable_title: "GitLab CLI unavailable",
    remote_cli_unavailable_title: "Remote GitLab CLI unavailable",
    cli_unavailable_description: "Install GitLab CLI before creating a merge request.",
    remote_cli_unavailable_description:
        "Install GitLab CLI on the remote machine before creating a merge request.",
    cli_missing_disabled_reason: "Install GitLab CLI to create or view merge requests.",
    branch_required_disabled_reason:
        "Create and checkout a branch before pushing or creating an MR.",
    origin_required_disabled_reason: "Add an \"origin\" remote before creating an MR.",
    no_branch_state_disabled_reason: "No branch state available for MR creation.",
    cli_create_or_find_failed: "GitLab CLI could not create or find an open merge request.",
    no_open_for_project: "No open merge request is available for this project.",
    no_open_remote_for_project: "No open remote merge request is available for this project.",
    could_not_open: "Ghostex couldn't open the merge request.",
    could_not_open_remote: "Ghostex couldn't open the remote merge request.",
    not_found_for_project: "No merge request was found for this project.",
    workflow_started_title: "Merge request workflow started",
    remote_workflow_started_title: "Remote merge request workflow started",
    workflow_failed_title: "Merge request workflow failed",
    remote_workflow_failed_title: "Remote merge request workflow failed",
    agent_required_description: "Choose a configured prompt agent before creating a merge request.",
    invalid_files_description:
        "Choose files from the current Git review before creating a merge request.",
    remote_invalid_files_description:
        "Choose files from the current remote Git review before creating a merge request.",
    creation_incomplete_description: "Merge request creation did not complete.",
};

/// Extracts the lowercased hostname from a Git remote URL (HTTPS, HTTP, SSH URL, or SCP-style
/// `git@host:path`), ignoring credentials and ports.
pub fn extract_remote_host(remote_url: &str) -> Option<String> {
    let first = remote_url.split_whitespace().next()?.trim();
    if first.is_empty() {
        return None;
    }
    let host = if let Some((_, rest)) = first.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let without_user = authority.rsplit('@').next().unwrap_or(authority);
        without_user.split(':').next().unwrap_or(without_user)
    } else if let Some((before_colon, _)) = first.split_once(':') {
        if before_colon.contains('/') {
            return None;
        }
        before_colon.rsplit('@').next().unwrap_or(before_colon)
    } else {
        return None;
    };
    let host = host.trim().to_ascii_lowercase();
    if is_valid_host(&host) {
        Some(host)
    } else {
        None
    }
}

/// Detects the hosting provider from a Git remote URL's hostname.
pub fn detect_provider_from_remote_url(remote_url: &str) -> Option<GitHostProvider> {
    let host = extract_remote_host(remote_url)?;
    detect_provider_from_host(&host)
}

/// Detects the hosting provider from a lowercased or mixed-case hostname.
pub fn detect_provider_from_host(host: &str) -> Option<GitHostProvider> {
    let host = host.trim().to_ascii_lowercase();
    if host == "github.com" {
        return Some(GitHostProvider::GitHub);
    }
    if is_gitlab_host(&host) {
        return Some(GitHostProvider::GitLab);
    }
    None
}

/// Whether `host` is `gitlab.com`, a self-hosted `gitlab.*` / `*.gitlab.*` host, or a `gitlab-*`
/// SSH alias.
pub fn is_gitlab_host(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    if !is_valid_host(&lower) {
        return false;
    }
    lower == "gitlab.com"
        || lower
            .split('.')
            .any(|label| label == "gitlab" || label.starts_with("gitlab-"))
}

/// Validates a trusted HTTPS GitHub pull request URL (`https://github.com/<owner>/<repo>/pull/<n>`).
pub fn validate_github_pull_request_url(candidate: &str) -> Option<String> {
    let (host, port, segments) = parse_trusted_https_url(candidate)?;
    if !host.eq_ignore_ascii_case("github.com") || port.is_some() {
        return None;
    }
    if segments.len() != 4
        || segments[2] != "pull"
        || !segments[3].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(format!(
        "https://github.com/{}/{}/pull/{}",
        segments[0], segments[1], segments[3]
    ))
}

/// Validates a trusted HTTPS GitLab merge request URL
/// (`https://<host>[:<port>]/<namespace...>/<project>/-/merge_requests/<iid>`).
///
/// When `expected_host` is supplied (from the repository's `origin` remote), the URL's host must
/// match it case-insensitively.
pub fn validate_gitlab_merge_request_url(
    candidate: &str,
    expected_host: Option<&str>,
) -> Option<String> {
    let (host, port, segments) = parse_trusted_https_url(candidate)?;
    let lower_host = host.to_ascii_lowercase();
    if let Some(expected) = expected_host.map(str::trim).filter(|host| !host.is_empty()) {
        if !lower_host.eq_ignore_ascii_case(expected) {
            return None;
        }
    } else if !is_gitlab_host(&lower_host) {
        return None;
    }
    // `<group>[/<subgroup>...]/<project>/-/merge_requests/<iid>`: at least 5 segments.
    let len = segments.len();
    if len < 5
        || segments[len - 3] != "-"
        || segments[len - 2] != "merge_requests"
        || !segments[len - 1].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let path = segments.join("/");
    match port {
        Some(port) => Some(format!("https://{lower_host}:{port}/{path}")),
        None => Some(format!("https://{lower_host}/{path}")),
    }
}

/// Validates a pull/merge request URL for the given provider.
pub fn validate_pull_request_url(
    provider: GitHostProvider,
    candidate: &str,
    expected_host: Option<&str>,
) -> Option<String> {
    match provider {
        GitHostProvider::GitHub => validate_github_pull_request_url(candidate),
        GitHostProvider::GitLab => validate_gitlab_merge_request_url(candidate, expected_host),
    }
}

/// Detects the provider of a validated PR/MR URL, if it matches a supported host.
pub fn detect_provider_from_pr_url(candidate: &str) -> Option<GitHostProvider> {
    if validate_github_pull_request_url(candidate).is_some() {
        return Some(GitHostProvider::GitHub);
    }
    if validate_gitlab_merge_request_url(candidate, None).is_some() {
        return Some(GitHostProvider::GitLab);
    }
    None
}

/// Extracts the PR/MR number from a validated URL for the given provider.
pub fn pull_request_number_from_url(provider: GitHostProvider, url: &str) -> Option<u64> {
    let (_, _, segments) = parse_trusted_https_url(url)?;
    match provider {
        GitHostProvider::GitHub => segments.get(3)?.parse::<u64>().ok(),
        GitHostProvider::GitLab => segments.last()?.parse::<u64>().ok(),
    }
}

fn parse_trusted_https_url(candidate: &str) -> Option<(&str, Option<u16>, Vec<&str>)> {
    let trimmed = candidate.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 2048
        || trimmed.contains('\\')
        || trimmed.contains('?')
        || trimmed.contains('#')
        || trimmed
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return None;
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| {
            trimmed
                .get(..8)
                .filter(|scheme| scheme.eq_ignore_ascii_case("https://"))
                .and_then(|_| trimmed.get(8..))
        })?;
    let (authority, path) = rest.split_once('/')?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port_text)) => {
            if port_text.is_empty() || !port_text.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let port = port_text.parse::<u16>().ok().filter(|port| *port > 0)?;
            (host, Some(port))
        }
        None => (authority, None),
    };
    if !is_valid_host(host) {
        return None;
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }
    Some((host, port, segments))
}

fn is_valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && host
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        && !host.contains("..")
}
