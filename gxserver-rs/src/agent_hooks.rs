use std::{
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Map, Value};

use crate::{
    constants::{GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION},
    domain::DomainStateError,
    paths::{AgentPaths, GxserverPaths},
    platform::shell::{command_shell, command_shell_for_path, login_shell_candidates},
};

const NOTIFY_HOOK_MARKER: &str = "ghostex-gxserver-agent-notify-hook-marker";
const NOTIFY_HOOK_VERSION: usize = 6;
const OPENCODE_PLUGIN_MARKER: &str = "ghostex-opencode-session-plugin-marker";
const OPENCODE_PLUGIN_SPEC: &str = "./plugins/ghostex-session.js";
const AMP_PLUGIN_MARKER: &str = "ghostex-amp-session-extension-marker";
const PI_EXTENSION_MARKER: &str = "ghostex-pi-session-extension-marker";
const OMP_EXTENSION_MARKER: &str = "ghostex-omp-session-extension-marker";
const SHELL_PATH_SENTINEL: &str = "__GHOSTEX_GXSERVER_SHELL_PATH__";
const GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS: &[&str] =
    &["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"];

struct HookDefinition {
    agent_id: &'static str,
    cli_command: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookFormat {
    Antigravity,
    FlatJson,
    KiroJson,
    MarkedYaml,
    NestedJson,
    Opencode,
    PluginFile,
}

const HOOK_DEFINITIONS: &[HookDefinition] = &[
    HookDefinition {
        agent_id: "codex",
        cli_command: "codex",
    },
    HookDefinition {
        agent_id: "claude",
        cli_command: "claude",
    },
    HookDefinition {
        agent_id: "cursor",
        cli_command: "cursor-agent",
    },
    HookDefinition {
        agent_id: "gemini",
        cli_command: "gemini",
    },
    HookDefinition {
        agent_id: "kiro",
        cli_command: "kiro-cli",
    },
    HookDefinition {
        agent_id: "copilot",
        cli_command: "copilot",
    },
    HookDefinition {
        agent_id: "droid",
        cli_command: "droid",
    },
    HookDefinition {
        agent_id: "grok",
        cli_command: "grok",
    },
    HookDefinition {
        agent_id: "antigravity",
        cli_command: "agy",
    },
    HookDefinition {
        agent_id: "amp",
        cli_command: "amp",
    },
    HookDefinition {
        agent_id: "omp",
        cli_command: "omp",
    },
    HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    },
    HookDefinition {
        agent_id: "rovodev",
        cli_command: "acli",
    },
    HookDefinition {
        agent_id: "hermes-agent",
        cli_command: "hermes",
    },
    HookDefinition {
        agent_id: "codebuddy",
        cli_command: "codebuddy",
    },
    HookDefinition {
        agent_id: "qoder",
        cli_command: "qodercli",
    },
    HookDefinition {
        agent_id: "opencode",
        cli_command: "opencode",
    },
];

/*
CDXC:AgentHooks 2026-06-16-10:00:
Rust Phase 6 exposes the same local-only hook status and install RPCs without putting raw hook payloads, terminal titles, paths, or command output into persistent logs. Status reports deterministic metadata, while explicit install writes only Ghostex-owned hook artifacts under the selected HOME.
*/
pub fn read_agent_hook_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let auto_upgrade = params.get("autoUpgradeInstalled").and_then(Value::as_bool) != Some(false);
    let mut auto_upgraded_paths = Vec::new();
    let mut rows = Vec::new();
    for agent_id in agent_ids {
        if let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        {
            let mut row = read_hook_status(definition, &hook_paths)?;
            if auto_upgrade
                && row.get("status").and_then(Value::as_str) == Some("updateRequired")
                && row.get("cliInstalled").and_then(Value::as_bool) == Some(true)
            {
                install_notify_hook(&hook_paths)?;
                auto_upgraded_paths.push(path_string(&hook_paths.notify_hook_path));
                for upgraded_path in install_agent_hook(definition, &hook_paths)? {
                    push_unique_path(&mut auto_upgraded_paths, upgraded_path);
                }
                row = read_hook_status(definition, &hook_paths)?;
            }
            rows.push(row);
        }
    }
    let mut result = Map::new();
    result.insert("agents".to_string(), Value::Array(rows));
    if !auto_upgraded_paths.is_empty() {
        result.insert("autoUpgradedPaths".to_string(), json!(auto_upgraded_paths));
    }
    result.insert("generatedAt".to_string(), json!(now_iso()));
    result.insert(
        "hookStateDirectory".to_string(),
        json!(path_string(&hook_paths.hook_state_directory)),
    );
    result.insert(
        "notifyHookPath".to_string(),
        json!(path_string(&hook_paths.notify_hook_path)),
    );
    result.insert("type".to_string(), json!("agentHookStatus"));
    Ok(Value::Object(result))
}

pub fn install_agent_hooks(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let mut installed_paths = Vec::new();
    install_notify_hook(&hook_paths)?;
    installed_paths.push(path_string(&hook_paths.notify_hook_path));
    for agent_id in agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        if !command_exists(definition.cli_command, &hook_paths.home_dir) {
            continue;
        }
        installed_paths.extend(install_agent_hook(definition, &hook_paths)?);
    }
    let mut status = read_agent_hook_status(paths, params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installedPaths".to_string(), json!(installed_paths));
    Ok(Value::Object(status))
}

pub fn uninstall_agent_hooks(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let mut removed_paths = Vec::new();
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Advanced Settings uninstall must remove only Ghostex-owned hook commands, marked YAML blocks, plugin registrations, and Ghostex extension files while leaving user-managed provider hooks intact. The shared notify hook is removed after provider cleanup and status is reread with auto-upgrade disabled so uninstall never recreates hooks it just removed.
    */
    for agent_id in &agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        for removed_path in uninstall_agent_hook(definition, &hook_paths)? {
            push_unique_path(&mut removed_paths, removed_path);
        }
    }
    match fs::remove_file(&hook_paths.notify_hook_path) {
        Ok(()) => push_unique_path(
            &mut removed_paths,
            path_string(&hook_paths.notify_hook_path),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    let mut status_params = params.clone();
    status_params.insert("agentIds".to_string(), json!(agent_ids));
    status_params.insert("autoUpgradeInstalled".to_string(), json!(false));
    let mut status = read_agent_hook_status(paths, &status_params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("removedPaths".to_string(), json!(removed_paths));
    Ok(Value::Object(status))
}

struct HookPaths {
    home_dir: PathBuf,
    hook_state_directory: PathBuf,
    notify_hook_path: PathBuf,
}

impl HookPaths {
    fn new(home_dir: PathBuf) -> Self {
        let agent_paths = AgentPaths::new(&home_dir);
        Self {
            hook_state_directory: home_dir.join(".ghostexterm"),
            notify_hook_path: agent_paths.hooks_root.join("agent-shell-notify.sh"),
            home_dir,
        }
    }
}

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
    let event_name =
        first_string([payload.get("hook_event_name"), payload.get("event")]).unwrap_or_default();
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

    if let Some(next_activity) = activity_for_hook_event(&agent_key, &event_name, &payload) {
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
    );
    if has_state_path {
        write_hook_state(Path::new(state_path), &state)?;
    }
    Ok(())
}

fn read_hook_state(path: &Path) -> Map<String, Value> {
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

fn read_state_string(state: &Map<String, Value>, key: &str) -> Option<String> {
    state
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn nested_get<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    Some(current)
}

fn first_string<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(normalize_prompt_text)
            .filter(|text| !text.is_empty())
        {
            return Some(text);
        }
    }
    None
}

fn first_path<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

fn env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| normalize_prompt_text(&value))
        .filter(|value| !value.is_empty())
}

fn normalized_hook_agent_key(value: &str) -> String {
    let normalized = normalize_prompt_text(&value.to_ascii_lowercase());
    let mapped = match normalized.as_str() {
        "claude" | "claude code" => "claude",
        "codex" | "openai codex" | "codex cli" => "codex",
        "pi" | "π" => "pi",
        "omp" => "omp",
        "opencode" | "open code" => "opencode",
        "grok" | "grok build" => "grok",
        "amp" | "amp cli" => "amp",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "gemini" | "gemini cli" => "gemini",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "copilot" | "github copilot" => "copilot",
        "codebuddy" | "code buddy" => "codebuddy",
        "droid" | "factory" | "factory droid" => "droid",
        "kiro" | "kiro-cli" | "kiro cli" => "kiro",
        "qoder" | "qodercli" => "qoder",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "codex".to_string()
    } else {
        cleaned
    }
}

fn activity_for_hook_event(agent_key: &str, event_name: &str, payload: &Value) -> Option<String> {
    let normalized_event_name = normalize_prompt_text(event_name);
    let lower = normalized_event_name.to_ascii_lowercase();
    if agent_key == "claude" {
        if matches!(lower.as_str(), "stop" | "idle" | "sessionend") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit" | "prompt-submit" | "pretooluse" | "pre-tool-use"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(agent_key, "copilot" | "codebuddy" | "droid" | "qoder") {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if agent_key == "antigravity" {
        let fully_idle = payload_boolean(
            payload,
            &[
                "fullyIdle",
                "fully_idle",
                "metadata.fullyIdle",
                "properties.fullyIdle",
            ],
        );
        if fully_idle == Some(false)
            && matches!(lower.as_str(), "stop" | "turn-completion" | "notification")
        {
            return Some("working".to_string());
        }
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    let compact = lower.replace(['_', '-', '.'], "");
    if matches!(
        compact.as_str(),
        "agentstart"
            | "beforeagentstart"
            | "beforeagent"
            | "beforeshellexecution"
            | "beforesubmitprompt"
            | "onsessionreset"
            | "onsessionstart"
            | "ontoolpermission"
            | "postapprovalresponse"
            | "posttooluse"
            | "prellmcall"
            | "pretoolcall"
            | "preinvocation"
            | "pretooluse"
            | "promptsubmit"
            | "userpromptsubmit"
    ) {
        return Some("working".to_string());
    }
    if matches!(
        compact.as_str(),
        "notification" | "notify" | "permissionrequest" | "preapprovalrequest"
    ) {
        return Some("attention".to_string());
    }
    if matches!(
        compact.as_str(),
        "afteragent"
            | "afteragentresponse"
            | "agentend"
            | "agentresponse"
            | "oncomplete"
            | "onerror"
            | "onsessionend"
            | "onsessionfinalize"
            | "postllmcall"
            | "release"
            | "sessionend"
            | "sessionshutdown"
            | "stop"
            | "turncompletion"
    ) {
        return Some("idle".to_string());
    }
    None
}

fn payload_boolean(payload: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        let value = if key.contains('.') {
            nested_get(payload, &key.split('.').collect::<Vec<_>>())
        } else {
            payload.get(*key)
        };
        match value {
            Some(Value::Bool(value)) => return Some(*value),
            Some(Value::String(value)) if matches!(value.as_str(), "true" | "1") => {
                return Some(true)
            }
            Some(Value::String(value)) if matches!(value.as_str(), "false" | "0") => {
                return Some(false)
            }
            _ => {}
        }
    }
    None
}

