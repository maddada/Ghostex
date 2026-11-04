//! `POST /api/startFeedbackAgent`: the "Collect more data with an agent" half of Send Bug /
//! Feedback. gxserver saves the pasted screenshots, writes the agent's instructions, creates the
//! agent session in the Ghostex config folder's project and starts it, so every client gets the
//! same flow and only has to open the session it answers with.

use std::fs;
use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::agents::{
    apply_created_session_identity, create_agent_session_params_for_project, read_agent_settings,
};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, queue_session_chat_prompt, restore_parked_project_for_new_session,
    routed_json, schedule_presentation_project_delta, schedule_presentation_session_delta,
    AppState, RoutedResponse,
};
use crate::sidebar_hud::sidebar_agent_buttons_from_projects;
use crate::storage::open_gxserver_database;
use crate::zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext};

use super::brief_wait::wait_for_feedback_agent_brief;
use super::draft::github_username;
use super::relay::FeedbackImage;

/// The new session's title in the sidebar.
const FEEDBACK_AGENT_SESSION_TITLE: &str = "Bug / Feedback";

/// Whether the agent shows the user its draft and waits for a yes before it posts.
///
/// CDXC:Feedback 2026-10-16 DECISION:
/// User, asked whether the feedback agent may post the issue straight away: "Show me the draft in its chat first". The agent shows its full draft (title, body and the screenshots it will attach) in its chat and posts only after the user says yes there, as the 2026-10-04 rule has the pop-up's Review step do (apps/desktop/src/app/window/feedback_modal/state.rs). `feedback_agent_prompt` words the last step from this switch.
const FEEDBACK_AGENT_ASKS_BEFORE_POSTING: bool = true;

/// `startFeedbackAgent { message, githubUsername, images: [{ name, dataBase64 }], agentId? }`
/// answers `{ projectId, sessionId, agentId, agentName, providerStarted }` once the agent has
/// taken its brief, or `feedbackAgentNotReady` saying why it could not (brief_wait.rs).
pub(crate) async fn handle_start_feedback_agent_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let worker_state = state.clone();
    let outcome =
        tokio::task::spawn_blocking(move || start_feedback_agent(&worker_state, &params)).await;
    match outcome {
        Ok(Ok(result)) => {
            let agent_name = result["agentName"]
                .as_str()
                .unwrap_or("The agent")
                .to_string();
            match wait_for_feedback_agent_brief(state, &result, &agent_name).await {
                Ok(()) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => domain_error_response(endpoint_path, request_id, error),
            }
        }
        Ok(Err(error)) => domain_error_response(endpoint_path, request_id, error),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("The feedback agent could not start: {error}"),
            },
        ),
    }
}

