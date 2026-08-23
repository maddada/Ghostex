// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds local workspace terminal attach plan
// preparation, session creation, and terminal insertion/attach helpers. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{collections::HashMap, time::Duration};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_prepare_local_workspace_attach_terminal_plan(
    reference: &GpuiLocalWorkspaceSessionKey,
    intent: GpuiLocalWorkspaceAttachIntent,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(reference, None, intent)
}

pub(crate) fn gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(
    reference: &GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
    intent: GpuiLocalWorkspaceAttachIntent,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    Local GPUI sidebar session clicks follow macOS zmx attach sequencing: Rust asks localhost gxserver for wake/attach metadata, starts every missing zmx provider through gxserver (with queued startup text when present), then opens an awake Agents Running tab whose exact mount slot consumes the daemon-built attach command. CEF cannot provide commands, cwd, titles, paths, daemon bodies, tokens, stdout/stderr, or terminal content.

    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    Command-pane fresh Action launches pass their one-shot startup text as the
    explicit gxserver attach parameter, matching native's wake/attach/provider
    sequence. The text is consumed only by gxserver's startupTextDisposition path;
    it is not stored in launchSettings or used as the terminal process command.
    */
    let attach_params = gpui_local_workspace_attach_rpc_params(reference, startup_text);
    let mut result =
        gpui_gxserver_rpc_result(intent.rpc_path(), &attach_params, Duration::from_secs(15))?;
    let mut attach = gpui_local_workspace_attach_object(&result)?;
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    let mut startup_text_for_plan =
        gpui_local_workspace_attach_startup_text(attach).map(str::to_string);
    let mut startup_text_disposition_for_plan =
        gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string);

    if should_start_local_zmx_provider_before_gpui_attach(attach) {
        let provider_params =
            gpui_local_workspace_attach_rpc_params(reference, startup_text_for_plan.as_deref());
        gpui_gxserver_rpc_result(
            "/api/startSessionProvider",
            &provider_params,
            Duration::from_secs(30),
        )?;
        result = gpui_gxserver_rpc_result(
            "/api/attachSessionMetadata",
            &attach_params,
            Duration::from_secs(15),
        )?;
        attach = gpui_local_workspace_attach_object(&result)?;
        gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
        if startup_text_for_plan.is_none() {
            startup_text_for_plan =
                gpui_local_workspace_attach_startup_text(attach).map(str::to_string);
        }
        if startup_text_disposition_for_plan.is_none() {
            startup_text_disposition_for_plan =
                gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string);
        }
    }

    gpui_local_workspace_attach_terminal_plan_from_result(
        &result,
        startup_text_for_plan,
        startup_text_disposition_for_plan,
    )
}

pub(crate) fn gpui_local_workspace_attach_terminal_plan_from_result(
    result: &serde_json::Value,
    startup_text_for_plan: Option<String>,
    startup_text_disposition_for_plan: Option<String>,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    let attach = gpui_local_workspace_attach_object(result)?;
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    gpui_validate_local_workspace_attach_metadata(attach)?;
    let agent_icon = gpui_workspace_attach_agent_icon(attach);
    let attach_command = gpui_local_workspace_attach_string(attach, "attachCommand")
        .ok_or_else(|| "Session attach metadata is unavailable.".to_string())?
        .to_string();
    let command_id = attach
        .get("session")
        .and_then(serde_json::Value::as_object)
        .and_then(|session| gpui_trimmed_json_string_field(session, "commandId"))
        .and_then(valid_action_command_id);
    let persistence_session_created =
        gpui_local_workspace_attach_persistence_session_created(attach);
    let startup_text = startup_text_for_plan
        .or_else(|| gpui_local_workspace_attach_startup_text(attach).map(str::to_string));
    let startup_text_disposition = startup_text_disposition_for_plan.or_else(|| {
        gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string)
    });
    let title = gpui_workspace_attach_title(attach);
    let working_directory = gpui_local_workspace_attach_string(attach, "cwd").map(str::to_string);
    let zmx_name = gpui_local_workspace_attach_string(attach, "zmxName").map(str::to_string);

    Ok(GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    })
}

