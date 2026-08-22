use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{json, Map, Value};
use tokio::{process::Command, time::timeout};

use crate::repository_clone::canonical_repository_lookup_url;

/*
CDXC:AddProjectDialog 2026-07-30:
The Add Project dialog offers "clone from GitHub / GitLab" rows, and whether
those rows are usable is a property of the MACHINE the project would land on,
not of the client. gxserver therefore owns provider discovery and repository
lookup: it probes `gh`/`glab` on its own host, reports readiness plus an install
or auth hint, and resolves `owner/repo` into clone URLs. Clients only render
what the daemon reports, which is what makes the dialog work identically for a
local machine and for a remote machine over the tunnel.

Provider output is transport-visible, so auth details are filtered before they
leave this module: any `token:` / `token scopes:` line from `gh auth status` is
dropped, and only the first surviving line is forwarded as a hint.

Bitbucket and Azure DevOps are reported `unsupported` with a hint instead of
being probed. They are not implemented server-side in this pass, and reporting
them as merely "missing" would tell the user to install a CLI that would still
not make the row work.
*/

const PROVIDER_PROBE_TIMEOUT_MS: u64 = 5_000;
const REPOSITORY_LOOKUP_TIMEOUT_MS: u64 = 15_000;
const PROBE_OUTPUT_LIMIT_BYTES: usize = 8_000;
const REPOSITORY_INPUT_LIMIT: usize = 512;

const GITHUB_INSTALL_HINT: &str = "Install the GitHub command-line tool (`gh`) from https://cli.github.com/ or with your package manager (for example `brew install gh`).";
const GITLAB_INSTALL_HINT: &str = "Install the GitLab command-line tool (`glab`) from https://gitlab.com/gitlab-org/cli or with your package manager (for example `brew install glab`).";
const BITBUCKET_UNSUPPORTED_HINT: &str = "Ghostex cannot clone Bitbucket repositories by name yet. Choose Git URL and paste the repository's clone URL instead.";
const AZURE_DEVOPS_UNSUPPORTED_HINT: &str = "Ghostex cannot clone Azure DevOps repositories by name yet. Choose Git URL and paste the repository's clone URL instead.";

#[derive(Clone, Debug)]
pub struct SourceControlError {
    pub code: &'static str,
    pub message: String,
}

impl SourceControlError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "dependencyUnavailable",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for SourceControlError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    AzureDevOps,
    Bitbucket,
    GitHub,
    GitLab,
}

impl ProviderKind {
    fn wire_name(self) -> &'static str {
        match self {
            ProviderKind::AzureDevOps => "azure-devops",
            ProviderKind::Bitbucket => "bitbucket",
            ProviderKind::GitHub => "github",
            ProviderKind::GitLab => "gitlab",
        }
    }

    fn label(self) -> &'static str {
        match self {
            ProviderKind::AzureDevOps => "Azure DevOps",
            ProviderKind::Bitbucket => "Bitbucket",
            ProviderKind::GitHub => "GitHub",
            ProviderKind::GitLab => "GitLab",
        }
    }

    fn install_hint(self) -> &'static str {
        match self {
            ProviderKind::AzureDevOps => AZURE_DEVOPS_UNSUPPORTED_HINT,
            ProviderKind::Bitbucket => BITBUCKET_UNSUPPORTED_HINT,
            ProviderKind::GitHub => GITHUB_INSTALL_HINT,
            ProviderKind::GitLab => GITLAB_INSTALL_HINT,
        }
    }

    fn executable(self) -> Option<&'static str> {
        match self {
            ProviderKind::AzureDevOps | ProviderKind::Bitbucket => None,
            ProviderKind::GitHub => Some("gh"),
            ProviderKind::GitLab => Some("glab"),
        }
    }

    fn from_wire_name(value: &str) -> Option<Self> {
        match value {
            "azure-devops" => Some(ProviderKind::AzureDevOps),
            "bitbucket" => Some(ProviderKind::Bitbucket),
            "github" => Some(ProviderKind::GitHub),
            "gitlab" => Some(ProviderKind::GitLab),
            _ => None,
        }
    }
}

