use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionType {
    Browser,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionRunMode {
    Default,
    Debug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarActionLinkTarget {
    External,
    Integrated,
}

/*
CDXC:Projects 2026-07-31-12:00:
Terminal Actions can carry saved links that open alongside the command run, so a
dev-server Action starts the server and surfaces its localhost URL in one click.
Each link targets the project's integrated Browser tab or the OS default browser.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarActionLink {
    pub(crate) target: GpuiTitlebarActionLinkTarget,
    pub(crate) url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarAction {
    pub(crate) action_type: GpuiTitlebarActionType,
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command: Option<String>,
    pub(crate) command_id: String,
    pub(crate) icon: Option<String>,
    pub(crate) links: Vec<GpuiTitlebarActionLink>,
    pub(crate) name: String,
    pub(crate) play_completion_sound: bool,
    pub(crate) run_mode: GpuiTitlebarActionRunMode,
    pub(crate) url: Option<String>,
}

impl GpuiTitlebarAction {
    pub(crate) fn is_configured(&self) -> bool {
        match self.action_type {
            GpuiTitlebarActionType::Browser => self
                .url
                .as_deref()
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                .is_some(),
            GpuiTitlebarActionType::Terminal => self
                .command
                .as_deref()
                .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
                .is_some(),
        }
    }

    pub(crate) fn titlebar_menu_name(&self) -> String {
        gpui_normalized_sidebar_command_title(Some(&self.name))
            .unwrap_or_else(|| self.command_id.clone())
    }

    pub(crate) fn titlebar_menu_preview(&self) -> (String, bool) {
        let preview = match self.action_type {
            GpuiTitlebarActionType::Browser => self
                .url
                .as_deref()
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url))),
            GpuiTitlebarActionType::Terminal => self
                .command
                .as_deref()
                .and_then(|command| gpui_trimmed_nonempty_str(Some(command))),
        };
        preview
            .map(|preview| (preview.to_string(), false))
            .unwrap_or_else(|| (TITLEBAR_ACTION_UNCONFIGURED_PREVIEW.to_string(), true))
    }

    pub(crate) fn command_title(&self) -> String {
        /*
        CDXC:CommandPane 2026-06-25-11:42:
        Command-pane Action tabs must use the same visible title rule as macOS: normalized Action name first, otherwise the normalized command text truncated to 20 characters. Do not substitute command ids for unnamed terminal Actions because title-owned reuse and duplicate-title checks depend on this user-facing Action title.
        */
        gpui_normalized_sidebar_command_title(Some(&self.name))
            .or_else(|| {
                gpui_normalized_sidebar_command_title(self.command.as_deref())
                    .map(gpui_sidebar_command_short_title)
            })
            .unwrap_or_else(|| self.command_id.clone())
    }
}

pub(crate) const GPUI_SIDEBAR_METADATA_GENERIC_ERROR: &str = "sidebar metadata write failed";
pub(crate) const GPUI_SIDEBAR_DUPLICATE_ACTION_TITLE_ERROR: &str = "duplicate action title";

pub(crate) fn gpui_titlebar_actions_for_active_project_id(
    active_project_id: Option<&str>,
) -> Vec<GpuiTitlebarAction> {
    /*
    CDXC:Titlebar 2026-06-24-14:24:
    The visible GPUI titlebar Actions control must run the same sidebar/gxserver-projected command definitions that Settings and the SidebarApp expose. Read only the shared `hud.commands` contract shape from gxserver-derived project metadata, keep command text and URLs in memory for immediate Browser/command-terminal routing, and never infer actions from paths, git state, labels, env, terminal titles, or filesystem probes.

    CDXC:AgentLauncher 2026-06-24-20:34:
    Titlebar Actions consume `/api/readSidebarHud` so active-project command scoping comes from the production gxserver contract rather than the GPUI app-modal Rust mirror.
    */
    let commands = gpui_sidebar_hud_from_gxserver(Duration::from_secs(2), active_project_id)
        .map(|hud| hud.commands)
        .unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
    gpui_titlebar_actions_from_sidebar_command_buttons(&commands)
}

/// The Start menu's Global section: the composed sidebar HUD's `globalCommands`, in their saved
/// order, parsed the way a project's own Actions are. Empty before the store's first HUD.
///
/// CDXC:AgentLauncher 2026-11-08 WHY: the list comes from the same HUD `runSidebarCommand` resolves
/// a Global Action against (`plan_sidebar_command_run`), so a row is always one the run can find,
/// and the row hands only its id back: the command text is resolved again at run time.
pub(crate) fn gpui_titlebar_global_actions_from_sidebar_hud(
    hud: Option<&serde_json::Value>,
) -> Vec<GpuiTitlebarAction> {
    hud.and_then(|hud| hud.get("globalCommands"))
        .map(gpui_titlebar_actions_from_sidebar_command_buttons)
        .unwrap_or_default()
}

