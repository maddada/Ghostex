//! Starting work on a ticket (`/api/startWorkOnTicket`): a new agent session in a worktree on the
//! ticket's branch, linked to the ticket, with nothing typed into it.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: starting work on a ticket "should just start it but not send any message to that
//! agent": the session starts in a new worktree on the ticket's branch, linked to the ticket, and
//! the user types the first message. A second session on a ticket that already has one works in
//! the first one's worktree and branch (one ticket, one branch, one PR). The Work page's Start
//! chat, Create Linear ticket and `ghostex work-mode start` all come through here.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use crate::agents::{read_agent_settings, DEFAULT_PROMPT_AGENT_ID};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{
    create_worktree_session, schedule_presentation_session_delta, AppState,
    ProjectWorktreeOperationError,
};
use crate::session_git_status::run_gh_command;
use crate::storage::open_gxserver_database;
use crate::worktree_sessions::{read_worktree_session_marker, slugify_branch_title};

use super::*;

/// The ticket a request names: exactly one Linear issue, one GitHub issue or one GitHub PR.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorkTicket {
    Linear(String),
    Github(u64),
    /// A PR number (as text) or URL, as `gh pr view` takes it.
    PullRequest(String),
}

impl WorkTicket {
    pub(crate) fn from_params(params: &Map<String, Value>) -> Result<Self, DomainStateError> {
        let linear = params
            .get("linearIssue")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let github = params.get("githubIssue").filter(|value| !value.is_null());
        let pull_request = params.get("pullRequest").filter(|value| !value.is_null());
        match (linear, github, pull_request) {
            (None, None, Some(value)) => value
                .as_u64()
                .map(|number| number.to_string())
                .or_else(|| {
                    let text = value.as_str()?.trim().trim_start_matches('#');
                    let valid = text.parse::<u64>().is_ok_and(|number| number > 0)
                        || (text.starts_with("https://") && text.contains("/pull/"));
                    valid.then(|| text.to_string())
                })
                .map(Self::PullRequest)
                .ok_or_else(|| {
                    DomainStateError::bad_request("pullRequest must be a PR number or URL.")
                }),
            (Some(text), None, None) => normalize_linear_identifier(text)
                .map(Self::Linear)
                .ok_or_else(|| {
                    DomainStateError::bad_request(format!(
                        "\"{text}\" is not a Linear issue ID like SPX-1245."
                    ))
                }),
            (None, Some(value), None) => value
                .as_u64()
                .or_else(|| {
                    value
                        .as_str()
                        .and_then(|text| text.trim().trim_start_matches('#').parse().ok())
                })
                .filter(|number| *number > 0)
                .map(Self::Github)
                .ok_or_else(|| {
                    DomainStateError::bad_request("githubIssue must be an issue number.")
                }),
            _ => Err(DomainStateError::bad_request(
                "Pass one of linearIssue (SPX-1245), githubIssue (218) or pullRequest (412 or its URL).",
            )),
        }
    }

    /// The `setSessionWorkLinks`-shaped request that links a session to this ticket.
    fn link_request(&self) -> Map<String, Value> {
        let mut request = Map::new();
        match self {
            Self::Linear(identifier) => {
                request.insert("linearIssues".to_string(), json!([identifier]));
            }
            Self::Github(number) => {
                request.insert("githubIssues".to_string(), json!([number]));
            }
            Self::PullRequest(selector) => {
                request.insert("pullRequest".to_string(), json!(selector));
            }
        }
        request
    }

    /// Whether a session already works on this ticket, by its links or its worktree's branch.
    fn is_linked(&self, targets: &WorkTargets, worktree_branch: &str) -> bool {
        match self {
            Self::Linear(identifier) => {
                let key = identifier.split_once('-').map(|(key, _)| key.to_string());
                targets.linear_issues.contains(identifier)
                    || key.is_some_and(|key| {
                        linear_identifiers_in_branch(worktree_branch, &[key]).contains(identifier)
                    })
            }
            Self::Github(number) => {
                targets.github_issues.contains(number)
                    || github_issue_in_branch(worktree_branch) == Some(*number)
            }
            // Decided by the PR's plan, which knows its number and branch.
            Self::PullRequest(_) => false,
        }
    }
}

