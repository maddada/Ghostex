// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the agent-hook status ordering/merge
// helpers, the settings-command agent-id parsing helpers, and the home-dir,
// prompt-stash marker, which-command, and preferred-agent-interface path
// helpers. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{Hsla, rgb};

use crate::app::helpers::*;
use crate::*;

pub(crate) const GPUI_AGENT_HOOK_PRIORITY_STATUS_AGENT_IDS: [&str; 4] =
    ["codex", "claude", "opencode", "pi"];

pub(crate) fn gpui_ordered_agent_hook_status_agent_ids(
    requested: Option<Vec<String>>,
) -> Vec<String> {
    let requested_ids = match requested {
        Some(ids) if !ids.is_empty() => ids,
        _ => GPUI_DEFAULT_SIDEBAR_AGENTS
            .iter()
            .map(|agent| agent.agent_id.to_string())
            .collect(),
    };
    let mut seen = HashSet::new();
    let normalized = requested_ids
        .into_iter()
        .map(|agent_id| agent_id.trim().to_string())
        .filter(|agent_id| !agent_id.is_empty() && seen.insert(agent_id.clone()))
        .collect::<Vec<_>>();
    let mut ordered = GPUI_AGENT_HOOK_PRIORITY_STATUS_AGENT_IDS
        .iter()
        .filter(|agent_id| seen.contains(**agent_id))
        .map(|agent_id| agent_id.to_string())
        .collect::<Vec<_>>();
    ordered.extend(normalized.into_iter().filter(|agent_id| {
        !GPUI_AGENT_HOOK_PRIORITY_STATUS_AGENT_IDS.contains(&agent_id.as_str())
    }));
    ordered
}

