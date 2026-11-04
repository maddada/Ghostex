use serde_json::{json, Value};

use crate::ghostex_cli::args::{parse_args, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};
use crate::ghostex_cli::{selector, sessions};

use super::*;

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
    ExportTranscript,
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
    CDXC:SessionNotes 2026-08-24:
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
    CustomSessionTagsState,
    /// session selector plus readSessionChat paging/long-poll flags.
    SessionChatRead,
    /// session selector plus the project agent id for a draft-agent switch.
    SessionChatDraftAgent,
    /// session selector plus one serialized Session Chat key name.
    SessionChatKey,
    SessionChatModel,
    /*
    CDXC:PromptSearch 2026-08-20:
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
    CDXC:SessionChat 2026-08-21:
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
    CDXC:KeepAwake 2026-08-19:
    `--sessions-json` carries the whole attached-tab set in one exec so a phone
    renewing several holds costs one SSH round trip, not one per tab.
    */
    KeepSessionsAwake,
    /// `--client mobile --os <android|ios> [--os-version <v>] [--app-version <v>]`
    /// for the analytics hello (`/api/recordClientEvent`).
    ClientHello,
    /// `--client mobile --view <mobile_*>` for a phone screen opened
    /// (`/api/recordClientEvent`, `surface.opened`).
    ClientView,
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
    finish_bridge_action(action, payload, &parsed.flags, options)
}

/// `run_bridge_action` for a verb that hands another session a message
/// (`send-session-chat-message`, `queue-session-chat-prompt`): run from inside an agent session,
/// the text carries that agent's sender block and gxserver may refuse the write
/// (agents/outbound.rs).
pub fn run_message_bridge_action(
    action: &str,
    parser: Parser,
    options: BridgeOptions,
    args: &[String],
) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut payload = evaluate_parser(parser, &parsed.rest, &parsed.flags)?;
    if let Some(selector_text) = payload["sessionId"].as_str().map(str::to_string) {
        if let Some(write) =
            crate::ghostex_cli::agents::outbound::agent_write(&selector_text, &parsed.flags)?
        {
            crate::ghostex_cli::agents::outbound::sign_message_payload(&mut payload, |body| {
                write.signed(body)
            });
        }
    }
    finish_bridge_action(action, payload, &parsed.flags, options)
}

fn finish_bridge_action(
    action: &str,
    payload: Value,
    flags: &Flags,
    options: BridgeOptions,
) -> CliResult<()> {
    let parsed_flags = flags;
    /*
    Long-running bridge commands (`edit --wait`) disable the CLI timeout when
    the caller did not pass one, exactly like the JS
    `payload.wait === true && flags.timeout === undefined` special case.
    */
    let bridge_flags =
        if payload.get("wait") == Some(&Value::Bool(true)) && !parsed_flags.contains("timeout") {
            let mut flags = parsed_flags.clone();
            flags.insert_text("timeout", "0");
            flags
        } else {
            parsed_flags.clone()
        };
    let result = send_gxserver_cli_action(action, &payload, &bridge_flags)?;
    /*
    CDXC:Mobile 2026-05-17-14:24:
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
    CDXC:Cli 2026-06-04-03:20:
    gxserver session ids are project-scoped. Selector-backed bridge actions
    must carry the resolved projectId with the sessionId so remote and mobile
    clients reconnect through the same S/P/G zmx route instead of addressing
    a bare G id.
    */
    // From inside an agent session, gxserver may refuse typing into this session
    // (agents/outbound.rs: a thread never types into its own orchestrator).
    if let Some(session) = resolved_session
        .as_ref()
        .filter(|_| matches!(action, "sendText" | "sendEnter" | "sendKey"))
    {
        if let Some(sender) = crate::ghostex_cli::agents::identity::agent_caller()? {
            crate::ghostex_cli::agents::outbound::check_agent_session_write(
                &sender,
                session,
                &parsed.flags,
            )?;
        }
    }
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

pub(super) fn ensure_gxserver_project_for_path(
    project_path: &str,
    flags: &Flags,
) -> CliResult<Value> {
    let mut params = json!({ "path": project_path });
    // `--workspace <name|id>` puts a new project there; without it, Personal.
    if let Some(workspace) = flags.string_value("workspace") {
        params["workspaceId"] = json!(workspace);
    }
    let result = rpc::call_gxserver_rpc("/api/addProjectPath", &params, flags)?;
    Ok(result.get("project").cloned().unwrap_or(Value::Null))
}

pub(super) fn normalize_required_project_id(
    value: Option<Value>,
    command_name: &str,
) -> CliResult<String> {
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

pub(super) fn with_resolved_session_params(payload: &Value, flags: &Flags) -> Value {
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

pub(crate) fn with_resolved_gxserver_session_params(
    payload: &Value,
    flags: &Flags,
) -> CliResult<Value> {
    let params = with_resolved_session_params(payload, flags);
    if js_truthy(params.get("projectId")) || !js_truthy(params.get("sessionId")) {
        return Ok(params);
    }
    /*
    CDXC:Sessions 2026-05-31-08:45:
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