pub(crate) fn gpui_command_terminal_create_session_params(
    input: &GpuiCommandTerminalCreateInput,
) -> serde_json::Value {
    /*
    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    GPUI command-pane creation mirrors native `createCommandTerminal`: create
    the gxserver row first with `surface:"commands"` and zmx provider metadata,
    then pass Action startup text through the attach RPC sequence rather than
    persisting it in session launchSettings or spawning it as the rendered
    terminal process command.
    */
    let mut launch_settings = serde_json::Map::new();
    launch_settings.insert(
        "surface".to_string(),
        serde_json::Value::String("commands".to_string()),
    );
    if let Some(command_title) = input.command_title.as_deref() {
        launch_settings.insert(
            "commandTitle".to_string(),
            serde_json::Value::String(command_title.to_string()),
        );
    }
    let mut params = serde_json::Map::new();
    if let Some(command_id) = input.command_id.as_deref() {
        params.insert(
            "commandId".to_string(),
            serde_json::Value::String(command_id.to_string()),
        );
    }
    params.insert(
        "cwd".to_string(),
        serde_json::Value::String(input.cwd.clone()),
    );
    params.insert(
        "kind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    params.insert(
        "launchSettings".to_string(),
        serde_json::Value::Object(launch_settings),
    );
    params.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("running".to_string()),
    );
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(input.project_id.clone()),
    );
    params.insert(
        "providerState".to_string(),
        serde_json::json!({
            "lifecycleState": "exists",
            "provider": "zmx",
        }),
    );
    params.insert(
        "runtimeSettings".to_string(),
        serde_json::json!({
            "sessionPersistenceProvider": "zmx",
            "titleSource": "user",
        }),
    );
    params.insert(
        "surface".to_string(),
        serde_json::Value::String("commands".to_string()),
    );
    params.insert(
        "title".to_string(),
        serde_json::Value::String(input.title.clone()),
    );
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_create_local_project_workspace_terminal(
    project_id: &str,
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
    /*
    CDXC:GPUIWindowsTerminalStartup 2026-07-26:
    The selected WSL gxserver can create a fresh workspace terminal, start its
    never-reused zmx identity, and return the final require-existing attach
    plan in one operation. Windows uses that atomic path so New Terminal does
    not serialize three known-missing provider probes before materialization.
    macOS and Linux retain that established lifecycle.
    */
    #[cfg(target_os = "windows")]
    let result = {
        let mut params = serde_json::Map::new();
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        if gpui_current_zmx_prompt_editor_attach_mode_is_monaco() {
            params.insert(
                "promptEditor".to_string(),
                serde_json::Value::String("monaco".to_string()),
            );
        }
        gpui_gxserver_rpc_result(
            "/api/createWorkspaceTerminal",
            &serde_json::Value::Object(params),
            Duration::from_secs(30),
        )?
    };
    #[cfg(not(target_os = "windows"))]
    let result = gpui_gxserver_rpc_result(
        "/api/createSession",
        &serde_json::json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "projectId": project_id,
            "surface": "workspace",
            "title": "Terminal",
        }),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create a companion terminal.".to_string())?;
    let created_project_id = gpui_trimmed_json_string_field(session, "projectId")
        .unwrap_or(project_id)
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return a companion terminal id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid companion terminal id.".to_string());
    }
    let key = GpuiLocalWorkspaceSessionKey {
        project_id: created_project_id,
        session_id,
    };
    #[cfg(target_os = "windows")]
    let plan = gpui_local_workspace_attach_terminal_plan_from_result(&result, None, None)?;
    #[cfg(not(target_os = "windows"))]
    let plan = gpui_prepare_local_workspace_attach_terminal_plan(
        &key,
        GpuiLocalWorkspaceAttachIntent::Attach,
    )?;
    Ok((key, plan))
}

