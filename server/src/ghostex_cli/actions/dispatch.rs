use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{self, CliError, CliResult};
use crate::ghostex_cli::sessions;

use super::*;

/// sendGxserverCliAction: hard-cutover action → gxserver endpoint switch.
pub fn send_gxserver_cli_action(action: &str, payload: &Value, flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:Cli 2026-05-30-15:15:
    gx/ghostex remains the user CLI, but hard-cutover commands must talk to
    gxserver instead of the macOS app bridge. Renderer-only commands still
    enter through a gxserver API endpoint so auth, protocol, remote access,
    and unsupported-action failures stay daemon-owned.
    */
    match action {
        "listSessions" => sessions::fetch_gxserver_session_list(flags),
        "state" | "dumpState" => sessions::fetch_gxserver_state(flags),
        "createQuickTerminal" => create_gxserver_quick_terminal(payload, flags),
        "createSession" => create_gxserver_session(payload, flags),
        "createAgentSession" | "runAgent" => create_gxserver_agent_session(payload, flags),
        "saveCommand" => save_gxserver_command(payload, flags),
        "addProject" => rpc::call_gxserver_rpc("/api/addProjectPath", payload, flags),
        "browseDirectories" => {
            rpc::call_gxserver_rpc("/api/browseProjectDirectories", payload, flags)
        }
        "discoverSourceControl" => {
            rpc::call_gxserver_rpc("/api/discoverSourceControl", payload, flags)
        }
        "lookupRepository" => rpc::call_gxserver_rpc("/api/lookupRepository", payload, flags),
        "cloneRepository" => clone_repository_and_wait(payload, flags),
        "holdSessionsAwake" => rpc::call_gxserver_rpc("/api/holdSessionsAwake", payload, flags),
        "holdSessionChatGrid" => rpc::call_gxserver_rpc("/api/holdSessionChatGrid", payload, flags),
        "recordClientEvent" => {
            rpc::call_gxserver_flat_body("/api/recordClientEvent", payload, flags)
        }
        "removeProject" => rpc::call_gxserver_rpc("/api/removeProject", payload, flags),
        "restoreRecentProject" => {
            rpc::call_gxserver_rpc("/api/restoreRecentProject", payload, flags)
        }
        "readSidebarProjectCollections" => {
            rpc::call_gxserver_rpc("/api/readSidebarProjectCollections", payload, flags)
        }
        "updateSidebarProjectCollections" => {
            rpc::call_gxserver_rpc("/api/updateSidebarProjectCollections", payload, flags)
        }
        "assignProjectToSidebarCollection" => {
            rpc::call_gxserver_rpc("/api/assignProjectToSidebarCollection", payload, flags)
        }
        "readSidebarSpaces" => rpc::call_gxserver_rpc("/api/readSidebarSpaces", payload, flags),
        "updateSidebarSpaces" => rpc::call_gxserver_rpc("/api/updateSidebarSpaces", payload, flags),
        "readCustomSessionTags" => {
            rpc::call_gxserver_rpc("/api/readCustomSessionTags", payload, flags)
        }
        "updateCustomSessionTags" => {
            rpc::call_gxserver_rpc("/api/updateCustomSessionTags", payload, flags)
        }
        "closeSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/killSession", &params, flags)
        }
        "sleepSession" => {
            let pathname = if payload.get("sleeping") == Some(&Value::Bool(false)) {
                "/api/wakeSession"
            } else {
                "/api/sleepSession"
            };
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc(pathname, &params, flags)
        }
        "forkSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/forkSession", &params, flags)
        }
        "switchDraftAgent" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/switchDraftAgent", &params, flags)
        }
        "renameSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/updateSession", &params, flags)
        }
        "tagSession" => {
            let payload = resolve_custom_session_tag_for_tag_session(payload, flags)?;
            let params = with_resolved_gxserver_session_params(&payload, flags)?;
            rpc::call_gxserver_rpc("/api/updateSession", &params, flags)
        }
        "requestSessionRename" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/requestSessionRename", &params, flags)
        }
        /*
        CDXC:SessionNotes 2026-08-24:
        Ghostex mobile has no HTTP path to gxserver, so the session-note pair is
        exposed as CLI verbs the phone SSH-execs, exactly like the Session Chat
        endpoints below. The daemon resolves the note's agent session id itself,
        so the phone only ever sends the session selector.
        */
        "readSessionAgentNote" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionAgentNote", &params, flags)
        }
        "saveSessionAgentNote" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/saveSessionAgentNote", &params, flags)
        }
        "parkSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            super::session_parking::set_parked(&params, flags)
        }
        "pinSession" => {
            let mut object = payload.as_object().cloned().unwrap_or_default();
            set_or_remove(&mut object, "isPinned", payload.get("pinned").cloned());
            let params = with_resolved_gxserver_session_params(&Value::Object(object), flags)?;
            rpc::call_gxserver_rpc("/api/updateSession", &params, flags)
        }
        "acknowledgeSessionAttention" => {
            let mut object = payload.as_object().cloned().unwrap_or_default();
            object.insert("event".to_string(), json!("acknowledge"));
            let params = with_resolved_gxserver_session_params(&Value::Object(object), flags)?;
            rpc::call_gxserver_rpc("/api/updateAgentActivity", &params, flags)
        }
        "focusSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/focusSession", &params, flags)
        }
        "readSessionText" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionText", &params, flags)
        }
        "searchAgentPrompts" => rpc::call_gxserver_rpc("/api/searchAgentPrompts", payload, flags),
        "readAgentPromptText" => rpc::call_gxserver_rpc("/api/readAgentPromptText", payload, flags),
        "toggleAgentPromptFavorite" => {
            rpc::call_gxserver_rpc("/api/toggleAgentPromptFavorite", payload, flags)
        }
        "resolveAgentPromptLaunch" => {
            rpc::call_gxserver_rpc("/api/resolveAgentPromptLaunch", payload, flags)
        }
        "readSessionChat" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionChat", &params, flags)
        }
        "readSessionChatSkills" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionChatSkills", &params, flags)
        }
        "readSessionChatFiles" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionChatFiles", &params, flags)
        }
        "readSessionFilesLinks" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionFilesLinks", &params, flags)
        }
        "readSessionDetails" => {
            let mut params = with_resolved_gxserver_session_params(payload, flags)?;
            if flags.contains("refresh") {
                params["breakdown"] = json!("refresh");
            }
            rpc::call_gxserver_rpc("/api/readSessionDetails", &params, flags)
        }
        "selectSessionChatModel" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/selectSessionChatModel", &params, flags)
        }
        "sendSessionChatMessage" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/sendSessionChatMessage", &params, flags)
        }
        "answerSessionChatPrompt" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/answerSessionChatPrompt", &params, flags)
        }
        "rewindSessionChat" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            /*
            CDXC:SessionChat 2026-09-02:
            The daemon holds this request while it drives the agent's rewind
            dialog and verifies every step against the screen, which is several
            six-second waits in the worst case, so the default 15s RPC timeout
            would cut off a slow-but-successful drive. Callers can still
            override.
            */
            let mut flags = flags.clone();
            if !flags.contains("timeout") && !flags.contains("timeoutMs") {
                flags.insert_text("timeoutMs", "90000");
            }
            rpc::call_gxserver_rpc("/api/rewindSessionChat", &params, &flags)
        }
        "interruptSessionChat" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/interruptSessionChat", &params, flags)
        }
        "handoffSessionChatDraft" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            /*
            The daemon holds this request while the agent CLI answers the
            Ctrl+G handshake (up to 16s), so the default 15s RPC timeout would
            cut off a slow-but-successful transfer. Callers can still override.
            */
            let mut flags = flags.clone();
            if !flags.contains("timeout") && !flags.contains("timeoutMs") {
                flags.insert_text("timeoutMs", "30000");
            }
            rpc::call_gxserver_rpc("/api/handoffSessionChatDraft", &params, &flags)
        }
        "readSessionChatQueue" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/readSessionChatQueue", &params, flags)
        }
        "queueSessionChatPrompt" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/queueSessionChatPrompt", &params, flags)
        }
        "updateSessionChatQueuedPrompt" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/updateSessionChatQueuedPrompt", &params, flags)
        }
        "removeSessionChatQueuedPrompt" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/removeSessionChatQueuedPrompt", &params, flags)
        }
        "reorderSessionChatQueue" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/reorderSessionChatQueue", &params, flags)
        }
        "sendSessionChatQueuedPrompt" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/sendSessionChatQueuedPrompt", &params, flags)
        }
        "setSessionChatDraft" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/setSessionChatDraft", &params, flags)
        }
        "exportSessionTranscript" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/exportSessionTranscript", &params, flags)
        }
        "countSessionTranscriptTurns" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/countSessionTranscriptTurns", &params, flags)
        }
        "sendText" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/sendSessionText", &params, flags)
        }
        "sendEnter" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/sendSessionEnter", &params, flags)
        }
        "sendKey" => send_gxserver_session_key(payload, flags),
        "renameCommand" => send_gxserver_rename_command(payload, flags),
        "sendMessage" => rpc::call_gxserver_rpc("/api/sendSessionMessage", payload, flags),
        "scheduleDelayedSend" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/scheduleDelayedSend", &params, flags)
        }
        "cancelDelayedSend" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/cancelDelayedSend", &params, flags)
        }
        "assertSidebarCard" | "saveAgent" | "setViewMode" | "setVisibleCount" | "waitFor" => {
            Err(retired_renderer_action_error(action))
        }
        /*
        CDXC:Sessions 2026-08-17:
        Close After Done remains owned by the connected sidebar renderer, so
        the mobile CLI forwards its existing command payload instead of
        pretending that it is an unsupported CLI action. Delayed Send uses the
        first-class gxserver endpoints above.
        */
        "clickButton"
        | "focusGroup"
        | "fullReloadSession"
        | "listBrowserPages"
        | "moveProject"
        | "openBrowser"
        | "openBrowserPane"
        | "openPaths"
        | "openSettings"
        | "readResourcesSnapshot"
        | "restartSession"
        | "runCommand"
        | "switchProject"
        | "toggleCloseAfterDone"
        | "toggleSidebarCollapsed"
        | "updateSettingsPatch" => dispatch_gxserver_renderer_command(action, payload, flags),
        other => Err(rpc::unsupported_action_error(other)),
    }
}

