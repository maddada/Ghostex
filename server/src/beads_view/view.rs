//! The two endpoints the Beads view adds: `/api/beadsView` and `/api/beadsGenerateTitle`.

use ghostex_gx_protocol::beads_view::{BeadsViewSnapshot, BeadsWorkspaceState};
use serde_json::{json, Map, Value};

use crate::toolchain::require_system_bd;
use crate::typed_operations::{
    resolve_beads_cwd, resolve_project_operation_context, TypedOperationContext,
    TypedOperationError,
};

use super::cache;
use super::discovery::{discover, forget, last_known, Discovered};
use super::snapshot::{read_snapshot, workspace_less_reply};
use super::sync::load_saved;

const POLL_DISCOVERY_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

/// The project's Beads scope, resolved exactly as the Board's `runBeadsAction` calls with
/// `projectBoardScope: true` resolve it (the project's Beads folder, the global default, the root).
fn beads_context(
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<TypedOperationContext, TypedOperationError> {
    let mut scoped = Map::new();
    for key in ["projectId", "projectPath"] {
        if let Some(value) = params.get(key) {
            scoped.insert(key.to_string(), value.clone());
        }
    }
    scoped.insert("projectBoardScope".to_string(), json!(true));
    resolve_project_operation_context("/api/runBeadsAction", &scoped, projects)
}

pub(crate) async fn beads_view_snapshot(
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<BeadsViewSnapshot, TypedOperationError> {
    let context = beads_context(params, projects)?;
    let cwd = resolve_beads_cwd(&context);
    let bd = match require_system_bd() {
        Ok(bd) => bd,
        Err(message) => {
            return Ok(workspace_less_reply(
                BeadsWorkspaceState::NoBd,
                Some(message),
            ))
        }
    };
    // One 1.5-second budget for the whole poll: discovery and the refresh share it.
    let poll_deadline = tokio::time::Instant::now() + POLL_DISCOVERY_WAIT;
    let refresh = params.get("refresh").and_then(Value::as_bool) == Some(true);
    if refresh {
        forget(&cwd);
    }
    // A view poll waits at most 1.5 seconds for discovery; past that it answers with what bd last
    // said while discovery finishes in the background (its own 10-second deadline).
    let discovery = tokio::spawn({
        let (cwd, bd_path, context) = (cwd.clone(), bd.executable_path.clone(), context.clone());
        async move { discover(&cwd, &bd_path, &context).await }
    });
    let discovered = match tokio::time::timeout_at(poll_deadline, discovery).await {
        Ok(Ok(discovered)) => discovered,
        _ => match last_known(&cwd) {
            Some(discovered) => discovered,
            None => {
                let mut reply = workspace_less_reply(BeadsWorkspaceState::Failed, None);
                reply.busy = true;
                return Ok(reply);
            }
        },
    };
    let info = match discovered {
        Discovered::Workspace(info) => info,
        Discovered::None => return Ok(workspace_less_reply(BeadsWorkspaceState::None, None)),
        Discovered::Failed(message) => {
            return Ok(workspace_less_reply(
                BeadsWorkspaceState::Failed,
                Some(message),
            ))
        }
    };
    let entry = cache::entry(&info.path);
    if refresh {
        entry.invalidate();
    }
    load_saved(&entry);
    let known_revision = params
        .get("knownRevision")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(read_snapshot(
        entry,
        info,
        bd.executable_path.clone(),
        cwd,
        context,
        known_revision,
        poll_deadline,
    )
    .await)
}

/// The project folder a title is generated in, checked against the registered projects.
pub(crate) fn generate_bead_title_scope(
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<(String, String), TypedOperationError> {
    let context = beads_context(params, projects)?;
    let prompt = params
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| TypedOperationError::bad_request("prompt is required."))?;
    Ok((context.cwd, prompt.chars().take(4_000).collect()))
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// A bead created without a title gets one from the Title Generation Agent setting, run by the gxserver that owns the project, so it works for a project on another computer and in the web build. The desktop used to run a prompt-agent command it chose itself on its own computer, which failed for remote projects; a remote-allowed endpoint must never run a command the caller names, so the agent comes only from this computer's settings.
pub(crate) async fn generate_bead_title(
    state: &crate::server::AppState,
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<String, TypedOperationError> {
    let (cwd, prompt) = generate_bead_title_scope(params, projects)?;
    let settings = crate::session_chat_catch_up::read_settings(&state.paths);
    let (agent, command) = crate::session_chat_catch_up::summary_agent(&settings, &json!({}))
        .map_err(|error| TypedOperationError::bad_request(error.message))?;
    let generator = json!({
        "runtimeSettings": {
            "firstPromptTitleGenerationAgent": agent,
            "firstPromptTitleGenerationCommand": command,
        }
    });
    crate::server::title_generation::generate_first_prompt_session_title(
        state,
        Some(&cwd),
        &prompt,
        4_000,
        &generator,
    )
    .await
    .map_err(TypedOperationError::dependency_unavailable)
}
