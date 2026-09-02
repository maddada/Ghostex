use std::path::{Component, Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, parse_boolean, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};
use crate::ghostex_cli::{selector, sessions};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's bridgeAction/resolvedSessionBridgeAction
wrappers, sendGxserverCliAction (the action → gxserver endpoint switch), the
renderer-command dispatch helpers, and every parse* payload parser. Payload
field names and undefined-vs-null semantics match the JS exactly: JS
`undefined` is represented as an absent key, JS `null` as Value::Null, and the
JS compactObject (which filters only `undefined`) therefore keeps explicit
nulls such as `sessionTag: null`.
*/

/// Sidebar session tags accepted by `ghostex tag-session` (insertion order
/// matters: it is reproduced in error messages).
const SIDEBAR_SESSION_TAGS: [&str; 13] = [
    "favorite",
    "high-priority",
    "low-priority",
    "todo",
    "research",
    "in-progress",
    "testing",
    "blocked",
    "on-hold",
    "done",
    "bug",
    "feature",
    "design",
];

const CLEAR_SESSION_TAG_VALUES: [&str; 5] = ["", "clear", "none", "null", "unset"];

#[derive(Clone, Copy, Debug)]
pub enum Parser {
    None,
    OpenPaths,
    EditPaths,
    QuickTerminal,
    CreateSession,
    Agent,
    CommandButton,
    ClickButton,
    SaveCommand,
    SaveAgent,
    SessionSelector,
    Group,
    Project,
    ProjectMove,
    ProjectPath,
    ProjectCollection,
    BrowseDirectories,
    LookupRepository,
    CloneRepository,
    Rename,
    /// `Rename` plus the agent-metadata flags `/api/requestSessionRename` takes.
    RenameRequest,
    SessionBoolean(&'static str),
    SessionTag,
    /*
    CDXC:SessionAgentNotes 2026-08-24:
    Session selector plus `--note`. The note is a user-authored body, so it
    travels as its own CLI argument (SSH quoting keeps it away from the
    selector) and is never printed anywhere but the JSON result.
    */
    SessionNote,
    /// Parse a session selector plus one Delayed Send trigger.
    DelayedSend,
    SendText,
    SendKey,
    VisibleCount,
    ViewMode,
    Url,
    /// parseBrowserOpen — used by the `browser open` subcommand
    /// (`bridgeAction("openBrowserPane", parseBrowserOpen)` in the Node CLI).
    BrowserOpen,
    AssertCard,
    WaitFor,
    SidebarProjectCollectionsState,
    SidebarSpacesState,
    /// session selector plus readSessionChat paging/long-poll flags.
    SessionChatRead,
    /// session selector plus the project agent id for a draft-agent switch.
    SessionChatDraftAgent,
    /// session selector plus one serialized Session Chat key name.
    SessionChatKey,
    /*
    CDXC:AgentHistorySearch 2026-08-20:
    Find over SSH for Ghostex mobile. `AgentPromptSearch` carries the query and
    filters; `AgentPromptRef` carries the stable prompt key that every follow-up
    call addresses a result by.
    */
    AgentPromptSearch,
    AgentPromptRef,
    AgentPromptLaunch,
    /// session selector plus `--answer-json` for answerSessionChatPrompt.
    SessionChatAnswer,
    /// session selector plus `--message-id` for rewindSessionChat.
    SessionChatRewind,
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    Queue rows are addressed by the `--prompt-id` the daemon handed out, never
    by a list position, so a phone acting on a row minutes after it rendered
    still lands on the prompt it displayed.
    */
    /// session selector plus `--prompt-id` (and `--text` / `--retry` for edits).
    SessionChatQueuedPrompt,
    /// session selector plus `--prompt-ids` as the new head-first row order.
    SessionChatQueueOrder,
    /// session selector plus `--content` and `--client-id` for the synced draft.
    SessionChatDraft,
    /*
    CDXC:MobileKeepAwake 2026-08-19:
    `--sessions-json` carries the whole attached-tab set in one exec so a phone
    renewing several holds costs one SSH round trip, not one per tab.
    */
    KeepSessionsAwake,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BridgeOptions {
    pub fail_on_not_ok: bool,
    pub assert_ok: bool,
}

/// bridgeAction(action, parser, options) applied to `args`.
pub fn run_bridge_action(
    action: &str,
    parser: Parser,
    options: BridgeOptions,
    args: &[String],
) -> CliResult<()> {
    let parsed = parse_args(args);
    let payload = evaluate_parser(parser, &parsed.rest, &parsed.flags)?;
    /*
    Long-running bridge commands (`edit --wait`) disable the CLI timeout when
    the caller did not pass one, exactly like the JS
    `payload.wait === true && flags.timeout === undefined` special case.
    */
    let bridge_flags =
        if payload.get("wait") == Some(&Value::Bool(true)) && !parsed.flags.contains("timeout") {
            let mut flags = parsed.flags.clone();
            flags.insert_text("timeout", "0");
            flags
        } else {
            parsed.flags.clone()
        };
    let result = send_gxserver_cli_action(action, &payload, &bridge_flags)?;
    /*
    CDXC:AndroidRemoteSessions 2026-05-17-14:24:
    Android remote actions use SSH exit status to decide whether to show
    recovery UI. Android-facing bridge commands such as rename-session must
    convert `{ ok: false }` bridge replies into a nonzero CLI exit.
    */
    if (options.assert_ok || options.fail_on_not_ok) && is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}

/// resolvedSessionBridgeAction(action, parser, options) applied to `args`.
pub fn run_resolved_session_bridge_action(
    action: &str,
    parser: Parser,
    options: BridgeOptions,
    args: &[String],
) -> CliResult<()> {
    let parsed = parse_args(args);
    let payload = evaluate_parser(parser, &parsed.rest, &parsed.flags)?;
    let selector_value = selector::session_selector_from_args(&parsed.rest, &parsed.flags);
    let resolved_session = match selector_value {
        Some(value) => Some(selector::resolve_cli_session_selector(
            &value,
            &parsed.flags,
        )?),
        None => None,
    };
    /*
    CDXC:CliSessionSelectors 2026-06-04-03:20:
    gxserver session ids are project-scoped. Selector-backed bridge actions
    must carry the resolved projectId with the sessionId so remote and mobile
    clients reconnect through the same S/P/G zmx route instead of addressing
    a bare G id.
    */
    let resolved_payload = if let Some(session) = &resolved_session {
        let mut object = payload.as_object().cloned().unwrap_or_default();
        let project_id = match payload.get("projectId") {
            Some(value) if !value.is_null() => Some(value.clone()),
            _ => session.get("projectId").cloned(),
        };
        set_or_remove(&mut object, "projectId", project_id);
        set_or_remove(&mut object, "sessionId", session.get("sessionId").cloned());
        Value::Object(object)
    } else {
        payload
    };
    let result = send_gxserver_cli_action(action, &resolved_payload, &parsed.flags)?;
    if (options.assert_ok || options.fail_on_not_ok) && is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}

/// sendGxserverCliAction: hard-cutover action → gxserver endpoint switch.
pub fn send_gxserver_cli_action(action: &str, payload: &Value, flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:GxserverCliCutover 2026-05-30-15:15:
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
        "createChatSession" => create_gxserver_chat_session(payload, flags),
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
        "renameSession" | "tagSession" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/updateSession", &params, flags)
        }
        "requestSessionRename" => {
            let params = with_resolved_gxserver_session_params(payload, flags)?;
            rpc::call_gxserver_rpc("/api/requestSessionRename", &params, flags)
        }
        /*
        CDXC:SessionAgentNotes 2026-08-24:
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
            CDXC:SessionChatRewind 2026-09-02:
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
        /*
        CDXC:MobileCloseAfterDone 2026-08-17:
        Close After Done remains owned by the connected sidebar renderer, so
        the mobile CLI forwards its existing command payload instead of
        pretending that it is an unsupported CLI action. Delayed Send uses the
        first-class gxserver endpoints above.
        */
        "assertSidebarCard"
        | "clickButton"
        | "focusGroup"
        | "fullReloadSession"
        | "moveProject"
        | "moveSidebar"
        | "openBrowser"
        | "openBrowserPane"
        | "openPaths"
        | "restartSession"
        | "runCommand"
        | "saveAgent"
        | "setViewMode"
        | "setVisibleCount"
        | "switchProject"
        | "toggleCloseAfterDone"
        | "toggleSidebarCollapsed"
        | "waitFor" => dispatch_gxserver_renderer_command(action, payload, flags),
        other => Err(rpc::unsupported_action_error(other)),
    }
}

/// dispatchGxserverRendererCommand: CLI commands that still need visible macOS
/// workspace state route through gxserver's renderer-command endpoint.
fn dispatch_gxserver_renderer_command(
    action: &str,
    payload: &Value,
    flags: &Flags,
) -> CliResult<Value> {
    /*
    CDXC:GxserverRendererCommands 2026-06-21-19:22:
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

fn with_renderer_session_target(payload: &Value) -> Value {
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
    CDXC:GenerateTitleSkill 2026-06-17-16:17:
    Claude Code leaves `/rename <title>` staged when Enter is sent as zmx text.
    Route generated-title renames through the native renderer command so the
    macOS host sends the same real `sendTerminalEnter` event used before the
    gxserver cutover.
    */
    let mut object = params.as_object().cloned().unwrap_or_default();
    object.insert("title".to_string(), Value::String(title));
    dispatch_gxserver_renderer_command("renameCommand", &Value::Object(object), flags)
}