fn start_feedback_agent(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let message = text("message");
    if message.is_empty() {
        return Err(DomainStateError::bad_request("Write your feedback first."));
    }
    let username = github_username(&text("githubUsername"))
        .map_err(DomainStateError::bad_request)?
        .ok_or_else(|| DomainStateError::bad_request("Add your GitHub username."))?;
    let images = FeedbackImage::from_params(params).map_err(DomainStateError::bad_request)?;

    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let projects = repository.list_projects()?;
    let agents: Vec<Value> = sidebar_agent_buttons_from_projects(&projects)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|agent| {
            agent
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| !command.trim().is_empty())
        })
        .collect();
    let settings = read_agent_settings(&db)?;
    let agent = [
        text("agentId"),
        settings
            .get("defaultPromptAgentId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ]
    .iter()
    .filter(|id| !id.is_empty())
    .find_map(|id| {
        agents
            .iter()
            .find(|agent| agent.get("agentId").and_then(Value::as_str) == Some(id.as_str()))
    })
    .or_else(|| agents.first())
    .cloned()
    .ok_or_else(|| {
        DomainStateError::bad_request("No agent is set up to start. Add one in the Agents Hub.")
    })?;
    let agent_id = agent
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let agent_name = agent
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("The agent")
        .to_string();

    let report = start_report_folder(state, &repository, &db, params)?;
    let image_paths = save_screenshots(&report.folder, &images)?;
    let project = report.project;
    let project_id = text_of(&project, "projectId")?;

    let mut launch_settings = Map::new();
    for (key, field) in [("agentCommand", "command"), ("icon", "icon")] {
        if let Some(value) = agent.get(field) {
            launch_settings.insert(key.to_string(), value.clone());
        }
    }
    let mut create_params = Map::new();
    create_params.insert("agentId".to_string(), json!(agent_id));
    create_params.insert("projectId".to_string(), json!(project_id));
    create_params.insert("surface".to_string(), json!("workspace"));
    create_params.insert("requireLaunchCommand".to_string(), json!(true));
    create_params.insert("title".to_string(), json!(FEEDBACK_AGENT_SESSION_TITLE));
    create_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    let prompt = feedback_agent_prompt(&message, &username, &report.folder, &image_paths);
    // `firstUserMessage` only names the session; the startup send below delivers the prompt.
    create_params.insert(
        "runtimeSettings".to_string(),
        json!({ "firstUserMessage": prompt }),
    );
    let normalized = create_agent_session_params_for_project(&db, &project, &create_params)?;
    let created = repository.create_session(&normalized, false)?;
    let session = apply_created_session_identity(&repository, &created, &normalized)?;
    let project_id = text_of(&session, "projectId")?;
    let session_id = text_of(&session, "sessionId")?;
    schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;

    let mut result = json!({
        "projectId": project_id,
        "sessionId": session_id,
        "agentId": agent_id,
        "agentName": agent_name,
    });
    start_session_provider(
        state,
        &db,
        &repository,
        &project_id,
        &session_id,
        &mut result,
    )?;
    // CDXC:Feedback 2026-10-16 WHY: `runtimeSettings.firstUserMessage` is metadata nothing types, so the live check's Codex sat at an empty prompt. The brief goes through the session's chat queue as its startup send, the way orchestrator threads get their task: the queue holds it until the agent's input box is up, for every agent, and sends it once. A failed provider start still queues it, and the send wakes the session.
    if let Err(error) = queue_session_chat_prompt(state, &project_id, &session_id, &prompt, true) {
        result["promptQueued"] = json!(false);
        result["promptQueueError"] = json!(error.message);
    } else {
        result["promptQueued"] = json!(true);
    }
    Ok(result)
}

/// The project the feedback agent runs in and this report's own folder inside it.
struct ReportFolder {
    project: Value,
    folder: PathBuf,
}

/// Name given to the Ghostex config folder's project when no project has that path yet; an
/// existing one keeps its name. Not "Ghostex", which is the repository's own project.
const CONFIG_PROJECT_NAME: &str = "Ghostex config";

/// CDXC:Feedback 2026-10-16 DECISION:
/// User: "when sending the agent report from an agent let's please send it fron the .config/ghostex folder instead of creating a new folder. also please auto-trust that folder when we run from it". The agent runs in the Ghostex config folder's project (the one already registered for that path, brought back from Recent Projects, or registered as "Ghostex config"), not in a new Quick chat workspace per report, so reports leave no new folders or sidebar projects behind. Each report's screenshots, notes and draft go in `feedback/<timestamp>/` inside it, and the agent is told to write only there. The folder's trust prompt is answered by Ghostex (`ghostex_config_folder_trusted` in session_chat_trust_memory.rs).
fn start_report_folder(
    state: &AppState,
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<ReportFolder, DomainStateError> {
    let config_dir = &state.paths.app_config_dir;
    let folder = config_dir.join("feedback").join(
        chrono::Local::now()
            .format("%Y-%m-%d-%H%M%S%3f")
            .to_string(),
    );
    fs::create_dir_all(&folder).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not create the report folder: {error}"),
    })?;
    let known: Vec<String> = repository
        .list_projects()?
        .iter()
        .filter_map(|project| project.get("projectId").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let mut project_params = Map::new();
    project_params.insert(
        "path".to_string(),
        json!(config_dir.to_string_lossy().to_string()),
    );
    project_params.insert("name".to_string(), json!(CONFIG_PROJECT_NAME));
    let project = repository.add_project_path(&project_params)?;
    let project_id = text_of(&project, "projectId")?;
    let project = if known.contains(&project_id) {
        restore_parked_project_for_new_session(state, db, repository, &project_id)?;
        repository.get_project(&project_id)?.unwrap_or(project)
    } else {
        let project = crate::workspaces::place_added_project(repository, db, project, params)?;
        schedule_presentation_project_delta(state, db, repository, &project_id, "projectAdded")?;
        project
    };
    Ok(ReportFolder { project, folder })
}

/// Writes the pasted screenshots into the report's folder (`screenshots/screenshot-N.png`) and
/// returns their paths, in order.
fn save_screenshots(
    workspace: &Path,
    images: &[FeedbackImage],
) -> Result<Vec<PathBuf>, DomainStateError> {
    if images.is_empty() {
        return Ok(Vec::new());
    }
    let folder = workspace.join("screenshots");
    fs::create_dir_all(&folder).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the screenshots: {error}"),
    })?;
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let path = folder.join(format!("screenshot-{}.{}", index + 1, image.extension()));
            fs::write(&path, image.bytes()).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Could not save {}: {error}", image.name()),
            })?;
            Ok(path)
        })
        .collect()
}

