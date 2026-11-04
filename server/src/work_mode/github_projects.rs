//! GitHub Projects (Projects v2) for work mode: whether `gh` may read them (the `read:project`
//! scope), the project an issue's or PR's project items name (with the item's Status), a hand-set
//! project's title, and the projects the Link to picker offers. Behind TTL caches the presentation
//! projection only reads; fetched by the background pass and the routes.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User (Q33): link sessions to GitHub Projects too, even though `gh` usually lacks the permission:
//! "Ask user to run that command if needed with a closable notice on the page that appears once.
//! and also in settings." The scope is detected from `gh auth status` (or a refused GraphQL call)
//! and the answer cached; the Work page shows a dismissible notice with the command and Settings
//! shows it on the workspace's tracker row while it is missing.
//!
//! SEE-ALSO: `githubProject` in links.rs (the hand-set link), presentation.rs (the card chip),
//! candidates.rs (the picker), items.rs (the Work page's project filter and notice).

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::session_git_status::run_gh_command;

/// The command the notice and Settings show.
pub(crate) const GITHUB_PROJECTS_SCOPE_COMMAND: &str = "gh auth refresh -s read:project";
const ACCESS_TTL: Duration = Duration::from_secs(10 * 60);
const ITEMS_TTL: Duration = Duration::from_secs(5 * 60);
const PROJECT_TTL: Duration = Duration::from_secs(30 * 60);
const GH_TIMEOUT: Duration = Duration::from_secs(30);
const PROJECT_LIST_LIMIT: &str = "50";

/// Whether `gh` may read GitHub Projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GithubProjectsAccess {
    Granted,
    /// Signed in, but without `read:project` (or `project`).
    MissingScope,
    /// Not asked yet, or `gh` gave no answer (not installed, not signed in).
    Unknown,
}

impl GithubProjectsAccess {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::MissingScope => "missingScope",
            Self::Unknown => "unknown",
        }
    }
}

/// One GitHub Project as the cards and the Work page show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubProjectInfo {
    /// The login that owns the project (a user or an organization).
    pub(crate) owner: String,
    pub(crate) number: u64,
    pub(crate) title: Option<String>,
    pub(crate) url: Option<String>,
    /// The item's Status field (`In progress`), when read from an issue's or PR's project item.
    pub(crate) status: Option<String>,
}

impl GithubProjectInfo {
    /// `owner/number`, what `ghostex link-session --github-project` and the picker store.
    pub(crate) fn reference(&self) -> String {
        format!("{}/{}", self.owner, self.number)
    }
}

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

#[derive(Default)]
struct ProjectsCache {
    access: Option<Cached<GithubProjectsAccess>>,
    /// The projects of an issue's or PR's project items, by checkout and number.
    items: HashMap<String, Cached<Vec<GithubProjectInfo>>>,
    /// `owner/repo` of a checkout's `origin`, so a refresh asks git once per checkout.
    repos: HashMap<String, Option<String>>,
    /// One project by `owner/number`.
    projects: HashMap<String, Cached<Option<GithubProjectInfo>>>,
}

fn cache() -> &'static Mutex<ProjectsCache> {
    static CACHE: OnceLock<Mutex<ProjectsCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProjectsCache::default()))
}

/// What a `gh` run printed, on success or failure.
pub(crate) struct GhRun {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl GhRun {
    /// `gh`'s own words for a failure, for an error the user reads.
    pub(crate) fn error_text(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Runs `gh` like `run_gh_command`, but keeps stdout and stderr when it fails: a refused scope
/// and `gh issue create`'s own error both arrive that way. `None` when `gh` could not start or
/// took longer than `timeout`. After `gh` exits it reads both pipes to their end, so a process `gh`
/// left holding one keeps it waiting.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// `run_gh_command` drops a failed command's output (right for the status probes), but the scope
/// check needs GitHub's refusal text and a failed issue create needs `gh`'s message for the dialog.
pub(crate) fn run_gh_full(cwd: Option<&str>, args: &[&str], timeout: Duration) -> Option<GhRun> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .env("PATH", crate::managed_tools::run::job_path(&[]))
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut tree = crate::platform::process::ProcessTree::spawn(&mut command).ok()?;
    let mut stdout = tree.child().stdout.take()?;
    let mut stderr = tree.child().stderr.take()?;
    let out_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let err_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match tree.child().try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                tree.end();
                return None;
            }
        }
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Some(GhRun {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
    })
}