fn terminal_text_for_cli_key(key: &str) -> Option<&'static str> {
    match key {
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

fn create_gxserver_quick_terminal(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let cwd = node_path_resolve(&match payload.get("cwd") {
        Some(value) if !value.is_null() => js_string(value),
        _ => cwd_string(),
    });
    let project_id: Option<Value> = if let Some(value) = flags.0.get("projectId") {
        Some(value.as_json())
    } else if let Some(value) = payload.get("projectId").filter(|value| !value.is_null()) {
        Some(value.clone())
    } else {
        let project = ensure_gxserver_project_for_path(&cwd, flags)?;
        if project.is_null() {
            // JS crashes here with a TypeError when result.project is missing.
            return Err(CliError::Other(
                "Cannot read properties of undefined (reading 'projectId')".to_string(),
            ));
        }
        project.get("projectId").cloned()
    };
    let mut inner = Map::new();
    if let Some(value) = payload.get("command") {
        inner.insert("command".to_string(), value.clone());
    }
    inner.insert("cwd".to_string(), json!(cwd));
    set_or_remove(&mut inner, "projectId", project_id);
    let title = match payload.get("title") {
        Some(value) if !value.is_null() => value.clone(),
        _ => {
            if js_truthy(payload.get("command")) {
                Value::String(js_slice_utf16(
                    &js_string(payload.get("command").expect("truthy command")),
                    80,
                ))
            } else {
                json!("Terminal")
            }
        }
    };
    inner.insert("title".to_string(), title);
    create_gxserver_session(&Value::Object(inner), flags)
}

fn create_gxserver_chat_session(payload: &Value, flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:MobileQuickSessions 2026-07-18:
    Mobile Quick "+" must mirror the GPUI Quick header: create a fresh
    projectless chat workspace through gxserver's createQuickProject, then
    create the initial terminal session inside it through the ordinary
    create-session path. gxserver stays the filesystem authority for
    ~/ghostex/chats so mobile never derives chat storage paths itself.
    */
    let created_project = rpc::call_gxserver_rpc(
        "/api/createQuickProject",
        &json!({ "kind": "terminal" }),
        flags,
    )?;
    let project_id = created_project
        .get("project")
        .and_then(|project| project.get("projectId"))
        .filter(|value| !value.is_null())
        .cloned();
    let project_id = match project_id {
        Some(value) => value,
        None => {
            return Err(CliError::Other(
                "createQuickProject did not return a projectId.".to_string(),
            ))
        }
    };
    let mut inner = payload.as_object().cloned().unwrap_or_default();
    inner.insert("projectId".to_string(), project_id);
    create_gxserver_session(&Value::Object(inner), flags)
}

fn create_gxserver_session(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let project_id_value = payload
        .get("projectId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("projectId").map(FlagValue::as_json));
    let project_id = normalize_required_project_id(project_id_value, "create-session")?;
    let input = string_or_empty(payload.get("input")).trim().to_string();
    let mut launch_settings = Map::new();
    if let Some(value) = payload.get("command") {
        launch_settings.insert("startupCommand".to_string(), value.clone());
    }
    if !input.is_empty() {
        launch_settings.insert("startupText".to_string(), json!(input));
    }
    let mut params = Map::new();
    if let Some(value) = payload.get("cwd") {
        params.insert("cwd".to_string(), value.clone());
    }
    params.insert("kind".to_string(), json!("terminal"));
    /*
    CDXC:GxserverSessionTitle 2026-06-23-08:40:
    Mobile and CLI create-session callers may provide first-message input, but
    server must remain the owner of first-prompt auto-name generation.
    Pass the prompt through as runtime metadata and startup text instead of
    generating or staging title commands in the CLI.
    */
    if !launch_settings.is_empty() {
        params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    }
    params.insert("projectId".to_string(), json!(project_id));
    if !input.is_empty() {
        params.insert(
            "runtimeSettings".to_string(),
            json!({ "firstUserMessage": input }),
        );
    }
    params.insert(
        "title".to_string(),
        if js_truthy(payload.get("title")) {
            payload.get("title").expect("truthy title").clone()
        } else {
            json!("Terminal")
        },
    );
    let created = rpc::call_gxserver_rpc("/api/createSession", &Value::Object(params), flags)?;
    if payload.get("start") != Some(&Value::Bool(true)) {
        return Ok(created);
    }
    /*
    CDXC:GxserverCliSessionStart 2026-07-04-17:05:
    `ghostex create-session --start` mirrors `create-agent`: gxserver rows are
    created lazily and the zmx provider only materializes once something starts
    it, so `send-text`/`send-message`/`read-text` fail with "session does not
    exist" until then. Orchestration callers pass --start so the terminal is
    live immediately without waking/focusing panes through the UI.
    */
    start_created_session_provider(created, flags)
}

fn create_gxserver_agent_session(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let project_id_value = payload
        .get("projectId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("projectId").map(FlagValue::as_json));
    let project_id = normalize_required_project_id(project_id_value, "create-agent")?;
    let agent_id_value = payload
        .get("agentId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("agentId").map(FlagValue::as_json));
    let agent_id = match agent_id_value {
        Some(value) => js_string(&value).trim().to_string(),
        None => String::new(),
    };
    if agent_id.is_empty() {
        return Err(CliError::Other(
            "create-agent requires an agent id.".to_string(),
        ));
    }
    /*
    CDXC:GxserverCliAgents 2026-06-19-15:55:
    `ghostex create-agent` is a spawn command for automation and agent
    orchestration, not just a row-creation helper. After creating the gxserver
    session, immediately ask gxserver to materialize the zmx provider so
    subsequent `send-message` targets a live agent process instead of a shell
    prompt.
    */
    let mut params = Map::new();
    params.insert("agentId".to_string(), json!(agent_id));
    params.insert("projectId".to_string(), json!(project_id));
    /*
    CDXC:GxserverFirstUserInputDraft 2026-08-20:
    `--first-input-draft` is the opposite of a first user message: gxserver
    types the text into the new agent's composer once the provider starts and
    never submits it, so SSH-only clients can hand the user a mention such as
    `@/path/export.md ` to write their own prompt around. The value is passed
    verbatim — a trailing space separates the mention from what the user types.
    */
    if let Some(draft) = payload
        .get("firstInputDraft")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        params.insert(
            "runtimeSettings".to_string(),
            json!({ "firstUserInputDraft": draft }),
        );
    }
    let title = flags
        .0
        .get("title")
        .map(FlagValue::as_json)
        .or_else(|| payload.get("title").cloned());
    set_or_remove(&mut params, "title", title);
    let created = rpc::call_gxserver_rpc("/api/createAgentSession", &Value::Object(params), flags)?;
    start_created_session_provider(created, flags)
}

/// Shared `startSessionProvider` follow-up used by create-session --start and
/// create-agent: returns `created` untouched when the session row is missing
/// projectId/sessionId, otherwise merges the provider result.
fn start_created_session_provider(created: Value, flags: &Flags) -> CliResult<Value> {
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    if !js_truthy(session.get("projectId")) || !js_truthy(session.get("sessionId")) {
        return Ok(created);
    }
    let provider = rpc::call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({
            "projectId": session.get("projectId").cloned().unwrap_or(Value::Null),
            "sessionId": session.get("sessionId").cloned().unwrap_or(Value::Null),
        }),
        flags,
    )?;
    let mut object = created.as_object().cloned().unwrap_or_default();
    object.insert(
        "session".to_string(),
        match provider.get("session") {
            Some(value) if !value.is_null() => value.clone(),
            _ => session,
        },
    );
    object.insert("provider".to_string(), provider);
    Ok(Value::Object(object))
}

/*
CDXC:AddProjectDialog 2026-07-30:
`ghostex clone-repository` is the blocking front end to gxserver's clone JOB
endpoints, because Ghostex mobile drives the Add Project flow over one SSH exec
and cannot hold a polling loop of its own. The daemon still owns the clone, the
project registration, and the presentation delta; the CLI only waits for the job
to leave `running` and reports the final job record.

The wait timeout never cancels the job. A clone that outlives the CLI's patience
is still a clone the user asked for, so the command returns the still-running
job with `waitTimedOut: true` and leaves it to finish server-side.
*/
fn clone_repository_and_wait(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let target = rpc::resolve_gxserver_server_target(flags, payload)?;
    let started = rpc::request_gxserver_rpc(&target, "/api/startRepositoryClone", payload, flags)?;
    let Some(job_id) = started
        .get("job")
        .and_then(|job| job.get("jobId"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(started);
    };
    let wait_timeout_ms = flags
        .number("waitTimeoutMs")
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(900_000.0) as u64;
    let poll_interval = std::time::Duration::from_millis(500);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_timeout_ms);
    let poll_params = json!({ "jobId": job_id });
    let mut latest = started;
    loop {
        if std::time::Instant::now() >= deadline {
            let mut object = latest.as_object().cloned().unwrap_or_default();
            object.insert("waitTimedOut".to_string(), Value::Bool(true));
            crate::ghostex_cli::set_exit_code(1);
            return Ok(Value::Object(object));
        }
        std::thread::sleep(poll_interval);
        latest =
            rpc::request_gxserver_rpc(&target, "/api/readRepositoryCloneJob", &poll_params, flags)?;
        let state = latest
            .get("job")
            .and_then(|job| job.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("running");
        match state {
            "running" => {}
            "completed" => return Ok(latest),
            _ => {
                crate::ghostex_cli::set_exit_code(1);
                return Ok(latest);
            }
        }
    }
}

fn ensure_gxserver_project_for_path(project_path: &str, flags: &Flags) -> CliResult<Value> {
    let result = rpc::call_gxserver_rpc(
        "/api/addProjectPath",
        &json!({ "path": project_path }),
        flags,
    )?;
    Ok(result.get("project").cloned().unwrap_or(Value::Null))
}

fn normalize_required_project_id(value: Option<Value>, command_name: &str) -> CliResult<String> {
    let project_id = match value {
        Some(value) if !value.is_null() => js_string(&value),
        _ => String::new(),
    }
    .trim()
    .to_string();
    if project_id.is_empty() {
        return Err(CliError::Other(format!(
            "{command_name} requires --project-id until gxserver active-project routing lands."
        )));
    }
    Ok(project_id)
}

fn with_resolved_session_params(payload: &Value, flags: &Flags) -> Value {
    let session_id_string = payload.get("sessionId").map(js_string);
    let global_parts: Option<Vec<String>> = match &session_id_string {
        Some(value) if rpc::is_gxserver_global_session_ref(value) => {
            Some(value.split(':').map(str::to_string).collect())
        }
        _ => None,
    };
    let mut object = payload.as_object().cloned().unwrap_or_default();
    let global_ref = payload
        .get("globalRef")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            if global_parts.is_some() {
                payload.get("sessionId").cloned()
            } else {
                None
            }
        });
    set_or_remove(&mut object, "globalRef", global_ref);
    let project_id = payload
        .get("projectId")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| flags.0.get("projectId").map(FlagValue::as_json))
        .or_else(|| {
            global_parts
                .as_ref()
                .map(|parts| Value::String(parts[1].clone()))
        });
    set_or_remove(&mut object, "projectId", project_id);
    let session_id = global_parts
        .as_ref()
        .map(|parts| Value::String(parts[2].clone()))
        .or_else(|| payload.get("sessionId").cloned());
    set_or_remove(&mut object, "sessionId", session_id);
    Value::Object(object)
}

