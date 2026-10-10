//! Running a `slack.request` command on the requester's Ghostex: step 4 of the Slack command flow
//! (packages/team-sync/convex/slackFlow.ts decided what to do; this does it).
//!
//! - `start`: one working session per ticket. A session here already linked to the ticket gets
//!   the request; otherwise `local` starts a session in a worktree on the ticket's branch
//!   (`startWorkOnTicket`) and sends it the prompt, and `cloud` starts one through the cloud
//!   runner (server/src/cloud_runner.rs).
//! - `message`: the ticket's session exists; send it the request (a cloud session through the
//!   cloud runner).
//! - A GitHub ticket that is a pull request starts on the PR's head branch, linked to the PR.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: a session started from Slack gets the ticket, every message of the thread, the working
//! thread's link and the team instructions, and agents post milestones with
//! `ghostex slack post --session <ref> "<text>"`; the instructions Ghostex adds say so.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::server::{AppState, ProjectWorktreeOperationError};
use crate::session_chat_queue_runtime::{
    send_session_chat_message_internal, SessionChatMessageSource,
};
use crate::storage::open_gxserver_database;
use crate::work_mode::{
    create_github_issue, origin_repo, plan_pull_request, start_work_on_ticket, work_targets,
    NewGithubIssue,
};
use crate::workspaces::{project_workspace_id, read_sidebar_workspaces};
use crate::worktree_sessions::read_worktree_session_marker;

use super::commands::CommandOutcome;
use super::connections::TeamConnection;
use super::slack_requirements::summarize_requirements_in_background;
use crate::cloud_runner::{cloud_runner, CloudStartRequest, MAX_CLOUD_PROMPT_CHARS};

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

fn text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// A session of this workspace that works on the ticket.
struct TicketSession {
    project_id: String,
    session_id: String,
    branch: Option<String>,
}

pub(crate) fn run_slack_request(
    state: &AppState,
    connection: &TeamConnection,
    command: &Value,
) -> CommandOutcome {
    let payload = command.get("payload").cloned().unwrap_or(Value::Null);
    let command_id = command
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("command");
    let result = match text(&payload, "/action") {
        Some("start") => start(state, connection, command_id, &payload),
        Some("message") => message(state, connection, command_id, &payload),
        Some("createIssue") => create_issue(state, connection, &payload),
        other => Err(format!(
            "This Ghostex does not know the Slack action {}.",
            other.unwrap_or("(none)")
        )),
    };
    match result {
        Ok(value) => CommandOutcome::Done(value),
        Err(error) => CommandOutcome::Failed(error),
    }
}