pub(crate) fn gpui_local_workspace_attach_rpc_params(
    reference: &GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(reference.project_id.clone()),
    );
    params.insert(
        "sessionId".to_string(),
        serde_json::Value::String(reference.session_id.clone()),
    );
    if let Some(startup_text) = startup_text {
        params.insert(
            "startupText".to_string(),
            serde_json::Value::String(startup_text.to_string()),
        );
    }
    if gpui_current_zmx_prompt_editor_attach_mode_is_monaco() {
        params.insert(
            "promptEditor".to_string(),
            serde_json::Value::String("monaco".to_string()),
        );
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_local_workspace_attach_object(
    result: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, String> {
    result
        .get("attach")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Session attach metadata is unavailable.".to_string())
}

pub(crate) fn gpui_validate_local_workspace_attach_metadata(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    if attach.get("provider").and_then(serde_json::Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|provider| provider.get("lifecycleState"))
            .and_then(serde_json::Value::as_str)
            != Some("exists")
    {
        return Err(
            "gxserver did not confirm the zmx provider exists before terminal attach.".to_string(),
        );
    }
    if gpui_local_workspace_attach_string(attach, "attachCommand").is_none() {
        return Err("Session attach metadata is unavailable.".to_string());
    }
    if gpui_local_workspace_attach_has_terminal_ready_startup_text(attach)
        && gpui_local_workspace_attach_persistence_session_created(attach) == Some(false)
    {
        return Err(
            "gxserver did not confirm the session provider started before terminal attach."
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn gpui_validate_local_workspace_attach_not_restore_blocked(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    if attach.get("restoreBlocked").is_some() {
        return Err(
            "Session restore is blocked because its working directory is unavailable.".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn should_start_local_zmx_provider_before_gpui_attach(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    /*
    CDXC:GPUIZmxProviderOwnership 2026-07-15:
    A GPUI attach must never be the operation that creates a missing zmx
    provider. gxserver owns provider startup for both restored agents and
    blank terminal rows because that path installs the prompt-editor wrapper
    before the interactive shell starts. The current attach client still
    decides Monaco versus the machine editor through its advertised zmx
    capability; provider initialization does not make Monaco durable.
    */
    attach.get("provider").and_then(serde_json::Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|provider| provider.get("lifecycleState"))
            .and_then(serde_json::Value::as_str)
            == Some("missing")
}

pub(crate) fn gpui_local_workspace_attach_startup_text(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&str> {
    attach
        .get("startupText")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_local_workspace_attach_startup_text_disposition(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&str> {
    attach
        .get("startupTextDisposition")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_local_workspace_attach_has_terminal_ready_startup_text(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    gpui_local_workspace_attach_startup_text_disposition(attach) == Some("queueAfterTerminalReady")
        && gpui_local_workspace_attach_startup_text(attach).is_some()
}

pub(crate) fn gpui_local_workspace_attach_persistence_session_created(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<bool> {
    attach
        .get("persistenceSessionCreated")
        .and_then(serde_json::Value::as_bool)
}

pub(crate) fn gpui_local_workspace_attach_string<'a>(
    attach: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    attach
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
}

pub(crate) fn gpui_workspace_attach_title(attach: &serde_json::Map<String, serde_json::Value>) -> String {
    attach
        .get("session")
        .and_then(|session| session.get("title"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| {
            !title.is_empty()
                && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !title.contains('\0')
                && !title.chars().any(char::is_control)
        })
        .unwrap_or("Terminal")
        .to_string()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_gpui_local_workspace_attach_terminal_in_new_leaf(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    requested_pane_id: WorkspacePaneId,
    placement: AgentsWorkspaceNewTerminalPlacement,
    key: GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<(WorkspacePaneId, TerminalSessionId), &'static str> {
    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the session attach terminal command.")?;

    let created = match placement {
        AgentsWorkspaceNewTerminalPlacement::Tab => {
            return Err("GPUI cannot open a tab placement as a new workspace leaf.");
        }
        AgentsWorkspaceNewTerminalPlacement::SplitRight => {
            workspace.split_mounting_session_to_right_of_pane(requested_pane_id)
        }
        AgentsWorkspaceNewTerminalPlacement::SplitBelow => {
            workspace.split_mounting_session_below_pane(requested_pane_id)
        }
        AgentsWorkspaceNewTerminalPlacement::BottomRow => workspace
            .resolve_action_pane_id(requested_pane_id)
            .map(|resolved_pane_id| {
                workspace.focus_pane(resolved_pane_id);
                workspace.append_mounting_session_bottom_row()
            }),
    };
    let Some((pane_id, session_id)) = created else {
        return Err("GPUI could not create a workspace pane for the session.");
    };
    let gxserver_session_id = key.session_id.clone();
    let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    else {
        return Err("GPUI could not find the terminal tab for the session.");
    };
    session.title = title;
    session.agent_icon = agent_icon;
    session.zmx_session_name = zmx_name;
    session.set_presentation_state_with_startup_eligibility(
        TerminalSessionPresentationState::Running,
        false,
    );
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        },
        payload,
    );
    local_workspace_session_mappings.insert(key, session_id);
    local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
    Ok((pane_id, session_id))
}

pub(crate) fn insert_gpui_local_workspace_attach_terminal(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    requested_pane_id: WorkspacePaneId,
    force_requested_pane_placement: bool,
    key: GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<(WorkspacePaneId, TerminalSessionId), &'static str> {
    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the session attach terminal command.")?;

    let gxserver_session_id = key.session_id.clone();
    if let Some(session_id) = local_workspace_session_mappings.get(&key).copied() {
        let existing_pane_id = workspace
            .pane_id_for_session(session_id)
            .filter(|pane_id| workspace.session_belongs_to_pane(*pane_id, session_id));
        if let Some(mut pane_id) = existing_pane_id {
            if pane_id != requested_pane_id && force_requested_pane_placement {
                if !workspace.group_tab_into_pane(pane_id, requested_pane_id, session_id) {
                    return Err("GPUI could not move the mapped terminal into the target group.");
                }
                pane_id = requested_pane_id;
            }
            let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
            let mount_slot_id = AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            };
            launch_payload_source.insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                mount_slot_id,
                payload,
            );
            let Some(session) = workspace
                .terminal_sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            else {
                return Err("GPUI could not find the mapped terminal tab for the session.");
            };
            session.title = title;
            session.agent_icon = agent_icon;
            session.zmx_session_name = zmx_name;
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Running,
                false,
            );
            workspace.select_tab(pane_id, session_id);
            local_workspace_session_mappings.insert(key, session_id);
            local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
            return Ok((pane_id, session_id));
        }

        local_workspace_session_mappings.remove(&key);
        local_app_shot_session_mappings
            .retain(|_, mapped_session_id| *mapped_session_id != session_id);
    }

    if workspace.find_leaf(requested_pane_id).is_none() {
        return Err("GPUI could not find the target pane for the session.");
    }
    let Some((pane_id, session_id)) =
        workspace.add_running_session_to_pane(requested_pane_id, title, agent_icon)
    else {
        return Err("GPUI could not create a terminal tab for the session.");
    };
    if let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.zmx_session_name = zmx_name;
    }
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
    let mount_slot_id = AgentsTerminalBodyMountSlotId {
        pane_id,
        session_id,
    };
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        mount_slot_id,
        payload,
    );
    local_workspace_session_mappings.insert(key, session_id);
    local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
    Ok((pane_id, session_id))
}

pub(crate) fn attach_gpui_surfaced_local_workspace_terminal(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    pane_id: WorkspacePaneId,
    key: &GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<TerminalSessionId, &'static str> {
    /*
    CDXC:GPUIWorkspaceSessionReattach 2026-07-24:
    Startup reattachment for a surfaced non-focused terminal is an ownership
    repair, not a selection action. Require the canonical gxserver mapping to
    still identify the active Running terminal in this exact rendered pane,
    then seed only that slot's process-local launch payload. Do not select the
    tab, change the focused pane, publish sidebar focus, or derive a fallback
    target if the restored layout changed while metadata was loading.
    */
    let shell_session_id = local_workspace_session_mappings
        .get(key)
        .copied()
        .ok_or("GPUI could not find the surfaced terminal mapping.")?;
    if workspace.pane_id_for_session(shell_session_id) != Some(pane_id)
        || workspace.active_session_in_pane(pane_id) != Some(shell_session_id)
    {
        return Err("GPUI surfaced terminal placement changed before attach completed.");
    }
    let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == shell_session_id)
    else {
        return Err("GPUI could not find the surfaced terminal tab.");
    };
    if session.presentation_state != TerminalSessionPresentationState::Running {
        return Err("GPUI surfaced terminal is not attachable.");
    }

    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the surfaced session attach command.")?;

    session.title = title;
    session.agent_icon = agent_icon;
    session.zmx_session_name = zmx_name;
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(shell_session_id);
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        },
        payload,
    );
    local_app_shot_session_mappings.insert(key.session_id.clone(), shell_session_id);
    Ok(shell_session_id)
}