fn with_resolved_gxserver_session_params(payload: &Value, flags: &Flags) -> CliResult<Value> {
    let params = with_resolved_session_params(payload, flags);
    if js_truthy(params.get("projectId")) || !js_truthy(params.get("sessionId")) {
        return Ok(params);
    }
    /*
    CDXC:GxserverSessionLifecycle 2026-05-31-08:45:
    React Native Android, the gx TUI, and plain `gx` lifecycle commands send stable
    `--session-id G...` selectors from `ghostex sessions --json`. gxserver
    lifecycle RPCs require projectId too, so resolve bare session ids through
    the daemon inventory instead of falling back to the retired macOS bridge or
    making every client learn project-scoped RPC payloads.
    */
    let session_id = js_string(params.get("sessionId").expect("truthy sessionId"));
    let session =
        sessions::resolve_gxserver_inventory_session(&session_id, flags)?.ok_or_else(|| {
            CliError::Other(format!(
                "No gxserver session matched \"{}\".",
                session_id.trim()
            ))
        })?;
    let mut object = params.as_object().cloned().unwrap_or_default();
    set_or_remove(&mut object, "projectId", session.get("projectId").cloned());
    set_or_remove(&mut object, "sessionId", session.get("sessionId").cloned());
    Ok(Value::Object(object))
}

// ---------------------------------------------------------------------------
// Payload parsers (parse* functions from the Node CLI)
// ---------------------------------------------------------------------------

fn evaluate_parser(parser: Parser, rest: &[String], flags: &Flags) -> CliResult<Value> {
    Ok(match parser {
        Parser::None => json!({}),
        Parser::OpenPaths => parse_open_paths(rest, flags),
        Parser::EditPaths => parse_edit_paths(rest, flags),
        Parser::QuickTerminal => parse_quick_terminal(rest, flags),
        Parser::CreateSession => parse_create_session(rest, flags),
        Parser::Agent => parse_agent(rest, flags),
        Parser::CommandButton => parse_command_button(rest, flags),
        Parser::ClickButton => parse_click_button(rest, flags),
        Parser::SaveCommand => parse_save_command(rest, flags),
        Parser::SaveAgent => parse_save_agent(rest, flags),
        Parser::SessionSelector => Value::Object(parse_session_selector(rest, flags)),
        Parser::Group => parse_group(rest, flags),
        Parser::Project => parse_project(rest, flags),
        Parser::ProjectMove => parse_project_move(rest, flags),
        Parser::ProjectPath => parse_project_path(rest, flags),
        Parser::ProjectCollection => parse_project_collection(rest, flags),
        Parser::BrowseDirectories => parse_browse_directories(rest, flags),
        Parser::LookupRepository => parse_lookup_repository(rest, flags),
        Parser::CloneRepository => parse_clone_repository(rest, flags),
        Parser::Rename => parse_rename(rest, flags),
        Parser::RenameRequest => parse_rename_request(rest, flags),
        Parser::SessionBoolean(name) => parse_session_boolean(name, rest, flags),
        Parser::SessionTag => parse_session_tag(rest, flags)?,
        Parser::SessionNote => parse_session_note(rest, flags),
        Parser::DelayedSend => parse_delayed_send(rest, flags)?,
        Parser::SendText => parse_send_text(rest, flags),
        Parser::SendKey => parse_send_key(rest, flags),
        Parser::VisibleCount => parse_visible_count(rest, flags),
        Parser::ViewMode => parse_view_mode(rest, flags),
        Parser::Url => parse_url(rest, flags),
        Parser::BrowserOpen => parse_browser_open(rest, flags),
        Parser::AssertCard => Value::Object(parse_assert_card(rest, flags)),
        Parser::WaitFor => parse_wait_for(rest, flags),
        Parser::SidebarProjectCollectionsState => {
            parse_sidebar_project_collections_state(rest, flags)?
        }
        Parser::SidebarSpacesState => parse_sidebar_spaces_state(rest, flags)?,
        Parser::SessionChatRead => parse_session_chat_read(rest, flags),
        Parser::SessionChatDraftAgent => parse_session_chat_draft_agent(rest, flags)?,
        Parser::SessionChatKey => parse_session_chat_key(rest, flags)?,
        Parser::AgentPromptSearch => parse_agent_prompt_search(flags)?,
        Parser::AgentPromptRef => parse_agent_prompt_ref(flags)?,
        Parser::AgentPromptLaunch => parse_agent_prompt_launch(flags)?,
        Parser::SessionChatAnswer => parse_session_chat_answer(rest, flags)?,
        Parser::SessionChatRewind => parse_session_chat_rewind(rest, flags)?,
        Parser::SessionChatQueuedPrompt => parse_session_chat_queued_prompt(rest, flags)?,
        Parser::SessionChatQueueOrder => parse_session_chat_queue_order(rest, flags)?,
        Parser::SessionChatDraft => parse_session_chat_draft(rest, flags)?,
        Parser::KeepSessionsAwake => parse_keep_sessions_awake(flags)?,
    })
}

/*
CDXC:SessionChatMobileCli 2026-07-31:
Ghostex mobile has no HTTP path to gxserver, so the Session Chat endpoints are
exposed as CLI verbs the phone SSH-execs, exactly like the Add Project flow.
`read-session-chat` carries the long-poll pair (--wait-ms + --fingerprint): the
daemon holds the request until the chat fingerprint changes, which is how the
phone tails a conversation without an /api/events socket.
*/
/*
CDXC:AgentHistorySearch 2026-08-20:
Prompt history lives on the machine that ran the agent, so Ghostex mobile
reaches Find the same way it reaches chat: these verbs SSH-exec on that machine
and forward to the daemon's own endpoints. Every follow-up verb addresses a
result by its stable `--key`, never by a list position, so a phone acting on a
result minutes later still lands on the prompt it displayed.
*/
fn parse_agent_prompt_search(flags: &Flags) -> CliResult<Value> {
    let mut map = Map::new();
    if let Some(query) = flags.text("query") {
        map.insert("query".to_string(), Value::String(query));
    }
    if let Some(project) = flags.text("project") {
        map.insert("project".to_string(), Value::String(project));
    }
    if let Some(agents) = flags.text("agents") {
        let list = agents
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect::<Vec<_>>();
        if !list.is_empty() {
            map.insert("agents".to_string(), Value::Array(list));
        }
    }
    if flags.contains("groupByDay") {
        map.insert(
            "groupByDay".to_string(),
            Value::Bool(flags.truthy("groupByDay")),
        );
    }
    if flags.contains("includeFacets") {
        map.insert(
            "includeFacets".to_string(),
            Value::Bool(flags.truthy("includeFacets")),
        );
    }
    if flags.contains("refresh") {
        map.insert("refresh".to_string(), Value::Bool(flags.truthy("refresh")));
    }
    for key in ["limit", "offset", "textLimit"] {
        if flags.contains(key) {
            map.insert(key.to_string(), flag_number_value(flags, key));
        }
    }
    Ok(Value::Object(map))
}

fn agent_prompt_key(flags: &Flags) -> CliResult<String> {
    flags
        .text("key")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::Other(
                "--key is required; it is the `key` field of a searchAgentPrompts row.".to_string(),
            )
        })
}

fn parse_agent_prompt_ref(flags: &Flags) -> CliResult<Value> {
    let mut map = Map::new();
    map.insert("key".to_string(), Value::String(agent_prompt_key(flags)?));
    if flags.contains("favorite") {
        map.insert(
            "favorite".to_string(),
            Value::Bool(flags.truthy("favorite")),
        );
    }
    Ok(Value::Object(map))
}

