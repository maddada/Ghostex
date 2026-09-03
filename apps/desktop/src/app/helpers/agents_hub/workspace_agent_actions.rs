// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the settings-action status message,
// the Windows local-project workspace-agent create/attach helpers, and the
// bundled agent skill uninstall helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fs, time::Duration};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_settings_action_status_message(
    action: &str,
    available: bool,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "available": available,
        "generatedAt": gpui_status_generated_at(),
        "message": message,
        "type": "settingsActionStatus",
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_create_local_project_workspace_agent(
    project_id: &str,
    agent_id: &str,
) -> Result<
    (
        GpuiLocalWorkspaceSessionKey,
        GpuiLocalWorkspaceAttachTerminalPlan,
    ),
    String,
> {
    if !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_agent_id_allowed(agent_id)
    {
        return Err("The selected project agent is unavailable.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/createAgentSession",
        &serde_json::json!({
            "agentId": agent_id,
            /*
            CDXC:Drafts 2026-08-28:
            The Windows project-header agent launch is the same promptless
            sidebar create as the macOS/Linux CEF path, so it creates a draft
            row too. The attach step below starts the provider, so the agent CLI
            is warm while the user types; gxserver clears `draftStatus` when the
            first prompt reaches the agent.
            */
            "draft": true,
            "projectId": project_id,
            "requireLaunchCommand": true,
            "surface": "workspace",
        }),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create an agent session.".to_string())?;
    let created_project_id = gpui_trimmed_json_string_field(session, "projectId")
        .unwrap_or(project_id)
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return an agent session id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid agent session id.".to_string());
    }
    let key = GpuiLocalWorkspaceSessionKey {
        project_id: created_project_id,
        session_id,
    };
    match gpui_prepare_local_workspace_attach_terminal_plan(
        &key,
        GpuiLocalWorkspaceAttachIntent::Attach,
    ) {
        Ok(plan) => Ok((key, plan)),
        Err(message) => {
            gpui_close_command_terminal_gxserver_session(&key);
            Err(message)
        }
    }
}

pub(crate) fn gpui_workspace_attach_agent_icon(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&'static str> {
    let session = attach.get("session").and_then(serde_json::Value::as_object);
    let candidate = session
        .and_then(|session| json_string_field(session, "agentIcon"))
        .or_else(|| session.and_then(|session| json_string_field(session, "agentName")))
        .or_else(|| session.and_then(|session| json_string_field(session, "agentId")))
        .or_else(|| json_string_field(attach, "agentIcon"))
        .or_else(|| json_string_field(attach, "agentName"))
        .or_else(|| json_string_field(attach, "agentId"));
    gpui_sidebar_agent_icon(candidate)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceNewTerminalPlacement {
    Tab,
    SplitRight,
    SplitBelow,
    BottomRow,
}

#[derive(Clone)]
pub(crate) struct AgentsWorkspaceTerminalLaunch {
    pub(crate) title: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) startup_text: String,
}

pub(crate) fn gpui_create_local_project_workspace_terminal_with_launch(
    project_id: &str,
    launch: &AgentsWorkspaceTerminalLaunch,
) -> Result<
    (
        GpuiLocalWorkspaceSessionKey,
        GpuiLocalWorkspaceAttachTerminalPlan,
    ),
    String,
> {
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return Err("The active project is unavailable.".to_string());
    }
    let mut params = serde_json::Map::new();
    params.insert("kind".to_string(), serde_json::json!("terminal"));
    params.insert("lifecycleState".to_string(), serde_json::json!("running"));
    params.insert("projectId".to_string(), serde_json::json!(project_id));
    params.insert("surface".to_string(), serde_json::json!("workspace"));
    params.insert("title".to_string(), serde_json::json!(launch.title));
    if let Some(working_directory) = launch.working_directory.as_deref() {
        params.insert("cwd".to_string(), serde_json::json!(working_directory));
    }
    let result = gpui_gxserver_rpc_result(
        "/api/createSession",
        &serde_json::Value::Object(params),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create the extension terminal.".to_string())?;
    let created_project_id = gpui_trimmed_json_string_field(session, "projectId")
        .unwrap_or(project_id)
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return an extension terminal id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(&created_project_id)
        || !gpui_remote_sidebar_session_id_allowed(&session_id)
    {
        return Err("gxserver returned an invalid extension terminal id.".to_string());
    }
    let key = GpuiLocalWorkspaceSessionKey {
        project_id: created_project_id,
        session_id,
    };
    match gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(
        &key,
        Some(&launch.startup_text),
        GpuiLocalWorkspaceAttachIntent::Attach,
    ) {
        Ok(plan) => Ok((key, plan)),
        Err(message) => {
            gpui_close_command_terminal_gxserver_session(&key);
            Err(message)
        }
    }
}

/*
CDXC:RemoteMachines 2026-08-18:
`open_gpui_remote_attach_terminal` serves two different intents and they need
different completion rules. Attaching a session that already existed must not
yank the workspace back if the user focused something else while the SSH plan
was prepared in the background. A session this very action just created on the
remote daemon has nothing to drift away from: dropping it there produced no tab
and no toast, and left an orphaned terminal row on the remote machine, which is
exactly what made the sidebar project-header Create Terminal button look inert.
*/
pub(crate) fn gpui_uninstall_bundled_agent_skills() -> Result<String, String> {
    /*
    CDXC:AgentSkills 2026-06-24-12:56:
    The bundled skill uninstall action is intentionally narrower than a generic skills cleanup. Remove only the catalog-owned shared Ghostex skill names under `~/.agents/skills`, handle missing folders as already-uninstalled state, and do not follow React-provided paths or delete other user/system skills.
    */
    let mut removed_count = 0usize;
    for skill_name in GPUI_BUNDLED_GHOSTEX_AGENT_SKILL_NAMES {
        removed_count += usize::from(gpui_uninstall_bundled_agent_skill(skill_name)?);
    }
    if removed_count == 0 {
        Ok("No bundled Ghostex agent skills were installed. Current integration status was refreshed.".to_string())
    } else {
        Ok(
            "Bundled Ghostex agent skills uninstalled. You can install them again from Settings."
                .to_string(),
        )
    }
}

pub(crate) fn gpui_uninstall_bundled_agent_skill(skill_name: &str) -> Result<bool, String> {
    if !GPUI_BUNDLED_GHOSTEX_AGENT_SKILL_NAMES.contains(&skill_name) {
        return Err(
            "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
                .to_string(),
        );
    }
    let skill_path = gpui_home_dir()
        .join(".agents")
        .join("skills")
        .join(skill_name);
    let metadata = match fs::symlink_metadata(&skill_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => {
            return Err(
                "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
                    .to_string(),
            );
        }
    };
    let remove_result = if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(&skill_path)
    } else if metadata.is_dir() {
        fs::remove_dir_all(&skill_path)
    } else {
        fs::remove_file(&skill_path)
    };
    remove_result.map(|_| true).map_err(|_| {
        "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
            .to_string()
    })
}