fn update_hook_status(state: &mut Map<String, Value>, status: &str) {
    let timestamp = now_iso();
    state.insert("status".to_string(), json!(status));
    state.insert("statusUpdatedAt".to_string(), json!(timestamp.clone()));
    state.insert("lastActivityAt".to_string(), json!(timestamp.clone()));
    if status == "attention" {
        state.insert(
            "attentionEventId".to_string(),
            json!(format!("{timestamp}:attention")),
        );
        state.insert("attentionAcknowledgedAt".to_string(), json!(""));
        state.insert("attentionAcknowledgedEventId".to_string(), json!(""));
    } else if status == "working" {
        state.insert("attentionAcknowledgedAt".to_string(), json!(timestamp));
        let event_id = read_state_string(state, "attentionEventId").unwrap_or_default();
        state.insert("attentionAcknowledgedEventId".to_string(), json!(event_id));
    }
}

fn is_prompt_event(event_name: &str) -> bool {
    let lower = event_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "userpromptsubmit"
            | "beforeagent"
            | "preinvocation"
            | "pretooluse"
            | "beforesubmitprompt"
            | "beforeshellexecution"
            | "pre_llm_call"
            | "pre_tool_call"
            | "on_tool_permission"
            | "agent_start"
            | "agent.start"
            | "before_agent_start"
    )
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
            "pid": unsafe { libc::getppid() },
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

fn post_gxserver_hook_event(
    agent_key: &str,
    session_id: Option<&str>,
    transcript_path: Option<&str>,
    first_user_message: Option<&str>,
    event_name: &str,
    state: &Map<String, Value>,
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

fn parse_global_session_ref(value: &str) -> (Option<String>, Option<String>) {
    let parts = value.trim().split(':').collect::<Vec<_>>();
    if parts.len() == 3 && !parts[1].is_empty() && !parts[2].is_empty() {
        (Some(parts[1].to_string()), Some(parts[2].to_string()))
    } else {
        (None, None)
    }
}

fn decode_base64_text(value: &str) -> String {
    BASE64_STANDARD
        .decode(value)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn normalize_prompt_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn expand_home_path(value: &str) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return dirs_home();
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return dirs_home().join(relative);
    }
    PathBuf::from(trimmed)
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("hook-state");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ))
}

fn insert_json_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn read_hook_status(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Value, DomainStateError> {
    let cli_installed = command_exists(definition.cli_command, &hook_paths.home_dir);
    /*
    CDXC:AgentHooks 2026-06-19-18:43:
    Hook status must report every candidate provider config path and inspect all candidates for Ghostex-owned hooks so profile-only Codex and Claude installs are not misreported as missing.
    Keep first-time install conservative, but status reads should treat any current provider candidate as installed once the shared notify hook is current.
    */
    let provider_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let paths = provider_paths
        .iter()
        .map(|path| path_string(path))
        .collect::<Vec<_>>();
    let notify_current = is_notify_hook_current(&hook_paths.notify_hook_path);
    let inspection = inspect_agent_hook_installation(definition, hook_paths, &provider_paths);
    let provider_current = inspection.current_hook_installed;
    let ghostex_hook_present = inspection.ghostex_hook_present;
    let hook_installed = notify_current && provider_current;
    let status = if !cli_installed {
        "cliMissing"
    } else if hook_installed {
        "installed"
    } else if ghostex_hook_present {
        "updateRequired"
    } else {
        "missing"
    };
    Ok(json!({
        "agentId": definition.agent_id,
        "cliCommand": definition.cli_command,
        "cliInstalled": cli_installed,
        "detail": hook_detail(definition, hook_paths, status, notify_current, paths.first().map(String::as_str)),
        "hookInstalled": hook_installed,
        "paths": paths,
        "status": status,
    }))
}

fn hook_detail(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    status: &str,
    notify_current: bool,
    first_path: Option<&str>,
) -> String {
    let display = display_path(
        first_path.unwrap_or_else(|| {
            hook_paths
                .notify_hook_path
                .to_str()
                .unwrap_or("~/.ghostex/hooks/agent-shell-notify.sh")
        }),
        &hook_paths.home_dir,
    );
    match status {
        "cliMissing" => format!("{} was not found on PATH.", definition.cli_command),
        "installed" => format!("Installed in {display}"),
        "updateRequired" if notify_current => format!("Run Update Hooks to repair {display}"),
        "updateRequired" => format!(
            "Run Update Hooks to update {}",
            display_path(
                &path_string(&hook_paths.notify_hook_path),
                &hook_paths.home_dir
            )
        ),
        _ => format!("Run Install Hooks to write {display}"),
    }
}

fn provider_hook_paths(agent_id: &str, hook_paths: &HookPaths) -> Vec<PathBuf> {
    match agent_id {
        "codex" => {
            let mut paths =
                vec![
                    resolve_config_directory(&hook_paths.home_dir, "CODEX_HOME", ".codex", None)
                        .join("hooks.json"),
                ];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".codex-profiles",
                "hooks.json",
            ));
            paths
        }
        "claude" => {
            let mut paths = vec![hook_paths.home_dir.join(".claude").join("settings.json")];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".claude-profiles",
                "settings.json",
            ));
            paths
        }
        "cursor" => vec![hook_paths.home_dir.join(".cursor").join("hooks.json")],
        "gemini" => vec![hook_paths.home_dir.join(".gemini").join("settings.json")],
        "opencode" => {
            let config_dir = resolve_config_directory(
                &hook_paths.home_dir,
                "OPENCODE_CONFIG_DIR",
                ".config/opencode",
                None,
            );
            vec![
                config_dir.join("plugins").join("ghostex-session.js"),
                config_dir.join("opencode.json"),
            ]
        }
        "amp" => vec![hook_paths
            .home_dir
            .join(".config")
            .join("amp")
            .join("plugins")
            .join("ghostex-session.ts")],
        "pi" => pi_extension_paths(&hook_paths.home_dir),
        "omp" => vec![resolve_omp_agent_directory(&hook_paths.home_dir)
            .join("extensions")
            .join("ghostex-omp-session.ts")],
        "grok" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "GROK_HOME", ".grok/hooks", None)
                    .join("ghostex-session.json"),
            ]
        }
        "antigravity" => vec![hook_paths
            .home_dir
            .join(".gemini")
            .join("config")
            .join("hooks.json")],
        "kiro" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            "KIRO_HOME",
            ".kiro/agents",
            Some("agents"),
        )
        .join("ghostex.json")],
        "copilot" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "COPILOT_HOME", ".copilot", None)
                    .join("config.json"),
            ]
        }
        "droid" => vec![hook_paths.home_dir.join(".factory").join("settings.json")],
        "rovodev" => vec![hook_paths.home_dir.join(".rovodev").join("config.yml")],
        "hermes-agent" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "HERMES_HOME", ".hermes", None)
                    .join("config.yaml"),
            ]
        }
        "codebuddy" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            "CODEBUDDY_CONFIG_DIR",
            ".codebuddy",
            None,
        )
        .join("settings.json")],
        "qoder" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "QODER_CONFIG_DIR", ".qoder", None)
                    .join("settings.json"),
            ]
        }
        _ => Vec::new(),
    }
}

fn uninstall_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return uninstall_opencode_hook(hook_paths);
    }
    let config_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::PluginFile => uninstall_plugin_file_hook(definition, config_paths),
        HookFormat::MarkedYaml => uninstall_marked_yaml_hook(definition, config_paths),
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut removed_paths = Vec::new();
            for config_path in config_paths {
                if remove_json_hook(&config_path, definition, &command)? {
                    removed_paths.push(path_string(&config_path));
                }
            }
            Ok(removed_paths)
        }
        HookFormat::Opencode => Ok(Vec::new()),
    }
}

fn uninstall_plugin_file_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    for config_path in config_paths {
        let text = read_file_text(&config_path);
        if !text_contains_ghostex_owned_hook_command(&text)
            && hook_marker(definition.agent_id)
                .map(|marker| !text.contains(marker))
                .unwrap_or(true)
        {
            continue;
        }
        if remove_file_if_exists(&config_path)? {
            push_unique_path(&mut removed_paths, path_string(&config_path));
        }
    }
    Ok(removed_paths)
}

fn uninstall_marked_yaml_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_path) = config_paths.into_iter().next() else {
        return Ok(Vec::new());
    };
    let begin_marker = format!("# ghostex hooks {} begin", definition.agent_id);
    let end_marker = format!("# ghostex hooks {} end", definition.agent_id);
    let current_text = read_file_text(&config_path);
    let normalized_text = current_text.replace("\r\n", "\n");
    let lines = normalized_text
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let next_lines = without_marked_block(&lines, &begin_marker, &end_marker);
    let next_text = format!("{}\n", next_lines.join("\n").trim_end_matches('\n'));
    if current_text == next_text {
        return Ok(Vec::new());
    }
    fs::write(&config_path, next_text).map_err(io_error)?;
    Ok(vec![path_string(&config_path)])
}

fn uninstall_opencode_hook(hook_paths: &HookPaths) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    uninstall_opencode_hook_paths(plugin_path, config_path)
}

fn uninstall_opencode_hook_paths(
    plugin_path: &Path,
    config_path: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    let config_text = read_file_text(config_path);
    if !config_text.trim().is_empty() {
        let mut data = read_json_object(&config_text);
        if let Some(object) = data.as_object_mut() {
            if let Some(plugins) = object.get_mut("plugin").and_then(Value::as_array_mut) {
                let next_plugins = plugins
                    .iter()
                    .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
                    .cloned()
                    .collect::<Vec<_>>();
                if next_plugins.len() != plugins.len() {
                    *plugins = next_plugins;
                    write_json_file(config_path, &data)?;
                    push_unique_path(&mut removed_paths, path_string(config_path));
                }
            }
        }
    }
    let plugin_text = read_file_text(plugin_path);
    if (plugin_text.contains(OPENCODE_PLUGIN_MARKER)
        || text_contains_ghostex_owned_hook_command(&plugin_text))
        && remove_file_if_exists(plugin_path)?
    {
        push_unique_path(&mut removed_paths, path_string(plugin_path));
    }
    Ok(removed_paths)
}