/// The scopes line of `gh auth status` (`- Token scopes: 'gist', 'read:org', 'repo'`): granted
/// with `read:project` or `project`, missing when the line lists neither, unknown without the
/// line (a token `gh` cannot list scopes for).
pub(crate) fn access_from_auth_status(output: &str) -> GithubProjectsAccess {
    let Some(line) = output
        .lines()
        .find(|line| line.to_ascii_lowercase().contains("token scopes"))
    else {
        return GithubProjectsAccess::Unknown;
    };
    let scopes: Vec<String> = line
        .split_once(':')
        .map(|(_, scopes)| scopes)
        .unwrap_or_default()
        .split(',')
        .map(|scope| scope.trim().trim_matches(['\'', '"']).to_ascii_lowercase())
        .collect();
    if scopes
        .iter()
        .any(|scope| scope == "read:project" || scope == "project")
    {
        GithubProjectsAccess::Granted
    } else {
        GithubProjectsAccess::MissingScope
    }
}

/// Whether GitHub refused a call for a missing Projects scope.
pub(crate) fn is_missing_project_scope(text: &str) -> bool {
    (text.contains("INSUFFICIENT_SCOPES") || text.contains("missing required scopes"))
        && text.contains("project")
}

fn remember_access(access: GithubProjectsAccess) {
    if let Ok(mut cache) = cache().lock() {
        cache.access = Some(Cached {
            value: access,
            fetched_at: Instant::now(),
        });
    }
}

/// The last answer, without asking `gh`.
pub(crate) fn cached_github_projects_access() -> GithubProjectsAccess {
    cache()
        .lock()
        .ok()
        .and_then(|cache| cache.access.as_ref().map(|cached| cached.value))
        .unwrap_or(GithubProjectsAccess::Unknown)
}

/// Asks `gh auth status` when the answer is missing or stale (or `force`). Blocking.
pub(crate) fn refresh_github_projects_access(force: bool) -> GithubProjectsAccess {
    let fresh = cache().lock().ok().and_then(|cache| {
        cache
            .access
            .as_ref()
            .filter(|cached| !force && cached.fetched_at.elapsed() < ACCESS_TTL)
            .map(|cached| cached.value)
    });
    if let Some(access) = fresh {
        return access;
    }
    let access = run_gh_command(None, &["auth", "status"])
        .map(|output| access_from_auth_status(&output))
        .unwrap_or(GithubProjectsAccess::Unknown);
    remember_access(access);
    access
}

const PROJECT_ITEMS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issueOrPullRequest(number: $number) { ... on Issue { projectItems(first: 10) { nodes { ...item } } } ... on PullRequest { projectItems(first: 10) { nodes { ...item } } } } } } fragment item on ProjectV2Item { isArchived project { title url number closed owner { ... on Organization { login } ... on User { login } } } fieldValueByName(name: \"Status\") { ... on ProjectV2ItemFieldSingleSelectValue { name } } }";

fn items_key(cwd: &str, number: u64) -> String {
    format!("{cwd}\u{1f}{number}")
}

/// The GitHub Projects issue or PR `number` of the checkout `cwd` is in, as last read. Empty when
/// it is in none or was not read yet.
pub(crate) fn cached_github_project_items(cwd: &str, number: u64) -> Vec<GithubProjectInfo> {
    cache()
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .items
                .get(&items_key(cwd, number))
                .map(|cached| cached.value.clone())
        })
        .unwrap_or_default()
}

