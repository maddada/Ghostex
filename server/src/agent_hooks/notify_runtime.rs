use std::{
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Map, Value};

use crate::{
    constants::{GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION},
    domain::DomainStateError,
};

use super::event_mapping::{
    activity_for_hook_event, claude_notification_is_idle_input, env_string, first_path,
    first_string, is_prompt_event, nested_get, normalized_hook_agent_key, update_hook_status,
};
use super::install::{parent_process_id, read_json_object};
use super::probing::{
    decode_base64_text, expand_home_path, insert_json_string, io_error, normalize_prompt_text,
    now_iso, parse_global_session_ref, read_file_text, temp_path_for,
};

/*
CDXC:AgentHooks 2026-06-21-19:26:
The Rust hook artifact must perform the same work as TypeScript gxserver's installed notify script: normalize provider lifecycle events, update the local sidecar for legacy clients, persist hook-session identity for restore, capture the first user prompt for gxserver-owned auto-title jobs, and post authenticated hook events back to gxserver. The shell wrapper calls this hidden helper so Rust does not depend on a random system Node runtime.
*/
pub fn run_notify_hook(args: Vec<String>) -> Result<(), DomainStateError> {
    let state_path = args.first().map(String::as_str).unwrap_or_default();
    let input_arg = args.get(1).cloned().unwrap_or_else(|| {
        let mut input = String::new();
        let _ = std::io::stdin().read_to_string(&mut input);
        input
    });
    let hook_state_dir = expand_home_path(
        args.get(2)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("~/.ghostexterm"),
    );
    let has_state_path = !state_path.trim().is_empty();
    let mut state = if has_state_path {
        read_hook_state(Path::new(state_path))
    } else {
        Map::new()
    };
    let payload = serde_json::from_str::<Value>(&input_arg)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));

    let explicit_agent_name = first_string([payload.get("agent")])
        .or_else(|| env_string("GHOSTEX_AGENT"))
        .or_else(|| env_string("ghostex_AGENT"));
    let agent_name = explicit_agent_name
        .clone()
        .or_else(|| env_string("VSMUX_AGENT"))
        .or_else(|| read_state_string(&state, "agent"))
        .unwrap_or_else(|| "codex".to_string());
    let agent_key = normalized_hook_agent_key(&agent_name);
    let event_name = first_string([
        payload.get("hook_event_name"),
        payload.get("hookEventName"),
        payload.get("event"),
    ])
    .unwrap_or_default();
    let session_id = first_string([
        payload.get("session_id"),
        payload.get("sessionId"),
        payload.get("conversation_id"),
        payload.get("conversationId"),
        payload.get("thread_id"),
        payload.get("threadId"),
        nested_get(&payload, &["session", "id"]),
        nested_get(&payload, &["thread", "id"]),
        nested_get(&payload, &["properties", "sessionID"]),
        nested_get(&payload, &["properties", "sessionId"]),
        nested_get(&payload, &["properties", "session_id"]),
        nested_get(&payload, &["properties", "info", "id"]),
    ]);
    let transcript_path = first_path([
        payload.get("transcript_path"),
        payload.get("transcriptPath"),
        payload.get("log_path"),
        payload.get("logPath"),
    ]);
    let prompt = first_string([
        payload.get("prompt"),
        payload.get("text"),
        payload.get("message"),
        payload.get("input"),
        nested_get(&payload, &["prompt", "text"]),
    ]);

    ensure_state_default(&mut state, "status", "idle");
    if read_state_string(&state, "statusUpdatedAt").is_none() {
        if let Some(last_activity_at) = read_state_string(&state, "lastActivityAt") {
            state.insert("statusUpdatedAt".to_string(), json!(last_activity_at));
        }
    }
    if explicit_agent_name.is_some() || read_state_string(&state, "agent").is_none() {
        state.insert("agent".to_string(), json!(agent_key.clone()));
    }
    if let Some(session_id) = session_id.clone() {
        state.insert("agentSessionId".to_string(), json!(session_id.clone()));
        write_hook_store(
            &hook_state_dir,
            &agent_key,
            &session_id,
            transcript_path.as_deref(),
            &payload,
        );
    }
    if let Some(transcript_path) = transcript_path.clone() {
        state.insert("agentSessionPath".to_string(), json!(transcript_path));
    }

    let notification_is_idle_input = matches!(agent_key.as_str(), "claude" | "openclaude")
        && event_name.to_ascii_lowercase().contains("notification")
        && claude_notification_is_idle_input(&payload);
    if let Some(next_activity) = activity_for_hook_event(&agent_key, &event_name, &payload) {
        /*
        CDXC:SessionChatPromptQueue 2026-08-24:
        Claude's 60s "waiting for your input" reminder means the CLI is idle at
        its input prompt. It is not a completion, permission, or approval
        event, so it must always settle to idle regardless of the prior state.
        Genuine permission notifications do not match notification_is_idle_input
        and retain their attention transition.
        */
        let next_activity = if notification_is_idle_input && next_activity == "attention" {
            "idle".to_string()
        } else {
            next_activity
        };
        update_hook_status(&mut state, &next_activity);
    }

    if is_prompt_event(&event_name) {
        if let Some(prompt) = prompt.clone() {
            if read_state_string(&state, "firstUserMessageBase64").is_none() {
                state.insert(
                    "firstUserMessageBase64".to_string(),
                    json!(BASE64_STANDARD.encode(prompt.as_bytes())),
                );
            }
            if read_state_string(&state, "lastActivityAt").is_none() {
                state.insert("lastActivityAt".to_string(), json!(now_iso()));
            }
            if !matches!(agent_key.as_str(), "claude" | "cursor")
                && !matches!(
                    read_state_string(&state, "autoTitleFromFirstPrompt").as_deref(),
                    Some("1" | "true" | "TRUE" | "True")
                )
                && read_state_string(&state, "pendingFirstPromptAutoRenamePrompt").is_none()
            {
                let first_prompt = normalize_prompt_text(
                    decode_base64_text(
                        read_state_string(&state, "firstUserMessageBase64")
                            .as_deref()
                            .unwrap_or_default(),
                    )
                    .as_str(),
                );
                let current_prompt = normalize_prompt_text(&prompt);
                let pending = if !first_prompt.is_empty() && first_prompt != current_prompt {
                    normalize_prompt_text(&format!("{first_prompt}\n{current_prompt}"))
                } else {
                    current_prompt
                };
                if !pending.is_empty() {
                    state.insert(
                        "pendingFirstPromptAutoRenamePrompt".to_string(),
                        json!(pending),
                    );
                }
            }
        }
    }

    let decoded_first_prompt = decode_base64_text(
        read_state_string(&state, "firstUserMessageBase64")
            .as_deref()
            .unwrap_or_default(),
    );
    let first_user_message = read_state_string(&state, "pendingFirstPromptAutoRenamePrompt")
        .or_else(|| (!decoded_first_prompt.is_empty()).then_some(decoded_first_prompt))
        .or(prompt);
    post_gxserver_hook_event(
        &agent_key,
        session_id.as_deref(),
        transcript_path.as_deref(),
        first_user_message.as_deref(),
        &event_name,
        &state,
        &payload,
    );
    if has_state_path {
        write_hook_state(Path::new(state_path), &state)?;
    }
    Ok(())
}