fn remove_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<bool, DomainStateError> {
    let current_text = read_file_text(config_path);
    if current_text.trim().is_empty() {
        return Ok(false);
    }
    let mut data = read_json_object(&current_text);
    let events = all_hook_events(definition.agent_id);
    let mut changed = false;
    if hook_format(definition.agent_id) == HookFormat::Antigravity {
        if let Some(ghostex) = data.get_mut("ghostex").and_then(Value::as_object_mut) {
            for event_name in events {
                let Some(entries) = ghostex.get_mut(event_name).and_then(Value::as_array_mut)
                else {
                    continue;
                };
                let next_entries = remove_antigravity_entries(entries, command);
                changed = changed || next_entries != *entries;
                *entries = next_entries;
            }
        }
    } else if matches!(
        hook_format(definition.agent_id),
        HookFormat::FlatJson | HookFormat::KiroJson
    ) {
        if let Some(hooks) = data.get_mut("hooks").and_then(Value::as_object_mut) {
            for event_name in events {
                let Some(entries) = hooks.get_mut(event_name).and_then(Value::as_array_mut) else {
                    continue;
                };
                let next_entries = entries
                    .iter()
                    .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
                    .cloned()
                    .collect::<Vec<_>>();
                changed = changed || next_entries.len() != entries.len();
                *entries = next_entries;
            }
        }
    } else if let Some(hooks) = data.get_mut("hooks").and_then(Value::as_object_mut) {
        for event_name in events {
            let Some(groups) = hooks.get_mut(event_name).and_then(Value::as_array_mut) else {
                continue;
            };
            let next_groups = remove_nested_hook_groups(groups, command);
            changed = changed || next_groups != *groups;
            *groups = next_groups;
        }
    }
    if !changed {
        return Ok(false);
    }
    write_json_file(config_path, &data)?;
    Ok(true)
}

fn remove_antigravity_entries(entries: &[Value], command: &str) -> Vec<Value> {
    let mut next_entries = Vec::new();
    for entry in entries {
        if is_ghostex_owned_hook_command(entry, command) {
            continue;
        }
        let Some(object) = entry.as_object() else {
            next_entries.push(entry.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_entries.push(entry.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_entry = object.clone();
            next_entry.insert("hooks".to_string(), Value::Array(next_hooks));
            next_entries.push(Value::Object(next_entry));
        }
    }
    next_entries
}

fn remove_nested_hook_groups(groups: &[Value], command: &str) -> Vec<Value> {
    let mut next_groups = Vec::new();
    for group in groups {
        let Some(object) = group.as_object() else {
            next_groups.push(group.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_groups.push(group.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_group = object.clone();
            next_group.insert("hooks".to_string(), Value::Array(next_hooks));
            next_groups.push(Value::Object(next_group));
        }
    }
    next_groups
}

fn without_marked_block(lines: &[String], begin_marker: &str, end_marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != begin_marker {
            result.push(lines[index].clone());
            index += 1;
            continue;
        }
        while index < lines.len() && lines[index].trim() != end_marker {
            index += 1;
        }
        if index < lines.len() {
            index += 1;
        }
    }
    while result.last().map(|line| line.trim().is_empty()) == Some(true) {
        result.pop();
    }
    result
}

struct HookInspection {
    current_hook_installed: bool,
    ghostex_hook_present: bool,
}

fn inspect_agent_hook_installation(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    config_paths: &[PathBuf],
) -> HookInspection {
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::Opencode => {
            let plugin_text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let config_text = config_paths
                .get(1)
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let current = plugin_text.contains(&current_plugin_marker(OPENCODE_PLUGIN_MARKER))
                && plugin_text.contains(&path_string(&hook_paths.notify_hook_path))
                && config_text.contains(OPENCODE_PLUGIN_SPEC);
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || plugin_text.contains(OPENCODE_PLUGIN_MARKER)
                    || config_text.contains(OPENCODE_PLUGIN_SPEC),
            }
        }
        HookFormat::PluginFile => {
            let marker = hook_marker(definition.agent_id).unwrap_or_default();
            let inspections = config_paths
                .iter()
                .map(|path| {
                    let text = read_file_text(path);
                    let current = !marker.is_empty()
                        && text.contains(&current_plugin_marker(marker))
                        && text.contains(&path_string(&hook_paths.notify_hook_path));
                    HookInspection {
                        current_hook_installed: current,
                        ghostex_hook_present: current
                            || (!marker.is_empty() && text.contains(marker))
                            || text_contains_ghostex_owned_hook_command(&text),
                    }
                })
                .collect::<Vec<_>>();
            HookInspection {
                current_hook_installed: inspections
                    .iter()
                    .any(|inspection| inspection.current_hook_installed),
                ghostex_hook_present: inspections
                    .iter()
                    .any(|inspection| inspection.ghostex_hook_present),
            }
        }
        HookFormat::MarkedYaml => {
            let text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let marker = format!("ghostex hooks {} begin", definition.agent_id);
            let current =
                text.contains(&marker) && text.contains(&path_string(&hook_paths.notify_hook_path));
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || text.contains(&marker)
                    || text_contains_ghostex_owned_hook_command(&text),
            }
        }
        HookFormat::Antigravity => {
            let text = config_paths
                .first()
                .map(|path| read_file_text(path))
                .unwrap_or_default();
            let current = text.contains(&command);
            HookInspection {
                current_hook_installed: current,
                ghostex_hook_present: current
                    || text.contains("\"ghostex\"")
                    || text_contains_ghostex_owned_hook_command(&text),
            }
        }
        HookFormat::FlatJson | HookFormat::KiroJson | HookFormat::NestedJson => {
            let existing_paths = config_paths
                .iter()
                .filter(|path| !read_file_text(path).trim().is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let should_inspect_all =
                matches!(definition.agent_id, "codex" | "claude") && !existing_paths.is_empty();
            let paths_to_check = if should_inspect_all {
                existing_paths
            } else {
                config_paths.iter().take(1).cloned().collect()
            };
            if paths_to_check.is_empty() {
                return HookInspection {
                    current_hook_installed: false,
                    ghostex_hook_present: false,
                };
            }
            let inspections = paths_to_check
                .iter()
                .map(|path| inspect_json_hook_config(path, &command))
                .collect::<Vec<_>>();
            HookInspection {
                current_hook_installed: if matches!(definition.agent_id, "codex" | "claude") {
                    inspections
                        .iter()
                        .all(|inspection| inspection.current_hook_installed)
                } else {
                    inspections
                        .iter()
                        .any(|inspection| inspection.current_hook_installed)
                },
                ghostex_hook_present: inspections
                    .iter()
                    .any(|inspection| inspection.ghostex_hook_present),
            }
        }
    }
}

fn inspect_json_hook_config(config_path: &Path, command: &str) -> HookInspection {
    let data = read_json_object(&read_file_text(config_path));
    HookInspection {
        current_hook_installed: json_contains_hook_command(&data, command),
        ghostex_hook_present: json_contains_ghostex_owned_hook_command(&data, command),
    }
}

fn json_contains_hook_command(value: &Value, command: &str) -> bool {
    if is_hook_command(value, command) {
        return true;
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .any(|item| json_contains_hook_command(item, command));
    }
    if let Some(object) = value.as_object() {
        return object
            .values()
            .any(|item| json_contains_hook_command(item, command));
    }
    false
}

fn json_contains_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    if is_ghostex_owned_hook_command(value, command) {
        return true;
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .any(|item| json_contains_ghostex_owned_hook_command(item, command));
    }
    if let Some(object) = value.as_object() {
        return object
            .values()
            .any(|item| json_contains_ghostex_owned_hook_command(item, command));
    }
    false
}

fn read_json_object(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

fn write_json_file(path: &Path, data: &Value) -> Result<(), DomainStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(data).map_err(json_error)?
    );
    fs::write(path, text).map_err(io_error)
}

fn remove_file_if_exists(path: &Path) -> Result<bool, DomainStateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn install_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return install_opencode_hook(hook_paths);
    }
    let config_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::PluginFile => {
            let Some(config_path) = config_paths.first() else {
                return Ok(Vec::new());
            };
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::write(
                config_path,
                build_plugin_file_source(definition.agent_id, &hook_paths.notify_hook_path),
            )
            .map_err(io_error)?;
            Ok(vec![path_string(config_path)])
        }
        HookFormat::MarkedYaml => {
            let Some(config_path) = config_paths.first() else {
                return Ok(Vec::new());
            };
            install_marked_yaml_hook(config_path, definition.agent_id, &command)?;
            Ok(vec![path_string(config_path)])
        }
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut installed_paths = Vec::new();
            for config_path in config_paths {
                merge_json_hook(&config_path, definition, &command)?;
                installed_paths.push(path_string(&config_path));
            }
            Ok(installed_paths)
        }
        HookFormat::Opencode => Ok(Vec::new()),
    }
}

fn merge_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<(), DomainStateError> {
    let mut data = read_json_object(&read_file_text(config_path));
    let events = all_hook_events(definition.agent_id);
    match hook_format(definition.agent_id) {
        HookFormat::Antigravity => {
            let object = ensure_json_object(&mut data);
            let ghostex = ensure_object_property(object, "ghostex");
            for event_name in events {
                ghostex.insert(
                    event_name.to_string(),
                    Value::Array(vec![antigravity_hook_entry(command, event_name)]),
                );
            }
        }
        HookFormat::FlatJson => {
            let object = ensure_json_object(&mut data);
            object.entry("version".to_string()).or_insert(json!(1));
            let hooks = ensure_object_property(object, "hooks");
            for event_name in events {
                let entries = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                hooks.insert(
                    event_name.to_string(),
                    Value::Array(merge_flat_hook_entries(&entries, command, None)),
                );
            }
        }
        HookFormat::KiroJson => {
            let object = ensure_json_object(&mut data);
            object.entry("name".to_string()).or_insert(json!("ghostex"));
            object
                .entry("description".to_string())
                .or_insert(json!("Ghostex notification hooks for Kiro CLI."));
            object.entry("tools".to_string()).or_insert(json!(["*"]));
            let hooks = ensure_object_property(object, "hooks");
            for event_name in events {
                let entries = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                hooks.insert(
                    event_name.to_string(),
                    Value::Array(merge_flat_hook_entries(
                        &entries,
                        command,
                        Some(json!({ "timeout_ms": 5000 })),
                    )),
                );
            }
        }
        HookFormat::NestedJson => {
            let object = ensure_json_object(&mut data);
            let hooks = ensure_object_property(object, "hooks");
            for event_name in events {
                let groups = hooks
                    .get(event_name)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut next_groups = remove_nested_hook_groups(&groups, command);
                if !next_groups
                    .iter()
                    .any(|group| group_contains_hook_command(group, command))
                {
                    let mut group = Map::new();
                    let mut hook = Map::new();
                    hook.insert("type".to_string(), json!("command"));
                    hook.insert("command".to_string(), json!(command));
                    if command_agent(definition.agent_id).is_some()
                        || nested_timeout(definition.agent_id).is_some()
                    {
                        hook.insert(
                            "timeout".to_string(),
                            json!(nested_timeout(definition.agent_id).unwrap_or(5000)),
                        );
                    }
                    group.insert("hooks".to_string(), Value::Array(vec![Value::Object(hook)]));
                    if definition.agent_id == "claude" {
                        group.insert("matcher".to_string(), json!("*"));
                    }
                    next_groups.push(Value::Object(group));
                }
                hooks.insert(event_name.to_string(), Value::Array(next_groups));
            }
        }
        HookFormat::Opencode | HookFormat::PluginFile | HookFormat::MarkedYaml => {}
    }
    write_json_file(config_path, &data)
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("json object")
}

