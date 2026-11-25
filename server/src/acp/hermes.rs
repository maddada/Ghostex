//! Hermes Agent ACP helpers: profile resolution, transcript mirror path,
//! permission formatting, model/effort resolution, and live ACP model catalog.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::session_chat_options::SessionChatDetectedSelection;

use super::spec::{split_leading_env_assignments, tokenize_cli_command};

const HERMES_ACP_CATALOG_UPDATED_AT: &str = "2026-11-25";

/// Validate a Hermes profile name (`hermes -p <name>`). Accepts `"default"` plus
/// any valid Ghostex bot profile name.
pub(crate) fn is_valid_hermes_profile(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == "default" || crate::bot_projects::is_bot_profile_name(trimmed)
}

/// Extract a validated `-p <profile>` / `--profile <profile>` / `--profile=<profile>`
/// value from a Hermes CLI command line.
pub(crate) fn extract_hermes_profile_from_command(cmd: &str) -> Option<String> {
    let tokens = tokenize_cli_command(cmd);
    let (_, base_tokens) = split_leading_env_assignments(tokens);
    let mut i = 1;
    while i < base_tokens.len() {
        let tok = base_tokens[i].as_str();
        if let Some(val) = tok.strip_prefix("--profile=") {
            let trimmed = val.trim();
            if is_valid_hermes_profile(trimmed) {
                return Some(trimmed.to_string());
            }
        } else if matches!(tok, "-p" | "--profile") {
            if let Some(next) = base_tokens.get(i + 1) {
                let trimmed = next.trim();
                if is_valid_hermes_profile(trimmed) {
                    return Some(trimmed.to_string());
                }
                i += 1;
            }
        }
        i += 1;
    }
    None
}

/// Resolve the Hermes profile for a fresh Native launch from Ghostex's launch settings
/// (`botProfile`) or the owner-configured command (`hermes -p <profile>`).
pub(crate) fn resolve_hermes_profile_for_launch(
    launch_settings: &Map<String, Value>,
    owner_command: Option<&str>,
) -> Option<String> {
    if let Some(bot_profile) = launch_settings
        .get("botProfile")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| is_valid_hermes_profile(p))
    {
        return Some(bot_profile.to_string());
    }
    owner_command.and_then(extract_hermes_profile_from_command)
}

/// Resolve the Hermes profile for a resumed Native session: prefer the `state.db` store
/// holding `agentSessionId` (matching `resume_plan/hermes_profile.rs`), then `externalAgentHome`,
/// then `botProfile` / command `-p <profile>`.
pub(crate) fn resolve_hermes_profile_for_resume(
    runtime_settings: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
    command: Option<&str>,
) -> Option<String> {
    let stored_home = runtime_settings
        .get("agentSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| crate::session_chat_hermes::is_safe_hermes_session_id(id))
        .and_then(|id| {
            crate::session_chat_hermes::hermes_session_store(
                &crate::session_chat_hermes::hermes_home(),
                id,
            )
        })
        .and_then(|store| store.parent().map(PathBuf::from));
    let home = runtime_settings
        .get("externalAgentHome")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or(stored_home);
    if let Some(home) = home {
        let is_named_profile = home
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("profiles");
        if !is_named_profile {
            return Some("default".to_string());
        }
        if let Some(name) = home
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| is_valid_hermes_profile(n))
        {
            return Some(name.to_string());
        }
    }
    resolve_hermes_profile_for_launch(launch_settings, command).or_else(|| {
        runtime_settings
            .get("agentCommand")
            .and_then(Value::as_str)
            .and_then(extract_hermes_profile_from_command)
    })
}