const DISCOVERY_PROVIDER_ORDER: [ProviderKind; 4] = [
    ProviderKind::GitHub,
    ProviderKind::GitLab,
    ProviderKind::Bitbucket,
    ProviderKind::AzureDevOps,
];

#[derive(Debug)]
struct ProbeOutput {
    exit_code: i32,
    stderr: String,
    stdout: String,
}

impl ProbeOutput {
    fn combined(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for entry in [self.stdout.as_str(), self.stderr.as_str()] {
            if !entry.trim().is_empty() {
                parts.push(entry);
            }
        }
        parts.join("\n")
    }
}

pub async fn dispatch_source_control_endpoint(
    endpoint_path: &str,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<Value, SourceControlError> {
    match endpoint_path {
        "/api/discoverSourceControl" => {
            let cwd = resolve_probe_cwd(params.get("cwd"), home_dir)?;
            Ok(json!({ "discovery": discover_source_control(&cwd).await }))
        }
        "/api/lookupRepository" => {
            let cwd = resolve_probe_cwd(params.get("cwd"), home_dir)?;
            Ok(json!({ "repository": lookup_repository(params, &cwd).await? }))
        }
        _ => Err(SourceControlError::not_found(format!(
            "{endpoint_path} is not a gxserver source control endpoint."
        ))),
    }
}

async fn discover_source_control(cwd: &Path) -> Value {
    let mut providers = Vec::with_capacity(DISCOVERY_PROVIDER_ORDER.len());
    for provider in DISCOVERY_PROVIDER_ORDER {
        providers.push(discover_provider(provider, cwd).await);
    }
    json!({
        "checkedAt": now_iso(),
        "providers": providers,
    })
}

async fn discover_provider(provider: ProviderKind, cwd: &Path) -> Value {
    let Some(executable) = provider.executable() else {
        return provider_discovery_item(
            provider,
            None,
            "unsupported",
            None,
            Some(provider.install_hint().to_string()),
            unknown_auth(Some(provider.install_hint().to_string())),
        );
    };
    let version_probe = run_probe(executable, &["--version"], cwd, PROVIDER_PROBE_TIMEOUT_MS).await;
    let version_probe = match version_probe {
        Ok(output) if output.exit_code == 0 => output,
        Ok(output) => {
            let detail = first_safe_auth_line(&output.combined()).unwrap_or_else(|| {
                format!("`{executable} --version` exited {}.", output.exit_code)
            });
            return provider_discovery_item(
                provider,
                Some(executable),
                "missing",
                None,
                Some(detail),
                unknown_auth(Some(
                    "Hosting integration command was not found on the server PATH.".to_string(),
                )),
            );
        }
        Err(detail) => {
            return provider_discovery_item(
                provider,
                Some(executable),
                "missing",
                None,
                Some(detail),
                unknown_auth(Some(
                    "Hosting integration command was not found on the server PATH.".to_string(),
                )),
            );
        }
    };
    let version = first_non_empty_line(&version_probe.stdout)
        .or_else(|| first_non_empty_line(&version_probe.stderr));
    let auth_args: &[&str] = match provider {
        ProviderKind::GitHub => &["auth", "status", "--json", "hosts"],
        ProviderKind::GitLab => &["auth", "status"],
        _ => &[],
    };
    let auth = match run_probe(executable, auth_args, cwd, PROVIDER_PROBE_TIMEOUT_MS).await {
        Ok(output) => match provider {
            ProviderKind::GitHub => parse_github_auth(&output),
            _ => parse_gitlab_auth(&output),
        },
        Err(detail) => unknown_auth(Some(detail)),
    };
    provider_discovery_item(provider, Some(executable), "available", version, None, auth)
}

fn provider_discovery_item(
    provider: ProviderKind,
    executable: Option<&str>,
    status: &str,
    version: Option<String>,
    detail: Option<String>,
    auth: Value,
) -> Value {
    let mut item = Map::new();
    item.insert("auth".to_string(), auth);
    if let Some(detail) = detail {
        item.insert("detail".to_string(), json!(detail));
    }
    if let Some(executable) = executable {
        item.insert("executable".to_string(), json!(executable));
    }
    item.insert("installHint".to_string(), json!(provider.install_hint()));
    item.insert("label".to_string(), json!(provider.label()));
    item.insert("provider".to_string(), json!(provider.wire_name()));
    item.insert("status".to_string(), json!(status));
    if let Some(version) = version {
        item.insert("version".to_string(), json!(version));
    }
    Value::Object(item)
}

fn provider_auth(
    status: &str,
    account: Option<String>,
    host: Option<String>,
    detail: Option<String>,
) -> Value {
    let mut auth = Map::new();
    if let Some(account) = non_empty(account) {
        auth.insert("account".to_string(), json!(account));
    }
    if let Some(detail) = non_empty(detail) {
        auth.insert("detail".to_string(), json!(detail));
    }
    if let Some(host) = non_empty(host) {
        auth.insert("host".to_string(), json!(host));
    }
    auth.insert("status".to_string(), json!(status));
    Value::Object(auth)
}

fn unknown_auth(detail: Option<String>) -> Value {
    provider_auth("unknown", None, None, detail)
}

#[derive(Debug)]
struct GitHubAuthAccount {
    account: String,
    active: bool,
    authenticated: bool,
    error: Option<String>,
    host: String,
}

fn parse_github_auth_accounts(stdout: &str) -> Option<Vec<GitHubAuthAccount>> {
    let parsed: Value = serde_json::from_str(stdout.trim()).ok()?;
    let hosts = parsed.get("hosts")?.as_object()?;
    let mut accounts = Vec::new();
    for entries in hosts.values() {
        for entry in entries.as_array()? {
            let host = entry
                .get("host")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            let login = entry
                .get("login")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            if host.is_empty() || login.is_empty() {
                continue;
            }
            accounts.push(GitHubAuthAccount {
                account: login,
                active: entry.get("active").and_then(Value::as_bool) == Some(true),
                authenticated: entry.get("state").and_then(Value::as_str) == Some("success"),
                error: entry
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                host: host.to_lowercase(),
            });
        }
    }
    Some(accounts)
}

fn parse_github_auth(output: &ProbeOutput) -> Value {
    let combined = output.combined();
    let accounts = parse_github_auth_accounts(&output.stdout);
    if let Some(accounts) = accounts {
        let authenticated = accounts
            .iter()
            .find(|entry| entry.authenticated && entry.active)
            .or_else(|| accounts.iter().find(|entry| entry.authenticated));
        if let Some(account) = authenticated {
            return provider_auth(
                "authenticated",
                Some(account.account.clone()),
                Some(account.host.clone()),
                None,
            );
        }
        let failed = accounts
            .iter()
            .find(|entry| entry.active)
            .or_else(|| accounts.first());
        return provider_auth(
            "unauthenticated",
            None,
            failed.map(|entry| entry.host.clone()),
            Some(
                failed
                    .and_then(|entry| entry.error.clone())
                    .unwrap_or_else(|| {
                        "Run `gh auth login` to authenticate GitHub CLI with an active account."
                            .to_string()
                    }),
            ),
        );
    }
    if output.exit_code != 0 {
        return provider_auth(
            "unauthenticated",
            None,
            None,
            Some(
                first_safe_auth_line(&combined).unwrap_or_else(|| {
                    "Run `gh auth login` to authenticate GitHub CLI.".to_string()
                }),
            ),
        );
    }
    unknown_auth(Some(first_safe_auth_line(&combined).unwrap_or_else(|| {
        "GitHub CLI auth status could not be parsed.".to_string()
    })))
}

fn parse_gitlab_auth(output: &ProbeOutput) -> Value {
    let combined = output.combined();
    let account = parse_logged_in_account(&combined);
    let host = parse_cli_host(&combined);
    if let Some(account) = account {
        return provider_auth("authenticated", Some(account), host, None);
    }
    if output.exit_code != 0 {
        return provider_auth(
            "unauthenticated",
            None,
            host,
            Some(first_safe_auth_line(&combined).unwrap_or_else(|| {
                "Run `glab auth login` to authenticate GitLab CLI.".to_string()
            })),
        );
    }
    provider_auth(
        "unknown",
        None,
        host,
        Some(
            first_safe_auth_line(&combined)
                .unwrap_or_else(|| "GitLab CLI auth status could not be parsed.".to_string()),
        ),
    )
}

async fn lookup_repository(
    params: &Map<String, Value>,
    cwd: &Path,
) -> Result<Value, SourceControlError> {
    let provider_name = params
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let Some(provider) = ProviderKind::from_wire_name(provider_name) else {
        return Err(SourceControlError::bad_request(
            "provider must be one of github, gitlab, bitbucket, azure-devops.",
        ));
    };
    let repository = normalize_repository_input(params.get("repository"), provider)?;
    let Some(executable) = provider.executable() else {
        return Err(SourceControlError::bad_request(provider.install_hint()));
    };
    let args: Vec<String> = match provider {
        ProviderKind::GitHub => vec![
            "repo".to_string(),
            "view".to_string(),
            repository.clone(),
            "--json".to_string(),
            "nameWithOwner,url,sshUrl".to_string(),
        ],
        _ => vec![
            "api".to_string(),
            format!("projects/{}", percent_encode_path_segment(&repository)),
        ],
    };
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_probe(executable, &borrowed, cwd, REPOSITORY_LOOKUP_TIMEOUT_MS)
        .await
        .map_err(|detail| {
            SourceControlError::dependency_unavailable(format!(
                "{} is not available on this machine. {detail}",
                provider.label()
            ))
        })?;
    if output.exit_code != 0 {
        return Err(SourceControlError::not_found(
            first_safe_auth_line(&output.combined()).unwrap_or_else(|| {
                format!(
                    "{} could not find repository {repository}.",
                    provider.label()
                )
            }),
        ));
    }
    let Some(repository_info) = parse_repository_info(provider, &output.stdout) else {
        return Err(SourceControlError::dependency_unavailable(format!(
            "{} returned an unreadable repository response.",
            provider.label()
        )));
    };
    Ok(repository_info)
}

fn parse_repository_info(provider: ProviderKind, stdout: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(stdout.trim()).ok()?;
    let (name_with_owner, url, ssh_url) = match provider {
        ProviderKind::GitHub => (
            parsed.get("nameWithOwner").and_then(Value::as_str)?,
            parsed.get("url").and_then(Value::as_str)?,
            parsed.get("sshUrl").and_then(Value::as_str)?,
        ),
        _ => (
            parsed.get("path_with_namespace").and_then(Value::as_str)?,
            parsed.get("web_url").and_then(Value::as_str)?,
            parsed.get("ssh_url_to_repo").and_then(Value::as_str)?,
        ),
    };
    if name_with_owner.trim().is_empty() || url.trim().is_empty() || ssh_url.trim().is_empty() {
        return None;
    }
    Some(json!({
        "nameWithOwner": name_with_owner.trim(),
        "provider": provider.wire_name(),
        "sshUrl": ssh_url.trim(),
        "url": url.trim(),
    }))
}

fn normalize_repository_input(
    input: Option<&Value>,
    provider: ProviderKind,
) -> Result<String, SourceControlError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if value.is_empty() {
        return Err(SourceControlError::bad_request(
            "repository must be a non-empty string.",
        ));
    }
    if value.chars().count() > REPOSITORY_INPUT_LIMIT {
        return Err(SourceControlError::bad_request(format!(
            "repository exceeds {REPOSITORY_INPUT_LIMIT} characters."
        )));
    }
    if value.contains('\0') {
        return Err(SourceControlError::bad_request(
            "repository must not contain null characters.",
        ));
    }
    /*
    CDXC:AddProjectCloneInput 2026-08-03:
    The GitHub source accepts the same paste formats as Clone Repository. Parse
    commands, shorthand, browser URLs, HTTPS URLs, and SSH URLs into one clone
    URL before handing the positional repository argument to `gh repo view`.
    This keeps lookup and clone identity aligned instead of asking `gh` to
    interpret arbitrary command text such as `gh repo clone owner/repo`.
    */
    if provider == ProviderKind::GitHub {
        return canonical_repository_lookup_url(value)
            .ok_or_else(|| SourceControlError::bad_request("Enter a GitHub repository to clone."));
    }
    if value.contains(char::is_whitespace) {
        return Err(SourceControlError::bad_request(
            "repository must not contain whitespace.",
        ));
    }
    /*
    A repository name is a positional argument to `gh`/`glab`. A leading dash
    would be read as a flag, so it is rejected here rather than handed to the
    CLI.
    */
    if value.starts_with('-') {
        return Err(SourceControlError::bad_request(
            "repository must not start with a dash.",
        ));
    }
    Ok(value.to_string())
}

