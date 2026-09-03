// Remote sidebar HUD reads and remote Action execution.
//
// A project's Actions are stored by the gxserver daemon that owns the project,
// so a remote project's Actions can only come from that machine. This module
// holds the bridge shaping for the remote `/api/readSidebarHud` read, the
// titlebar's own remote HUD read, and the create/start/attach sequence that
// runs a terminal Action on the owning machine.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

/*
CDXC:RemoteMachines 2026-08-29:
`/api/readSidebarHud` is a pure read on the machine that owns the project, so
the bridge reduces its params to one validated project id and pins the
per-project command block on. CEF cannot use this route to ask a remote daemon
for anything else, and the daemon's answer is reduced below to the Action
button contract the sidebar renders.
*/
pub(crate) fn gpui_remote_sidebar_read_sidebar_hud_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "includeAllProjectCommands".to_string(),
        serde_json::Value::Bool(true),
    );
    if let Some(active_project_id) = object
        .get("activeProjectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        shaped.insert(
            "activeProjectId".to_string(),
            serde_json::Value::String(active_project_id.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_hud_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:RemoteMachines 2026-08-29:
    Only the Action button lists cross this boundary. The remote daemon also
    projects agents, project settings rows, and project paths through the same
    endpoint; those stay on that machine because remote project rows and the
    titlebar Actions control render Actions and nothing else.
    */
    let mut response = serde_json::Map::new();
    response.insert(
        "commands".to_string(),
        gpui_remote_sidebar_hud_command_list_payload(result.get("commands")),
    );
    response.insert(
        "globalCommands".to_string(),
        gpui_remote_sidebar_hud_command_list_payload(result.get("globalCommands")),
    );
    if let Some(commands_by_project) = result
        .get("commandsByProject")
        .and_then(serde_json::Value::as_object)
    {
        let mut shaped = serde_json::Map::new();
        for (project_id, commands) in commands_by_project {
            if !gpui_remote_sidebar_project_id_allowed(project_id.as_str()) {
                continue;
            }
            shaped.insert(
                project_id.clone(),
                gpui_remote_sidebar_hud_command_list_payload(Some(commands)),
            );
        }
        response.insert(
            "commandsByProject".to_string(),
            serde_json::Value::Object(shaped),
        );
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_hud_command_list_payload(
    commands: Option<&serde_json::Value>,
) -> serde_json::Value {
    const MAX_COMMANDS: usize = 128;
    serde_json::Value::Array(
        commands
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(gpui_remote_sidebar_hud_command_payload)
            .take(MAX_COMMANDS)
            .collect(),
    )
}

pub(crate) fn gpui_remote_sidebar_hud_command_payload(
    command: &serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_COMMAND_CHARS: usize = 4_000;
    const MAX_LABEL_CHARS: usize = 400;
    const MAX_LINKS: usize = 16;

    let object = command.as_object()?;
    let command_id = json_string_field(object, "commandId")
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_bounded_text_label_allowed(value))?;
    let action_type = json_string_field(object, "actionType")
        .filter(|value| matches!(*value, "browser" | "terminal"))
        .unwrap_or("terminal");
    let bounded = |value: Option<&str>, max_chars: usize| -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.chars().count() <= max_chars)
            .map(str::to_string)
    };
    let mut output = serde_json::Map::new();
    output.insert("actionType".to_string(), serde_json::json!(action_type));
    output.insert(
        "closeTerminalOnExit".to_string(),
        serde_json::json!(json_bool_field(object, "closeTerminalOnExit").unwrap_or(false)),
    );
    output.insert("commandId".to_string(), serde_json::json!(command_id));
    output.insert(
        "isDefault".to_string(),
        serde_json::json!(json_bool_field(object, "isDefault").unwrap_or(false)),
    );
    output.insert(
        "name".to_string(),
        serde_json::json!(
            bounded(json_string_field(object, "name"), MAX_LABEL_CHARS).unwrap_or_default()
        ),
    );
    output.insert(
        "playCompletionSound".to_string(),
        serde_json::json!(json_bool_field(object, "playCompletionSound").unwrap_or(false)),
    );
    output.insert(
        "showOnProjectRow".to_string(),
        serde_json::json!(json_bool_field(object, "showOnProjectRow").unwrap_or(false)),
    );
    if let Some(command) = bounded(json_string_field(object, "command"), MAX_COMMAND_CHARS) {
        output.insert("command".to_string(), serde_json::json!(command));
    }
    if let Some(icon) = bounded(json_string_field(object, "icon"), MAX_LABEL_CHARS) {
        output.insert("icon".to_string(), serde_json::json!(icon));
    }
    if let Some(url) = bounded(json_string_field(object, "url"), MAX_COMMAND_CHARS) {
        output.insert("url".to_string(), serde_json::json!(url));
    }
    let links = json_array_field(object, "links")
        .into_iter()
        .flatten()
        .filter_map(|link| {
            let link = link.as_object()?;
            let target = json_string_field(link, "target")
                .filter(|value| matches!(*value, "external" | "integrated"))?;
            let url = bounded(json_string_field(link, "url"), MAX_COMMAND_CHARS)?;
            Some(serde_json::json!({ "target": target, "url": url }))
        })
        .take(MAX_LINKS)
        .collect::<Vec<_>>();
    if !links.is_empty() {
        output.insert("links".to_string(), serde_json::Value::Array(links));
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_titlebar_actions_for_project(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Vec<GpuiTitlebarAction> {
    /*
    CDXC:RemoteMachines 2026-08-29:
    When the active project is remote, the titlebar Actions control reads the
    owning machine's HUD projection through the same Rust-owned tunnel every
    other remote gxserver read uses. There is no local fallback: the local
    daemon does not know a remote project id and would answer with its own
    unconfigured defaults.
    */
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/readSidebarHud",
        &serde_json::json!({ "activeProjectId": project_id }),
        Duration::from_secs(5),
    )
    .ok()
    .map(|hud| {
        gpui_titlebar_actions_from_sidebar_command_buttons(
            hud.get("commands").unwrap_or(&serde_json::Value::Null),
        )
    })
    .unwrap_or_default()
}

pub(crate) fn gpui_run_remote_project_action_terminal(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    project: &GpuiRemoteProjectReference,
    title: &str,
    command: &str,
) -> Result<
    (
        GpuiRemoteAttachSessionReference,
        GpuiRemoteAttachTerminalPlan,
    ),
    String,
> {
    /*
    CDXC:RemoteMachines 2026-08-29:
    A terminal Action on a remote project has to run on the machine that owns
    the project, so it materializes as an ordinary gxserver session there: the
    daemon creates the row, starts the zmx provider with the Action command as
    its startup command, and the desktop attaches to it like any other remote
    terminal. Running it through the local command pane would execute the
    project's command on the wrong machine, against a path that does not exist
    here.

    CDXC:RemoteMachines 2026-08-29:
    The row is created on the `commands` surface, exactly like the local
    command-pane Action session in `gpui_command_terminal_create_session_params`.
    That is what makes gxserver mark it `visibleInSidebarByDefault: false`, so
    the remote project's session list and Agents tab strip stay free of a row
    for a terminal that lives in this app's command pane — the same contract the
    local Action tabs already have.
    */
    let created = gpui_remote_gxserver_rpc_result(
        target,
        "/api/createSession",
        &serde_json::json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "projectId": project.project_id.as_str(),
            "surface": "commands",
            "title": title,
        }),
        Duration::from_secs(30),
    )?;
    let session = created
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote gxserver did not create an Action session.".to_string())?;
    let project_id = gpui_trimmed_json_string_field(session, "projectId")
        .ok_or_else(|| "Remote gxserver did not return an Action project id.".to_string())?
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "Remote gxserver did not return an Action session id.".to_string())?
        .to_string();
    if project_id != project.project_id
        || !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("Remote gxserver returned an invalid Action session.".to_string());
    }
    let reference = GpuiRemoteAttachSessionReference {
        remote_machine_id: project.remote_machine_id.clone(),
        project_id,
        session_id,
    };
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/startSessionProvider",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
            "startupText": command,
        }),
        Duration::from_secs(30),
    )?;
    /*
    CDXC:RemoteMachines 2026-08-29:
    The provider was just started with the Action command as its startup text,
    so this plan reads attach metadata instead of waking the session (a wake
    would restart the command). It is still an interactive attach: the command
    pane spawns this ssh itself, so it needs the saved-password askpass helper
    or ssh prompts for the password inside the Action's own pane.
    */
    let plan = gpui_prepare_remote_attach_terminal_plan(config, target, &reference, false, true)?;
    Ok((reference, plan))
}