pub(crate) fn gpui_merge_agent_hook_status_messages(
    previous: &serde_json::Value,
    next: &serde_json::Value,
) -> serde_json::Value {
    let mut merged = previous.as_object().cloned().unwrap_or_default();
    if let Some(next_object) = next.as_object() {
        for (key, value) in next_object {
            merged.insert(key.clone(), value.clone());
        }
    }
    let next_agents = next
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let next_agent_ids = next_agents
        .iter()
        .filter_map(|agent| agent.get("agentId").and_then(serde_json::Value::as_str))
        .collect::<HashSet<_>>();
    let mut agents = previous
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .map(|previous_agents| {
            previous_agents
                .iter()
                .filter(|agent| {
                    agent
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .map(|agent_id| !next_agent_ids.contains(agent_id))
                        .unwrap_or(true)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    agents.extend(next_agents);
    merged.insert("agents".to_string(), serde_json::Value::Array(agents));
    // generatedAt prefers the newest non-empty value; the hook state directory
    // and notify hook path keep the first non-empty value, matching the macOS
    // mergeAgentHookStatusMessages field rules.
    let generated_at = [next, previous]
        .iter()
        .find_map(|value| {
            value
                .get("generatedAt")
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_default()
        .to_string();
    merged.insert(
        "generatedAt".to_string(),
        serde_json::Value::String(generated_at),
    );
    for key in ["hookStateDirectory", "notifyHookPath"] {
        let value = [previous, next]
            .iter()
            .find_map(|value| {
                value
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|text| !text.is_empty())
            })
            .unwrap_or_default()
            .to_string();
        merged.insert(key.to_string(), serde_json::Value::String(value));
    }
    serde_json::Value::Object(merged)
}

pub(crate) fn gpui_agent_hook_status_message(
    endpoint: &str,
    requested_agent_ids: Option<HashSet<String>>,
    error_message: &str,
) -> serde_json::Value {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
    Agent hook status/install/uninstall in GPUI is gxserver-owned, matching macOS Settings. Call the real hook endpoints and pass the daemon's `agentHookStatus` payload through; on transport/API failure, return an explicit empty error payload instead of local fixture rows or fake installed hooks.
    */
    let mut params = serde_json::Map::new();
    if let Some(agent_ids) = requested_agent_ids {
        params.insert(
            "agentIds".to_string(),
            serde_json::Value::Array(
                agent_ids
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
    }

    if let Ok(result) = gpui_gxserver_rpc_result(
        endpoint,
        &serde_json::Value::Object(params),
        Duration::from_secs(45),
    ) {
        if result.get("type").and_then(serde_json::Value::as_str) == Some("agentHookStatus") {
            return result;
        }
    }
    serde_json::json!({
        "agents": [],
        "errorMessage": error_message,
        "generatedAt": gpui_status_generated_at(),
        "hookStateDirectory": "",
        "notifyHookPath": "",
        "type": "agentHookStatus",
    })
}

pub(crate) fn gpui_daemon_agent_status(value: Option<&str>) -> &'static str {
    match value {
        Some("working") => "working",
        Some("attention") => "attention",
        _ => "idle",
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiAgentAcceptAllModeUpdate {
    Preserve,
    Set(Option<String>),
}

pub(crate) fn gpui_settings_command_ordered_agent_ids(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<Vec<String>> {
    let agent_ids = command.get("agentIds")?.as_array()?;
    let mut seen = HashSet::new();
    let normalized = agent_ids
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|agent_id| !agent_id.is_empty())
        .map(str::to_string)
        .filter(|agent_id| seen.insert(agent_id.clone()))
        .collect::<Vec<_>>();
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn gpui_settings_command_agent_ids(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<HashSet<String>> {
    let agent_ids = command.get("agentIds")?.as_array()?;
    let normalized = agent_ids
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|agent_id| !agent_id.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn gpui_home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/Shared"))
}

/// Stash-request markers mirror the CLI's shared state-directory resolution
/// (see `prompt_stash_request_marker_path` in
/// server/src/ghostex_cli/editors.rs). One marker per session ref; the
/// CLI consumes it on its next `prompt-editor` invocation and treats stale
/// markers (older than its freshness window) as expired.
pub(crate) fn gpui_prompt_stash_request_marker_path(project_id: &str, session_id: &str) -> PathBuf {
    shared_settings::ghostex_storage_paths()
        .state_dir
        .join("prompt-stash-requests")
        .join(format!("{project_id}-{session_id}"))
}

pub(crate) fn gpui_write_prompt_stash_request_marker(
    project_id: &str,
    session_id: &str,
    marker: &str,
) -> bool {
    let path = gpui_prompt_stash_request_marker_path(project_id, session_id);
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    std::fs::write(&path, marker.as_bytes()).is_ok()
}

pub(crate) fn gpui_remove_prompt_stash_request_marker(project_id: &str, session_id: &str) -> bool {
    std::fs::remove_file(gpui_prompt_stash_request_marker_path(
        project_id, session_id,
    ))
    .is_ok()
}

pub(crate) fn gpui_which_command(command: &str) -> Option<PathBuf> {
    if command.contains('/') || command.trim().is_empty() {
        return None;
    }
    env::var_os("PATH")
        .into_iter()
        .flat_map(|path_value| env::split_paths(&path_value).collect::<Vec<_>>())
        .map(|directory| directory.join(command))
        .find(|candidate| gpui_is_executable_file(candidate))
}

pub(crate) fn gpui_is_probably_ghostex_command(path: &Path, command: &str) -> bool {
    /*
    CDXC:GPUISettingsAgentSkills 2026-06-24-13:08:
    This predicate feeds both read-only CLI status and Settings skill execution. Keep it strict enough for execution: only marked wrappers or app-owned command realpaths count as Ghostex-owned, not broad file text such as "Ghostex CLI".
    */
    if gpui_is_marked_ghostex_wrapper_file(path) {
        return true;
    }
    let realpath = gpui_realpath_or_self(path);
    if gpui_is_ghostex_app_owned_command_realpath(command, &realpath) {
        return true;
    }
    gpui_current_bundle_cli_dir_for_ownership_probe()
        .as_ref()
        .map(|cli_dir| gpui_path_is_relative_to(&realpath, cli_dir))
        .unwrap_or(false)
}

pub(crate) fn gpui_preferred_agent_interface_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPreferredAgentInterface {
    settings
        .get("preferredAgentInterface")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiPreferredAgentInterface::from_str)
        .unwrap_or_default()
}

/// Per-agent override of the Default Agent View, read straight off the shared
/// settings JSON. A missing key means "inherit", so an unknown agent, an
/// unknown value, or a settings file that predates the setting all resolve to
/// `None` and leave the global preference in charge.
pub(crate) fn gpui_preferred_agent_interface_override_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
    agent_id: &str,
) -> Option<GpuiPreferredAgentInterface> {
    settings
        .get("preferredAgentInterfaceOverrides")
        .and_then(serde_json::Value::as_object)?
        .get(agent_id)
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiPreferredAgentInterface::from_str)
}

/// Resolve the effective Default Agent View for a session, which the desktop
/// knows only by its agent icon. Overrides are keyed by agent id, and icon is
/// not agent id for every agent (`grok-build` is agent `grok`, `rovo-dev` is
/// agent `rovodev`), so the default catalog does the translation instead of a
/// second hand-written match that could drift from it.
pub(crate) fn gpui_effective_preferred_agent_interface_for_agent_icon(
    settings: &serde_json::Map<String, serde_json::Value>,
    agent_icon: Option<&str>,
) -> GpuiPreferredAgentInterface {
    agent_icon
        .and_then(gpui_default_sidebar_agent_by_icon)
        .and_then(|agent| {
            gpui_preferred_agent_interface_override_from_settings(settings, agent.agent_id)
        })
        .unwrap_or_else(|| gpui_preferred_agent_interface_from_settings(settings))
}

pub(crate) fn gpui_session_chat_background_color() -> Hsla {
    if gpui_session_chat_theme_from_settings(
        shared_settings::shared_sidebar_settings_snapshot().object(),
    ) == "light"
    {
        rgb(0xfdfdfd).into()
    } else {
        rgb(0x0a0a0a).into()
    }
}