/// Expected Hermes JSONL mirror path for `native_session_id`, even before the first
/// message row is written to `state.db`, so the transcript follower binds immediately
/// and syncs rows on each drain tick.
pub(crate) fn expected_transcript_mirror_path(native_session_id: &str) -> Option<PathBuf> {
    let trimmed = native_session_id.trim();
    if !crate::session_chat_hermes::is_safe_hermes_session_id(trimmed) {
        return None;
    }
    if let Some(existing) = crate::session_chat_hermes::resolve_hermes_chat_transcript_path(trimmed)
    {
        return Some(existing);
    }
    Some(
        ghostex_paths::GhostexPaths::resolve()
            .gxserver_state_dir()
            .join("hermes-chat-mirror")
            .join(format!("{trimmed}.jsonl")),
    )
}

/// Strip a Hermes ACP `<provider>:` prefix (`"openai-codex:gpt-5.6-luna"` -> `"gpt-5.6-luna"`)
/// for clean UI labels and CLI `--model` carry-over.
pub(crate) fn strip_hermes_provider_prefix(model_id: &str) -> &str {
    let trimmed = model_id.trim();
    match trimmed.split_once(':') {
        Some((provider, rest))
            if !provider.is_empty()
                && !rest.is_empty()
                && !provider.contains('/')
                && provider
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) =>
        {
            rest
        }
        _ => trimmed,
    }
}

