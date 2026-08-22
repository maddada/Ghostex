use std::{collections::HashMap, path::PathBuf};

use serde_json::Value;

use crate::paths::GxserverPaths;

use super::config::{
    AMP_PLUGIN_MARKER, HOOK_DEFINITIONS, HookPaths, OMP_EXTENSION_MARKER, OPENCODE_PLUGIN_MARKER,
    OPENCODE_PLUGIN_SPEC, PI_EXTENSION_MARKER, pi_extension_paths, resolve_config_directory,
    resolve_omp_agent_directory,
};
use super::install::read_json_object;
use super::probing::{list_profile_hook_paths, read_file_text};

#[derive(Clone, Debug)]
pub(crate) struct CodexHookSessionIdentity {
    pub agent_session_id: String,
    pub agent_session_path: Option<String>,
}

pub(crate) fn read_codex_hook_session_identities(
    paths: &GxserverPaths,
) -> HashMap<String, CodexHookSessionIdentity> {
    let store_path = paths
        .app_state_dir
        .join("agent-hooks")
        .join("codex-hook-sessions.json");
    let data = read_json_object(&read_file_text(&store_path));
    let Some(sessions) = data.get("sessions").and_then(Value::as_object) else {
        return HashMap::new();
    };
    let mut latest_by_surface: HashMap<String, (f64, CodexHookSessionIdentity)> = HashMap::new();
    for entry in sessions.values().filter_map(Value::as_object) {
        let Some(surface_id) = entry
            .get("surfaceId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(agent_session_id) = entry
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let updated_at = entry
            .get("updatedAt")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        if latest_by_surface
            .get(surface_id)
            .is_some_and(|(current_updated_at, _)| *current_updated_at > updated_at)
        {
            continue;
        }
        let agent_session_path = entry
            .get("transcriptPath")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        latest_by_surface.insert(
            surface_id.to_string(),
            (
                updated_at,
                CodexHookSessionIdentity {
                    agent_session_id: agent_session_id.to_string(),
                    agent_session_path,
                },
            ),
        );
    }
    latest_by_surface
        .into_iter()
        .map(|(surface_id, (_, identity))| (surface_id, identity))
        .collect()
}

pub(crate) fn provider_hook_paths(agent_id: &str, hook_paths: &HookPaths) -> Vec<PathBuf> {
    match agent_id {
        "codex" => {
            let mut paths = vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "CODEX_HOME",
                ".codex",
                None,
            )
            .join("hooks.json")];
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
                hook_paths.respect_config_environment,
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
        "pi" => pi_extension_paths(&hook_paths.home_dir, hook_paths.respect_config_environment),
        "omp" => vec![resolve_omp_agent_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
        )
        .join("extensions")
        .join("ghostex-omp-session.ts")],
        "grok" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "GROK_HOME",
                ".grok/hooks",
                None,
            )
            .join("ghostex-session.json")]
        }
        "antigravity" => vec![hook_paths
            .home_dir
            .join(".gemini")
            .join("config")
            .join("hooks.json")],
        "kiro" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
            "KIRO_HOME",
            ".kiro/agents",
            Some("agents"),
        )
        .join("ghostex.json")],
        "copilot" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "COPILOT_HOME",
                ".copilot",
                None,
            )
            .join("config.json")]
        }
        "droid" => vec![hook_paths.home_dir.join(".factory").join("settings.json")],
        "rovodev" => vec![hook_paths.home_dir.join(".rovodev").join("config.yml")],
        "hermes-agent" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "HERMES_HOME",
                ".hermes",
                None,
            )
            .join("config.yaml")]
        }
        "codebuddy" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            hook_paths.respect_config_environment,
            "CODEBUDDY_CONFIG_DIR",
            ".codebuddy",
            None,
        )
        .join("settings.json")],
        "qoder" => {
            vec![resolve_config_directory(
                &hook_paths.home_dir,
                hook_paths.respect_config_environment,
                "QODER_CONFIG_DIR",
                ".qoder",
                None,
            )
            .join("settings.json")]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn is_opencode_session_plugin_registration(value: &Value) -> bool {
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

pub(crate) fn is_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    let Some(command_value) = value.get("command").and_then(Value::as_str) else {
        return false;
    };
    command_value == command || text_contains_ghostex_owned_hook_command(command_value)
}

pub(crate) fn text_contains_ghostex_owned_hook_command(text: &str) -> bool {
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

pub(crate) fn normalize_agent_ids(value: Option<&Value>) -> Vec<String> {
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