fn start(
    state: &AppState,
    connection: &TeamConnection,
    command_id: &str,
    payload: &Value,
) -> Result<Value, String> {
    let ticket = text(payload, "/ticket/key").ok_or("The request names no ticket.")?;
    summarize_requirements_in_background(state, connection, command_id, payload);
    let images = download_images(state, command_id, payload)?;
    if let Some(existing) = find_ticket_session(state, &connection.workspace_id, payload)? {
        send(
            state,
            &existing.project_id,
            &existing.session_id,
            &request_message(payload),
            &images,
        )?;
        return Ok(json!({
            "runPlace": "local",
            "reused": true,
            "projectId": existing.project_id,
            "sessionId": existing.session_id,
            "branch": existing.branch,
            "machine": machine_name(),
        }));
    }
    let project = resolve_project(state, &connection.workspace_id, payload)?;
    let project_id = text(&project, "/projectId").unwrap_or_default().to_string();
    let project_path = text(&project, "/path").unwrap_or_default().to_string();
    let pull_request = pull_request_number(&project_path, payload);

    if text(payload, "/runPlace") == Some("cloud") {
        let runner = cloud_runner(&state.paths.home_dir);
        let prompt = clip(&start_prompt(payload, None), MAX_CLOUD_PROMPT_CHARS);
        // A PR's session works on (and pushes to) the PR's own branch.
        let head_branch = match pull_request {
            Some(number) => Some(
                plan_pull_request(&project_path, None, &number.to_string())
                    .map_err(|error| error.message)?
                    .branch,
            ),
            None => None,
        };
        let session = runner.start(&CloudStartRequest {
            repo_dir: std::path::Path::new(&project_path),
            prompt: &prompt,
            on_branch: head_branch.as_deref(),
        })?;
        return Ok(json!({
            "runPlace": "cloud",
            "reused": false,
            "projectId": project_id,
            "sessionUrl": session.url,
            "branch": head_branch.as_deref().or(text(payload, "/ticket/branchName")),
            "runner": runner.name(),
        }));
    }

    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    match (text(payload, "/ticket/kind"), pull_request) {
        (Some("github"), Some(number)) => {
            params.insert("pullRequest".to_string(), json!(number));
        }
        (Some("github"), None) => {
            params.insert(
                "githubIssue".to_string(),
                payload
                    .pointer("/ticket/number")
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
        _ => {
            params.insert("linearIssue".to_string(), json!(ticket));
        }
    }
    let started = tokio::runtime::Handle::current()
        .block_on(start_work_on_ticket(state, &params))
        .map_err(worktree_error_text)?;
    let session_id = text(&started, "/sessionId")
        .ok_or("Starting the session returned no session.")?
        .to_string();
    let project_id = text(&started, "/projectId")
        .unwrap_or(project_id.as_str())
        .to_string();
    send(
        state,
        &project_id,
        &session_id,
        &start_prompt(payload, Some(&session_id)),
        &images,
    )?;
    Ok(json!({
        "runPlace": "local",
        "reused": false,
        "projectId": project_id,
        "sessionId": session_id,
        "branch": started.get("branch").cloned().unwrap_or(Value::Null),
        "worktreePath": started.get("worktreePath").cloned().unwrap_or(Value::Null),
        "machine": machine_name(),
    }))
}

fn message(
    state: &AppState,
    connection: &TeamConnection,
    command_id: &str,
    payload: &Value,
) -> Result<Value, String> {
    if text(payload, "/session/runPlace") == Some("cloud") {
        let url = text(payload, "/session/sessionUrl")
            .ok_or("The ticket's cloud session has no link to send to.")?;
        let runner = cloud_runner(&state.paths.home_dir);
        runner.send(url, &cloud_request_message(payload))?;
        return Ok(json!({
            "delivered": true,
            "runPlace": "cloud",
            "sessionUrl": url,
            "runner": runner.name(),
        }));
    }
    let images = download_images(state, command_id, payload)?;
    let target = match (
        text(payload, "/session/projectId"),
        text(payload, "/session/sessionId"),
    ) {
        (Some(project_id), Some(session_id)) => TicketSession {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            branch: None,
        },
        _ => find_ticket_session(state, &connection.workspace_id, payload)?.ok_or_else(|| {
            format!(
                "No session on this computer works on {} yet.",
                text(payload, "/ticket/key").unwrap_or("this ticket")
            )
        })?,
    };
    send(
        state,
        &target.project_id,
        &target.session_id,
        &request_message(payload),
        &images,
    )?;
    Ok(json!({
        "delivered": true,
        "projectId": target.project_id,
        "sessionId": target.session_id,
    }))
}

/// A GitHub team's Slack request with no GitHub issue in its thread: create the issue in the repo
/// the channel maps to (`gh issue create`, assigned to this person), and report it as
/// `owner/repo#number` so the Slack flow runs again with it (packages/team-sync/convex/
/// slackGithubIssue.ts).
fn create_issue(
    state: &AppState,
    connection: &TeamConnection,
    payload: &Value,
) -> Result<Value, String> {
    let project = resolve_project(state, &connection.workspace_id, payload)?;
    let cwd = text(&project, "/path").ok_or("The project for this channel has no folder.")?;
    let created = create_github_issue(
        cwd,
        &NewGithubIssue {
            title: text(payload, "/title").unwrap_or("Request from Slack"),
            body: text(payload, "/body"),
            assign_to_me: true,
            repo: text(payload, "/repo/repo"),
        },
    )?;
    Ok(json!({
        "ticket": format!("{}#{}", created.repo, created.number),
        "number": created.number,
        "url": created.url,
        "repo": created.repo,
        "projectId": text(&project, "/projectId"),
    }))
}

fn worktree_error_text(error: ProjectWorktreeOperationError) -> String {
    match error {
        ProjectWorktreeOperationError::Domain(error) => error.message,
        ProjectWorktreeOperationError::Typed(error) => error.message,
        ProjectWorktreeOperationError::ProjectPath(error) => format!("{error:?}"),
    }
}

fn send(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    images: &[String],
) -> Result<(), String> {
    tokio::runtime::Handle::current()
        .block_on(send_session_chat_message_internal(
            state,
            project_id,
            session_id,
            text,
            images,
            SessionChatMessageSource::ManualQueue,
        ))
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Could not send the request to the session: {}",
                error.message
            )
        })
}

/// The workspace's projects (worktree projects left out) with the database they came from.
fn workspace_projects(
    state: &AppState,
    workspace_id: &str,
) -> Result<(Vec<Value>, Vec<Value>), String> {
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| format!("SQLite gxserver state error: {error}"))?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects().map_err(|error| error.message)?;
    let workspaces = read_sidebar_workspaces(&db).map_err(|error| error.message)?;
    let mine: Vec<Value> = projects
        .iter()
        .filter(|project| project_workspace_id(&workspaces, project, &projects) == workspace_id)
        .cloned()
        .collect();
    Ok((mine, projects))
}