fn ensure_object_property<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    if !object.get(key).map(Value::is_object).unwrap_or(false) {
        object.insert(key.to_string(), json!({}));
    }
    object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("object property")
}

fn antigravity_hook_entry(command: &str, event_name: &str) -> Value {
    let hook = json!({ "type": "command", "command": command, "timeout": 10 });
    if matches!(event_name, "PreToolUse" | "PostToolUse") {
        json!({ "matcher": "*", "hooks": [hook] })
    } else {
        hook
    }
}

fn merge_flat_hook_entries(entries: &[Value], command: &str, extra: Option<Value>) -> Vec<Value> {
    let mut next = entries
        .iter()
        .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
        .cloned()
        .collect::<Vec<_>>();
    let mut entry = Map::new();
    entry.insert("command".to_string(), json!(command));
    if let Some(extra) = extra.and_then(|value| value.as_object().cloned()) {
        for (key, value) in extra {
            entry.insert(key, value);
        }
    }
    next.push(Value::Object(entry));
    next
}

fn group_contains_hook_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| hooks.iter().any(|hook| is_hook_command(hook, command)))
        .unwrap_or(false)
}

fn is_hook_command(value: &Value, command: &str) -> bool {
    value.get("command").and_then(Value::as_str) == Some(command)
}

fn install_marked_yaml_hook(
    config_path: &Path,
    agent_id: &str,
    command: &str,
) -> Result<(), DomainStateError> {
    let begin_marker = format!("# ghostex hooks {agent_id} begin");
    let end_marker = format!("# ghostex hooks {agent_id} end");
    let current_text = read_file_text(config_path);
    let current_lines = current_text
        .replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut lines = without_marked_block(&current_lines, &begin_marker, &end_marker);
    if lines.last().map(|line| !line.trim().is_empty()) == Some(true) {
        lines.push(String::new());
    }
    if agent_id == "hermes-agent" {
        let shell_command = format!("sh -c {}", shell_quote(command));
        lines.extend([
            begin_marker.clone(),
            "hooks:".to_string(),
            "  on_session_start:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_llm_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  post_llm_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_approval_request:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  post_approval_response:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_end:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_finalize:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  on_session_reset:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 5".to_string(),
            "  pre_tool_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 120".to_string(),
            "  post_tool_call:".to_string(),
            format!("    - command: {}", yaml_double_quote(&shell_command)),
            "      timeout: 120".to_string(),
            end_marker,
        ]);
    } else {
        lines.extend([
            begin_marker.clone(),
            "eventHooks:".to_string(),
            "  events:".to_string(),
            "    - name: on_complete".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            "    - name: on_error".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            "    - name: on_tool_permission".to_string(),
            "      commands:".to_string(),
            format!("        - command: {}", yaml_double_quote(command)),
            end_marker,
        ]);
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(
        config_path,
        format!("{}\n", lines.join("\n").trim_end_matches('\n')),
    )
    .map_err(io_error)
}

fn install_opencode_hook(hook_paths: &HookPaths) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    if let Some(parent) = plugin_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(
        plugin_path,
        build_opencode_plugin_source(&hook_paths.notify_hook_path),
    )
    .map_err(io_error)?;
    update_opencode_config_plugin_registration(config_path)?;
    Ok(vec![path_string(plugin_path)])
}

fn update_opencode_config_plugin_registration(config_path: &Path) -> Result<(), DomainStateError> {
    let mut data = read_json_object(&read_file_text(config_path));
    let object = ensure_json_object(&mut data);
    let plugins = object
        .get("plugin")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut next_plugins = plugins
        .into_iter()
        .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
        .collect::<Vec<_>>();
    next_plugins.push(json!(OPENCODE_PLUGIN_SPEC));
    object.insert("plugin".to_string(), Value::Array(next_plugins));
    write_json_file(config_path, &data)
}

fn install_notify_hook(hook_paths: &HookPaths) -> Result<(), DomainStateError> {
    if let Some(parent) = hook_paths.notify_hook_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::create_dir_all(&hook_paths.hook_state_directory).map_err(io_error)?;
    let executable = env::current_exe()
        .ok()
        .map(|path| path_string(&path))
        .unwrap_or_else(|| "gxserver".to_string());
    let script = build_notify_hook_script(&executable);
    write_executable_notify_hook(&hook_paths.notify_hook_path, &script)?;
    Ok(())
}

fn build_notify_hook_script(executable: &str) -> String {
    format!(
        r#"#!/bin/bash
# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}
if [ -n "${{1:-}}" ]; then
  INPUT_ARG="$1"
else
  INPUT_ARG=""
  IFS= read -r -t 1 INPUT_ARG || true
fi

SESSION_STATE_FILE="${{VSMUX_SESSION_STATE_FILE:-${{GHOSTEX_SESSION_STATE_FILE:-$ghostex_SESSION_STATE_FILE}}}}"
HOOK_STATE_DIR="${{GHOSTEX_AGENT_HOOK_STATE_DIR:-$HOME/.ghostexterm}}"
if [ "${{GHOSTEX_INTERNAL_PROMPT_GENERATION:-}}" = "1" ] || [ "${{GHOSTEX_INTERNAL_TITLE_GENERATION:-}}" = "1" ]; then
  printf '{{"continue":true}}'
  exit 0
fi
if [ -z "$SESSION_STATE_FILE" ] && {{ [ -z "${{GHOSTEX_GLOBAL_SESSION_REF:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_BASE_URL:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_AUTH_TOKEN_FILE:-}}" ]; }}; then
  printf '{{"continue":true}}'
  exit 0
fi

{executable} agent-hook-notify "$SESSION_STATE_FILE" "$INPUT_ARG" "$HOOK_STATE_DIR" >/dev/null 2>/dev/null || true
printf '{{"continue":true}}'
exit 0
"#,
        executable = shell_quote(executable)
    )
}

fn write_executable_notify_hook(path: &Path, contents: &str) -> Result<(), DomainStateError> {
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, contents).map_err(io_error)?;
    fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(&temp_path);
    fs::rename(&temp_path, path).map_err(io_error)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
    remove_macos_notify_hook_execution_attributes(path);
    Ok(())
}

fn remove_macos_notify_hook_execution_attributes(path: &Path) {
    if std::env::consts::OS != "macos" {
        return;
    }
    for attribute in ["com.apple.quarantine", "com.apple.provenance"] {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .arg("-d")
            .arg(attribute)
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn is_notify_hook_current(path: &Path) -> bool {
    /*
    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 parity requires Rust status/install/uninstall to treat the TypeScript gxserver v6 hook marker as the shared notify-hook currency contract. Do not require Rust-only helper text here; existing gxserver-owned v6 hooks should stay installed instead of forcing a needless updateRequired state.
    */
    read_file_text(path).contains(&format!("{NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}"))
}

fn hook_format(agent_id: &str) -> HookFormat {
    match agent_id {
        "antigravity" => HookFormat::Antigravity,
        "cursor" => HookFormat::FlatJson,
        "kiro" => HookFormat::KiroJson,
        "rovodev" | "hermes-agent" => HookFormat::MarkedYaml,
        "opencode" => HookFormat::Opencode,
        "amp" | "omp" | "pi" => HookFormat::PluginFile,
        _ => HookFormat::NestedJson,
    }
}

fn hook_marker(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some(AMP_PLUGIN_MARKER),
        "omp" => Some(OMP_EXTENSION_MARKER),
        "pi" => Some(PI_EXTENSION_MARKER),
        "opencode" => Some(OPENCODE_PLUGIN_MARKER),
        _ => None,
    }
}

fn command_agent(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("claude"),
        "cursor" => Some("cursor"),
        "gemini" => Some("gemini"),
        "kiro" => Some("kiro"),
        "copilot" => Some("copilot"),
        "droid" => Some("factory"),
        "grok" => Some("grok"),
        "antigravity" => Some("antigravity"),
        "amp" => Some("amp"),
        "omp" => Some("omp"),
        "pi" => Some("pi"),
        "rovodev" => Some("rovodev"),
        "hermes-agent" => Some("hermes-agent"),
        "codebuddy" => Some("codebuddy"),
        "qoder" => Some("qoder"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

fn nested_timeout(agent_id: &str) -> Option<i64> {
    match agent_id {
        "codex" | "grok" => Some(5),
        "gemini" => Some(10000),
        _ => None,
    }
}

fn all_hook_events(agent_id: &str) -> Vec<&'static str> {
    let events: &[&str] = match agent_id {
        "codex" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "PreToolUse",
            "PermissionRequest",
        ],
        "claude" => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Stop",
            "Notification",
            "SessionEnd",
        ],
        "cursor" => &[
            "beforeSubmitPrompt",
            "stop",
            "afterAgentResponse",
            "beforeShellExecution",
            "afterShellExecution",
        ],
        "gemini" => &[
            "SessionStart",
            "BeforeAgent",
            "AfterAgent",
            "SessionEnd",
            "PreToolUse",
        ],
        "kiro" => &[
            "agentSpawn",
            "userPromptSubmit",
            "stop",
            "preToolUse",
            "postToolUse",
        ],
        "copilot" | "droid" | "codebuddy" => &[
            "SessionStart",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "grok" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "antigravity" => &[
            "SessionStart",
            "PreInvocation",
            "Stop",
            "turn-completion",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
        ],
        "qoder" => &["SessionStart", "Stop", "SessionEnd", "PreToolUse"],
        _ => &[],
    };
    let mut output = Vec::new();
    for event in events {
        if !output.contains(event) {
            output.push(*event);
        }
    }
    output
}

fn build_plugin_file_source(agent_id: &str, notify_hook_path: &Path) -> String {
    /*
    CDXC:AgentHooks 2026-06-21-19:26:
    Plugin-file agents must keep the same provider-specific hook scripts as TypeScript gxserver, including launch argv metadata, provider disable flags, transcript fields, and first-prompt payload capture. Shared generic hooks lose restore information and make the Rust hook installer report parity while silently weakening sleep/wake.
    */
    match agent_id {
        "amp" => build_amp_plugin_source(notify_hook_path),
        "omp" => build_omp_extension_source(notify_hook_path),
        "pi" => build_pi_extension_source(notify_hook_path),
        _ => build_pi_extension_source(notify_hook_path),
    }
}

fn build_opencode_plugin_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const PLUGIN_INSTALLED_KEY = Symbol.for("ghostex.session.restore.plugin.installed");

function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function eventProperties(event) {
  return (event && typeof event === "object" && event.properties) || {};
}

function sessionIdFor(event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.id,
    props.sessionID,
    props.sessionId,
    props.session_id,
    props.session && props.session.id,
    event && event.sessionID,
    event && event.sessionId,
    event && event.id
  );
}