/// What start-work settled before the session is created.
struct TicketWorkPlan {
    project_id: String,
    agent_id: String,
    /// A worktree a session on this ticket already works in.
    existing_worktree: Option<String>,
    /// The ticket's branch, when there is no such worktree.
    branch: Option<String>,
    /// The links the new session gets.
    links: Map<String, Value>,
    /// `{ number, url }` when the ticket is a PR.
    pull_request: Option<Value>,
}

pub(crate) async fn start_work_on_ticket(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, ProjectWorktreeOperationError> {
    let ticket = WorkTicket::from_params(params)?;
    let plan_state = state.clone();
    let plan_params = params.clone();
    let plan_ticket = ticket.clone();
    let plan = tokio::task::spawn_blocking(move || {
        plan_ticket_work(&plan_state, &plan_params, &plan_ticket)
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Start work task failed: {error}"),
    })??;
    let project_id = plan.project_id.clone();

    let mut create = Map::new();
    create.insert("projectId".to_string(), json!(project_id));
    create.insert("agentId".to_string(), json!(plan.agent_id));
    for (keys, target) in [
        (["model", "agentModel"], "agentModel"),
        (["effort", "agentEffort"], "agentEffort"),
    ] {
        if let Some(value) = keys.iter().find_map(|key| text_param(params, key)) {
            create.insert(target.to_string(), json!(value));
        }
    }
    match (&plan.existing_worktree, &plan.branch) {
        (Some(path), _) => {
            create.insert("existingWorktree".to_string(), json!({ "path": path }));
        }
        (None, Some(branch)) => {
            create.insert("branch".to_string(), json!(branch));
        }
        (None, None) => {
            return Err(
                DomainStateError::bad_request("No branch was found for this ticket.").into(),
            )
        }
    }
    let created = create_worktree_session(state, &create).await?;
    let session_id = created
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // The session is live from here on: a link that fails to save is reported, not rolled back.
    link_session_to_ticket(state, &project_id, &session_id, &plan.links)?;
    Ok(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "branch": created.get("branch").cloned().unwrap_or(Value::Null),
        "worktreePath": created.get("worktreePath").cloned().unwrap_or(Value::Null),
        "pullRequest": plan.pull_request.clone().unwrap_or(Value::Null),
        "links": plan.links,
    }))
}