/// A verb whose feature is gone from the desktop app (why each one was retired:
/// packages/gx-core/src/renderer_commands/verbs.rs).
fn retired_renderer_action_error(action: &str) -> CliError {
    let verb = match action {
        "assertSidebarCard" => "assert-card",
        "saveAgent" => "save-agent",
        "setViewMode" => "set-view-mode",
        "setVisibleCount" => "set-visible-count",
        "waitFor" => "wait-for",
        other => other,
    };
    CliError::Other(format!(
        "`ghostex {verb}` was retired: the Ghostex app no longer has the feature it drove."
    ))
}

/// dispatchGxserverRendererCommand: CLI commands that still need visible macOS
/// workspace state route through gxserver's renderer-command endpoint.
fn dispatch_gxserver_renderer_command(
    action: &str,
    payload: &Value,
    flags: &Flags,
) -> CliResult<Value> {
    /*
    CDXC:CefRuntime 2026-06-21-19:22:
    Renderer commands may target sessions by gxserver's raw project-scoped id,
    while the macOS sidebar can render combined project/session ids. Carry a
    structured `sessionTarget` whenever projectId/sessionId are present so the
    renderer can resolve the target without callers learning presentation ids.
    */
    rpc::call_gxserver_rpc(
        "/api/dispatchRendererCommand",
        &json!({ "action": action, "payload": with_renderer_session_target(payload) }),
        flags,
    )
}