pub(crate) fn read_hook_state(path: &Path) -> Map<String, Value> {
    let mut state = Map::new();
    let Ok(text) = fs::read_to_string(path) else {
        return state;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = if matches!(key, "firstUserMessageBase64" | "agentSessionPath") {
            value.trim().to_string()
        } else {
            normalize_prompt_text(value)
        };
        if !value.is_empty() {
            state.insert(key.to_string(), Value::String(value));
        }
    }
    state
}

fn write_hook_state(path: &Path, state: &Map<String, Value>) -> Result<(), DomainStateError> {
    let keys = [
        "status",
        "statusUpdatedAt",
        "attentionEventId",
        "attentionAcknowledgedAt",
        "attentionAcknowledgedEventId",
        "agent",
        "agentSessionId",
        "agentSessionPath",
        "firstUserMessageBase64",
        "frozenAt",
        "autoTitleFromFirstPrompt",
        "historyBase64",
        "lastActivityAt",
        "pendingFirstPromptAutoRenamePrompt",
        "title",
    ];
    let mut text = String::new();
    for key in keys {
        text.push_str(key);
        text.push('=');
        text.push_str(read_state_string(state, key).as_deref().unwrap_or_default());
        text.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, text).map_err(io_error)?;
    fs::rename(&temp_path, path).map_err(io_error)
}