/// The agent's first message: what the user wrote, their screenshots, and how to turn it into an
/// issue.
pub(super) fn feedback_agent_prompt(
    message: &str,
    username: &str,
    report_folder: &Path,
    images: &[PathBuf],
) -> String {
    let report_folder = report_folder.display();
    let screenshots = if images.is_empty() {
        "They pasted no screenshots.".to_string()
    } else {
        let list = images
            .iter()
            .enumerate()
            .map(|(index, path)| format!("[Image #{}]({})", index + 1, path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Their screenshots:\n{list}")
    };
    let image_flags = images
        .iter()
        .map(|path| format!(" --image \"{}\"", path.display()))
        .collect::<String>();
    let posting = if FEEDBACK_AGENT_ASKS_BEFORE_POSTING {
        "7. Show the user the whole draft here: the title, the full body, and the screenshots you will attach. Ask whether to post it, apply any changes they ask for, and post only after they say yes."
    } else {
        "7. Post the issue when the draft is complete."
    };
    format!(
        r#"A Ghostex user is reporting a bug or giving feedback with Send Bug / Feedback, and asked you to collect the details that help the Ghostex team fix it, then file it as a GitHub issue on the Ghostex repository.

Their GitHub username: @{username}

What they wrote:
"""
{message}
"""

{screenshots}

What to do:
1. Read `ghostex guide` (the overview) and `ghostex guide features` for how the part of Ghostex they describe is meant to work, so the issue says what was expected.
2. Gather what helps us fix it from this computer: the Ghostex version (`ghostex server status --json`), the operating system, the settings that touch this feature (`ghostex settings list`, `ghostex settings get <key>`), the recent gxserver and app logs around it (`ghostex logs --lines 200`, `ghostex logs --grep <text>`), and the sessions involved (`ghostex sessions`).
3. Debug it, and reproduce it where you can do that safely, to find what goes wrong and, if you can, why.
4. Change nothing on this computer: do not change settings, files, projects or sessions, do not restart Ghostex or its background service, and do not install anything. The only files you write are your notes and the draft, in this report's folder: {report_folder}
5. Keep private things out of the issue: no tokens, keys, passwords, cookies, email addresses or account names other than @{username}, and no contents of the user's own files. Shorten paths in their home folder to ~, and quote only the log lines that matter.
6. Draft the issue: a short title, and a body with What happened, What was expected, Steps to reproduce, What you found (Ghostex version, operating system, relevant settings, log excerpts) and the likely cause if you found one.
{posting}
8. Post it: write the body to a file in {report_folder}, then run
   ghostex feedback send --title "<title>" --body-file <file> --github-user {username}{image_flags}
   It adds "Reported by @{username}" at the top so GitHub tells them about the issue, and Ghostex adds a line with its version and the operating system. Run it with --dry-run first if you want to see exactly what will be sent. Then give the user the issue link it prints."#
    )
}

/// Starts the new session's zmx provider, as `ghostex create-agent` does, so the first message
/// reaches a live agent; a failed start is reported on `result`.
fn start_session_provider(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    result: &mut Value,
) -> Result<(), DomainStateError> {
    let context = zmx_context(state);
    let agent_settings = read_agent_settings(db)?;
    let mut provider_params = Map::new();
    provider_params.insert("projectId".to_string(), json!(project_id));
    provider_params.insert("sessionId".to_string(), json!(session_id));
    match dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        &provider_params,
        &context,
        &agent_settings,
    ) {
        Ok(_) => result["providerStarted"] = json!(true),
        Err(error) => {
            let message = match error {
                ZmxEndpointError::DependencyUnavailable(message) => message,
                ZmxEndpointError::Domain(error) => error.message,
            };
            result["providerStarted"] = json!(false);
            result["providerStartError"] = json!(message);
        }
    }
    let _ = schedule_presentation_session_delta(state, db, repository, project_id, session_id);
    Ok(())
}

/// How gxserver reaches itself for a zmx lifecycle call.
pub(super) fn zmx_context(state: &AppState) -> ZmxServerContext {
    ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    }
}

fn text_of(value: &Value, key: &str) -> Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state(format!("Missing {key}.")))
}
