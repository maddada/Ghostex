//! "Start in cloud" on a ticket (the Work page's menu, `ghostex work-mode start <ticket> --cloud`):
//! a cloud session (crate::cloud_runner) on the ticket's branch with a task made from the ticket,
//! recorded against the ticket so the ticket's Conversations card lists it.
//!
//! CDXC:WorkMode 2026-10-10 WHY:
//! A cloud session needs its first message to start (it cannot start empty like Start chat), so
//! the task is drafted here (`draft_cloud_work`) and shown for editing before Start; the CLI's
//! `--cloud` without `--prompt-file` sends the same draft. The session works on the ticket's
//! branch when GitHub has it (`--on-branch`) and is told to create it otherwise, since
//! `--on-branch` needs a branch the cloud can check out.
//!
//! CDXC:WorkMode 2026-10-10 WHY:
//! Every cloud session started here is kept in `work-cloud-sessions.json` on this computer, and a
//! team workspace's is also added to the team's `ticketSessions` (so teammates see it and a Slack
//! request for the ticket goes to it). The local record is what shows it as yours; the details
//! page leaves out the team's row with the same link.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::cloud_runner::{
    cloud_provider, CloudStartRequest, CLOUD_PROVIDERS, MAX_CLOUD_PROMPT_CHARS,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::storage::open_gxserver_database;
use crate::workspaces::{project_workspace_id, read_sidebar_workspaces};

use super::*;

const CLOUD_SESSIONS_FILE_NAME: &str = "work-cloud-sessions.json";
/// The newest records kept; older ones are dropped when a new one is added.
const MAX_RECORDS: usize = 500;
/// How much of the ticket's description the drafted task quotes.
const EXCERPT_CHARS: usize = 600;

static RECORDS_LOCK: Mutex<()> = Mutex::new(());

/// What a cloud start on a ticket settled before the task is written.
struct CloudTicketPlan {
    project_id: String,
    project_path: String,
    workspace_id: String,
    ticket: WorkTicket,
    /// The key the team's Convex project and the local records use: `SPX-1245`, or
    /// `owner/repo#12` for a GitHub issue or PR (team_sync/work_page.rs `team_ticket_key`).
    ticket_key: String,
    /// How the task names the ticket: `SPX-1245`, `#218`, `PR #412`.
    label: String,
    title: Option<String>,
    url: Option<String>,
    description: Option<String>,
    branch: String,
    branch_on_remote: bool,
    team: bool,
    instructions: Option<String>,
    /// Things that did not stop the start, shown next to the task.
    warnings: Vec<String>,
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn internal(message: String) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message,
    }
}

/// The cloud menu's rows, for the details page.
pub(crate) fn cloud_providers_wire() -> Value {
    Value::Array(
        CLOUD_PROVIDERS
            .iter()
            .map(|provider| provider.wire())
            .collect(),
    )
}

fn plan_cloud_ticket(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<CloudTicketPlan, DomainStateError> {
    let ticket = WorkTicket::from_params(params)?;
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = resolve_work_mode_project(&repository, params)?;
    let project_id = value_text(&project, "projectId").unwrap_or_default();
    let project_path = value_text(&project, "path")
        .ok_or_else(|| DomainStateError::bad_request("This project has no folder."))?;
    let repo = work_repo_of(&project_path)
        .map(|repo| repo.to_ascii_lowercase())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This project has no GitHub remote (origin), so a cloud session cannot get its code.",
            )
        })?;
    let projects = repository.list_projects()?;
    let workspaces = read_sidebar_workspaces(&db)?;
    let workspace_id = project_workspace_id(&workspaces, &project, &projects);
    let linear_key = project_linear_api_key(state, &project);
    let detail = work_ticket_detail(linear_key.as_deref(), &project_path, &ticket)
        .map_err(DomainStateError::bad_request)?;

    let (ticket_key, label, branch) = match &ticket {
        WorkTicket::Linear(identifier) => (
            identifier.clone(),
            identifier.clone(),
            value_text(&detail, "branchName").ok_or_else(|| {
                DomainStateError::bad_request(format!(
                    "Linear gave no branch name for {identifier}."
                ))
            })?,
        ),
        WorkTicket::Github(number) => (
            format!("{repo}#{number}"),
            format!("#{number}"),
            github_issue_branch(&project_path, *number)?,
        ),
        WorkTicket::PullRequest(_) => {
            let number = detail
                .get("number")
                .and_then(Value::as_u64)
                .ok_or_else(|| DomainStateError::bad_request("The PR could not be read."))?;
            (
                format!("{repo}#{number}"),
                format!("PR #{number}"),
                value_text(&detail, "headBranch").ok_or_else(|| {
                    DomainStateError::bad_request("GitHub gave no branch for this PR.")
                })?,
            )
        }
    };

    let mut warnings = Vec::new();
    let team = crate::team_sync::workspace_has_team(&state.paths, &workspace_id);
    let instructions = if team {
        match crate::team_sync::read_team_flow(&state.paths, &workspace_params(&workspace_id)) {
            Ok(flow) => value_text(&flow, "instructions"),
            Err(error) => {
                warnings.push(format!("The team instructions could not be read: {error}"));
                None
            }
        }
    } else {
        None
    };
    Ok(CloudTicketPlan {
        branch_on_remote: branch_on_remote(&project_path, &branch),
        project_id,
        project_path,
        workspace_id,
        ticket,
        ticket_key,
        label,
        title: value_text(&detail, "title"),
        url: value_text(&detail, "url"),
        description: value_text(&detail, "description").or_else(|| value_text(&detail, "body")),
        branch,
        team,
        instructions,
        warnings,
    })
}