fn ensure_state_default(state: &mut Map<String, Value>, key: &str, value: &str) {
    if read_state_string(state, key).is_none() {
        state.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) fn read_state_string(state: &Map<String, Value>, key: &str) -> Option<String> {
    state
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn write_hook_store(
    hook_state_dir: &Path,
    agent_key: &str,
    session_id: &str,
    transcript_path: Option<&str>,
    payload: &Value,
) {
    let (global_project_id, global_session_id) = parse_global_session_ref(
        env::var("GHOSTEX_GLOBAL_SESSION_REF")
            .unwrap_or_default()
            .as_str(),
    );
    let workspace_id = env_string("GHOSTEX_WORKSPACE_ID")
        .or_else(|| env_string("VSMUX_WORKSPACE_ID"))
        .or_else(|| env_string("ghostex_WORKSPACE_ID"))
        .or(global_project_id);
    let surface_id = env_string("GHOSTEX_SESSION_ID")
        .or_else(|| env_string("VSMUX_SESSION_ID"))
        .or_else(|| env_string("ghostex_SESSION_ID"))
        .or(global_session_id);
    let (Some(workspace_id), Some(surface_id)) = (workspace_id, surface_id) else {
        return;
    };
    let store_path = hook_state_dir.join(format!("{agent_key}-hook-sessions.json"));
    let mut data = read_json_object(&read_file_text(&store_path));
    if !data.is_object() {
        data = json!({});
    }
    let object = data.as_object_mut().expect("object");
    let sessions = object
        .entry("sessions".to_string())
        .or_insert_with(|| json!({}));
    if !sessions.is_object() {
        *sessions = json!({});
    }
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| env_string("GHOSTEX_WORKSPACE_ROOT"))
        .or_else(|| env_string("VSMUX_WORKSPACE_ROOT"))
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .to_string_lossy()
                .to_string()
        });
    sessions.as_object_mut().expect("sessions object").insert(
        session_id.to_string(),
        json!({
            "sessionId": session_id,
            "workspaceId": workspace_id,
            "surfaceId": surface_id,
            "cwd": cwd,
            "transcriptPath": transcript_path,
            "pid": parent_process_id(),
            "isRestorable": true,
            "updatedAt": UtcTimestamp::now_seconds(),
        }),
    );
    object.insert("version".to_string(), json!(1));
    if let Some(parent) = store_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temp_path = temp_path_for(&store_path);
    if let Ok(text) = serde_json::to_string_pretty(&data) {
        let _ = fs::write(&temp_path, format!("{text}\n"));
        let _ = fs::rename(&temp_path, &store_path);
    }
}

struct UtcTimestamp;

impl UtcTimestamp {
    fn now_seconds() -> f64 {
        chrono::Utc::now().timestamp_millis() as f64 / 1000.0
    }
}

