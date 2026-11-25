//! Add Project's GitHub repository list and its sign-in, answered by the computer the project
//! would land on.
//!
//! CDXC:AddProject 2026-11-25 DECISION:
//! User, on the Clone a repo page (Q4961-Q4965): with GitHub picked, the Repository field lists
//! the user's own repositories, one line each, grouped by owner (their account, then each
//! organisation, then repositories other people shared with them), forks included and archived
//! ones left out, newest push first. The list comes from the chosen computer's own `gh` (the
//! sign-in its clone uses, so This Mac, m7-arch and M7-Desktop each list what they can clone),
//! and signing in runs that computer's `gh auth login --web` and shows GitHub's one-time code in
//! the list ("we must use gh cli please"). gxserver keeps the last list in memory so the field
//! shows it at once; the client then asks for a fresh one. Ghostex stores no token, and no
//! token line ever leaves this module: the list holds only names and clone URLs.
//! SEE-ALSO: apps/desktop/src/app/window/add_project_modal/repo_picker.rs,
//! apps/mobile/app src/addProject/repositoryList.ts, server/src/connectors/cli/signin.rs.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use serde_json::{json, Map, Value};

use super::{
    first_safe_auth_line, now_iso, parse_github_auth, run_probe, run_probe_up_to,
    SourceControlError, PROVIDER_PROBE_TIMEOUT_MS,
};

/// `gh api --paginate` over every page of `user/repos` (321 repositories took about 5 seconds).
const LIST_TIMEOUT_MS: u64 = 120_000;
/// 3000 repositories at about 250 bytes a line.
const LIST_OUTPUT_LIMIT_BYTES: usize = 1_000_000;
const LIST_LIMIT: usize = 3_000;
const DEVICE_PAGE: &str = "https://github.com/login/device";

/// One line per repository the user can read, archived ones dropped, compact JSON per line.
const REPOSITORIES_JQ: &str = ".[] | select(.archived | not) | {owner: .owner.login, ownerType: .owner.type, name: .name, private: .private, fork: .fork, pushedAt: .pushed_at, url: .html_url, sshUrl: .ssh_url} | @json";

/// The last list this computer gave, shown at once the next time the field opens.
static CACHE: Mutex<Option<Value>> = Mutex::new(None);

fn cached() -> Option<Value> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn set_cache(value: Option<Value>) {
    *CACHE.lock().unwrap_or_else(PoisonError::into_inner) = value;
}

/// `/api/listRepositories`: `{ provider: "github", refresh?: bool }`. Always answers with a
/// `status` (`ready`, `signedOut`, `missing` or `failed`) rather than an error, so a client can
/// tell this computer's answer apart from an older gxserver that has no such request.
pub(super) async fn list_repositories(params: &Map<String, Value>, cwd: &Path) -> Value {
    let provider = params
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("github");
    if provider != "github" {
        return failed("Only GitHub repositories can be listed.");
    }
    let refresh = params.get("refresh").and_then(Value::as_bool) == Some(true);
    if !refresh {
        if let Some(mut list) = cached() {
            if let Some(object) = list.as_object_mut() {
                object.insert("fromCache".to_string(), json!(true));
            }
            return list;
        }
    }
    let answer = fetch_repositories(cwd).await;
    match answer.get("status").and_then(Value::as_str) {
        Some("ready") => set_cache(Some(answer.clone())),
        Some("signedOut") | Some("missing") => set_cache(None),
        _ => {}
    }
    answer
}

async fn fetch_repositories(cwd: &Path) -> Value {
    let auth = match run_probe(
        "gh",
        &["auth", "status", "--active", "--hostname", "github.com"],
        cwd,
        PROVIDER_PROBE_TIMEOUT_MS,
    )
    .await
    {
        Ok(output) => parse_github_auth(&output),
        Err(detail) => {
            return json!({
                "status": "missing",
                "detail": detail,
                "installTool": "gh",
            });
        }
    };
    if auth.get("status").and_then(Value::as_str) == Some("unauthenticated") {
        return signed_out(auth.get("detail").and_then(Value::as_str));
    }
    let viewer = auth
        .get("account")
        .and_then(Value::as_str)
        .map(str::to_string);
    let output = match run_probe_up_to(
        "gh",
        &[
            "api",
            "--paginate",
            "user/repos?affiliation=owner,collaborator,organization_member&sort=pushed&per_page=100",
            "--jq",
            REPOSITORIES_JQ,
        ],
        cwd,
        LIST_TIMEOUT_MS,
        LIST_OUTPUT_LIMIT_BYTES,
    )
    .await
    {
        Ok(output) => output,
        Err(detail) => return failed(&detail),
    };
    if output.exit_code != 0 {
        let combined = output.combined();
        if combined.contains("401") || combined.contains("auth login") {
            return signed_out(None);
        }
        return failed(
            &first_safe_auth_line(&combined)
                .unwrap_or_else(|| "GitHub CLI couldn't list your repositories.".to_string()),
        );
    }
    let (repositories, truncated) = parse_repository_lines(&output.stdout);
    // An unknown auth status (an older gh) still lists; the owner of the most repositories
    // that is a user stands in for the account.
    let viewer = viewer.or_else(|| busiest_user_owner(&repositories));
    let mut answer = Map::new();
    answer.insert("status".to_string(), json!("ready"));
    if let Some(viewer) = viewer {
        answer.insert("viewer".to_string(), json!(viewer));
    }
    answer.insert("repositories".to_string(), Value::Array(repositories));
    answer.insert("fetchedAt".to_string(), json!(now_iso()));
    answer.insert("fromCache".to_string(), json!(false));
    if truncated {
        answer.insert("truncated".to_string(), json!(true));
    }
    Value::Object(answer)
}