/// Whether `origin` has the branch (exact name), asked without a credentials prompt.
fn branch_on_remote(project_path: &str, branch: &str) -> bool {
    let reference = format!("refs/heads/{branch}");
    crate::platform::process::background_command("git")
        .args([
            "-C",
            project_path,
            "ls-remote",
            "--exit-code",
            "--heads",
            "origin",
        ])
        .arg(&reference)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The task's line about the branch and the PR.
fn branch_line(plan: &CloudTicketPlan) -> String {
    let branch = &plan.branch;
    let label = &plan.label;
    match &plan.ticket {
        WorkTicket::PullRequest(_) => format!(
            "This is {label}: work on its branch `{branch}`, push your commits there and do not open another pull request."
        ),
        ticket => {
            let link = match ticket {
                WorkTicket::Github(number) => format!(
                    "that says \"Closes #{number}\" in its description, so GitHub links it to the issue"
                ),
                _ => format!(
                    "that names {label} in its title or description, so Linear links it to the ticket"
                ),
            };
            if plan.branch_on_remote {
                format!("Work on the branch `{branch}` and open a pull request from it {link}.")
            } else {
                format!(
                    "Create the branch `{branch}` from the default branch, work on it, and open a pull request from it {link}."
                )
            }
        }
    }
}

fn excerpt(description: &str) -> String {
    let description = description.trim();
    if description.chars().count() <= EXCERPT_CHARS {
        return description.to_string();
    }
    let cut: String = description.chars().take(EXCERPT_CHARS).collect();
    let end = cut
        .rfind('\n')
        .filter(|end| *end > EXCERPT_CHARS / 2)
        .or_else(|| cut.rfind(' '))
        .unwrap_or(cut.len());
    format!(
        "{}…\n(The full description is on the ticket.)",
        cut[..end].trim_end()
    )
}

/// The task the box opens with, and what `--cloud` sends without `--prompt-file`.
fn default_task(plan: &CloudTicketPlan) -> String {
    let mut out = format!("Work on {}", plan.label);
    if let Some(title) = &plan.title {
        out.push_str(&format!(": {title}"));
    }
    out.push('\n');
    if let Some(url) = &plan.url {
        out.push_str(&format!("{url}\n"));
    }
    if let Some(description) = &plan.description {
        out.push_str(&format!("\n{}\n", excerpt(description)));
    }
    out.push_str(&format!("\n{}\n", branch_line(plan)));
    if let Some(instructions) = &plan.instructions {
        out.push_str(&format!("\n## Team instructions\n{instructions}\n"));
    }
    out
}

/// A task written by hand still names the branch the session must use.
fn task_with_branch(task: &str, plan: &CloudTicketPlan) -> String {
    if task.contains(&plan.branch) {
        task.to_string()
    } else {
        format!("{}\n\n{}", task.trim_end(), branch_line(plan))
    }
}

/// `{ projectId | path, linearIssue | githubIssue | pullRequest }` → `{ prompt, branch,
/// branchOnRemote, ticket, label, team, maxChars, warnings }`. Blocking.
pub(crate) fn draft_cloud_work(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let plan = plan_cloud_ticket(state, params)?;
    Ok(json!({
        "prompt": default_task(&plan),
        "branch": plan.branch,
        "branchOnRemote": plan.branch_on_remote,
        "ticket": plan.ticket_key,
        "label": plan.label,
        "projectId": plan.project_id,
        "team": plan.team,
        "maxChars": MAX_CLOUD_PROMPT_CHARS,
        "warnings": plan.warnings,
    }))
}

/// `{ projectId | path, linearIssue | githubIssue | pullRequest, provider, prompt? }` → `{
/// sessionUrl, provider, runner, branch, onBranch, ticket, teamRecorded, warnings }`. Blocking:
/// the cloud start takes 5 to 30 seconds (up to the runner's own limit).
pub(crate) fn start_cloud_work(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let provider_id = text(params, "provider").unwrap_or_default();
    let provider = cloud_provider(&provider_id).ok_or_else(|| {
        let ids: Vec<&str> = CLOUD_PROVIDERS.iter().map(|provider| provider.id).collect();
        DomainStateError::bad_request(format!("Pass the cloud to start in: {}.", ids.join(", ")))
    })?;
    let plan = plan_cloud_ticket(state, params)?;
    let prompt = match text(params, "prompt") {
        Some(task) => task_with_branch(&task, &plan),
        None => default_task(&plan),
    };
    let length = prompt.chars().count();
    if length > MAX_CLOUD_PROMPT_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "The task is {length} characters; a cloud session starts with at most {MAX_CLOUD_PROMPT_CHARS}. Shorten it and try again."
        )));
    }
    let runner = provider.runner(&state.paths.home_dir);
    let session = runner
        .start(&CloudStartRequest {
            repo_dir: Path::new(&plan.project_path),
            prompt: &prompt,
            on_branch: plan.branch_on_remote.then_some(plan.branch.as_str()),
        })
        .map_err(DomainStateError::bad_request)?;
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    let record = json!({
        "sessionUrl": session.url,
        "provider": provider.id,
        "providerName": provider.name,
        "runner": runner.name(),
        "ticket": plan.ticket_key,
        "ticketLabel": plan.label,
        "projectId": plan.project_id,
        "workspaceId": plan.workspace_id,
        "branch": plan.branch,
        "startedAt": started_at,
    });
    let mut warnings = plan.warnings.clone();
    // The session is live from here on: a record that fails to save is reported, not undone.
    if let Err(error) = store_cloud_session(&state.paths, record) {
        warnings.push(format!(
            "The session started, but Ghostex could not remember it on this computer: {error}"
        ));
    }
    let mut team_recorded = false;
    if plan.team {
        match crate::team_sync::record_team_cloud_session(
            &state.paths,
            &plan.workspace_id,
            &crate::team_sync::TeamCloudSession {
                ticket: &plan.ticket_key,
                session_url: &session.url,
                branch: &plan.branch,
                project_id: &plan.project_id,
                runner_name: runner.name(),
            },
        ) {
            Ok(()) => team_recorded = true,
            Err(error) => warnings.push(format!(
                "The session started, but the team was not told: {error}"
            )),
        }
    }
    Ok(json!({
        "sessionUrl": session.url,
        "provider": provider.id,
        "runner": runner.name(),
        "branch": plan.branch,
        "onBranch": plan.branch_on_remote,
        "ticket": plan.ticket_key,
        "projectId": plan.project_id,
        "teamRecorded": team_recorded,
        "warnings": warnings,
    }))
}

fn records_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_data_dir.join(CLOUD_SESSIONS_FILE_NAME)
}

fn read_records(paths: &GxserverPaths) -> Vec<Value> {
    fs::read_to_string(records_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("sessions").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

fn store_cloud_session(paths: &GxserverPaths, record: Value) -> std::io::Result<()> {
    let _guard = RECORDS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut sessions = read_records(paths);
    sessions.insert(0, record);
    sessions.truncate(MAX_RECORDS);
    fs::create_dir_all(&paths.app_data_dir)?;
    let path = records_path(paths);
    let temp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(
            json!({ "version": 1, "sessions": sessions })
                .to_string()
                .as_bytes(),
        )?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)
}

/// The cloud sessions this computer started on a ticket (by its team key), newest first.
pub(crate) fn cloud_sessions_for_ticket(paths: &GxserverPaths, ticket_key: &str) -> Vec<Value> {
    read_records(paths)
        .into_iter()
        .filter(|record| {
            record
                .get("ticket")
                .and_then(Value::as_str)
                .is_some_and(|ticket| ticket.eq_ignore_ascii_case(ticket_key))
        })
        .collect()
}
