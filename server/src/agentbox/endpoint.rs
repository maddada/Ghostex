//! `POST /api/agentbox`: agentbox readiness, the box list, a box's web app or screen URL, stopping
//! and destroying boxes, and the setup commands the Cloud Boxes page runs in a command pane.

use std::path::Path;

use serde_json::{json, Map, Value};

use super::cli::{resolve_agentbox_binary, run_agentbox, STOP_TIMEOUT, URL_TIMEOUT};
use super::command::terminal_command;
use super::location::is_valid_box_name;
use super::session::session_agentbox;
use super::status::{list_boxes, read_status};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

fn unavailable(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "dependencyUnavailable",
        message: message.into(),
    }
}

fn text_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, DomainStateError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("agentbox task failed: {error}"),
        })
}

pub(crate) async fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let home = state.paths.home_dir.clone();
    let action = text_param(params, "action").unwrap_or_else(|| "status".to_string());
    match action.as_str() {
        "status" => {
            let refresh = params.get("refresh").and_then(Value::as_bool) == Some(true);
            blocking(move || read_status(&home, refresh)).await
        }
        "list" => {
            let boxes = blocking(move || {
                // No agentbox means no boxes, not an error the Cloud Boxes page has to show.
                if resolve_agentbox_binary(&home, false).is_none() {
                    return Ok(Vec::new());
                }
                list_boxes(&home)
            })
            .await?
            .map_err(unavailable)?;
            let sessions = box_sessions_by_name(state)?;
            Ok(json!({
                "boxes": boxes
                    .iter()
                    .map(|listed| {
                        let mut value = listed.to_value();
                        if let Some((project_id, session_id)) = sessions.get(&listed.name) {
                            value["projectId"] = json!(project_id);
                            value["sessionId"] = json!(session_id);
                        }
                        value
                    })
                    .collect::<Vec<_>>(),
            }))
        }
        "openTarget" => {
            let box_name = target_box_name(state, params)?;
            match text_param(params, "target").as_deref() {
                Some("web") | None => {
                    let url = blocking(move || box_web_url(&home, &box_name))
                        .await?
                        .map_err(unavailable)?;
                    Ok(json!({ "url": url }))
                }
                Some("screen") => {
                    let boxes = blocking(move || list_boxes(&home))
                        .await?
                        .map_err(unavailable)?;
                    let url = boxes
                        .into_iter()
                        .find(|listed| listed.name == box_name)
                        .ok_or_else(|| {
                            DomainStateError::not_found(format!(
                                "agentbox does not list a box named {box_name}."
                            ))
                        })?
                        .vnc_url
                        .ok_or_else(|| {
                            unavailable(format!("Box {box_name} has no screen (VNC is off)."))
                        })?;
                    Ok(json!({ "url": url }))
                }
                Some(other) => Err(DomainStateError::bad_request(format!(
                    "\"{other}\" is not a box target. Use web or screen."
                ))),
            }
        }
        "stop" | "destroy" => {
            let box_name = target_box_name(state, params)?;
            let destroy = action == "destroy";
            let output = blocking(move || {
                let args: Vec<&str> = if destroy {
                    vec!["destroy", box_name.as_str(), "-y"]
                } else {
                    vec!["stop", box_name.as_str()]
                };
                run_agentbox(&home, &args, STOP_TIMEOUT, None)
            })
            .await?
            .map_err(unavailable)?;
            let text = format!("{}{}", output.stdout, output.stderr);
            if !output.success {
                return Err(unavailable(output.failure_message()));
            }
            Ok(json!({ "ok": true, "output": text.trim() }))
        }
        "terminalCommand" => terminal_command(params),
        other => Err(DomainStateError::bad_request(format!(
            "\"{other}\" is not an agentbox action. Use status, list, openTarget, stop, destroy or terminalCommand."
        ))),
    }
}

/// `agentbox url <box> --print`: the box web app's URL on this computer.
fn box_web_url(home: &Path, box_name: &str) -> Result<String, String> {
    let output = run_agentbox(home, &["url", box_name, "--print"], URL_TIMEOUT, None)?;
    if !output.success {
        return Err(output.failure_message());
    }
    output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| format!("Box {box_name} has no web app. Declare services.web.expose.port in the project's agentbox.yaml."))
}

/// The box an action names: `boxName`, or the box of `projectId` + `sessionId`.
fn target_box_name(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    if let Some(box_name) = text_param(params, "boxName") {
        if !is_valid_box_name(&box_name) {
            return Err(DomainStateError::bad_request(format!(
                "\"{box_name}\" is not a box name."
            )));
        }
        return Ok(box_name);
    }
    let (Some(project_id), Some(session_id)) = (
        text_param(params, "projectId"),
        text_param(params, "sessionId"),
    ) else {
        return Err(DomainStateError::bad_request(
            "Name the box with boxName, or the session with projectId and sessionId.",
        ));
    };
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {project_id}/{session_id} does not exist."
            ))
        })?;
    session_agentbox(&session)
        .map(|agentbox| agentbox.box_name)
        .ok_or_else(|| DomainStateError::bad_request("This session does not run in a box."))
}

/// Every Ghostex session that runs in a box, by box name.
fn box_sessions_by_name(
    state: &AppState,
) -> Result<std::collections::HashMap<String, (String, String)>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    Ok(repository
        .list_agentbox_sessions(false)?
        .iter()
        .filter_map(|session| {
            let agentbox = session_agentbox(session)?;
            Some((
                agentbox.box_name,
                (
                    session.get("projectId")?.as_str()?.to_string(),
                    session.get("sessionId")?.as_str()?.to_string(),
                ),
            ))
        })
        .collect())
}