/// `projectId`, else the project holding `path` (the CLI's current folder).
fn plan_ticket_work(
    state: &AppState,
    params: &Map<String, Value>,
    ticket: &WorkTicket,
) -> Result<TicketWorkPlan, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = resolve_work_mode_project(&repository, params)?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let project_id = project_id.as_str();
    let agent_id = text_param(params, "agentId").unwrap_or_else(|| {
        read_agent_settings(&db)
            .ok()
            .and_then(|settings| {
                settings
                    .get("defaultPromptAgentId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string())
    });

    let pull_request = match ticket {
        WorkTicket::PullRequest(selector) => {
            let project_path = project
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let team_keys = project_linear_api_key(state, &project).and_then(|api_key| {
                refresh_linear_team_keys(&api_key);
                cached_linear_team_keys(linear_key_fingerprint(&api_key))
            });
            Some(plan_pull_request(
                project_path,
                team_keys.as_deref(),
                selector,
            )?)
        }
        _ => None,
    };
    let links = pull_request
        .as_ref()
        .map(|plan| plan.links.clone())
        .unwrap_or_else(|| ticket.link_request());
    let pull_request_value = pull_request
        .as_ref()
        .map(|plan| json!({ "number": plan.number, "url": plan.url }));

    let existing_worktree =
        repository
            .list_sessions(Some(project_id))?
            .iter()
            .find_map(|session| {
                let marker = read_worktree_session_marker(session)?;
                if !Path::new(&marker.path).is_dir() {
                    return None;
                }
                let targets = work_targets(&project, session);
                let linked = match &pull_request {
                    Some(plan) => plan.is_linked(&targets, &marker.branch),
                    None => ticket.is_linked(&targets, &marker.branch),
                };
                linked.then_some(marker.path)
            });
    if existing_worktree.is_some() {
        return Ok(TicketWorkPlan {
            project_id: project_id.to_string(),
            agent_id,
            existing_worktree,
            branch: None,
            links,
            pull_request: pull_request_value,
        });
    }

    let branch = match ticket {
        WorkTicket::Linear(identifier) => {
            let api_key = project_linear_api_key(state, &project).ok_or_else(|| {
                DomainStateError::bad_request(
                    "Set a Linear API key first (Settings, or ghostex work-mode linear-key).",
                )
            })?;
            linear_issue_ticket(&api_key, identifier)
                .map_err(DomainStateError::bad_request)?
                .branch_name
                .ok_or_else(|| {
                    DomainStateError::bad_request(format!(
                        "Linear gave no branch name for {identifier}."
                    ))
                })?
        }
        WorkTicket::Github(number) => {
            let cwd = project
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            github_issue_branch(cwd, *number)?
        }
        WorkTicket::PullRequest(_) => {
            let plan = pull_request
                .as_ref()
                .ok_or_else(|| DomainStateError::bad_request("The PR could not be read."))?;
            plan.prepare_branch(
                project
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?;
            plan.branch.clone()
        }
    };
    Ok(TicketWorkPlan {
        project_id: project_id.to_string(),
        agent_id,
        existing_worktree: None,
        branch: Some(branch),
        links,
        pull_request: pull_request_value,
    })
}

/// `<gh user>/<number>-<slug of the title>`, like `yahia/218-arabic-plan-cards`.
pub(crate) fn github_issue_branch(cwd: &str, number: u64) -> Result<String, DomainStateError> {
    let login = github_login()
        .ok_or_else(|| DomainStateError::bad_request("Sign in to GitHub first (gh auth login)."))?;
    let number_text = number.to_string();
    let title = run_gh_command(
        Some(cwd),
        &[
            "issue",
            "view",
            &number_text,
            "--json",
            "title",
            "--jq",
            ".title",
        ],
    )
    .map(|title| title.trim().to_string())
    .filter(|title| !title.is_empty())
    .ok_or_else(|| {
        DomainStateError::bad_request(format!("GitHub has no issue #{number} in this repository."))
    })?;
    Ok(match slugify_branch_title(&title) {
        Some(slug) => format!("{login}/{number}-{slug}"),
        None => format!("{login}/{number}"),
    })
}

/// The signed-in `gh` user, lowercased like Linear's branch names. Kept once known: it only
/// changes when the user signs in as someone else, and a restart picks that up.
fn github_login() -> Option<String> {
    static LOGIN: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    let cache = LOGIN.get_or_init(|| Mutex::new(None));
    if let Some(login) = cache.lock().ok().and_then(|login| login.clone()) {
        return Some(login);
    }
    let login = run_gh_command(None, &["api", "user", "--jq", ".login"])
        .map(|login| login.trim().to_ascii_lowercase())
        .filter(|login| {
            !login.is_empty() && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })?;
    if let Ok(mut cached) = cache.lock() {
        *cached = Some(login.clone());
    }
    Some(login)
}

fn link_session_to_ticket(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    request: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::bad_request("The new session is gone."))?;
    let existing = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("workLinks"))
        .and_then(Value::as_object);
    let links = merge_work_links(existing, request)?;
    write_work_links(&db, project_id, session_id, &links)?;
    schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)
}

/// The key a project's Linear calls use (its own, its workspace's, or the shared one).
pub(crate) fn project_linear_api_key(state: &AppState, project: &Value) -> Option<String> {
    linear_api_key(
        &state.paths,
        project.get("projectId").and_then(Value::as_str),
        crate::workspaces::stored_project_workspace_id(project),
    )
}

/// The Linear ticket IDs this project's sessions work on, for picking a new ticket's team.
pub(crate) fn project_linear_identifiers(
    state: &AppState,
    project_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(project) = repository.get_project(project_id)? else {
        return Ok(Vec::new());
    };
    Ok(repository
        .list_sessions(Some(project_id))?
        .iter()
        .flat_map(|session| work_targets(&project, session).linear_issues)
        .collect())
}

fn text_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