/// A session of this workspace linked to the ticket (by its links or its branch).
fn find_ticket_session(
    state: &AppState,
    workspace_id: &str,
    payload: &Value,
) -> Result<Option<TicketSession>, String> {
    let ticket = text(payload, "/ticket/key")
        .unwrap_or_default()
        .to_ascii_uppercase();
    let github_number = payload.pointer("/ticket/number").and_then(Value::as_u64);
    let is_github = text(payload, "/ticket/kind") == Some("github");
    let (projects, _) = workspace_projects(state, workspace_id)?;
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| format!("SQLite gxserver state error: {error}"))?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    for project in &projects {
        let project_id = text(project, "/projectId").unwrap_or_default();
        let sessions = repository
            .list_sessions(Some(project_id))
            .map_err(|error| error.message)?;
        for session in &sessions {
            let targets = work_targets(project, session);
            let linked = if is_github {
                github_number.is_some_and(|number| targets.github_issues.contains(&number))
            } else {
                targets
                    .linear_issues
                    .iter()
                    .any(|issue| issue.eq_ignore_ascii_case(&ticket))
            };
            if linked {
                return Ok(Some(TicketSession {
                    project_id: project_id.to_string(),
                    session_id: text(session, "/sessionId").unwrap_or_default().to_string(),
                    branch: read_worktree_session_marker(session).map(|marker| marker.branch),
                }));
            }
        }
    }
    Ok(None)
}

/// The project new work from this Slack channel goes to: the channel mapping's project name, else
/// its repo (by the project's `origin` remote), else the workspace's only project.
fn resolve_project(state: &AppState, workspace_id: &str, payload: &Value) -> Result<Value, String> {
    let (projects, _) = workspace_projects(state, workspace_id)?;
    let projects: Vec<Value> = projects
        .into_iter()
        .filter(|project| project.get("worktree").is_none_or(Value::is_null))
        .collect();
    if let Some(name) = text(payload, "/repo/project") {
        return projects
            .iter()
            .find(|project| {
                text(project, "/name").is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .cloned()
            .ok_or_else(|| format!("This workspace has no project named \"{name}\"."));
    }
    if let Some(repo) = text(payload, "/repo/repo") {
        let repo = repo.to_ascii_lowercase();
        return projects
            .iter()
            .find(|project| {
                text(project, "/path")
                    .and_then(origin_repo)
                    .is_some_and(|origin| origin == repo)
            })
            .cloned()
            .ok_or_else(|| {
                format!("No project in this workspace has the repo {repo}. Add its folder to the workspace in Ghostex.")
            });
    }
    // A GitHub issue or PR names its repo itself.
    if let Some(repo) = text(payload, "/ticket/repo") {
        let repo = repo.to_ascii_lowercase();
        if let Some(project) = projects.iter().find(|project| {
            text(project, "/path")
                .and_then(origin_repo)
                .is_some_and(|origin| origin == repo)
        }) {
            return Ok(project.clone());
        }
    }
    match projects.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(
            "This Slack channel is not mapped to a repo. Run `ghostex team flow map <channel ID> --repo owner/name`."
                .to_string(),
        ),
    }
}

fn machine_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
}