function cwdFor(ctx, event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.directory,
    props.cwd,
    props.directory,
    ctx && ctx.directory,
    process.cwd()
  );
}

function resolveExecutable(name) {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeOpenCodeScript(value) {
  if (!value) return false;
  const lower = String(value).toLowerCase();
  return lower.includes("opencode") || lower.includes("open-code");
}

function isOpenCodeInternalWorkerArg(value) {
  if (!value) return false;
  const normalized = String(value).replaceAll("\\", "/");
  return normalized.includes("/$bunfs/") && normalized.includes("/src/cli/cmd/tui/worker.js");
}

function withoutOpenCodeInternalWorkerArgs(argv) {
  const result = [];
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (i > 0 && isOpenCodeInternalWorkerArg(value)) continue;
    result.push(value);
  }
  return result.length > 0 ? result : [resolveExecutable("opencode")];
}

function normalizedLaunchArgv() {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("opencode")];

  const firstBase = path.basename(raw[0]).toLowerCase();
  if (looksLikeOpenCodeScript(firstBase)) return withoutOpenCodeInternalWorkerArgs(raw);

  let tail = raw.slice(1);
  if (tail.length > 0 && looksLikeOpenCodeScript(tail[0])) {
    tail = tail.slice(1);
  }
  return withoutOpenCodeInternalWorkerArgs([resolveExecutable("opencode"), ...tail]);
}