fn parse_agent_prompt_launch(flags: &Flags) -> CliResult<Value> {
    let mut map = Map::new();
    map.insert("key".to_string(), Value::String(agent_prompt_key(flags)?));
    let action = flags.text("action").unwrap_or_else(|| "resume".to_string());
    if action != "resume" && action != "fork" {
        return Err(CliError::Other(
            "--action must be \"resume\" or \"fork\".".to_string(),
        ));
    }
    map.insert("action".to_string(), Value::String(action));
    if let Some(agent) = flags.text("forkAgent") {
        map.insert("forkAgent".to_string(), Value::String(agent));
    }
    // Omitted means "use the daemon's Accept All setting", the same policy
    // `gx f` reads; passing it is an explicit override.
    if flags.contains("acceptAll") {
        map.insert(
            "acceptAll".to_string(),
            Value::Bool(flags.truthy("acceptAll")),
        );
    }
    Ok(Value::Object(map))
}

fn parse_session_chat_read(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_session_selector(rest, flags);
    if flags.contains("limit") {
        map.insert("limit".to_string(), flag_number_value(flags, "limit"));
    }
    if flags.contains("beforeOffset") {
        map.insert(
            "beforeOffset".to_string(),
            flag_number_value(flags, "beforeOffset"),
        );
    }
    if flags.contains("waitMs") {
        map.insert("waitMs".to_string(), flag_number_value(flags, "waitMs"));
    }
    set_or_remove(&mut map, "fingerprint", flag_json(flags, "fingerprint"));
    Value::Object(map)
}

fn parse_session_chat_draft_agent(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let Some(agent_id) = flags
        .text("agentId")
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(CliError::Other(
            "switch-draft-agent requires --agent-id <id> from read-session-chat.".to_string(),
        ));
    };
    map.insert("agentId".to_string(), Value::String(agent_id));
    Ok(Value::Object(map))
}

fn parse_session_chat_key(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let Some(key) = flags.text("key").filter(|value| !value.trim().is_empty()) else {
        return Err(CliError::Other(
            "send-session-chat-key requires --key <enter|shift-tab|shift-up|shift-down>."
                .to_string(),
        ));
    };
    map.insert("key".to_string(), Value::String(key));
    Ok(Value::Object(map))
}

fn parse_session_chat_rewind(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let Some(message_id) = flags
        .text("messageId")
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(CliError::Other(
            "rewind-session-chat requires --message-id <uuid> naming a user prompt of the session's active conversation."
                .to_string(),
        ));
    };
    map.insert(
        "messageId".to_string(),
        Value::String(message_id.trim().to_string()),
    );
    Ok(Value::Object(map))
}

fn parse_session_chat_answer(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let answer_text = flags
        .text("answerJson")
        .or_else(|| flags.text("answer"))
        .unwrap_or_default();
    if answer_text.trim().is_empty() {
        return Err(CliError::Other(
            "answer-session-chat-prompt requires --answer-json '<json>' with kind plus selections, approvalSend or choiceIndex.".to_string(),
        ));
    }
    let answer: Value = serde_json::from_str(&answer_text)
        .map_err(|error| CliError::Other(format!("Invalid --answer-json: {error}")))?;
    let Some(answer) = answer.as_object() else {
        return Err(CliError::Other(
            "Invalid --answer-json: expected a JSON object.".to_string(),
        ));
    };
    for (key, value) in answer {
        map.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(map))
}

fn parse_session_chat_queued_prompt(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let Some(prompt_id) = flags
        .text("promptId")
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(CliError::Other(
            "This verb requires --prompt-id <id> from read-session-chat-queue.".to_string(),
        ));
    };
    map.insert("promptId".to_string(), Value::String(prompt_id));
    if let Some(text) = flags.text("text") {
        map.insert("text".to_string(), Value::String(text));
    }
    if flags.truthy("retry") {
        map.insert("retry".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(map))
}

fn parse_session_chat_queue_order(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let ids = flags
        .text("promptIds")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Value::String(value.to_string()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err(CliError::Other(
            "reorder-session-chat-queue requires --prompt-ids <id,id,…> head first.".to_string(),
        ));
    }
    map.insert("promptIds".to_string(), Value::Array(ids));
    Ok(Value::Object(map))
}

/*
An EMPTY --content is how a draft is cleared, so it is valid input: the flag
must be present, but its value may be the empty string.
*/
fn parse_session_chat_draft(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let Some(content) = flags.text("content") else {
        return Err(CliError::Other(
            "set-session-chat-draft requires --content '<text>' (empty clears the draft)."
                .to_string(),
        ));
    };
    let Some(client_id) = flags
        .text("clientId")
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(CliError::Other(
            "set-session-chat-draft requires --client-id <id> so this device ignores its own echo."
                .to_string(),
        ));
    };
    map.insert("content".to_string(), Value::String(content));
    map.insert("clientId".to_string(), Value::String(client_id));
    Ok(Value::Object(map))
}