fn checkout_repo(cwd: &str) -> Option<String> {
    if let Some(repo) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.repos.get(cwd).cloned())
    {
        return repo;
    }
    let repo = super::work_repo_of(cwd);
    if let Ok(mut cache) = cache().lock() {
        cache.repos.insert(cwd.to_string(), repo.clone());
    }
    repo
}

/// Re-reads the project items of issue or PR `number` in the checkout `cwd` when stale. Returns
/// whether `gh` ran. Does nothing while the scope is known to be missing.
pub(crate) fn refresh_github_project_items(cwd: &str, number: u64) -> bool {
    if cached_github_projects_access() == GithubProjectsAccess::MissingScope {
        return false;
    }
    let key = items_key(cwd, number);
    if cache().lock().ok().is_some_and(|cache| {
        cache
            .items
            .get(&key)
            .is_some_and(|cached| cached.fetched_at.elapsed() < ITEMS_TTL)
    }) {
        return false;
    }
    let Some(repo) = checkout_repo(cwd) else {
        return false;
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    let query = format!("query={PROJECT_ITEMS_QUERY}");
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let number_arg = format!("number={number}");
    let Some(run) = run_gh_full(
        None,
        &[
            "api",
            "graphql",
            "-f",
            &query,
            "-F",
            &owner_arg,
            "-F",
            &name_arg,
            "-F",
            &number_arg,
        ],
        GH_TIMEOUT,
    ) else {
        return true;
    };
    if is_missing_project_scope(&run.stdout) || is_missing_project_scope(&run.stderr) {
        remember_access(GithubProjectsAccess::MissingScope);
        return true;
    }
    if !run.success {
        return true;
    }
    let Ok(body) = serde_json::from_str::<Value>(&run.stdout) else {
        return true;
    };
    remember_access(GithubProjectsAccess::Granted);
    let items = parse_project_items(&body);
    if let Ok(mut cache) = cache().lock() {
        cache.items.insert(
            key,
            Cached {
                value: items,
                fetched_at: Instant::now(),
            },
        );
    }
    true
}

/// `repository.issueOrPullRequest.projectItems` → the open projects, archived items left out.
pub(crate) fn parse_project_items(body: &Value) -> Vec<GithubProjectInfo> {
    body.pointer("/data/repository/issueOrPullRequest/projectItems/nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|item| item.get("isArchived").and_then(Value::as_bool) != Some(true))
        .filter_map(|item| {
            let project = item.get("project")?;
            if project.get("closed").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            Some(GithubProjectInfo {
                owner: project
                    .pointer("/owner/login")
                    .and_then(Value::as_str)?
                    .to_string(),
                number: project.get("number").and_then(Value::as_u64)?,
                title: text(project, "title"),
                url: text(project, "url"),
                status: item
                    .pointer("/fieldValueByName/name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

/// `owner/number`, `https://github.com/orgs/<owner>/projects/<n>` or
/// `https://github.com/users/<owner>/projects/<n>` → `(owner, number)`.
pub(crate) fn parse_github_project_reference(text: &str) -> Option<(String, u64)> {
    let text = text.trim().trim_end_matches('/');
    if let Some(rest) = text
        .strip_prefix("https://github.com/")
        .or_else(|| text.strip_prefix("http://github.com/"))
    {
        let mut parts = rest.split('/');
        let scope = parts.next()?;
        if scope != "orgs" && scope != "users" {
            return None;
        }
        let owner = parts.next()?;
        if parts.next()? != "projects" {
            return None;
        }
        let number = parts.next()?.split(['?', '#']).next()?.parse().ok()?;
        return valid_owner(owner).then(|| (owner.to_string(), number));
    }
    let (owner, number) = text.rsplit_once('/')?;
    let number: u64 = number.trim_start_matches('#').parse().ok()?;
    (number > 0 && valid_owner(owner)).then(|| (owner.to_string(), number))
}

fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A hand-set project as last read (`gh project view`).
pub(crate) fn cached_github_project(owner: &str, number: u64) -> Option<GithubProjectInfo> {
    cache()
        .lock()
        .ok()?
        .projects
        .get(&format!("{owner}/{number}"))
        .and_then(|cached| cached.value.clone())
}

/// Re-reads a hand-set project's title and link when stale. Returns whether `gh` ran.
pub(crate) fn refresh_github_project(owner: &str, number: u64) -> bool {
    if cached_github_projects_access() == GithubProjectsAccess::MissingScope {
        return false;
    }
    let key = format!("{owner}/{number}");
    if cache().lock().ok().is_some_and(|cache| {
        cache
            .projects
            .get(&key)
            .is_some_and(|cached| cached.fetched_at.elapsed() < PROJECT_TTL)
    }) {
        return false;
    }
    let number_text = number.to_string();
    let Some(run) = run_gh_full(
        None,
        &[
            "project",
            "view",
            &number_text,
            "--owner",
            owner,
            "--format",
            "json",
        ],
        GH_TIMEOUT,
    ) else {
        return true;
    };
    if is_missing_project_scope(&run.stdout) || is_missing_project_scope(&run.stderr) {
        remember_access(GithubProjectsAccess::MissingScope);
        return true;
    }
    let info = run
        .success
        .then(|| serde_json::from_str::<Value>(&run.stdout).ok())
        .flatten()
        .map(|project| GithubProjectInfo {
            owner: owner.to_string(),
            number,
            title: text(&project, "title"),
            url: text(&project, "url"),
            status: None,
        });
    if info.is_some() {
        remember_access(GithubProjectsAccess::Granted);
    }
    if let Ok(mut cache) = cache().lock() {
        cache.projects.insert(
            key,
            Cached {
                value: info,
                fetched_at: Instant::now(),
            },
        );
    }
    true
}

/// The open projects of one owner (`gh project list`), for the Link to picker. `Err` carries the
/// line the picker shows instead.
pub(crate) fn list_github_projects(owner: &str) -> Result<Vec<GithubProjectInfo>, String> {
    let run = run_gh_full(
        None,
        &[
            "project",
            "list",
            "--owner",
            owner,
            "--format",
            "json",
            "--limit",
            PROJECT_LIST_LIMIT,
        ],
        GH_TIMEOUT,
    )
    .ok_or_else(|| "The GitHub CLI (gh) did not answer.".to_string())?;
    if is_missing_project_scope(&run.stdout) || is_missing_project_scope(&run.stderr) {
        remember_access(GithubProjectsAccess::MissingScope);
        return Err(missing_scope_text());
    }
    if !run.success {
        return Err(format!("gh could not list projects: {}", run.error_text()));
    }
    remember_access(GithubProjectsAccess::Granted);
    let body: Value = serde_json::from_str(&run.stdout).unwrap_or(Value::Null);
    Ok(body
        .get("projects")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|project| project.get("closed").and_then(Value::as_bool) != Some(true))
        .filter_map(|project| {
            Some(GithubProjectInfo {
                owner: project
                    .pointer("/owner/login")
                    .and_then(Value::as_str)
                    .unwrap_or(owner)
                    .to_string(),
                number: project.get("number").and_then(Value::as_u64)?,
                title: text(project, "title"),
                url: text(project, "url"),
                status: None,
            })
        })
        .collect())
}

/// The sentence the picker, the Work page and Settings show while the scope is missing.
pub(crate) fn missing_scope_text() -> String {
    format!("To show GitHub Projects, run `{GITHUB_PROJECTS_SCOPE_COMMAND}`.")
}

/// `{ access, command }` for the Work page and Settings.
pub(crate) fn github_projects_status_json() -> Value {
    json!({
        "access": cached_github_projects_access().as_wire(),
        "command": GITHUB_PROJECTS_SCOPE_COMMAND,
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}