/// Resolve a requested model string (either full `"provider:model"` or bare `"model"`)
/// against Hermes ACP's `availableModels` list.
pub(crate) fn resolve_hermes_target_model_id(
    available_models: &Value,
    current_model: Option<&str>,
    requested: &str,
) -> String {
    let wanted = requested.trim();
    let Some(list) = available_models.as_array() else {
        return wanted.to_string();
    };
    let ids: Vec<&str> = list
        .iter()
        .filter_map(|entry| {
            entry
                .get("modelId")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .collect();

    if let Some(exact) = ids.iter().find(|id| **id == wanted) {
        return (*exact).to_string();
    }
    if let Some(ci) = ids.iter().find(|id| id.eq_ignore_ascii_case(wanted)) {
        return (*ci).to_string();
    }
    // Match bare model name against `<provider>:<model>`, preferring the active provider.
    let active_provider = current_model.and_then(|cur| cur.split_once(':').map(|(p, _)| p));
    if let Some(provider) = active_provider {
        if let Some(same_provider) = ids.iter().find(|id| {
            id.split_once(':')
                .is_some_and(|(p, m)| p == provider && m.eq_ignore_ascii_case(wanted))
        }) {
            return (*same_provider).to_string();
        }
    }
    if let Some(any_provider) = ids
        .iter()
        .find(|id| strip_hermes_provider_prefix(id).eq_ignore_ascii_case(wanted))
    {
        return (*any_provider).to_string();
    }
    wanted.to_string()
}

/// Extract a clean display summary for Hermes ACP `session/request_permission` tool calls
/// (`edit-approval-*` nests `path` under `rawInput.arguments.path` or `content[0].path`;
/// `model-confirm-*` carries `rawInput.model`).
pub(crate) fn extract_permission_summary(tool_call: &Value) -> Option<String> {
    let candidates = [
        tool_call.pointer("/rawInput/arguments/path"),
        tool_call.pointer("/rawInput/arguments/file_path"),
        tool_call.pointer("/content/0/path"),
        tool_call.pointer("/rawInput/model"),
    ];
    for candidate in candidates {
        if let Some(s) = candidate
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Extract a clean tool name for Hermes ACP edit approval requests (`rawInput.tool` =
/// `"write_file"` / `"patch"`).
pub(crate) fn extract_permission_tool_name(tool_call: &Value) -> Option<String> {
    tool_call
        .pointer("/rawInput/tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Extract `(current_effort, current_model)` updates from an ACP `configOptions` array.
pub(crate) fn extract_config_option_state(
    config_options: &Value,
) -> (Option<String>, Option<String>) {
    let Some(arr) = config_options.as_array() else {
        return (None, None);
    };
    let mut effort = None;
    let mut model = None;
    for item in arr {
        let Some(cid) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let cat = item
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current = item
            .get("currentValue")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if effort.is_none()
            && (cat.eq_ignore_ascii_case("thought_level")
                || cid.eq_ignore_ascii_case("reasoning_effort")
                || cid.contains("effort"))
        {
            if let Some(val) = current {
                if !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(val) {
                    effort = Some(val.to_ascii_lowercase());
                }
            }
        }
        if model.is_none()
            && (cat.eq_ignore_ascii_case("model") || cid.eq_ignore_ascii_case("model"))
        {
            if let Some(val) = current {
                if !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(val) {
                    model = Some(val.to_string());
                }
            }
        }
    }
    (effort, model)
}

/// Build an `AgentModelCatalog` JSON document for a Native Hermes session from live ACP
/// `availableModels` + `configOptions` (`reasoning_effort`), enriched with/falling back to
/// `session_chat_hermes_status::read_hermes_status_selection`.
pub(crate) fn build_native_model_catalog(
    acp_state: Option<&Value>,
    hermes_status_selection: Option<&SessionChatDetectedSelection>,
) -> Option<Value> {
    let bot_name = hermes_status_selection
        .and_then(|sel| sel.hermes_status.as_ref())
        .and_then(|s| s.get("name"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Hermes");

    let avail_models = acp_state
        .and_then(|s| s.get("availableModels"))
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty());

    let Some(avail_models) = avail_models else {
        return hermes_status_selection.and_then(|sel| sel.model_catalog.clone());
    };

    let current_model = acp_state
        .and_then(|s| s.get("model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut models = Vec::with_capacity(avail_models.len());
    for entry in avail_models {
        let Some(model_id) = entry
            .get("modelId")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let label = strip_hermes_provider_prefix(model_id);
        let description = entry
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                entry
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != label)
            });
        let mut row = json!({
            "value": model_id,
            "label": label,
        });
        if let Some(desc) = description {
            row["description"] = json!(desc);
        }
        if current_model == Some(model_id) {
            row["default"] = json!(true);
        }
        models.push(row);
    }

    if models.is_empty() {
        return hermes_status_selection.and_then(|sel| sel.model_catalog.clone());
    }

    // Read effort choices and labels from ACP `configOptions` (`reasoning_effort`).
    let mut efforts: Vec<String> = Vec::new();
    let mut effort_labels = Map::new();
    let mut default_effort: Option<String> = None;

    if let Some(configs) = acp_state
        .and_then(|s| s.get("configOptions"))
        .and_then(Value::as_array)
    {
        if let Some(cfg) = configs.iter().find(|item| {
            let cid = item.get("id").and_then(Value::as_str).unwrap_or_default();
            let cat = item
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or_default();
            cat.eq_ignore_ascii_case("thought_level")
                || cid.eq_ignore_ascii_case("reasoning_effort")
                || cid.contains("effort")
        }) {
            if let Some(cur) = cfg
                .get("currentValue")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(s))
            {
                default_effort = Some(cur.to_ascii_lowercase());
            }
            if let Some(opts) = cfg.get("options").and_then(Value::as_array) {
                for opt in opts {
                    let Some(val) = opt
                        .get("value")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| {
                            !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(s)
                        })
                    else {
                        continue;
                    };
                    let val_lower = val.to_ascii_lowercase();
                    if !efforts.contains(&val_lower) {
                        efforts.push(val_lower.clone());
                    }
                    let label = opt
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(val))
                        .map(str::to_string)
                        .unwrap_or_else(|| crate::agent_model_catalog::effort_display_label(val));
                    effort_labels.insert(val_lower, Value::String(label));
                }
            }
        }
    }

    if efforts.is_empty() {
        efforts = crate::session_chat_hermes_status::HERMES_PICKER_EFFORTS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    }

    Some(json!({
        "schemaVersion": 1,
        "updatedAt": HERMES_ACP_CATALOG_UPDATED_AT,
        "effortLabels": Value::Object(effort_labels),
        "agents": {
            "hermes": {
                "name": bot_name,
                "efforts": efforts,
                "defaultEffort": default_effort,
                "models": models,
            },
        },
    }))
}