/// Saves the request's images (Convex storage URLs) next to gxserver's data, for the send.
fn download_images(
    state: &AppState,
    command_id: &str,
    payload: &Value,
) -> Result<Vec<String>, String> {
    let Some(images) = payload.get("images").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if images.is_empty() {
        return Ok(Vec::new());
    }
    let safe_id: String = command_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    let dir: PathBuf = state.paths.app_data_dir.join("slack-files").join(safe_id);
    fs::create_dir_all(&dir).map_err(|error| format!("Could not save Slack images: {error}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut paths = Vec::new();
    for (index, image) in images.iter().enumerate() {
        let Some(url) = text(image, "/url") else {
            continue;
        };
        let name: String = text(image, "/name")
            .unwrap_or("image.png")
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{index}-{name}"));
        let response = agent
            .get(url)
            .call()
            .map_err(|error| format!("Could not fetch a Slack image: {error}"))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(response.into_reader(), MAX_IMAGE_BYTES),
            &mut bytes,
        )
        .map_err(|error| format!("Could not fetch a Slack image: {error}"))?;
        fs::write(&path, bytes)
            .map_err(|error| format!("Could not save a Slack image: {error}"))?;
        paths.push(path.to_string_lossy().to_string());
    }
    Ok(paths)
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

fn source_text(payload: &Value) -> String {
    match text(payload, "/source/permalink") {
        Some(link) => link.to_string(),
        None => "`/ghostex` in a Slack channel".to_string(),
    }
}

/// A GitHub ticket that is a pull request: the payload says so when the thread linked the PR, and a
/// ticket stored as `owner/repo#12` (issue and PR numbers share one sequence) is asked of `gh`.
fn pull_request_number(project_path: &str, payload: &Value) -> Option<u64> {
    if text(payload, "/ticket/kind") != Some("github") {
        return None;
    }
    let number = payload.pointer("/ticket/number").and_then(Value::as_u64)?;
    if payload.pointer("/ticket/pullRequest") == Some(&Value::Bool(true)) {
        return Some(number);
    }
    let number_text = number.to_string();
    let mut args = vec!["pr", "view", number_text.as_str(), "--json", "number"];
    if let Some(repo) = text(payload, "/ticket/repo") {
        args.extend(["--repo", repo]);
    }
    crate::session_git_status::run_gh_command(Some(project_path), &args).map(|_| number)
}

/// A follow-up for a cloud session: the request, with the request's screenshots as links (the
/// cloud session cannot take attachments from here).
fn cloud_request_message(payload: &Value) -> String {
    let mut message = request_message(payload);
    let links: Vec<&str> = payload
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|image| text(image, "/url"))
        .collect();
    if !links.is_empty() {
        message.push_str("\n\nScreenshots:\n");
        for link in links {
            message.push_str(&format!("- {link}\n"));
        }
    }
    message
}

/// A follow-up for the ticket's session: who, where, the words as typed.
fn request_message(payload: &Value) -> String {
    format!(
        "{} in Slack ({}):\n\n{}",
        text(payload, "/requester/name").unwrap_or("A teammate"),
        source_text(payload),
        text(payload, "/prompt").unwrap_or("(no words; see the thread)")
    )
}

/// The first message of a session started from Slack.
fn start_prompt(payload: &Value, session_ref: Option<&str>) -> String {
    let ticket = text(payload, "/ticket/key").unwrap_or("the ticket");
    let mut out = String::new();
    out.push_str(&format!(
        "{} asked in Slack ({}) to work on {}{}:\n\n{}\n",
        text(payload, "/requester/name").unwrap_or("A teammate"),
        source_text(payload),
        ticket,
        text(payload, "/ticket/title")
            .map(|title| format!(" · {title}"))
            .unwrap_or_default(),
        text(payload, "/prompt").unwrap_or("(no words; the thread says what to do)")
    ));
    out.push_str(&format!("\n## Ticket {ticket}\n"));
    if let Some(url) = text(payload, "/ticket/url") {
        out.push_str(&format!("{url}\n"));
    }
    if let Some(branch) = text(payload, "/ticket/branchName") {
        out.push_str(&format!("Branch: {branch}\n"));
    }
    let labels: Vec<&str> = payload
        .pointer("/ticket/labels")
        .and_then(Value::as_array)
        .map(|labels| labels.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if let Some(state) = text(payload, "/ticket/state") {
        out.push_str(&format!("State: {state}\n"));
    }
    if !labels.is_empty() {
        out.push_str(&format!("Labels: {}\n", labels.join(", ")));
    }
    if let Some(description) = text(payload, "/ticket/description") {
        out.push_str(&format!("\n{description}\n"));
    }
    if let Some(comments) = payload
        .pointer("/ticket/comments")
        .and_then(Value::as_array)
    {
        if !comments.is_empty() {
            out.push_str("\nComments:\n");
            for comment in comments {
                out.push_str(&format!(
                    "- {}: {}\n",
                    text(comment, "/author").unwrap_or("someone"),
                    text(comment, "/body").unwrap_or("").replace('\n', "\n  ")
                ));
            }
        }
    }
    if let Some(attachments) = payload
        .pointer("/ticket/attachments")
        .and_then(Value::as_array)
    {
        for attachment in attachments {
            if let Some(url) = text(attachment, "/url") {
                out.push_str(&format!(
                    "Attachment: {} {url}\n",
                    text(attachment, "/title").unwrap_or("")
                ));
            }
        }
    }
    if let Some(transcript) = text(payload, "/transcript") {
        out.push_str(&format!("\n## Slack thread\n{transcript}\n"));
    }
    if let Some(working) = text(payload, "/workingThread/permalink") {
        out.push_str(&format!("\n## Working thread\n{working}\n"));
    }
    if let Some(session_ref) = session_ref {
        out.push_str(&format!(
            "Post each milestone to the working thread as it lands:\n  ghostex slack post --session {session_ref} \"<text>\"\nWhen the work is done, post the final result once (PR link, QC package version, video) with --final; it also goes to the Slack threads the request came from.\n"
        ));
    }
    if let Some(instructions) = text(payload, "/instructions") {
        out.push_str(&format!("\n## Team instructions\n{instructions}\n"));
    }
    out
}