fn resolve_probe_cwd(
    input: Option<&Value>,
    home_dir: &Path,
) -> Result<PathBuf, SourceControlError> {
    let Some(value) = input.and_then(Value::as_str).map(str::trim) else {
        return Ok(home_dir.to_path_buf());
    };
    if value.is_empty() {
        return Ok(home_dir.to_path_buf());
    }
    let expanded = if value == "~" {
        home_dir.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home_dir.join(rest)
    } else {
        PathBuf::from(value)
    };
    if !expanded.is_absolute() {
        return Err(SourceControlError::bad_request(
            "cwd must be an absolute path or start with ~/",
        ));
    }
    let metadata = fs::metadata(&expanded).map_err(|_| {
        SourceControlError::not_found(format!("cwd does not exist: {}", expanded.display()))
    })?;
    if !metadata.is_dir() {
        return Err(SourceControlError::bad_request(format!(
            "cwd is not a directory: {}",
            expanded.display()
        )));
    }
    Ok(expanded)
}

/// Runs a discovery/lookup command. A non-zero exit is data, not an error:
/// `gh auth status` exits non-zero precisely when the answer is "not signed in".
/// Only spawn failure and the timeout are errors.
async fn run_probe(
    executable: &str,
    args: &[&str],
    cwd: &Path,
    timeout_ms: u64,
) -> Result<ProbeOutput, String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command
        .spawn()
        .map_err(|_| format!("`{executable}` was not found on the server PATH."))?;
    let output = match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("`{executable}` failed to run: {error}")),
        Err(_) => return Err(format!("`{executable}` timed out after {timeout_ms}ms.")),
    };
    Ok(ProbeOutput {
        exit_code: output.status.code().unwrap_or(1),
        stderr: capped_text(&output.stderr),
        stdout: capped_text(&output.stdout),
    })
}