pub(super) fn with_renderer_session_target(payload: &Value) -> Value {
    let Some(object) = payload.as_object() else {
        return payload.clone();
    };
    if matches!(object.get("sessionTarget"), Some(value) if value.is_object() || value.is_array()) {
        return payload.clone();
    }
    let project_id = string_or_empty(object.get("projectId")).trim().to_string();
    let session_id = string_or_empty(object.get("sessionId")).trim().to_string();
    if project_id.is_empty() || session_id.is_empty() {
        return payload.clone();
    }
    let global_ref = string_or_empty(object.get("globalRef")).trim().to_string();
    let mut session_target = Map::new();
    if !global_ref.is_empty() {
        session_target.insert("globalRef".to_string(), Value::String(global_ref));
    }
    session_target.insert("projectId".to_string(), Value::String(project_id));
    session_target.insert("sessionId".to_string(), Value::String(session_id));
    let mut next = object.clone();
    next.insert("sessionTarget".to_string(), Value::Object(session_target));
    Value::Object(next)
}

fn send_gxserver_session_key(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let key_string = match payload.get("key") {
        Some(value) => js_string(value),
        None => "undefined".to_string(),
    };
    let Some(text) = terminal_text_for_cli_key(&key_string) else {
        return Err(CliError::Other(format!("Unsupported key: {key_string}")));
    };
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.insert("text".to_string(), Value::String(text.to_string()));
    let params = with_resolved_gxserver_session_params(&Value::Object(object), flags)?;
    rpc::call_gxserver_rpc("/api/sendSessionText", &params, flags)
}