function base64NulSeparated(values) {
  const bytes = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd) {
  const env = { ...process.env, GHOSTEX_AGENT: "opencode" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "opencode";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("opencode");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function hookEventName(subcommand) {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "stop":
      return "Stop";
    case "session-end":
      return "SessionEnd";
    default:
      return subcommand;
  }
}

function sendHook(subcommand, ctx, event, extra = {}) {
  if (process.env.GHOSTEX_OPENCODE_HOOKS_DISABLED === "1") return;
  const sessionId = sessionIdFor(event);
  if (!sessionId) return;
  const cwd = cwdFor(ctx, event);
  const eventName = hookEventName(subcommand);
  const payload = {
    agent: "opencode",
    cwd,
    event: eventName,
    hook_event_name: eventName,
    session_id: sessionId,
    ...extra,
  };
  try {
    spawnSync(__NOTIFY_HOOK_PATH_JSON__, [], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

function handleEvent(ctx, event) {
  const props = eventProperties(event);
  switch (event && event.type) {
    case "session.created":
      sendHook("session-start", ctx, event);
      break;
    case "session.updated":
      if (props.info && props.info.time && props.info.time.archived) {
        sendHook("session-end", ctx, event);
      } else {
        sendHook("session-start", ctx, event);
      }
      break;
    case "session.status":
      if (props.status && props.status.type === "idle") {
        sendHook("stop", ctx, event);
      }
      break;
    case "session.idle":
      sendHook("stop", ctx, event);
      break;
    case "session.deleted":
      sendHook("session-end", ctx, event);
      break;
    default:
      break;
  }
}

const GhostexSessionRestore = async (ctx) => {
  if (globalThis[PLUGIN_INSTALLED_KEY]) return {};
  globalThis[PLUGIN_INSTALLED_KEY] = true;
  const bus = ctx && (ctx.bus || ctx.events || ctx.event);
  const on = bus && typeof bus.on === "function" ? bus.on.bind(bus) : ctx && typeof ctx.on === "function" ? ctx.on.bind(ctx) : null;
  if (on) {
    for (const eventName of ["session.created", "session.updated", "session.status", "session.idle", "session.deleted"]) {
      on(eventName, (event) => handleEvent(ctx, { ...event, type: event && event.type ? event.type : eventName }));
    }
    return {};
  }

  return {
    event: async ({ event }) => {
      handleEvent(ctx, event);
    },
  };
};

export { GhostexSessionRestore };
export default GhostexSessionRestore;
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(OPENCODE_PLUGIN_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_amp_plugin_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type {
  PluginAPI,
  AgentEndEvent,
  AgentStartEvent,
  SessionStartEvent,
  ToolCallEvent,
} from "@ampcode/plugin";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeAmpExecutable(value: string): boolean {
  return path.basename(value).toLowerCase() === "amp";
}

function looksLikeAmpScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/");
  const base = path.basename(normalized).toLowerCase();
  return normalized.includes("/@ampcode/") || (base === "cli.js" && normalized.includes("amp"));
}

function looksLikeJavaScriptRuntime(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "node" || base === "bun" || base === "deno" || base === "tsx" || base === "ts-node";
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("amp")];
  if (looksLikeAmpExecutable(raw[0])) return raw;
  if (raw.length > 1 && (looksLikeAmpScript(raw[1]) || looksLikeJavaScriptRuntime(raw[0]))) {
    return [resolveExecutable("amp"), ...raw.slice(2)];
  }
  return [resolveExecutable("amp")];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "amp" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "amp";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("amp");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function threadIdFrom(event: { thread?: { id?: string } } | undefined, ctx?: { thread?: { id?: string } }): string | null {
  return firstString(event?.thread?.id, ctx?.thread?.id);
}

function sendHook(
  eventName: string,
  sessionId: string | null,
  cwd: string,
  extra: Record<string, unknown> = {},
): void {
  if (process.env.GHOSTEX_AMP_HOOKS_DISABLED === "1") return;
  if (!sessionId) return;
  const payload: Record<string, unknown> = {
    agent: "amp",
    cwd,
    event: eventName,
    hook_event_name: eventName,
    session_id: sessionId,
    ...extra,
  };
  try {
    const child = spawn(__NOTIFY_HOOK_PATH_JSON__, [], {
      stdio: ["pipe", "ignore", "ignore"],
      env: hookEnvironment(cwd),
      detached: true,
    });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(payload));
    child.unref();
  } catch (_) {}
}

export default function ghostexAmpSessionPlugin(amp: PluginAPI) {
  const cwdFromEnv = (): string => firstString(process.env.PWD, process.cwd()) || process.cwd();

  amp.on("session.start", async (event: SessionStartEvent, ctx) => {
    sendHook("SessionStart", threadIdFrom(event, ctx), cwdFromEnv());
  });

  amp.on("agent.start", async (event: AgentStartEvent, ctx) => {
    sendHook("UserPromptSubmit", threadIdFrom(event, ctx), cwdFromEnv());
  });

  amp.on("tool.call", async (event: ToolCallEvent, ctx) => {
    sendHook("PreToolUse", threadIdFrom(undefined, ctx), cwdFromEnv(), { tool: event.tool });
    return { action: "allow" as const };
  });

  amp.on("agent.end", async (event: AgentEndEvent, ctx) => {
    sendHook("Stop", threadIdFrom(event, ctx), cwdFromEnv(), { status: event.status });
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(AMP_PLUGIN_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_pi_extension_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { AgentEndEvent, ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikePiExecutable(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "pi" || base === "pi-coding-agent";
}

function looksLikePiScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/");
  const base = path.basename(normalized).toLowerCase();
  return (
    normalized.includes("/@mariozechner/pi-coding-agent/") ||
    normalized.includes("/packages/coding-agent/") ||
    (base === "cli.js" && normalized.includes("pi-coding-agent")) ||
    (base === "cli.ts" && normalized.includes("coding-agent"))
  );
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("pi")];
  if (looksLikePiExecutable(raw[0])) return raw;
  if (raw.length > 1 && looksLikePiScript(raw[1])) {
    return [resolveExecutable("pi"), ...raw.slice(2)];
  }
  return [resolveExecutable("pi"), ...raw.slice(1)];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "pi" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "pi";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("pi");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function eventName(subcommand: string): string {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "prompt-submit":
      return "UserPromptSubmit";
    case "stop":
      return "Stop";
    default:
      return subcommand;
  }
}

function textFromContent(content: unknown): string | null {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return null;
  const parts: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const typed = block as { type?: unknown; text?: unknown };
    if (typed.type === "text" && typeof typed.text === "string") parts.push(typed.text);
  }
  return parts.join("\n") || null;
}

function lastAssistantMessage(event: AgentEndEvent): string | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (!message || typeof message !== "object") continue;
    const typed = message as { role?: unknown; content?: unknown };
    if (typed.role !== "assistant") continue;
    const text = firstString(textFromContent(typed.content));
    if (text) return text;
  }
  return undefined;
}

function sendHook(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}): void {
  if (process.env.GHOSTEX_PI_HOOKS_DISABLED === "1") return;

  const sessionId = firstString(ctx.sessionManager.getSessionId());
  if (!sessionId) return;

  const cwd = firstString(ctx.cwd, process.cwd()) || process.cwd();
  const event = eventName(subcommand);
  const payload: Record<string, unknown> = {
    agent: "pi",
    session_id: sessionId,
    cwd,
    hook_event_name: event,
    event,
    transcript_path: ctx.sessionManager.getSessionFile() || undefined,
    ...extra,
  };
  try {
    spawnSync(__NOTIFY_HOOK_PATH_JSON__, [], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

export default function ghostexPiSessionExtension(pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => {
    sendHook("session-start", ctx);
  });

  pi.on("before_agent_start", async (event, ctx) => {
    sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  pi.on("agent_end", async (event, ctx) => {
    sendHook("stop", ctx, { last_assistant_message: lastAssistantMessage(event) });
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(PI_EXTENSION_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_omp_extension_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { AgentEndEvent, ExtensionAPI, ExtensionContext } from "@oh-my-pi/pi-coding-agent";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.statSync(candidate).isFile()) return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeOmpExecutable(value: string): boolean {
  return path.basename(value).toLowerCase() === "omp";
}

function looksLikeOmpScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/").toLowerCase();
  const base = path.basename(normalized);
  return (
    normalized.includes("/@oh-my-pi/pi-coding-agent/") ||
    normalized.includes("/oh-my-pi/") ||
    ((base === "cli.js" || base === "cli.ts") && normalized.includes("pi-coding-agent"))
  );
}

function looksLikeJavaScriptRuntime(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "node" || base === "bun" || base === "deno" || base === "tsx" || base === "ts-node";
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("omp")];
  if (looksLikeOmpExecutable(raw[0])) return raw;
  if (raw.length > 1 && (looksLikeOmpScript(raw[1]) || looksLikeJavaScriptRuntime(raw[0]))) {
    return [resolveExecutable("omp"), ...raw.slice(2)];
  }
  return [resolveExecutable("omp"), ...raw.slice(1)];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "omp" };
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "omp";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("omp");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function eventName(subcommand: string): string {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "prompt-submit":
      return "UserPromptSubmit";
    case "stop":
      return "Stop";
    default:
      return subcommand;
  }
}

function textFromContent(content: unknown): string | null {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return null;
  const parts: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const typed = block as { type?: unknown; text?: unknown };
    if (typed.type === "text" && typeof typed.text === "string") parts.push(typed.text);
  }
  return parts.join("\n") || null;
}

function lastAssistantMessage(event: AgentEndEvent): string | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (!message || typeof message !== "object") continue;
    const typed = message as { role?: unknown; content?: unknown };
    if (typed.role !== "assistant") continue;
    const text = firstString(textFromContent(typed.content));
    if (text) return text;
  }
  return undefined;
}

function hookInvocation(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}) {
  if (process.env.GHOSTEX_OMP_HOOKS_DISABLED === "1") return null;

  const sessionId = firstString(ctx.sessionManager.getSessionId());
  if (!sessionId) return null;

  const cwd = firstString(ctx.cwd, process.cwd()) || process.cwd();
  const event = eventName(subcommand);
  const payload: Record<string, unknown> = {
    agent: "omp",
    session_id: sessionId,
    cwd,
    hook_event_name: event,
    event,
    ...extra,
  };
  return {
    cwd,
    payload: JSON.stringify(payload),
    env: hookEnvironment(cwd),
  };
}

async function sendHook(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}): Promise<void> {
  const invocation = hookInvocation(subcommand, ctx, extra);
  if (!invocation) return;
  await new Promise<void>((resolve) => {
    let settled = false;
    const settle = () => {
      if (settled) return;
      settled = true;
      resolve();
    };
    try {
      const child = spawn(__NOTIFY_HOOK_PATH_JSON__, [], {
        env: invocation.env,
        stdio: ["pipe", "ignore", "ignore"],
        detached: true,
      });
      child.on("error", settle);
      child.stdin.on("error", settle);
      child.stdin.on("finish", settle);
      child.unref();
      child.stdin.end(invocation.payload);
    } catch (_) {
      settle();
    }
  });
}

export default function ghostexOmpSessionExtension(api: ExtensionAPI) {
  api.on("session_start", async (_event, ctx) => {
    await sendHook("session-start", ctx);
  });

  api.on("before_agent_start", async (event, ctx) => {
    await sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  api.on("agent_end", async (event, ctx) => {
    await sendHook("stop", ctx, { last_assistant_message: lastAssistantMessage(event) });
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(OMP_EXTENSION_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn current_plugin_marker(marker: &str) -> String {
    if matches!(
        marker,
        OPENCODE_PLUGIN_MARKER | AMP_PLUGIN_MARKER | PI_EXTENSION_MARKER
    ) {
        format!("{marker} v3")
    } else if marker == OMP_EXTENSION_MARKER {
        format!("{marker} v1")
    } else {
        format!("{marker} v2")
    }
}

fn command_for_agent(definition: &HookDefinition, notify_hook_path: &Path) -> String {
    let notify_hook_path = path_string(notify_hook_path);
    match command_agent(definition.agent_id) {
        Some(agent) => format!(
            "GHOSTEX_AGENT={} {}",
            shell_quote(agent),
            shell_quote(&notify_hook_path)
        ),
        None => notify_hook_path,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn yaml_double_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn is_opencode_session_plugin_registration(value: &Value) -> bool {
    let plugin = value
        .as_str()
        .or_else(|| value.as_array().and_then(|items| items.first()?.as_str()))
        .unwrap_or_default();
    plugin == OPENCODE_PLUGIN_SPEC
        || plugin == "ghostex-session"
        || plugin == "./plugins/ghostex-session.js"
        || plugin.ends_with("/plugins/ghostex-session.js")
        || plugin.ends_with("/ghostex-session.js")
}

fn is_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    let Some(command_value) = value.get("command").and_then(Value::as_str) else {
        return false;
    };
    command_value == command || text_contains_ghostex_owned_hook_command(command_value)
}

fn text_contains_ghostex_owned_hook_command(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("agent-shell-notify")
        || normalized.contains(".ghostex/hooks")
        || normalized.contains(".ghostexterm")
        || normalized.contains("ghostex_notify_hook")
        || normalized.contains("ghostex-agent-notify")
        || normalized.contains(AMP_PLUGIN_MARKER)
        || normalized.contains(OMP_EXTENSION_MARKER)
        || normalized.contains(PI_EXTENSION_MARKER)
        || normalized.contains(OPENCODE_PLUGIN_MARKER)
        || normalized.contains("ghostex-session-plugin-marker")
        || normalized.contains("ghostex-session-extension-marker")
}

fn normalize_agent_ids(value: Option<&Value>) -> Vec<String> {
    let requested = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(normalize_requested_agent_id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            HOOK_DEFINITIONS
                .iter()
                .map(|definition| definition.agent_id.to_string())
                .collect()
        });
    let mut output = Vec::new();
    for agent_id in requested {
        if HOOK_DEFINITIONS
            .iter()
            .any(|definition| definition.agent_id == agent_id)
            && !output.contains(&agent_id)
        {
            output.push(agent_id);
        }
    }
    if output.is_empty() {
        HOOK_DEFINITIONS
            .iter()
            .map(|definition| definition.agent_id.to_string())
            .collect()
    } else {
        output
    }
}

fn normalize_requested_agent_id(value: &Value) -> Option<String> {
    let normalized = value
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mapped = match normalized.as_str() {
        "agy" | "antigravity cli" => "antigravity",
        "claude code" => "claude",
        "code buddy" => "codebuddy",
        "codex cli" => "codex",
        "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "factory" | "factory droid" => "droid",
        "gemini cli" => "gemini",
        "github copilot" => "copilot",
        "kiro cli" | "kiro-cli" => "kiro",
        "open code" => "opencode",
        "qodercli" => "qoder",
        "rovo" | "rovo dev" => "rovodev",
        other => other,
    };
    (!mapped.is_empty()).then_some(mapped.to_string())
}

fn resolve_config_directory(
    home_dir: &Path,
    env_key: &str,
    fallback_relative_path: &str,
    env_subpath: Option<&str>,
) -> PathBuf {
    match normalize_environment_path(std::env::var(env_key).ok().as_deref(), home_dir) {
        Some(path) => env_subpath
            .map(|subpath| path.join(subpath))
            .unwrap_or(path),
        None => home_dir.join(fallback_relative_path),
    }
}

fn resolve_omp_agent_directory(home_dir: &Path) -> PathBuf {
    if let Some(pi_agent_root) = normalize_environment_path(
        std::env::var("PI_CODING_AGENT_DIR").ok().as_deref(),
        home_dir,
    ) {
        return pi_agent_root;
    }
    let config_dir =
        normalize_environment_path(std::env::var("PI_CONFIG_DIR").ok().as_deref(), home_dir)
            .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

fn pi_extension_paths(home_dir: &Path) -> Vec<PathBuf> {
    /*
    CDXC:AgentHooks 2026-06-23-05:09:
    Pi's active extension loader uses the Pi root extensions directory, while
    older Ghostex installs and local customization experiments can leave hooks
    under the agent directory. Treat the root extension file as canonical for
    new installs, but keep inspecting the previous agent-scoped locations so
    existing current hooks do not warn and stale hooks report updateRequired.
    */
    let agent_dir = resolve_config_directory(home_dir, "PI_CODING_AGENT_DIR", ".pi/agent", None);
    let root_dir = agent_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home_dir.join(".pi"));
    unique_path_bufs(vec![
        root_dir.join("extensions").join("ghostex-session.ts"),
        agent_dir.join("extensions").join("ghostex-session.ts"),
        agent_dir
            .join("extensions")
            .join("ghostex-session")
            .join("index.ts"),
    ])
}

fn unique_path_bufs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut output = Vec::new();
    for path in paths {
        if !output.contains(&path) {
            output.push(path);
        }
    }
    output
}

fn normalize_environment_path(value: Option<&str>, home_dir: &Path) -> Option<PathBuf> {
    let trimmed = value.map(str::trim).filter(|value| !value.is_empty())?;
    if trimmed == "~" {
        return Some(home_dir.to_path_buf());
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return Some(home_dir.join(relative));
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(home_dir.join(path))
    }
}

fn list_profile_hook_paths(home_dir: &Path, profile_dir: &str, file_name: &str) -> Vec<PathBuf> {
    let profiles_path = home_dir.join(profile_dir);
    let Ok(entries) = fs::read_dir(&profiles_path) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_dir())
                .map(|_| entry.path().join(file_name))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn command_exists(command: &str, home_dir: &Path) -> bool {
    resolve_command_path(command, home_dir).is_some()
}

fn resolve_command_path(command: &str, home_dir: &Path) -> Option<String> {
    /*
    CDXC:AgentHooks 2026-06-23-07:52:
    Hook status must discover the same agent CLIs on macOS and Ubuntu before reporting cliMissing. Merge login-shell PATH entries, GUI/default tool directories, and user PATH, then run the final command-v probe in the platform shell so startup files cannot overwrite the normalized PATH and Linux does not require zsh.
    */
    let path_value =
        normalize_gxserver_process_path(std::env::var("PATH").ok().as_deref(), home_dir);
    let shell = command_shell();
    let mut command_process = Command::new(&shell.executable);
    command_process.args(shell.script_args(&format!("command -v {}", shell_quote(command))));
    apply_hook_command_environment(&mut command_process, home_dir);
    command_process.env("PATH", path_value);
    let stdout = run_command_stdout_with_timeout(command_process, Duration::from_millis(2_000))?;
    stdout
        .trim()
        .split('\n')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_gxserver_process_path(current_path: Option<&str>, home_dir: &Path) -> String {
    let mut entries = Vec::new();
    entries.extend(discover_login_shell_path_entries(home_dir));
    entries.extend(split_path(current_path));
    entries.extend([
        path_string(&home_dir.join(".opencode").join("bin")),
        path_string(
            &home_dir
                .join(".local")
                .join("share")
                .join("mise")
                .join("shims"),
        ),
        path_string(&home_dir.join(".local").join("bin")),
        path_string(&home_dir.join(".asdf").join("shims")),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ]);
    unique_path_entries(entries).join(":")
}

fn discover_login_shell_path_entries(home_dir: &Path) -> Vec<String> {
    for candidate in login_shell_candidates() {
        let candidate_path = PathBuf::from(&candidate);
        if !is_executable(&candidate_path) {
            continue;
        }
        let entries = run_login_shell_path_probe(&candidate, home_dir);
        if !entries.is_empty() {
            return entries;
        }
    }
    Vec::new()
}

fn run_login_shell_path_probe(shell_path: &str, home_dir: &Path) -> Vec<String> {
    let shell = command_shell_for_path(shell_path);
    let mut command = Command::new(&shell.executable);
    command.args(
        shell.interactive_script_args(&format!("printf '\\n{SHELL_PATH_SENTINEL}%s\\n' \"$PATH\"")),
    );
    apply_hook_command_environment(&mut command, home_dir);
    let Some(stdout) = run_command_stdout_with_timeout(command, Duration::from_millis(2_000))
    else {
        return Vec::new();
    };
    stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(SHELL_PATH_SENTINEL))
        .map(|path| split_path(Some(path)))
        .unwrap_or_default()
}

fn apply_hook_command_environment(command: &mut Command, home_dir: &Path) {
    command.env("HOME", home_dir);
    for key in GXSERVER_AGENT_HOOK_COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
}

fn run_command_stdout_with_timeout(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut stdout = String::new();
                if let Some(mut child_stdout) = child.stdout.take() {
                    let _ = child_stdout.read_to_string(&mut stdout);
                }
                let _ = child.wait();
                return Some(stdout);
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn split_path(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn unique_path_entries(entries: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    let mut output = Vec::new();
    for entry in entries {
        if entry.is_empty() || seen.contains(&entry) {
            continue;
        }
        seen.push(entry.clone());
        output.push(entry);
    }
    output
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn read_file_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn display_path(path: &str, home_dir: &Path) -> String {
    let home = path_string(home_dir);
    path.strip_prefix(&format!("{home}/"))
        .map(|relative| format!("~/{relative}"))
        .unwrap_or_else(|| path.to_string())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn push_unique_path(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn json_error(error: serde_json::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook JSON operation failed: {error}"),
    }
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook file operation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn hook_status_uses_home_scoped_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let status = read_agent_hook_status(
            &paths,
            json!({ "agentIds": ["qoder"], "autoUpgradeInstalled": false })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        assert_eq!(status.get("type"), Some(&json!("agentHookStatus")));
        assert!(status
            .get("notifyHookPath")
            .and_then(Value::as_str)
            .expect("notify path")
            .starts_with(temp.path().to_str().expect("temp path")));
    }

    #[test]
    fn hook_status_reports_profile_only_provider_hook_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let hook_paths = HookPaths::new(paths.home_dir.clone());
        write_test_executable(&temp.path().join(".local").join("bin").join("claude"));
        install_notify_hook(&hook_paths).expect("notify hook");
        let profile_path = temp
            .path()
            .join(".claude-profiles")
            .join("work")
            .join("settings.json");
        let claude = HookDefinition {
            agent_id: "claude",
            cli_command: "claude",
        };
        let command = command_for_agent(&claude, &hook_paths.notify_hook_path);
        write_test_file(
            &profile_path,
            &format!(
                "{}\n",
                json!({
                    "hooks": {
                        "SessionStart": [
                            { "hooks": [{ "type": "command", "command": command }] }
                        ]
                    }
                })
            ),
        );
        let expected_paths = provider_hook_paths("claude", &hook_paths)
            .iter()
            .map(|path| path_string(path))
            .collect::<Vec<_>>();
        assert!(json_contains_hook_command(
            &read_json_object(&read_file_text(&profile_path)),
            &command
        ));
        let claude_paths = provider_hook_paths("claude", &hook_paths);
        let inspection = inspect_agent_hook_installation(&claude, &hook_paths, &claude_paths);
        assert!(is_notify_hook_current(&hook_paths.notify_hook_path));
        assert!(inspection.current_hook_installed);

        let status = read_agent_hook_status(
            &paths,
            json!({ "agentIds": ["claude"], "autoUpgradeInstalled": false })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let row = status
            .get("agents")
            .and_then(Value::as_array)
            .and_then(|agents| agents.first())
            .expect("claude row");
        assert_eq!(row.get("status"), Some(&json!("installed")));
        assert_eq!(row.get("hookInstalled"), Some(&json!(true)));
        assert_eq!(row.get("paths"), Some(&json!(expected_paths)));
        assert!(row
            .get("paths")
            .and_then(Value::as_array)
            .expect("paths")
            .contains(&json!(path_string(&profile_path))));
    }

    #[test]
    fn hook_status_detects_stale_profile_only_provider_hook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        write_test_executable(&temp.path().join(".local").join("bin").join("codex"));
        let profile_path = temp
            .path()
            .join(".codex-profiles")
            .join("work")
            .join("hooks.json");
        write_test_file(
            &profile_path,
            r#"{"ghostex":{"command":"legacy ~/.ghostexterm/agent-shell-notify.sh","agent":"codex"}}"#,
        );

        let status = read_agent_hook_status(
            &paths,
            json!({ "agentIds": ["codex"], "autoUpgradeInstalled": false })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let row = status
            .get("agents")
            .and_then(Value::as_array)
            .and_then(|agents| agents.first())
            .expect("codex row");
        assert_eq!(row.get("status"), Some(&json!("updateRequired")));
        assert_eq!(row.get("hookInstalled"), Some(&json!(false)));
        assert!(row
            .get("paths")
            .and_then(Value::as_array)
            .expect("paths")
            .contains(&json!(path_string(&profile_path))));
    }

    #[test]
    fn hook_status_uses_pi_root_extension_before_legacy_agent_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let hook_paths = HookPaths::new(paths.home_dir.clone());
        write_test_executable(&temp.path().join(".local").join("bin").join("pi"));
        install_notify_hook(&hook_paths).expect("notify hook");
        let pi = HookDefinition {
            agent_id: "pi",
            cli_command: "pi",
        };
        let root_extension_path = temp
            .path()
            .join(".pi")
            .join("extensions")
            .join("ghostex-session.ts");
        let legacy_agent_extension_path = temp
            .path()
            .join(".pi")
            .join("agent")
            .join("extensions")
            .join("ghostex-session")
            .join("index.ts");
        write_test_file(
            &root_extension_path,
            &format!(
                "// {PI_EXTENSION_MARKER} v3\nconst hook = \"{}\";\n",
                path_string(&hook_paths.notify_hook_path)
            ),
        );
        write_test_file(
            &legacy_agent_extension_path,
            &format!("// {PI_EXTENSION_MARKER} v2\n"),
        );

        let provider_paths = provider_hook_paths("pi", &hook_paths);
        let inspection = inspect_agent_hook_installation(&pi, &hook_paths, &provider_paths);
        assert!(inspection.current_hook_installed);
        assert_eq!(
            provider_paths.first().map(|path| path_string(path)),
            Some(path_string(&root_extension_path))
        );
        assert!(provider_paths.contains(&legacy_agent_extension_path));

        let status = read_agent_hook_status(
            &paths,
            json!({ "agentIds": ["pi"], "autoUpgradeInstalled": false })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let row = status
            .get("agents")
            .and_then(Value::as_array)
            .and_then(|agents| agents.first())
            .expect("pi row");
        assert_eq!(row.get("status"), Some(&json!("installed")));
        assert_eq!(row.get("hookInstalled"), Some(&json!(true)));
        assert!(row
            .get("paths")
            .and_then(Value::as_array)
            .expect("paths")
            .contains(&json!(path_string(&legacy_agent_extension_path))));
    }

    #[test]
    fn hook_status_reports_legacy_pi_agent_extension_as_update_required() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let hook_paths = HookPaths::new(paths.home_dir.clone());
        write_test_executable(&temp.path().join(".local").join("bin").join("pi"));
        install_notify_hook(&hook_paths).expect("notify hook");
        let legacy_agent_extension_path = temp
            .path()
            .join(".pi")
            .join("agent")
            .join("extensions")
            .join("ghostex-session")
            .join("index.ts");
        write_test_file(
            &legacy_agent_extension_path,
            &format!("// {PI_EXTENSION_MARKER} v2\n"),
        );

        let status = read_agent_hook_status(
            &paths,
            json!({ "agentIds": ["pi"], "autoUpgradeInstalled": false })
                .as_object()
                .expect("params"),
        )
        .expect("status");
        let row = status
            .get("agents")
            .and_then(Value::as_array)
            .and_then(|agents| agents.first())
            .expect("pi row");
        assert_eq!(row.get("status"), Some(&json!("updateRequired")));
        assert_eq!(row.get("hookInstalled"), Some(&json!(false)));
    }

    #[test]
    fn notify_hook_current_uses_typescript_marker_contract() {
        let temp = tempfile::tempdir().expect("tempdir");
        let hook_path = temp.path().join("agent-shell-notify.sh");
        write_test_file(
            &hook_path,
            &format!("#!/bin/zsh\n# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}\n"),
        );
        assert!(is_notify_hook_current(&hook_path));
    }

    #[test]
    fn install_writes_notify_hook_without_payload_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let result = install_agent_hooks(
            &paths,
            json!({ "agentIds": ["qoder"] })
                .as_object()
                .expect("params"),
        )
        .expect("install");
        let installed = result
            .get("installedPaths")
            .and_then(Value::as_array)
            .expect("installed paths");
        // The notify hook is always pushed first by install_agent_hooks;
        // additional entries are agent-specific hooks whose presence depends
        // on which agent CLIs are installed on this machine. Do not assert an
        // absolute count so the test stays hermetic across environments.
        assert!(installed.len() >= 1);
        let hook_text = fs::read_to_string(installed[0].as_str().expect("path")).expect("hook");
        assert!(hook_text.contains(NOTIFY_HOOK_MARKER));
        assert!(!hook_text.contains("firstUserMessage"));
        assert!(!hook_text.contains("rawTitle"));
    }

    #[test]
    fn notify_hook_helper_records_working_status_and_first_prompt() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state_path = temp.path().join("session.state");
        let hook_store = temp.path().join("hook-store");
        run_notify_hook(vec![
            path_string(&state_path),
            json!({
                "agent": "codex",
                "event": "UserPromptSubmit",
                "hook_event_name": "UserPromptSubmit",
                "prompt": "Please fix flaky tests",
                "session_id": "codex-session-1"
            })
            .to_string(),
            path_string(&hook_store),
        ])
        .expect("notify helper");
        let state = read_hook_state(&state_path);
        assert_eq!(
            read_state_string(&state, "status").as_deref(),
            Some("working")
        );
        assert_eq!(read_state_string(&state, "agent").as_deref(), Some("codex"));
        assert_eq!(
            read_state_string(&state, "agentSessionId").as_deref(),
            Some("codex-session-1")
        );
        assert_eq!(
            decode_base64_text(
                read_state_string(&state, "firstUserMessageBase64")
                    .as_deref()
                    .expect("first prompt")
            ),
            "Please fix flaky tests"
        );
        assert_eq!(
            read_state_string(&state, "pendingFirstPromptAutoRenamePrompt").as_deref(),
            Some("Please fix flaky tests")
        );
    }

    #[test]
    fn uninstall_agent_hooks_removes_notify_and_flat_json_without_autoupgrade() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let hook_paths = HookPaths::new(paths.home_dir.clone());
        write_test_file(&hook_paths.notify_hook_path, "#!/bin/zsh\n# old notify\n");
        let cursor_path = temp.path().join(".cursor").join("hooks.json");
        write_test_file(
            &cursor_path,
            &format!(
                "{}\n",
                json!({
                    "hooks": {
                        "beforeSubmitPrompt": [
                            { "command": format!("node {}/.ghostexterm/agent-shell-notify.sh", temp.path().display()) },
                            { "command": "user-managed-cursor-hook" }
                        ]
                    },
                    "version": 1
                })
            ),
        );

        let result = uninstall_agent_hooks(
            &paths,
            json!({ "agentIds": ["cursor"] })
                .as_object()
                .expect("params"),
        )
        .expect("uninstall");
        let removed = result
            .get("removedPaths")
            .and_then(Value::as_array)
            .expect("removed paths");
        assert!(removed.contains(&json!(path_string(&cursor_path))));
        assert!(removed.contains(&json!(path_string(&hook_paths.notify_hook_path))));
        assert!(result.get("autoUpgradedPaths").is_none());
        assert_eq!(result.get("type"), Some(&json!("agentHookStatus")));
        assert!(!hook_paths.notify_hook_path.exists());
        let cursor_text = fs::read_to_string(cursor_path).expect("cursor config");
        assert!(!cursor_text.contains("agent-shell-notify"));
        assert!(cursor_text.contains("user-managed-cursor-hook"));
    }

    #[test]
    fn uninstall_marked_yaml_matches_typescript_missing_end_marker_behavior() {
        let temp = tempfile::tempdir().expect("tempdir");
        let rovodev = HookDefinition {
            agent_id: "rovodev",
            cli_command: "acli",
        };
        let yaml_path = temp.path().join("config.yml");
        let yaml_text = "user_before: true\n# ghostex hooks rovodev begin\nnotify: ~/.ghostex/hooks/agent-shell-notify.sh\nuser_after: true\n";
        write_test_file(&yaml_path, yaml_text);

        let removed =
            uninstall_marked_yaml_hook(&rovodev, vec![yaml_path.clone()]).expect("yaml uninstall");
        assert_eq!(removed, vec![path_string(&yaml_path)]);
        assert_eq!(
            fs::read_to_string(&yaml_path).expect("yaml text"),
            "user_before: true\n"
        );
    }

    #[test]
    fn remove_json_hook_preserves_user_entries_for_supported_json_formats() {
        let temp = tempfile::tempdir().expect("tempdir");
        let notify_hook_path = temp
            .path()
            .join(".ghostex")
            .join("hooks")
            .join("agent-shell-notify.sh");
        let notify_hook = path_string(&notify_hook_path);

        let nested_path = temp.path().join("nested.json");
        write_test_file(
            &nested_path,
            &format!(
                "{}\n",
                json!({
                    "hooks": {
                        "SessionStart": [
                            {
                                "matcher": "*",
                                "hooks": [
                                    { "type": "command", "command": notify_hook },
                                    { "type": "command", "command": "user-nested-hook" }
                                ]
                            },
                            {
                                "hooks": [
                                    { "type": "command", "command": "legacy ~/.ghostexterm/agent-shell-notify.sh" }
                                ]
                            }
                        ]
                    },
                    "other": true
                })
            ),
        );
        let codex = HookDefinition {
            agent_id: "codex",
            cli_command: "codex",
        };
        assert!(remove_json_hook(&nested_path, &codex, &notify_hook).expect("nested remove"));
        let nested_text = fs::read_to_string(&nested_path).expect("nested text");
        assert!(!nested_text.contains("agent-shell-notify"));
        assert!(nested_text.contains("user-nested-hook"));

        let flat_path = temp.path().join("flat.json");
        let cursor = HookDefinition {
            agent_id: "cursor",
            cli_command: "cursor-agent",
        };
        let cursor_command = command_for_agent(&cursor, &notify_hook_path);
        write_test_file(
            &flat_path,
            &format!(
                "{}\n",
                json!({
                    "hooks": {
                        "beforeSubmitPrompt": [
                            { "command": cursor_command },
                            { "command": "user-flat-hook" }
                        ],
                        "beforeShellExecution": [
                            { "command": "legacy ~/.ghostex/hooks/agent-shell-notify.sh" }
                        ]
                    }
                })
            ),
        );
        assert!(remove_json_hook(&flat_path, &cursor, &cursor_command).expect("flat remove"));
        let flat_text = fs::read_to_string(&flat_path).expect("flat text");
        assert!(!flat_text.contains("agent-shell-notify"));
        assert!(flat_text.contains("user-flat-hook"));

        let kiro_path = temp.path().join("kiro.json");
        let kiro = HookDefinition {
            agent_id: "kiro",
            cli_command: "kiro-cli",
        };
        let kiro_command = command_for_agent(&kiro, &notify_hook_path);
        write_test_file(
            &kiro_path,
            &format!(
                "{}\n",
                json!({
                    "hooks": {
                        "agentSpawn": [
                            { "command": kiro_command, "timeout_ms": 5000 },
                            { "command": "user-kiro-hook" }
                        ]
                    },
                    "name": "ghostex"
                })
            ),
        );
        assert!(remove_json_hook(&kiro_path, &kiro, &kiro_command).expect("kiro remove"));
        let kiro_text = fs::read_to_string(&kiro_path).expect("kiro text");
        assert!(!kiro_text.contains("agent-shell-notify"));
        assert!(kiro_text.contains("user-kiro-hook"));

        let antigravity_path = temp.path().join("antigravity.json");
        let antigravity = HookDefinition {
            agent_id: "antigravity",
            cli_command: "agy",
        };
        let antigravity_command = command_for_agent(&antigravity, &notify_hook_path);
        write_test_file(
            &antigravity_path,
            &format!(
                "{}\n",
                json!({
                    "ghostex": {
                        "SessionStart": [
                            { "type": "command", "command": antigravity_command },
                            { "type": "command", "command": "user-antigravity-hook" }
                        ],
                        "PreToolUse": [
                            {
                                "matcher": "*",
                                "hooks": [
                                    { "type": "command", "command": "legacy ~/.ghostex/hooks/agent-shell-notify.sh" },
                                    { "type": "command", "command": "user-antigravity-feed-hook" }
                                ]
                            }
                        ]
                    }
                })
            ),
        );
        assert!(
            remove_json_hook(&antigravity_path, &antigravity, &antigravity_command)
                .expect("antigravity remove")
        );
        let antigravity_text = fs::read_to_string(&antigravity_path).expect("antigravity text");
        assert!(!antigravity_text.contains("agent-shell-notify"));
        assert!(antigravity_text.contains("user-antigravity-hook"));
        assert!(antigravity_text.contains("user-antigravity-feed-hook"));
    }

    #[test]
    fn uninstall_removes_plugin_yaml_and_opencode_ghostex_content_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let amp = HookDefinition {
            agent_id: "amp",
            cli_command: "amp",
        };
        let amp_path = temp.path().join("ghostex-session.ts");
        write_test_file(
            &amp_path,
            &format!("// {AMP_PLUGIN_MARKER} v3\nexport default {{}};\n"),
        );
        let removed_amp =
            uninstall_plugin_file_hook(&amp, vec![amp_path.clone()]).expect("amp uninstall");
        assert_eq!(removed_amp, vec![path_string(&amp_path)]);
        assert!(!amp_path.exists());

        let pi = HookDefinition {
            agent_id: "pi",
            cli_command: "pi",
        };
        let pi_path = temp.path().join("user-owned-ghostex-session.ts");
        write_test_file(&pi_path, "export default function userPlugin() {}\n");
        let removed_pi =
            uninstall_plugin_file_hook(&pi, vec![pi_path.clone()]).expect("pi uninstall");
        assert!(removed_pi.is_empty());
        assert!(pi_path.exists());

        let rovodev = HookDefinition {
            agent_id: "rovodev",
            cli_command: "acli",
        };
        let yaml_path = temp.path().join("config.yml");
        write_test_file(
            &yaml_path,
            "user_before: true\n# ghostex hooks rovodev begin\nnotify: ~/.ghostex/hooks/agent-shell-notify.sh\n# ghostex hooks rovodev end\nuser_after: true\n",
        );
        let removed_yaml =
            uninstall_marked_yaml_hook(&rovodev, vec![yaml_path.clone()]).expect("yaml uninstall");
        assert_eq!(removed_yaml, vec![path_string(&yaml_path)]);
        let yaml_text = fs::read_to_string(&yaml_path).expect("yaml text");
        assert!(!yaml_text.contains("ghostex hooks rovodev"));
        assert!(yaml_text.contains("user_before"));
        assert!(yaml_text.contains("user_after"));

        let opencode_config_path = temp.path().join("opencode.json");
        let opencode_plugin_path = temp.path().join("plugins").join("ghostex-session.js");
        write_test_file(
            &opencode_config_path,
            &format!(
                "{}\n",
                json!({
                    "other": true,
                    "plugin": [
                        "./plugins/other.js",
                        "./plugins/ghostex-session.js",
                        ["ghostex-session", { "enabled": true }],
                        "/tmp/plugins/ghostex-session.js",
                        "/tmp/ghostex-session.js",
                        "not-ghostex"
                    ]
                })
            ),
        );
        write_test_file(
            &opencode_plugin_path,
            &format!("// {OPENCODE_PLUGIN_MARKER} v3\nexport default {{}};\n"),
        );
        let removed_opencode =
            uninstall_opencode_hook_paths(&opencode_plugin_path, &opencode_config_path)
                .expect("opencode uninstall");
        assert!(removed_opencode.contains(&path_string(&opencode_config_path)));
        assert!(removed_opencode.contains(&path_string(&opencode_plugin_path)));
        assert!(!opencode_plugin_path.exists());
        let opencode_config = serde_json::from_str::<Value>(
            &fs::read_to_string(opencode_config_path).expect("opencode config"),
        )
        .expect("opencode json");
        assert_eq!(
            opencode_config.get("plugin"),
            Some(&json!(["./plugins/other.js", "not-ghostex"]))
        );
    }

    #[test]
    fn command_exists_uses_typescript_default_tool_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let command = format!("ghostex-test-cli-{}", std::process::id());
        write_test_executable(
            &temp
                .path()
                .join(".local")
                .join("share")
                .join("mise")
                .join("shims")
                .join(&command),
        );

        assert!(command_exists(&command, temp.path()));
    }

    fn write_test_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, text).expect("write test file");
    }

    fn write_test_executable(path: &Path) {
        write_test_file(path, "#!/bin/sh\nexit 0\n");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("chmod test executable");
    }
}