pub(crate) fn gpui_titlebar_actions_from_sidebar_command_buttons(
    buttons: &serde_json::Value,
) -> Vec<GpuiTitlebarAction> {
    let Some(items) = buttons.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(gpui_titlebar_action_from_sidebar_command_button)
        .collect()
}

pub(crate) fn gpui_titlebar_action_from_sidebar_command_button(
    value: &serde_json::Value,
) -> Option<GpuiTitlebarAction> {
    let object = value.as_object()?;
    let command_id = gpui_trimmed_json_string_field(object, "commandId")?.to_string();
    let action_type = match object.get("actionType").and_then(serde_json::Value::as_str) {
        Some("browser") => GpuiTitlebarActionType::Browser,
        Some("terminal") | None => GpuiTitlebarActionType::Terminal,
        _ => return None,
    };
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        .map(str::to_string);
    let url = object
        .get("url")
        .and_then(serde_json::Value::as_str)
        .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
        .map(str::to_string);
    Some(GpuiTitlebarAction {
        action_type,
        close_terminal_on_exit: action_type == GpuiTitlebarActionType::Terminal
            && object
                .get("closeTerminalOnExit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        command,
        command_id,
        icon: object
            .get("icon")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sidebar_command_icon)
            .map(str::to_string),
        links: if action_type == GpuiTitlebarActionType::Terminal {
            gpui_titlebar_action_links_from_sidebar_command_button(object)
        } else {
            Vec::new()
        },
        name,
        play_completion_sound: action_type == GpuiTitlebarActionType::Terminal
            && object
                .get("playCompletionSound")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        run_mode: GpuiTitlebarActionRunMode::Default,
        url,
    })
}

pub(crate) fn gpui_titlebar_action_links_from_sidebar_command_button(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<GpuiTitlebarActionLink> {
    object
        .get("links")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .filter_map(|item| {
            let url = item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))?;
            Some(GpuiTitlebarActionLink {
                target: match item.get("target").and_then(serde_json::Value::as_str) {
                    Some("external") => GpuiTitlebarActionLinkTarget::External,
                    _ => GpuiTitlebarActionLinkTarget::Integrated,
                },
                url: url.to_string(),
            })
        })
        .collect()
}

pub(crate) fn gpui_completion_sound_label(sound: &str) -> &'static str {
    match gpui_normalize_completion_sound(Some(sound)) {
        "success-chime" => "Success Chime",
        "flawless-victory" => "Flawless Victory",
        "pingdouble" => "Ping Double",
        "ping" => "Ping",
        "glass" => "Glass",
        "glimmer" => "Glimmer",
        "shamisen" => "Shamisen",
        "shamisenreverb" => "Shamisen Reverb",
        "arcadeboost" => "Arcade Boost",
        "confirmation-001" => "Confirmation 001",
        "confirmation-002" => "Confirmation 002",
        "confirmation-003" => "Confirmation 003",
        "confirmation-004" => "Confirmation 004",
        "notification-pop" => "Notification Pop",
        "high-up" => "High Up",
        "high-down" => "High Down",
        "low-three-tone" => "Low Three Tone",
        "tone-1" => "Tone 1",
        "three-tone-1" => "Three Tone 1",
        "three-tone-2" => "Three Tone 2",
        "two-tone-1" => "Two Tone 1",
        "two-tone-2" => "Two Tone 2",
        "power-up-5" => "Power Up 5",
        "power-up-6" => "Power Up 6",
        "power-up-8" => "Power Up 8",
        "coin-collect" => "Coin Collect",
        "phaser-up-5" => "Phaser Up 5",
        "zap-two-tone" => "Zap Two Tone",
        "voiceover-pack-male-mission-completed" => "Mission Completed (Male)",
        "voiceover-pack-female-mission-completed" => "Mission Completed (Female)",
        "voiceover-pack-male-you-win" => "You Win (Male)",
        "voiceover-pack-female-congratulations" => "Congratulations (Female)",
        _ => "Arcade",
    }
}

pub(crate) fn append_url_query_params(mut url: String, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return url;
    }
    url.push(if url.contains('?') { '&' } else { '?' });
    for (index, (key, value)) in params.iter().enumerate() {
        if index > 0 {
            url.push('&');
        }
        url.push_str(&encode_search_query(key));
        url.push('=');
        url.push_str(&encode_search_query(value));
    }
    url
}
