use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

// ---------------------------------------------------------------------------
// Payload parsers (parse* functions from the Node CLI)
// ---------------------------------------------------------------------------

pub(super) fn evaluate_parser(parser: Parser, rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut value = match parser {
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
        Parser::ExportTranscript => parse_export_transcript(rest, flags),
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
        Parser::CustomSessionTagsState => parse_custom_session_tags_state(rest, flags)?,
        Parser::SessionChatRead => parse_session_chat_read(rest, flags),
        Parser::SessionChatDraftAgent => parse_session_chat_draft_agent(rest, flags)?,
        Parser::SessionChatKey => parse_session_chat_key(rest, flags)?,
        Parser::SessionChatModel => {
            super::session_chat_model::parse(parse_session_selector(rest, flags), flags)?
        }
        Parser::AgentPromptSearch => parse_agent_prompt_search(flags)?,
        Parser::AgentPromptRef => parse_agent_prompt_ref(flags)?,
        Parser::AgentPromptLaunch => parse_agent_prompt_launch(flags)?,
        Parser::SessionChatAnswer => parse_session_chat_answer(rest, flags)?,
        Parser::SessionChatRewind => parse_session_chat_rewind(rest, flags)?,
        Parser::SessionChatQueuedPrompt => parse_session_chat_queued_prompt(rest, flags)?,
        Parser::SessionChatQueueOrder => parse_session_chat_queue_order(rest, flags)?,
        Parser::SessionChatDraft => parse_session_chat_draft(rest, flags)?,
        Parser::KeepSessionsAwake => parse_keep_sessions_awake(flags)?,
        Parser::ClientHello => parse_client_hello(flags)?,
        Parser::ClientView => parse_client_view(flags)?,
    };
    if let Some(raw) = flags.text("draftVersionJson") {
        let version: Value = serde_json::from_str(&raw)
            .map_err(|error| CliError::Other(format!("Invalid --draft-version-json: {error}")))?;
        value["draftVersion"] = version;
    }
    // A caller that may retry names the send once (`--send-request-id`), so gxserver delivers it once.
    if let (Some(id), Some(object)) = (flags.text("sendRequestId"), value.as_object_mut()) {
        object.insert("sendRequestId".to_string(), Value::String(id));
    }
    Ok(value)
}

/*
CDXC:Mobile 2026-07-31:
Ghostex mobile has no HTTP path to gxserver, so the Session Chat endpoints are
exposed as CLI verbs the phone SSH-execs, exactly like the Add Project flow.
`read-session-chat` carries the long-poll pair (--wait-ms + --fingerprint): the
daemon holds the request until the chat fingerprint changes, which is how the
phone tails a conversation without an /api/events socket.
*/
/*
CDXC:PromptSearch 2026-08-20:
Prompt history lives on the machine that ran the agent, so Ghostex mobile
reaches Find the same way it reaches chat: these verbs SSH-exec on that machine
and forward to the daemon's own endpoints. Every follow-up verb addresses a
result by its stable `--key`, never by a list position, so a phone acting on a
result minutes later still lands on the prompt it displayed.
*/
/// CDXC:PromptSearch 2026-10-01 WHY:
/// The prompt-search verbs document `true|false` values, but JS truthiness reads the text "false" as true, so `--favorite false` starred instead of unstarring, `--group-by-day false` still grouped, and `--refresh false` rebuilt the index on every phone search.
fn agent_prompt_bool_flag(flags: &Flags, key: &str) -> bool {
    match flags.string_value(key) {
        Some(text) => !matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "" | "false" | "0" | "no" | "off"
        ),
        None => flags.truthy(key),
    }
}

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
            Value::Bool(agent_prompt_bool_flag(flags, "groupByDay")),
        );
    }
    if flags.contains("includeFacets") {
        map.insert(
            "includeFacets".to_string(),
            Value::Bool(agent_prompt_bool_flag(flags, "includeFacets")),
        );
    }
    if flags.contains("refresh") {
        map.insert(
            "refresh".to_string(),
            Value::Bool(agent_prompt_bool_flag(flags, "refresh")),
        );
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
            Value::Bool(agent_prompt_bool_flag(flags, "favorite")),
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
    set_or_remove(&mut map, "historyMode", flag_json(flags, "historyMode"));
    if flags.contains("preserveNewest") {
        map.insert(
            "preserveNewest".to_string(),
            Value::Bool(flags.truthy("preserveNewest")),
        );
    }
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
    set_or_remove(&mut map, "subagent", flag_json(flags, "subagent"));
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