#[allow(clippy::too_many_arguments)]
fn post_gxserver_hook_event(
    agent_key: &str,
    session_id: Option<&str>,
    transcript_path: Option<&str>,
    first_user_message: Option<&str>,
    event_name: &str,
    state: &Map<String, Value>,
    payload: &Value,
) {
    let base_url = match env_string("GHOSTEX_GXSERVER_BASE_URL") {
        Some(value) => value.trim_end_matches('/').to_string(),
        None => return,
    };
    let (Some(project_id), Some(surface_id)) = parse_global_session_ref(
        env::var("GHOSTEX_GLOBAL_SESSION_REF")
            .unwrap_or_default()
            .as_str(),
    ) else {
        return;
    };
    let token = read_gxserver_auth_token();
    if token.is_empty() {
        return;
    }
    let protocol_version = env_string("GHOSTEX_GXSERVER_PROTOCOL_VERSION")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(GXSERVER_PROTOCOL_VERSION as i64);
    let mut params = Map::new();
    params.insert("agentName".to_string(), json!(agent_key));
    params.insert("eventName".to_string(), json!(event_name));
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("rawEventName".to_string(), json!(event_name));
    params.insert("sessionId".to_string(), json!(surface_id));
    insert_json_string(&mut params, "agentSessionId", session_id);
    insert_json_string(&mut params, "agentSessionPath", transcript_path);
    insert_json_string(&mut params, "firstUserMessage", first_user_message);
    insert_json_string(
        &mut params,
        "status",
        read_state_string(state, "status").as_deref(),
    );
    insert_json_string(
        &mut params,
        "statusUpdatedAt",
        read_state_string(state, "statusUpdatedAt").as_deref(),
    );
    insert_json_string(
        &mut params,
        "title",
        read_state_string(state, "title").as_deref(),
    );
    /*
    CDXC:SessionChatSend 2026-07-31:
    Session Chat interactive-prompt capture needs the hook payload's tool
    identity: gxserver derives question/approval cards from
    AskUserQuestion-ish tool_input and PermissionRequest tool names at
    ingestion. Forward them verbatim; absent fields stay absent.
    */
    if let Some(tool_name) = first_string([payload.get("tool_name"), payload.get("toolName")]) {
        params.insert("toolName".to_string(), json!(tool_name));
    }
    /*
    CDXC:SessionChatPromptQueue 2026-08-24:
    gxserver's own event mapping outranks the posted sidecar status, so the
    idle-input distinction must travel with the event: without it the server
    re-derives Notification → attention and re-creates the stuck-attention
    state this hook just avoided.
    */
    if matches!(agent_key, "claude" | "openclaude")
        && event_name.to_ascii_lowercase().contains("notification")
        && claude_notification_is_idle_input(payload)
    {
        params.insert("notificationKind".to_string(), json!("idleInput"));
    }
    if let Some(tool_input) = payload
        .get("tool_input")
        .or_else(|| payload.get("toolInput"))
        .filter(|value| !value.is_null())
    {
        params.insert("toolInput".to_string(), tool_input.clone());
    }
    let body = json!({
        "protocolVersion": protocol_version,
        "params": params,
    });
    let _ = post_json(
        &base_url,
        "/api/ingestAgentHookEvent",
        &token,
        protocol_version,
        &body,
    );
}

fn post_json(
    base_url: &str,
    path: &str,
    token: &str,
    protocol_version: i64,
    body: &Value,
) -> std::io::Result<()> {
    let Ok(url) = url::Url::parse(base_url) else {
        return Ok(());
    };
    if url.scheme() != "http" {
        return Ok(());
    }
    let Some(host) = url.host_str() else {
        return Ok(());
    };
    let port = url.port_or_known_default().unwrap_or(80);
    let address = format!("{host}:{port}");
    let timeout = Duration::from_millis(1500);
    let mut stream = TcpStream::connect(&address)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let body = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{GXSERVER_PROTOCOL_HEADER}: {protocol_version}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    Ok(())
}

fn read_gxserver_auth_token() -> String {
    let Some(token_file) = env_string("GHOSTEX_GXSERVER_AUTH_TOKEN_FILE") else {
        return String::new();
    };
    fs::read_to_string(expand_home_path(&token_file))
        .unwrap_or_default()
        .trim()
        .to_string()
}