fn send_gxserver_rename_command(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let title = string_or_empty(payload.get("title")).trim().to_string();
    if title.is_empty() {
        return Err(CliError::Other(
            "rename-command requires --title or a positional title.".to_string(),
        ));
    }
    let params = with_resolved_gxserver_session_params(payload, flags)?;
    /*
    CDXC:SessionTitles 2026-09-27 WHY:
    This verb used to hand the rename to the desktop window, which pulled the session into view and typed `/rename` once its terminal mounted. A sleeping session never mounted in time, so the command was dropped while the verb still answered `accepted`, and the agent's older title then replaced the sidebar name on the next wake; it also moved the user's focus and did nothing without a desktop window. It now takes the rename modal's gxserver request, which records the title, types the agent's own command (`/rename`, Pi `/name`, Hermes `/title`) with a separate Enter, handles ZCode without a command, and wakes a sleeping session to deliver it. This supersedes the 2026-06-17 note that zmx text left `/rename` staged; gxserver sends Enter as its own write now.
    */
    let mut object = params.as_object().cloned().unwrap_or_default();
    object.insert("title".to_string(), Value::String(title));
    object.insert("submitAgentRenameCommand".to_string(), Value::Bool(true));
    rpc::call_gxserver_rpc("/api/requestSessionRename", &Value::Object(object), flags)
}

pub(super) fn terminal_text_for_cli_key(key: &str) -> Option<&'static str> {
    match key {
        "enter" | "Enter" => Some("\r"),
        "ctrl-c" | "Control+C" => Some("\u{0003}"),
        "escape" | "Escape" => Some("\u{001b}"),
        "tab" | "Tab" => Some("\t"),
        "arrow-up" | "ArrowUp" => Some("\u{001b}[A"),
        "arrow-down" | "ArrowDown" => Some("\u{001b}[B"),
        "arrow-right" | "ArrowRight" => Some("\u{001b}[C"),
        "arrow-left" | "ArrowLeft" => Some("\u{001b}[D"),
        _ => None,
    }
}