fn capped_text(bytes: &[u8]) -> String {
    let limit = bytes.len().min(PROBE_OUTPUT_LIMIT_BYTES);
    String::from_utf8_lossy(&bytes[..limit]).to_string()
}

fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Auth output can carry `token:` / `- Token scopes:` lines. Those never leave
/// this module.
fn sanitized_auth_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let stripped = line
            .trim_start_matches(|character: char| character == '-' || character.is_whitespace())
            .to_lowercase();
        if stripped.starts_with("token:")
            || stripped.starts_with("token scope:")
            || stripped.starts_with("token scopes:")
        {
            continue;
        }
        lines.push(line.to_string());
    }
    lines
}

fn first_safe_auth_line(text: &str) -> Option<String> {
    sanitized_auth_lines(text).into_iter().next()
}

fn parse_cli_host(text: &str) -> Option<String> {
    sanitized_auth_lines(text)
        .into_iter()
        .map(|line| {
            line.trim_start_matches(|character: char| !character.is_ascii_alphanumeric())
                .to_string()
        })
        .find(|line| is_host_token(line))
}

fn is_host_token(value: &str) -> bool {
    let host = match value.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => value,
    };
    if host.is_empty() {
        return false;
    }
    if !host
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return false;
    }
    host.chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '.' || character == '-')
}