pub(crate) fn gpui_update_remote_command_action_session_surface(
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    surface: &str,
) -> Result<(), String> {
    /*
    CDXC:RemoteMachines 2026-08-29:
    Dragging a remote Action's command tab into the Agents workspace keeps the
    same live SSH attach, so the remote row has to change surfaces with it for
    the same reason the local one does: the sidebar projects only `workspace`
    sessions, and an Agents tab missing from that projection is reconciled away
    together with its terminal. Only the fixed surface enum and the session
    selectors cross the tunnel.
    */
    if surface != "workspace" && surface != "commands" {
        return Err("The command terminal surface is invalid.".to_string());
    }
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/updateSession",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
            "surface": surface,
        }),
        Duration::from_secs(10),
    )
    .map(|_| ())
}

pub(crate) fn gpui_close_remote_command_action_session(
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
) {
    /*
    CDXC:RemoteMachines 2026-08-29:
    A remote Action's command tab owns the gxserver session it created on the
    owning machine, so closing the tab has to close that session — the same
    ownership `gpui_close_command_terminal_gxserver_session` implements for a
    local Action tab, over the remote tunnel. Without it every rerun (which
    always replaces the tab, see `run_gpui_remote_command_action_terminal`)
    would leave a live `commands`-surface session and its zmx process behind on
    the remote machine. Transition first so the daemon owns provider shutdown
    and lifecycle history, then remove the row.
    */
    let _ = gpui_remote_gxserver_rpc_result(
        target,
        "/api/transitionSession",
        &serde_json::json!({
            "action": "close",
            "projectId": reference.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(30),
    );
    let _ = gpui_remote_gxserver_rpc_result(
        target,
        "/api/removeSession",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "reason": "closeTerminal",
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(10),
    );
}