fn save_gxserver_command(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let requested_action_type = string_or_empty(payload.get("actionType"))
        .trim()
        .to_ascii_lowercase();
    let action_type = match requested_action_type.as_str() {
        "terminal" => "terminal",
        "browser" => "browser",
        _ => {
            return Err(CliError::Other(
                "save-command --type must be terminal or browser.".to_string(),
            ))
        }
    };
    let command_id = string_or_empty(payload.get("commandId")).trim().to_string();
    let name = string_or_empty(payload.get("name")).trim().to_string();
    let command = string_or_empty(payload.get("command")).trim().to_string();
    let url = string_or_empty(payload.get("url")).trim().to_string();
    let missing_primary = if action_type == "terminal" {
        command.is_empty()
    } else {
        url.is_empty()
    };
    if command_id.is_empty() || name.is_empty() || missing_primary {
        return Err(CliError::Other(
            if action_type == "terminal" {
                "save-command requires --command-id, --name, and --command."
            } else {
                "save-command requires --command-id, --name, and --url for browser actions."
            }
            .to_string(),
        ));
    }

    let projects_result = rpc::call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let projects: Vec<Value> = projects_result
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let requested_path = if js_truthy(payload.get("path")) {
        Some(node_path_resolve(&js_string(
            payload.get("path").expect("truthy path"),
        )))
    } else {
        None
    };
    let project = projects.iter().find(|candidate| {
        if js_truthy(payload.get("projectId")) {
            candidate.get("projectId") == payload.get("projectId")
        } else if let Some(requested) = &requested_path {
            node_path_resolve(&string_or_empty(candidate.get("path"))) == *requested
        } else if js_truthy(payload.get("projectName")) {
            candidate.get("name") == payload.get("projectName")
        } else {
            node_path_resolve(&string_or_empty(candidate.get("path")))
                == node_path_resolve(&cwd_string())
        }
    });
    let Some(project) = project else {
        return Err(CliError::Other(
            "Could not resolve the Ghostex project for save-command. Pass --path or --project-id."
                .to_string(),
        ));
    };

    let existing_commands: Vec<Value> = project
        .get("customCommands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut saved_command = Map::new();
    saved_command.insert("actionType".to_string(), json!(action_type));
    saved_command.insert(
        "closeTerminalOnExit".to_string(),
        json!(
            action_type == "terminal"
                && payload.get("closeTerminalOnExit") == Some(&Value::Bool(true))
        ),
    );
    saved_command.insert("commandId".to_string(), json!(command_id));
    if js_truthy(payload.get("icon")) {
        saved_command.insert(
            "icon".to_string(),
            json!(js_string(payload.get("icon").expect("truthy icon"))),
        );
    }
    saved_command.insert(
        "isDefault".to_string(),
        json!(matches!(
            command_id.as_str(),
            "dev" | "build" | "test" | "setup"
        )),
    );
    saved_command.insert("name".to_string(), json!(name));
    saved_command.insert(
        "playCompletionSound".to_string(),
        json!(
            action_type == "terminal"
                && payload.get("playCompletionSound") != Some(&Value::Bool(false))
        ),
    );
    saved_command.insert(
        "showOnProjectRow".to_string(),
        json!(payload.get("showOnProjectRow") == Some(&Value::Bool(true))),
    );
    if action_type == "browser" {
        saved_command.insert("url".to_string(), json!(url));
    } else {
        saved_command.insert("command".to_string(), json!(command));
    }
    let saved_command = Value::Object(saved_command);

    let has_existing = existing_commands.iter().any(|candidate| {
        candidate.get("commandId").and_then(Value::as_str) == Some(command_id.as_str())
    });
    let next_commands: Vec<Value> = if has_existing {
        existing_commands
            .iter()
            .map(|candidate| {
                if candidate.get("commandId").and_then(Value::as_str) == Some(command_id.as_str()) {
                    saved_command.clone()
                } else {
                    candidate.clone()
                }
            })
            .collect()
    } else {
        let mut commands = existing_commands.clone();
        commands.push(saved_command.clone());
        commands
    };
    let existing_order: Vec<Value> = project
        .get("customCommandOrder")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let next_order: Vec<Value> = if existing_order
        .iter()
        .any(|candidate| candidate.as_str() == Some(command_id.as_str()))
    {
        existing_order
    } else {
        let mut order = existing_order.clone();
        order.push(json!(command_id));
        order
    };
    let deleted_default_command_ids: Vec<Value> = project
        .get("deletedDefaultCommandIds")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|candidate| candidate.as_str() != Some(command_id.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut params = Map::new();
    params.insert("customCommandOrder".to_string(), Value::Array(next_order));
    params.insert("customCommands".to_string(), Value::Array(next_commands));
    params.insert(
        "deletedDefaultCommandIds".to_string(),
        Value::Array(deleted_default_command_ids),
    );
    set_or_remove(&mut params, "projectId", project.get("projectId").cloned());
    rpc::call_gxserver_rpc("/api/updateProject", &Value::Object(params), flags)
}