fn parse_logged_in_account(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_lowercase();
        let Some(start) = lower.find("logged in to ") else {
            continue;
        };
        let rest = &line[start + "logged in to ".len()..];
        let rest_lower = rest.to_lowercase();
        let marker = rest_lower
            .find(" as ")
            .map(|index| index + 4)
            .or_else(|| rest_lower.find(" account ").map(|index| index + 9));
        let Some(marker) = marker else {
            continue;
        };
        let token = read_account_token(&rest[marker..]);
        if let Some(token) = token {
            return Some(token);
        }
    }
    for line in text.lines() {
        let lower = line.to_lowercase();
        let Some(index) = lower.find("account:") else {
            continue;
        };
        if let Some(token) = read_account_token(&line[index + "account:".len()..]) {
            return Some(token);
        }
    }
    None
}

fn read_account_token(text: &str) -> Option<String> {
    let token: String = text
        .trim_start()
        .chars()
        .take_while(|character| !character.is_whitespace() && *character != '(')
        .collect();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let character = *byte as char;
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~') {
            encoded.push(character);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(stdout: &str, stderr: &str, exit_code: i32) -> ProbeOutput {
        ProbeOutput {
            exit_code,
            stderr: stderr.to_string(),
            stdout: stdout.to_string(),
        }
    }

    #[test]
    fn github_auth_reports_the_active_authenticated_account() {
        let auth = parse_github_auth(&probe(
            r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"GitHub.com","login":"octocat"}]}}"#,
            "",
            0,
        ));
        assert_eq!(auth["status"], json!("authenticated"));
        assert_eq!(auth["account"], json!("octocat"));
        assert_eq!(auth["host"], json!("github.com"));
        assert!(auth.get("detail").is_none());
    }

    #[test]
    fn github_auth_reports_unauthenticated_with_the_account_error() {
        let auth = parse_github_auth(&probe(
            r#"{"hosts":{"github.com":[{"state":"error","error":"token expired","active":true,"host":"github.com","login":"octocat"}]}}"#,
            "",
            1,
        ));
        assert_eq!(auth["status"], json!("unauthenticated"));
        assert_eq!(auth["detail"], json!("token expired"));
    }

    #[test]
    fn github_auth_never_forwards_token_lines() {
        let auth = parse_github_auth(&probe(
            "",
            "github.com\n  - Token: gho_secretvalue\n  - Not logged in to github.com\n",
            1,
        ));
        assert_eq!(auth["status"], json!("unauthenticated"));
        let detail = auth["detail"].as_str().expect("detail");
        assert!(!detail.contains("gho_secretvalue"), "{detail}");
        assert_eq!(detail, "github.com");
    }

    #[test]
    fn gitlab_auth_reads_the_logged_in_account_and_host() {
        let auth = parse_gitlab_auth(&probe(
            "",
            "gitlab.com\n  ✓ Logged in to gitlab.com as octocat (/home/u/.config/glab-cli/config.yml)\n  ✓ Token: abc123\n",
            0,
        ));
        assert_eq!(auth["status"], json!("authenticated"));
        assert_eq!(auth["account"], json!("octocat"));
        assert_eq!(auth["host"], json!("gitlab.com"));
    }

    #[test]
    fn gitlab_auth_reports_unauthenticated_on_failure() {
        let auth = parse_gitlab_auth(&probe("", "No token provided\n", 1));
        assert_eq!(auth["status"], json!("unauthenticated"));
        assert_eq!(auth["detail"], json!("No token provided"));
    }

    #[test]
    fn repository_info_parses_both_provider_shapes() {
        let github = parse_repository_info(
            ProviderKind::GitHub,
            r#"{"nameWithOwner":"octo/hello","url":"https://github.com/octo/hello","sshUrl":"git@github.com:octo/hello.git"}"#,
        )
        .expect("github info");
        assert_eq!(github["provider"], json!("github"));
        assert_eq!(github["sshUrl"], json!("git@github.com:octo/hello.git"));

        let gitlab = parse_repository_info(
            ProviderKind::GitLab,
            r#"{"path_with_namespace":"group/app","web_url":"https://gitlab.com/group/app","ssh_url_to_repo":"git@gitlab.com:group/app.git"}"#,
        )
        .expect("gitlab info");
        assert_eq!(gitlab["provider"], json!("gitlab"));
        assert_eq!(gitlab["nameWithOwner"], json!("group/app"));
        assert_eq!(gitlab["url"], json!("https://gitlab.com/group/app"));
    }

    #[test]
    fn repository_input_rejects_flag_like_and_blank_values() {
        assert!(normalize_repository_input(Some(&json!("  ")), ProviderKind::GitLab).is_err());
        assert!(normalize_repository_input(Some(&json!("--json")), ProviderKind::GitLab).is_err());
        assert!(
            normalize_repository_input(Some(&json!("octo cat")), ProviderKind::GitLab).is_err()
        );
        assert_eq!(
            normalize_repository_input(Some(&json!(" octo/hello ")), ProviderKind::GitLab)
                .expect("valid"),
            "octo/hello"
        );
    }

    #[test]
    fn github_repository_input_formats_resolve_to_one_canonical_repository() {
        for input in [
            "https://github.com/yyopc/yyork.git",
            "yyopc/yyork",
            "git@github.com:yyopc/yyork.git",
            "gh repo clone yyopc/yyork",
            "github.com/yyopc/yyork",
            "git clone https://github.com/yyopc/yyork",
            "ssh://git@github.com/yyopc/yyork.git",
            "https://github.com/yyopc/yyork/tree/main",
        ] {
            assert_eq!(
                normalize_repository_input(Some(&json!(input)), ProviderKind::GitHub)
                    .expect("valid GitHub repository"),
                "https://github.com/yyopc/yyork.git",
                "input: {input}"
            );
        }
    }

    #[test]
    fn gitlab_project_path_is_percent_encoded() {
        assert_eq!(
            percent_encode_path_segment("group/sub/app"),
            "group%2Fsub%2Fapp"
        );
    }

    #[tokio::test]
    async fn unsupported_providers_are_reported_with_a_hint_and_never_probed() {
        let cwd = std::env::temp_dir();
        for provider in [ProviderKind::Bitbucket, ProviderKind::AzureDevOps] {
            let item = discover_provider(provider, &cwd).await;
            assert_eq!(item["status"], json!("unsupported"));
            assert_eq!(item["provider"], json!(provider.wire_name()));
            assert_eq!(item["installHint"], json!(provider.install_hint()));
            assert_eq!(item["auth"]["status"], json!("unknown"));
            assert!(item.get("executable").is_none());
        }
    }

    #[tokio::test]
    async fn lookup_rejects_unsupported_providers_before_spawning_anything() {
        let error = lookup_repository(
            json!({ "provider": "bitbucket", "repository": "team/app" })
                .as_object()
                .expect("params"),
            &std::env::temp_dir(),
        )
        .await
        .expect_err("unsupported provider rejected");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, BITBUCKET_UNSUPPORTED_HINT);
    }
}