/// The repositories `REPOSITORIES_JQ` printed, in GitHub's order (newest push first). A line that
/// is not a repository is skipped; a stream cut at the output limit drops its last partial line.
fn parse_repository_lines(stdout: &str) -> (Vec<Value>, bool) {
    let mut repositories = Vec::new();
    for line in stdout.lines() {
        if repositories.len() >= LIST_LIMIT {
            return (repositories, true);
        }
        let Ok(Value::Object(raw)) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let text = |key: &str| {
            raw.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(owner), Some(name)) = (text("owner"), text("name")) else {
            continue;
        };
        let owner_kind = if text("ownerType").as_deref() == Some("Organization") {
            "organization"
        } else {
            "user"
        };
        let mut repository = Map::new();
        repository.insert("owner".to_string(), json!(owner));
        repository.insert("ownerKind".to_string(), json!(owner_kind));
        repository.insert("name".to_string(), json!(name));
        repository.insert(
            "private".to_string(),
            json!(raw.get("private").and_then(Value::as_bool) == Some(true)),
        );
        repository.insert(
            "fork".to_string(),
            json!(raw.get("fork").and_then(Value::as_bool) == Some(true)),
        );
        for (key, value) in [
            ("pushedAt", text("pushedAt")),
            ("url", text("url")),
            ("sshUrl", text("sshUrl")),
        ] {
            if let Some(value) = value {
                repository.insert(key.to_string(), json!(value));
            }
        }
        repositories.push(Value::Object(repository));
    }
    (repositories, false)
}

fn busiest_user_owner(repositories: &[Value]) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for repository in repositories {
        if repository.get("ownerKind").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(owner) = repository.get("owner").and_then(Value::as_str) else {
            continue;
        };
        match counts.iter_mut().find(|(name, _)| name == owner) {
            Some((_, count)) => *count += 1,
            None => counts.push((owner.to_string(), 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(owner, _)| owner)
}

fn signed_out(detail: Option<&str>) -> Value {
    json!({
        "status": "signedOut",
        "detail": detail.unwrap_or("Sign in to GitHub to list your repositories."),
    })
}

fn failed(detail: &str) -> Value {
    json!({ "status": "failed", "detail": detail })
}

/// `/api/githubSignIn`: `{ action: "start" | "status" | "cancel" }`. `start` runs this computer's
/// `gh auth login --web` and answers `{ state: "waiting", code, url }`; `status` answers
/// `{ state: "idle" | "waiting" | "done" | "failed" }`.
pub(super) async fn github_sign_in(
    params: &Map<String, Value>,
) -> Result<Value, SourceControlError> {
    use crate::connectors::{
        cancel_machine_gh_login, machine_gh_login, sign_in_machine_gh, MachineGhLogin,
    };
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("status");
    match action {
        "start" => {
            let started = tokio::task::spawn_blocking(|| {
                sign_in_machine_gh(|success| {
                    if success {
                        set_cache(None);
                    }
                })
            })
            .await
            .map_err(|_| SourceControlError::dependency_unavailable("GitHub sign-in stopped."))?
            .map_err(SourceControlError::dependency_unavailable)?;
            // Only GitHub's own page is handed to the client to open.
            let url = if started.url.starts_with("https://github.com/") {
                started.url
            } else {
                DEVICE_PAGE.to_string()
            };
            Ok(json!({ "state": "waiting", "code": started.code, "url": url }))
        }
        "cancel" => {
            cancel_machine_gh_login();
            Ok(json!({ "state": "idle" }))
        }
        "status" => Ok(json!({
            "state": match machine_gh_login() {
                None => "idle",
                Some(MachineGhLogin::Waiting) => "waiting",
                Some(MachineGhLogin::Done) => "done",
                Some(MachineGhLogin::Failed) => "failed",
            }
        })),
        _ => Err(SourceControlError::bad_request(
            "action must be one of start, status, cancel.",
        )),
    }
}