/*
CDXC:MobileKeepAwake 2026-08-19:
The payload is deliberately a list of `{projectId, sessionId}` pairs the caller
already knows from `ghostex sessions --json --mobile-summary`, so no lease
renewal has to re-resolve a bare session id through the daemon inventory.
*/
fn parse_keep_sessions_awake(flags: &Flags) -> CliResult<Value> {
    let sessions_text = flags.text("sessionsJson").unwrap_or_default();
    if sessions_text.trim().is_empty() {
        return Err(CliError::Other(
            "hold-sessions-awake requires --sessions-json '[{\"projectId\":\"...\",\"sessionId\":\"...\"}]'."
                .to_string(),
        ));
    }
    let sessions: Value = serde_json::from_str(&sessions_text)
        .map_err(|error| CliError::Other(format!("Invalid --sessions-json: {error}")))?;
    if !sessions.is_array() {
        return Err(CliError::Other(
            "Invalid --sessions-json: expected a JSON array.".to_string(),
        ));
    }
    let mut map = Map::new();
    map.insert("sessions".to_string(), sessions);
    if flags.contains("ttlMs") {
        map.insert("ttlMs".to_string(), flag_number_value(flags, "ttlMs"));
    }
    set_or_remove(&mut map, "holderId", flag_json(flags, "holderId"));
    if flags.truthy("release") {
        map.insert("release".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(map))
}

fn parse_create_session(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    map.insert(
        "input".to_string(),
        flag_json(flags, "input").unwrap_or_else(|| Value::String(join_rest(rest, 1))),
    );
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    if let Some(value) = flags.0.get("start") {
        map.insert("start".to_string(), Value::Bool(parse_boolean(value)));
    }
    set_or_remove(
        &mut map,
        "title",
        flag_json(flags, "title").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_agent(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "agentId",
        flag_json(flags, "agentId").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(
        &mut map,
        "firstInputDraft",
        flag_json(flags, "firstInputDraft"),
    );
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    Value::Object(map)
}

fn parse_command_button(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "commandId",
        flag_json(flags, "commandId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_click_button(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "id",
        flag_json(flags, "id").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "kind",
        flag_json(flags, "kind").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_save_agent(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "acceptAllMode", flag_json(flags, "acceptAllMode"));
    set_or_remove(
        &mut map,
        "agentId",
        flag_json(flags, "agentId").or_else(|| rest_string(rest, 0)),
    );
    map.insert(
        "command".to_string(),
        flag_json(flags, "command").unwrap_or_else(|| Value::String(join_rest(rest, 2))),
    );
    set_or_remove(&mut map, "icon", flag_json(flags, "icon"));
    set_or_remove(
        &mut map,
        "name",
        flag_json(flags, "name").or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

fn parse_save_command(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    map.insert(
        "actionType".to_string(),
        flag_json(flags, "actionType")
            .or_else(|| flag_json(flags, "type"))
            .unwrap_or_else(|| json!("terminal")),
    );
    map.insert(
        "closeTerminalOnExit".to_string(),
        Value::Bool(
            flags
                .0
                .get("closeTerminalOnExit")
                .map(parse_boolean)
                .unwrap_or(false),
        ),
    );
    map.insert(
        "command".to_string(),
        flag_json(flags, "command").unwrap_or_else(|| Value::String(join_rest(rest, 2))),
    );
    set_or_remove(
        &mut map,
        "commandId",
        flag_json(flags, "commandId").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(&mut map, "icon", flag_json(flags, "icon"));
    set_or_remove(
        &mut map,
        "name",
        flag_json(flags, "name").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(&mut map, "path", flag_json(flags, "path"));
    map.insert(
        "playCompletionSound".to_string(),
        Value::Bool(
            flags
                .0
                .get("playCompletionSound")
                .map(parse_boolean)
                .unwrap_or(true),
        ),
    );
    map.insert(
        "showOnProjectRow".to_string(),
        Value::Bool(
            flags
                .0
                .get("showOnProjectRow")
                .map(parse_boolean)
                .unwrap_or(false),
        ),
    );
    set_or_remove(&mut map, "url", flag_json(flags, "url"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    set_or_remove(&mut map, "projectName", flag_json(flags, "projectName"));
    Value::Object(map)
}

fn parse_group(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "groupId",
        flag_json(flags, "groupId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_project(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(
        &mut map,
        "path",
        flag_json(flags, "path").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    Value::Object(map)
}

fn parse_project_move(rest: &[String], flags: &Flags) -> Value {
    /*
    CDXC:AndroidSidebar 2026-05-18-16:13:
    Ghostex Android reorders project groups through the Mac CLI, not local
    phone state. The desktop sidebar remains the source of truth and later
    inventory calls return the persisted order to mobile.
    */
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "direction",
        flag_json(flags, "direction")
            .or_else(|| flag_json(flags, "dir"))
            .or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "projectId",
        flag_json(flags, "projectId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_project_path(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    /*
    CDXC:AddProjectDialog 2026-07-30:
    Ghostex mobile speaks the CLI, not the wire protocol, so the Add Project
    flow's "create this folder and add it" affordance reaches gxserver as
    `add-project --create-if-missing`. The flag is only sent when the caller
    passed it, so every existing `add-project` invocation keeps the old
    missing-path rejection.
    */
    if flags.contains("createIfMissing") {
        map.insert(
            "createIfMissing".to_string(),
            Value::Bool(parse_boolean(
                flags.0.get("createIfMissing").expect("flag present"),
            )),
        );
    }
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(
        &mut map,
        "path",
        flag_json(flags, "path").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_project_collection(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(&mut map, "path", flag_json(flags, "path"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    // A bare first positional is a project id. Paths and names stay explicit
    // so automation never guesses which registered project the user meant.
    if !map.contains_key("projectId") && !map.contains_key("path") && !map.contains_key("name") {
        set_or_remove(&mut map, "projectId", rest_string(rest, 0));
    }
    set_or_remove(
        &mut map,
        "collectionTitle",
        flag_json(flags, "group")
            .or_else(|| flag_json(flags, "collection"))
            .or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

fn parse_browse_directories(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "cwd", flag_json(flags, "cwd"));
    if flags.contains("limit") {
        map.insert("limit".to_string(), flag_number_value(flags, "limit"));
    }
    set_or_remove(
        &mut map,
        "partialPath",
        flag_json(flags, "partialPath").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_lookup_repository(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "cwd", flag_json(flags, "cwd"));
    set_or_remove(
        &mut map,
        "provider",
        flag_json(flags, "provider").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(
        &mut map,
        "repository",
        flag_json(flags, "repository").or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

fn parse_clone_repository(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "branchName", flag_json(flags, "branchName"));
    if let Some(value) = flags.0.get("cloneMainOnly") {
        map.insert(
            "cloneMainOnly".to_string(),
            Value::Bool(parse_boolean(value)),
        );
    }
    set_or_remove(
        &mut map,
        "destinationPath",
        flag_json(flags, "destinationPath").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "remoteUrl",
        flag_json(flags, "remoteUrl").or_else(|| rest_string(rest, 0)),
    );
    if let Some(value) = flags.0.get("shallowClone") {
        map.insert(
            "shallowClone".to_string(),
            Value::Bool(parse_boolean(value)),
        );
    }
    Value::Object(map)
}

fn parse_session_selector(rest: &[String], flags: &Flags) -> Map<String, Value> {
    let mut map = Map::new();
    if flags.contains("index") {
        map.insert("index".to_string(), flag_number_value(flags, "index"));
    }
    set_or_remove(
        &mut map,
        "sessionId",
        flag_json(flags, "sessionId").or_else(|| rest_string(rest, 0)),
    );
    if flags.contains("sessionNumber") {
        map.insert(
            "sessionNumber".to_string(),
            flag_number_value(flags, "sessionNumber"),
        );
    }
    map
}

fn parse_rename(rest: &[String], flags: &Flags) -> Value {
    /*
    CDXC:AndroidRemoteSessions 2026-05-17-13:23:
    Ghostex Android invokes remote rename through `ghostex rename-session
    --session-id <id> --title <title> --json` so SSH quoting can keep the
    stable session id and user-entered title as separate CLI arguments.
    */
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "title".to_string(),
        flag_json(flags, "title").unwrap_or_else(|| Value::String(join_rest(rest, 1))),
    );
    Value::Object(map)
}

/*
CDXC:MobileAgentActions 2026-08-01:
`/api/requestSessionRename` takes the rename-session payload plus the agent
identity hints and a title source. `titleSource` defaults to "user" here so
mobile does not have to send it on every rename; gxserver applies the same
default, but sending it keeps the CLI payload self-describing.
*/
fn parse_rename_request(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_rename(rest, flags)
        .as_object()
        .cloned()
        .unwrap_or_default();
    set_or_remove(&mut map, "agentName", flag_json(flags, "agentName"));
    set_or_remove(
        &mut map,
        "agentSessionId",
        flag_json(flags, "agentSessionId"),
    );
    set_or_remove(
        &mut map,
        "agentSessionPath",
        flag_json(flags, "agentSessionPath"),
    );
    map.insert(
        "titleSource".to_string(),
        flag_json(flags, "titleSource").unwrap_or_else(|| Value::String("user".to_string())),
    );
    Value::Object(map)
}

fn parse_session_boolean(name: &str, rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let positional_index = if has_flag_selector { 0 } else { 1 };
    let raw = flags
        .0
        .get(name)
        .cloned()
        .or_else(|| flags.0.get("value").cloned())
        .or_else(|| {
            rest.get(positional_index)
                .map(|text| FlagValue::Text(text.clone()))
        })
        .unwrap_or_else(|| FlagValue::Text("true".to_string()));
    let mut map = parse_session_selector(rest, flags);
    map.insert(name.to_string(), Value::Bool(parse_boolean(&raw)));
    Value::Object(map)
}

fn parse_delayed_send(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let send_when_agent_stops = flags.truthy("whenAgentFinishes");
    let send_when_all_project_sessions_stop = flags.truthy("whenAllAgentsFinish");
    let delay_ms = flags.number("delayMs").filter(|value| value.is_finite());

    let trigger_count = usize::from(delay_ms.is_some())
        + usize::from(send_when_agent_stops)
        + usize::from(send_when_all_project_sessions_stop);
    if trigger_count == 0 {
        return Err(CliError::Other(
            "Missing Delayed Send trigger. Use --delay-ms, --when-agent-finishes, or --when-all-agents-finish."
                .to_string(),
        ));
    }
    if trigger_count > 1 {
        return Err(CliError::Other(
            "Choose exactly one Delayed Send trigger: --delay-ms, --when-agent-finishes, or --when-all-agents-finish."
                .to_string(),
        ));
    }

    if let Some(delay_ms) = delay_ms {
        map.insert("delayMs".to_string(), js_number_to_value(delay_ms));
    } else if send_when_agent_stops {
        map.insert("sendWhenAgentStops".to_string(), Value::Bool(true));
    } else {
        map.insert(
            "sendWhenAllProjectSessionsStop".to_string(),
            Value::Bool(true),
        );
    }
    Ok(Value::Object(map))
}

fn parse_session_tag(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let positional_index = if has_flag_selector { 0 } else { 1 };
    let raw_tag = flag_json(flags, "tag")
        .or_else(|| flag_json(flags, "sessionTag"))
        .or_else(|| flag_json(flags, "value"))
        .or_else(|| rest_string(rest, positional_index));
    let tag_list = SIDEBAR_SESSION_TAGS.join(", ");
    let Some(raw_tag) = raw_tag else {
        return Err(CliError::Other(format!(
            "Missing session tag. Use one of: {tag_list}, or none."
        )));
    };
    let raw_tag_string = js_string(&raw_tag);
    let normalized_tag = raw_tag_string.trim().to_lowercase();
    let session_tag: Option<String> = if CLEAR_SESSION_TAG_VALUES.contains(&normalized_tag.as_str())
    {
        None
    } else {
        Some(normalized_tag)
    };
    if let Some(tag) = &session_tag {
        if !SIDEBAR_SESSION_TAGS.contains(&tag.as_str()) {
            return Err(CliError::Other(format!(
                "Unknown session tag \"{raw_tag_string}\". Use one of: {tag_list}, or none."
            )));
        }
    }
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "isFavorite".to_string(),
        Value::Bool(session_tag.as_deref() == Some("favorite")),
    );
    map.insert(
        "sessionTag".to_string(),
        session_tag.map(Value::String).unwrap_or(Value::Null),
    );
    Ok(Value::Object(map))
}

/*
CDXC:SessionAgentNotes 2026-08-24:
`ghostex session-note save --session-id <id> --note <text> --json` keeps the
stable session id and the user-entered note as separate CLI arguments, exactly
like `rename-session` does with `--title`, so SSH quoting never has to reunite
them. An omitted or empty note clears the note.
*/
fn parse_session_note(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "note".to_string(),
        flag_json(flags, "note").unwrap_or_else(|| {
            Value::String(join_rest(rest, if has_flag_selector { 0 } else { 1 }))
        }),
    );
    Value::Object(map)
}

fn parse_send_text(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector = ["sessionId", "selector", "session", "sessionTitle", "target"]
        .iter()
        .any(|key| flags.truthy(key));
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "text".to_string(),
        flag_json(flags, "text").unwrap_or_else(|| {
            Value::String(join_rest(rest, if has_flag_selector { 0 } else { 1 }))
        }),
    );
    Value::Object(map)
}

fn parse_send_key(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector = ["sessionId", "selector", "session", "sessionTitle", "target"]
        .iter()
        .any(|key| flags.truthy(key));
    let mut map = parse_session_selector(rest, flags);
    set_or_remove(
        &mut map,
        "key",
        flag_json(flags, "key")
            .or_else(|| rest_string(rest, if has_flag_selector { 0 } else { 1 })),
    );
    Value::Object(map)
}

fn parse_visible_count(rest: &[String], flags: &Flags) -> Value {
    let count = if flags.contains("count") {
        flag_number_value(flags, "count")
    } else if let Some(first) = rest.first() {
        crate::ghostex_cli::args::js_number(first)
            .map(js_number_to_value)
            .unwrap_or(Value::Null)
    } else {
        // Number(undefined) is NaN, which JSON-serializes to null.
        Value::Null
    };
    json!({ "count": count })
}

fn parse_view_mode(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "mode",
        flag_json(flags, "mode").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_url(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "url",
        flag_json(flags, "url").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_browser_open(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    set_or_remove(
        &mut map,
        "projectName",
        flag_json(flags, "projectName").or_else(|| flag_json(flags, "name")),
    );
    let project_path = flag_json(flags, "projectPath")
        .or_else(|| flag_json(flags, "path"))
        .or_else(|| {
            let active_project = flags
                .0
                .get("activeProject")
                .map(parse_boolean)
                .unwrap_or(false);
            if active_project {
                None
            } else {
                Some(Value::String(cwd_string()))
            }
        });
    set_or_remove(&mut map, "projectPath", project_path);
    map.insert(
        "reuse".to_string(),
        if flags.truthy("new") {
            json!("none")
        } else {
            flag_json(flags, "reuse").unwrap_or_else(|| json!("similar"))
        },
    );
    set_or_remove(
        &mut map,
        "url",
        flag_json(flags, "url").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

fn parse_open_paths(rest: &[String], flags: &Flags) -> Value {
    let targets: Vec<Value> = if !rest.is_empty() {
        rest.iter()
            .map(|value| Value::String(value.clone()))
            .collect()
    } else if flags.truthy("path") {
        vec![flags.0.get("path").expect("truthy path").as_json()]
    } else {
        Vec::new()
    };
    json!({
        "mode": "open",
        "targets": targets
            .iter()
            .map(|target| parse_open_path_target(target, false))
            .collect::<Vec<Value>>(),
    })
}

fn parse_edit_paths(rest: &[String], flags: &Flags) -> Value {
    let wait_consumed_target = flags.string_value("wait").map(str::to_string);
    let targets: Vec<Value> = if !rest.is_empty() {
        rest.iter()
            .map(|value| Value::String(value.clone()))
            .collect()
    } else if flags.truthy("goto") {
        vec![flags.0.get("goto").expect("truthy goto").as_json()]
    } else if flags.truthy("path") {
        vec![flags.0.get("path").expect("truthy path").as_json()]
    } else if matches!(&wait_consumed_target, Some(target) if !target.is_empty()) {
        vec![Value::String(
            wait_consumed_target.clone().expect("checked wait target"),
        )]
    } else {
        Vec::new()
    };
    let wait = flags.0.get("wait") == Some(&FlagValue::Bool(true))
        || wait_consumed_target.is_some()
        || flags.0.get("wait").map(parse_boolean).unwrap_or(false);
    json!({
        "mode": "edit",
        "targets": targets
            .iter()
            .map(|target| parse_open_path_target(target, wait))
            .collect::<Vec<Value>>(),
        "wait": wait,
    })
}

fn parse_quick_terminal(rest: &[String], flags: &Flags) -> Value {
    let command_separator_index = rest.iter().position(|arg| arg == "--");
    let command_rest: &[String] = match command_separator_index {
        Some(index) => &rest[index + 1..],
        None => rest,
    };
    let mut map = Map::new();
    if !command_rest.is_empty() {
        map.insert("command".to_string(), Value::String(command_rest.join(" ")));
    }
    set_or_remove(
        &mut map,
        "cwd",
        flag_json(flags, "cwd").or_else(|| flag_json(flags, "path")),
    );
    set_or_remove(
        &mut map,
        "title",
        flag_json(flags, "title").or_else(|| flag_json(flags, "name")),
    );
    Value::Object(map)
}

fn parse_open_path_target(value: &Value, wait: bool) -> Value {
    let raw = string_or_empty(Some(value)).trim().to_string();
    let (parsed_path, line, column) = parse_vs_code_path_position(&raw);
    let mut target = Map::new();
    if let Some(column) = column {
        target.insert("column".to_string(), json!(column));
    }
    if let Some(line) = line {
        target.insert("line".to_string(), json!(line));
    }
    target.insert("path".to_string(), json!(node_path_resolve(&parsed_path)));
    target.insert("raw".to_string(), json!(raw));
    if wait {
        /*
        CDXC:OSIntegration 2026-05-27-18:06:
        `ghostex edit --wait` waits for a concrete opened editor item, so each
        target carries a stable per-command wait token across the native
        bridge.
        */
        target.insert(
            "waitToken".to_string(),
            json!(format!(
                "wait-{}-{}",
                to_base36(chrono::Utc::now().timestamp_millis().max(0) as u128),
                random_base36(8)
            )),
        );
    }
    Value::Object(target)
}

/// parseVsCodePathPosition: VS Code-style `file:line:column` targets. Mirrors
/// `/^(?<path>.+?)(?::(?<line>[1-9]\d*))?(?::(?<column>[1-9]\d*))?$/u`.
fn parse_vs_code_path_position(value: &str) -> (String, Option<i64>, Option<i64>) {
    if value.is_empty() {
        // The regex requires a non-empty path; JS falls back to { path: value }.
        return (value.to_string(), None, None);
    }
    fn is_positive_int(candidate: &str) -> bool {
        let mut chars = candidate.chars();
        matches!(chars.next(), Some(first) if ('1'..='9').contains(&first))
            && chars.all(|c| c.is_ascii_digit())
    }
    let mut numbers: Vec<i64> = Vec::new();
    let mut current = value;
    for _ in 0..2 {
        let Some(index) = current.rfind(':') else {
            break;
        };
        let (head, tail) = current.split_at(index);
        let digits = &tail[1..];
        if head.is_empty() || !is_positive_int(digits) {
            break;
        }
        numbers.push(digits.parse().expect("validated digits"));
        current = head;
    }
    match numbers.len() {
        1 => (current.to_string(), Some(numbers[0]), None),
        2 => (current.to_string(), Some(numbers[1]), Some(numbers[0])),
        _ => (current.to_string(), None, None),
    }
}

fn parse_assert_card(rest: &[String], flags: &Flags) -> Map<String, Value> {
    let mut map = parse_session_selector(rest, flags);
    set_or_remove(&mut map, "agentIcon", flag_json(flags, "agentIcon"));
    set_or_remove(&mut map, "agentName", flag_json(flags, "agentName"));
    if let Some(value) = flags.0.get("visible") {
        map.insert("visible".to_string(), Value::Bool(parse_boolean(value)));
    }
    map
}

fn parse_wait_for(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_assert_card(rest, flags);
    if flags.contains("intervalMs") {
        map.insert(
            "intervalMs".to_string(),
            flag_number_value(flags, "intervalMs"),
        );
    }
    if flags.contains("timeoutMs") {
        map.insert(
            "timeoutMs".to_string(),
            flag_number_value(flags, "timeoutMs"),
        );
    }
    Value::Object(map)
}

fn parse_sidebar_project_collections_state(rest: &[String], flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:SidebarProjectCollections 2026-07-18-00:00:
    Mobile edits durable sidebar project collections by SSH-exec'ing `ghostex
    update-sidebar-project-collections --state-json '<json>'` for a full
    read-modify-write of the collections state. The CLI passes the whole state
    through untouched; gxserver owns normalization (order authority, one
    project per collection, limits) and the normalized result is printed back
    for the client to adopt. `--state-json` mirrors automation-save's
    `--definition-json` ergonomics; `--json` stays the CLI output flag.
    */
    let state_json = flag_json(flags, "stateJson")
        .or_else(|| flag_json(flags, "state"))
        .unwrap_or_else(|| Value::String(join_rest(rest, 0)));
    let state_text = match state_json {
        Value::String(text) => text,
        _ => String::new(),
    };
    if state_text.trim().is_empty() {
        return Err(CliError::Other(
            "update-sidebar-project-collections requires --state-json '<json>' with the full collections state.".to_string(),
        ));
    }
    let state: Value = serde_json::from_str(&state_text)
        .map_err(|error| CliError::Other(format!("Invalid --state-json: {error}")))?;
    if !state.is_object() {
        return Err(CliError::Other(
            "update-sidebar-project-collections --state-json must be a JSON object with collections, order, and nextCollectionNumber.".to_string(),
        ));
    }
    Ok(json!({ "state": state }))
}

fn parse_sidebar_spaces_state(rest: &[String], flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:SidebarSpaces 2026-08-27:
    Mobile edits durable sidebar Spaces by SSH-exec'ing `ghostex
    update-sidebar-spaces --state-json '<json>'` for a full read-modify-write of
    the Space document, exactly like the project-collections command beside it.
    The CLI passes the whole state through untouched; gxserver owns
    normalization (order authority, member dedupe, grouped-project exclusion,
    limits) and the normalized result is printed back for the client to adopt.
    */
    let state_json = flag_json(flags, "stateJson")
        .or_else(|| flag_json(flags, "state"))
        .unwrap_or_else(|| Value::String(join_rest(rest, 0)));
    let state_text = match state_json {
        Value::String(text) => text,
        _ => String::new(),
    };
    if state_text.trim().is_empty() {
        return Err(CliError::Other(
            "update-sidebar-spaces requires --state-json '<json>' with the full spaces state."
                .to_string(),
        ));
    }
    let state: Value = serde_json::from_str(&state_text)
        .map_err(|error| CliError::Other(format!("Invalid --state-json: {error}")))?;
    if !state.is_object() {
        return Err(CliError::Other(
            "update-sidebar-spaces --state-json must be a JSON object with spaces and order."
                .to_string(),
        ));
    }
    Ok(json!({ "state": state }))
}

// ---------------------------------------------------------------------------
// JS-coercion helpers
// ---------------------------------------------------------------------------

/// Insert the value when it is defined (JS non-undefined), otherwise remove
/// the key so JSON serialization matches JS `undefined` handling.
fn set_or_remove(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            map.insert(key.to_string(), value);
        }
        None => {
            map.remove(key);
        }
    }
}

fn flag_json(flags: &Flags, key: &str) -> Option<Value> {
    flags.0.get(key).map(FlagValue::as_json)
}

fn rest_string(rest: &[String], index: usize) -> Option<Value> {
    rest.get(index).map(|value| Value::String(value.clone()))
}

fn join_rest(rest: &[String], skip: usize) -> String {
    if rest.len() <= skip {
        return String::new();
    }
    rest[skip..].join(" ")
}

/// Number(flags.key) rendered like JSON.stringify: NaN/Infinity → null,
/// integral values without a decimal point.
fn flag_number_value(flags: &Flags, key: &str) -> Value {
    flags
        .number(key)
        .map(js_number_to_value)
        .unwrap_or(Value::Null)
}

fn js_number_to_value(number: f64) -> Value {
    if !number.is_finite() {
        return Value::Null;
    }
    if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 {
        return json!(number as i64);
    }
    json!(number)
}

/// JS truthiness of a possibly-absent JSON value.
fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number
            .as_f64()
            .map(|value| value != 0.0 && !value.is_nan())
            .unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// String(value) coercion for JSON values.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number
            .as_f64()
            .map(|value| {
                if value.is_finite()
                    && value.fract() == 0.0
                    && value.abs() < 9.007_199_254_740_992e15
                {
                    (value as i64).to_string()
                } else {
                    value.to_string()
                }
            })
            .unwrap_or_else(|| number.to_string()),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| {
                if item.is_null() {
                    String::new()
                } else {
                    js_string(item)
                }
            })
            .collect::<Vec<String>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — empty string for absent/null values.
fn string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(value) => js_string(value),
    }
}

/// String.prototype.slice(0, limit) over UTF-16 code units.
fn js_slice_utf16(value: &str, limit: usize) -> String {
    let units: Vec<u16> = value.encode_utf16().take(limit).collect();
    String::from_utf16_lossy(&units)
}

fn cwd_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Node path.resolve: absolute-ize against cwd and lexically normalize
/// (`.`/`..` handling, duplicate separators) without touching the filesystem.
fn node_path_resolve(input: &str) -> String {
    let path = Path::new(input);
    let joined: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result.to_string_lossy().into_owned()
}

fn to_base36(mut value: u128) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        output.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    output.reverse();
    String::from_utf8(output).expect("base36 digits are ASCII")
}

fn random_base36(length: usize) -> String {
    use rand::Rng;
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| DIGITS[rng.gen_range(0..36)] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn parsed(values: &[&str]) -> (Vec<String>, Flags) {
        let parsed = parse_args(&strings(values));
        (parsed.rest, parsed.flags)
    }

    #[test]
    fn create_session_payload_shape() {
        let (rest, flags) = parsed(&[
            "My Title",
            "run",
            "the",
            "tests",
            "--project-id",
            "P1",
            "--start",
        ]);
        let payload = parse_create_session(&rest, &flags);
        assert_eq!(
            payload,
            json!({
                "input": "run the tests",
                "projectId": "P1",
                "start": true,
                "title": "My Title",
            })
        );
    }

    #[test]
    fn create_session_omits_undefined_fields() {
        let (rest, flags) = parsed(&[]);
        let payload = parse_create_session(&rest, &flags);
        assert_eq!(payload, json!({ "input": "" }));
    }

    #[test]
    fn session_tag_valid_and_clear_values() {
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "Bug"]);
        let payload = parse_session_tag(&rest, &flags).expect("valid tag");
        assert_eq!(
            payload,
            json!({ "isFavorite": false, "sessionId": "G1abc", "sessionTag": "bug" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "none"]);
        let payload = parse_session_tag(&rest, &flags).expect("clear tag");
        assert_eq!(
            payload,
            json!({ "isFavorite": false, "sessionId": "G1abc", "sessionTag": null })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "favorite"]);
        let payload = parse_session_tag(&rest, &flags).expect("favorite tag");
        assert_eq!(payload.get("isFavorite"), Some(&Value::Bool(true)));
    }

    #[test]
    fn session_tag_error_messages_match_node_cli() {
        let list = "favorite, high-priority, low-priority, todo, research, in-progress, testing, blocked, on-hold, done, bug, feature, design";
        let (rest, flags) = parsed(&["--session-id", "G1abc"]);
        let error = parse_session_tag(&rest, &flags).expect_err("missing tag");
        assert_eq!(
            error.to_string(),
            format!("Missing session tag. Use one of: {list}, or none.")
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "Nonsense"]);
        let error = parse_session_tag(&rest, &flags).expect_err("unknown tag");
        assert_eq!(
            error.to_string(),
            format!("Unknown session tag \"Nonsense\". Use one of: {list}, or none.")
        );
    }

    #[test]
    fn session_boolean_positional_vs_flag_selector() {
        // Positional selector: rest[0] = session, rest[1] = value.
        let (rest, flags) = parsed(&["G1abc", "false"]);
        let payload = parse_session_boolean("sleeping", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "sleeping": false }));

        // Flag selector: rest[0] is the value.
        let (rest, flags) = parsed(&["yes", "--session-id", "G1abc"]);
        let payload = parse_session_boolean("pinned", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "pinned": true }));

        // Default value is "true".
        let (rest, flags) = parsed(&["--session-id", "G1abc"]);
        let payload = parse_session_boolean("sleeping", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "sleeping": true }));
    }

    #[test]
    fn delayed_send_accepts_each_trigger_and_rejects_ambiguous_triggers() {
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--delay-ms", "300000"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("delay trigger"),
            json!({ "delayMs": 300000, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--when-agent-finishes"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("agent trigger"),
            json!({ "sendWhenAgentStops": true, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--when-all-agents-finish"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("project trigger"),
            json!({ "sendWhenAllProjectSessionsStop": true, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&[
            "--session-id",
            "G1abc",
            "--delay-ms",
            "300000",
            "--when-agent-finishes",
        ]);
        assert!(parse_delayed_send(&rest, &flags).is_err());
    }

    #[test]
    fn send_text_selector_positional_split() {
        let (rest, flags) = parsed(&["G1abc", "hello", "world"]);
        let payload = parse_send_text(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "text": "hello world" })
        );

        let (rest, flags) = parsed(&["hello", "world", "--session-id", "G1abc"]);
        let payload = parse_send_text(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "text": "hello world" })
        );
    }

    #[test]
    fn visible_count_number_coercion() {
        let (rest, flags) = parsed(&["--count", "4"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": 4 }));

        let (rest, flags) = parsed(&["7"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": 7 }));

        let (rest, flags) = parsed(&["abc"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": null }));

        let (rest, flags) = parsed(&[]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": null }));
    }

    #[test]
    fn vs_code_path_positions() {
        assert_eq!(
            parse_vs_code_path_position("file.txt:12:5"),
            ("file.txt".to_string(), Some(12), Some(5))
        );
        assert_eq!(
            parse_vs_code_path_position("file.txt:12"),
            ("file.txt".to_string(), Some(12), None)
        );
        assert_eq!(
            parse_vs_code_path_position("file:0"),
            ("file:0".to_string(), None, None)
        );
        assert_eq!(
            parse_vs_code_path_position(":12"),
            (":12".to_string(), None, None)
        );
        assert_eq!(
            parse_vs_code_path_position("a:12:5:7"),
            ("a:12".to_string(), Some(5), Some(7))
        );
        assert_eq!(
            parse_vs_code_path_position(""),
            ("".to_string(), None, None)
        );
    }

    #[cfg(unix)]
    #[test]
    fn open_path_target_resolves_lexically() {
        let target = parse_open_path_target(&json!("/tmp/x/../logs/app.log:12:3"), false);
        assert_eq!(
            target,
            json!({
                "column": 3,
                "line": 12,
                "path": "/tmp/logs/app.log",
                "raw": "/tmp/x/../logs/app.log:12:3",
            })
        );
    }

    #[test]
    fn edit_paths_wait_consumed_target() {
        // `edit --wait file.txt` — parseArgs consumes file.txt as the wait value.
        let (rest, flags) = parsed(&["--wait", "file.txt"]);
        let payload = parse_edit_paths(&rest, &flags);
        assert_eq!(payload.get("mode"), Some(&json!("edit")));
        assert_eq!(payload.get("wait"), Some(&json!(true)));
        let targets = payload
            .get("targets")
            .and_then(Value::as_array)
            .expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].get("raw"), Some(&json!("file.txt")));
        let token = targets[0]
            .get("waitToken")
            .and_then(Value::as_str)
            .expect("wait token");
        assert!(token.starts_with("wait-"));

        // No wait flag: no wait tokens.
        let (rest, flags) = parsed(&["file.txt"]);
        let payload = parse_edit_paths(&rest, &flags);
        assert_eq!(payload.get("wait"), Some(&json!(false)));
        let targets = payload
            .get("targets")
            .and_then(Value::as_array)
            .expect("targets");
        assert!(targets[0].get("waitToken").is_none());
    }

    #[test]
    fn quick_terminal_double_dash_split() {
        let (rest, flags) = parsed(&["extra", "--", "npm", "run", "dev", "--title", "Dev"]);
        // "--" ends flag parsing, so everything after lands in rest untouched.
        let payload = parse_quick_terminal(&rest, &flags);
        assert_eq!(
            payload.get("command"),
            Some(&json!("extra npm run dev --title Dev"))
        );

        let (rest, flags) = parsed(&["--title", "Dev", "--", "npm", "run", "dev"]);
        let payload = parse_quick_terminal(&rest, &flags);
        assert_eq!(payload, json!({ "command": "npm run dev", "title": "Dev" }));
    }

    #[test]
    fn save_command_defaults() {
        let (rest, flags) = parsed(&["dev", "Dev Server", "npm", "run", "dev"]);
        let payload = parse_save_command(&rest, &flags);
        assert_eq!(
            payload,
            json!({
                "actionType": "terminal",
                "closeTerminalOnExit": false,
                "command": "npm run dev",
                "commandId": "dev",
                "name": "Dev Server",
                "playCompletionSound": true,
                "showOnProjectRow": false,
            })
        );
    }

    #[test]
    fn save_command_preserves_enabled_show_on_project_row() {
        // The flag is opt-in, so the enabled path needs its own coverage: a
        // default-only assertion would still pass if the flag were hardcoded.
        let (rest, flags) = parsed(&[
            "--showOnProjectRow",
            "true",
            "lazygit",
            "Lazygit",
            "lazygit",
        ]);
        let payload = parse_save_command(&rest, &flags);
        assert_eq!(payload.get("showOnProjectRow"), Some(&json!(true)));
        assert_eq!(payload.get("commandId"), Some(&json!("lazygit")));
    }

    #[test]
    fn browser_open_reuse_and_new() {
        let (rest, flags) = parsed(&["https://example.com", "--new", "--active-project"]);
        let payload = parse_browser_open(&rest, &flags);
        assert_eq!(payload.get("reuse"), Some(&json!("none")));
        assert_eq!(payload.get("url"), Some(&json!("https://example.com")));
        assert!(payload.get("projectPath").is_none());

        let (rest, flags) = parsed(&["https://example.com"]);
        let payload = parse_browser_open(&rest, &flags);
        assert_eq!(payload.get("reuse"), Some(&json!("similar")));
        assert_eq!(
            payload.get("projectPath"),
            Some(&Value::String(cwd_string()))
        );
    }

    #[test]
    fn rename_payload_joins_positional_title() {
        let (rest, flags) = parsed(&["G1abc", "My", "New", "Title"]);
        let payload = parse_rename(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "title": "My New Title" })
        );
    }

    #[test]
    fn resolved_session_params_split_global_ref() {
        let payload = json!({ "sessionId": "S1abc:P2def:G3xyz" });
        let flags = Flags::default();
        let params = with_resolved_session_params(&payload, &flags);
        assert_eq!(
            params,
            json!({
                "globalRef": "S1abc:P2def:G3xyz",
                "projectId": "P2def",
                "sessionId": "G3xyz",
            })
        );
    }

    #[test]
    fn resolved_session_params_prefer_explicit_project() {
        let payload = json!({ "sessionId": "G3xyz", "projectId": "P9" });
        let params = with_resolved_session_params(&payload, &Flags::default());
        assert_eq!(params, json!({ "projectId": "P9", "sessionId": "G3xyz" }));

        let mut flags = Flags::default();
        flags.insert_text("projectId", "P7");
        let params = with_resolved_session_params(&json!({ "sessionId": "G3xyz" }), &flags);
        assert_eq!(params, json!({ "projectId": "P7", "sessionId": "G3xyz" }));
    }

    #[test]
    fn resolved_session_params_keep_explicit_nulls() {
        // JS compactObject only strips undefined; sessionTag: null must survive.
        let payload = json!({ "sessionId": "G3xyz", "projectId": "P1", "sessionTag": null });
        let params = with_resolved_session_params(&payload, &Flags::default());
        assert_eq!(
            params,
            json!({ "projectId": "P1", "sessionId": "G3xyz", "sessionTag": null })
        );
    }

    #[test]
    fn renderer_session_target_shapes() {
        let payload = json!({ "projectId": "P1", "sessionId": "G2", "title": "x" });
        assert_eq!(
            with_renderer_session_target(&payload),
            json!({
                "projectId": "P1",
                "sessionId": "G2",
                "sessionTarget": { "projectId": "P1", "sessionId": "G2" },
                "title": "x",
            })
        );

        let payload = json!({ "projectId": "P1", "sessionId": "G2", "globalRef": "S1:P1:G2" });
        assert_eq!(
            with_renderer_session_target(&payload)
                .get("sessionTarget")
                .cloned(),
            Some(json!({ "globalRef": "S1:P1:G2", "projectId": "P1", "sessionId": "G2" }))
        );

        // Missing ids: payload unchanged.
        let payload = json!({ "sessionId": "G2" });
        assert_eq!(with_renderer_session_target(&payload), payload);

        // Existing sessionTarget object: payload unchanged.
        let payload = json!({ "projectId": "P1", "sessionId": "G2", "sessionTarget": { "a": 1 } });
        assert_eq!(with_renderer_session_target(&payload), payload);
    }

    #[test]
    fn terminal_key_mapping() {
        assert_eq!(terminal_text_for_cli_key("ctrl-c"), Some("\u{0003}"));
        assert_eq!(terminal_text_for_cli_key("Escape"), Some("\u{001b}"));
        assert_eq!(terminal_text_for_cli_key("tab"), Some("\t"));
        assert_eq!(terminal_text_for_cli_key("arrow-up"), Some("\u{001b}[A"));
        assert_eq!(terminal_text_for_cli_key("ArrowDown"), Some("\u{001b}[B"));
        assert_eq!(terminal_text_for_cli_key("arrow-right"), Some("\u{001b}[C"));
        assert_eq!(terminal_text_for_cli_key("ArrowLeft"), Some("\u{001b}[D"));
        assert_eq!(terminal_text_for_cli_key("enter"), None);
    }

    #[test]
    fn click_button_and_project_move_positionals() {
        let (rest, flags) = parsed(&["command", "dev"]);
        assert_eq!(
            parse_click_button(&rest, &flags),
            json!({ "id": "dev", "kind": "command" })
        );

        let (rest, flags) = parsed(&["P1", "up"]);
        assert_eq!(
            parse_project_move(&rest, &flags),
            json!({ "direction": "up", "projectId": "P1" })
        );
        let (rest, flags) = parsed(&["P1", "--dir", "down"]);
        assert_eq!(
            parse_project_move(&rest, &flags),
            json!({ "direction": "down", "projectId": "P1" })
        );
    }

    #[test]
    fn assert_card_and_wait_for_payloads() {
        let (rest, flags) = parsed(&[
            "--session-id",
            "G1",
            "--agent-name",
            "Claude",
            "--visible",
            "false",
        ]);
        assert_eq!(
            Value::Object(parse_assert_card(&rest, &flags)),
            json!({ "agentName": "Claude", "sessionId": "G1", "visible": false })
        );

        let (rest, flags) = parsed(&["--session-id", "G1", "--timeout-ms", "5000"]);
        assert_eq!(
            parse_wait_for(&rest, &flags),
            json!({ "sessionId": "G1", "timeoutMs": 5000 })
        );
    }

    #[test]
    fn session_selector_number_coercion() {
        let (rest, flags) = parsed(&["--index", "2", "--session-number", "abc"]);
        assert_eq!(
            Value::Object(parse_session_selector(&rest, &flags)),
            json!({ "index": 2, "sessionNumber": null })
        );
    }

    #[test]
    fn sidebar_project_collections_state_payload() {
        let state = r#"{"collections":{"C1":{"collectionId":"C1","title":"Group 1","color":"transparent","collapsed":false,"projectIds":["P1"]}},"order":["C1"],"nextCollectionNumber":2}"#;
        let (rest, flags) = parsed(&["--state-json", state]);
        let payload = parse_sidebar_project_collections_state(&rest, &flags).expect("valid state");
        assert_eq!(
            payload.get("state").and_then(|value| value.get("order")),
            Some(&json!(["C1"]))
        );

        // Positional JSON fallback mirrors automation-save's rest.join(" ").
        let (rest, flags) = parsed(&["{\"collections\":", "{}}"]);
        let payload =
            parse_sidebar_project_collections_state(&rest, &flags).expect("positional state");
        assert_eq!(payload, json!({ "state": { "collections": {} } }));

        // Missing state → error.
        let (rest, flags) = parsed(&[]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());

        // Invalid JSON → error.
        let (rest, flags) = parsed(&["--state-json", "{nope"]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());

        // Non-object JSON → error.
        let (rest, flags) = parsed(&["--state-json", "\"text\""]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());
    }

    #[test]
    fn project_collection_payload_supports_stable_selectors() {
        let (rest, flags) = parsed(&["--path", "/Users/example/project", "--group", "ShortPoint"]);
        assert_eq!(
            parse_project_collection(&rest, &flags),
            json!({
                "collectionTitle": "ShortPoint",
                "path": "/Users/example/project",
            })
        );

        let (rest, flags) = parsed(&["P123", "Clients"]);
        assert_eq!(
            parse_project_collection(&rest, &flags),
            json!({ "collectionTitle": "Clients", "projectId": "P123" })
        );
    }

    #[test]
    fn js_string_coercions() {
        assert_eq!(js_string(&json!(true)), "true");
        assert_eq!(js_string(&json!(2.0)), "2");
        assert_eq!(js_string(&json!(1.5)), "1.5");
        assert_eq!(js_string(&Value::Null), "null");
        assert_eq!(string_or_empty(None), "");
        assert_eq!(string_or_empty(Some(&Value::Null)), "");
    }
}
