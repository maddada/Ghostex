// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds sidebar visual constants, bridge-script builders, and pointer/menu-bar callback registries.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::sync::atomic::Ordering;

use gpui::{Hsla, rgb};

use crate::app::helpers::*;
use crate::*;

/// CDXC:Sidebar 2026-09-16 DECISION:
/// User: resize drag-area backgrounds are #F3F4F6 in light mode, superseding the earlier #C9C9C9 choice and the white sidebar grab handle requested on 2026-09-13.
pub(crate) fn sidebar_divider_background_color() -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        rgb(LIGHT_RESIZE_HANDLE_RGB).into()
    } else {
        workspace_background_color()
    }
}

/// CDXC:Sidebar 2026-09-19 DECISION:
/// User: the line between the sidebar and the workspace matches the companion sidepane's left border in both themes (#d4d4d4 in light mode, drawn in dark mode too). Since the same-day Workarea decision to make rails look like Waku's, the 2px resize rail is that line: it fills with this colour instead of drawing one pixel beside a black gap, and stays the single owner of the edge, so the sidebar itself must not add a border there as well.
pub(crate) fn sidebar_divider_line_color() -> Hsla {
    glass_divider(chrome_color(0x252525, 0xd4d4d4).into())
}

/// CDXC:Theming 2026-09-14 DECISION:
/// User: the resize drag handle's hovered/active color is light blue in light mode.
pub(crate) fn sidebar_divider_hover_line_color() -> Hsla {
    chrome_color(0xffffff, 0x93c5fd).into()
}

pub(crate) fn gpui_app_modal_sidebar_session_id_allowed(value: &str) -> bool {
    gpui_combined_presentation_session_key(value).is_some()
        || gpui_sidebar_gxserver_presentation_session_id_allowed(value)
}

pub(crate) fn gpui_percent_decoded_id_part(value: &str) -> Option<String> {
    let decoded = browser_favicon_percent_decode(value, GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .and_then(|bytes| String::from_utf8(bytes).ok())?;
    (!decoded.is_empty()
        && decoded.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !decoded.chars().any(char::is_control))
    .then_some(decoded)
}

pub(crate) fn gpui_action_completion_sound_from_settings() -> &'static str {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_normalize_completion_sound(
        settings
            .object()
            .get("actionCompletionSound")
            .and_then(serde_json::Value::as_str),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_sidebar_pointer_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiSidebarPointerCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_sidebar_pointer_callback_target() {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_sidebar_pointer_callback_target() -> Option<GpuiSidebarPointerCallbackTarget> {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_menu_bar_status_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:StatusPet 2026-06-26-06:05:
    The AppKit Running Agents dropdown is process-global, but project/session row actions must target only the live GPUI root. Register this callback target with the app lifecycle and keep row payloads to copied bounded ids that are later routed through fixed sidebar callbacks.
    */
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiMenuBarStatusCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_menu_bar_status_callback_target() {
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_callback_target() -> Option<GpuiMenuBarStatusCallbackTarget> {
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

/// The icon key of a row's agent icon, agent id or name: the aliases below, then any built-in agent
/// by its id or icon.
///
/// CDXC:ClaudeCloud 2026-10-27 WHY: every desktop tab takes its agent from here, and the chat gates
/// compare that to the agent's icon. A built-in agent missing from the aliases came back as no
/// agent at all: the Claude (cloud) agent did, so its sessions never opened in Chat View and their
/// claude.ai page never showed in the chat pane. The built-in table is the fallback so a new agent
/// cannot fall through again.
pub(crate) fn gpui_sidebar_agent_icon(value: Option<&str>) -> Option<&'static str> {
    let value = value?.trim().to_ascii_lowercase();
    match value.as_str() {
        "browser" => Some("browser"),
        "codex" => Some("codex"),
        "claude" => Some("claude"),
        "cursor" | "cursor cli" | "cursor-cli" => Some("cursor-cli"),
        "pi" | "pi agent" => Some("pi"),
        "opencode" | "open code" => Some("opencode"),
        "gemini" => Some("gemini"),
        "copilot" => Some("copilot"),
        "droid" | "factory droid" | "factory-droid" => Some("factory-droid"),
        "grok" | "grok build" | "grok-build" => Some("grok-build"),
        "antigravity" | "antigravity cli" | "antigravity-cli" => Some("antigravity-cli"),
        "amp" | "amp cli" | "amp-cli" => Some("amp-cli"),
        "rovodev" | "rovo dev" | "rovo-dev" => Some("rovo-dev"),
        "hermes-agent" | "hermes agent" => Some("hermes-agent"),
        "codebuddy" => Some("codebuddy"),
        "qoder" => Some("qoder"),
        "kiro" | "kiro cli" => Some("kiro"),
        "omp" => Some("omp"),
        "kimi" | "kimi code" | "kimi-code" => Some("kimi"),
        "openclaude" | "open claude" | "open-claude" => Some("openclaude"),
        "command-code" | "command code" | "commandcode" => Some("command-code"),
        "mastra" | "mastracode" | "mastra code" => Some("mastra"),
        "zcode" => Some("zcode"),
        "freebuff" => Some("freebuff"),
        "dsh" | "dsh-tui" | "deepseek" | "deepseek harness" => Some("dsh"),
        "devin" => Some("devin"),
        "empryo" | "em" => Some("empryo"),
        other => GPUI_DEFAULT_SIDEBAR_AGENTS
            .iter()
            .find(|agent| agent.agent_id == other || agent.icon == other)
            .map(|agent| agent.icon),
    }
}

pub(crate) fn gpui_previous_session_reference_from_history_id(
    history_id: &str,
) -> Option<(&str, &str)> {
    let payload = history_id.strip_prefix("gxserver:")?;
    let (project_id, session_id) = payload.split_once(':')?;
    if session_id.contains(':')
        || !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some((project_id, session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_agent_resolves_to_its_icon() {
        for agent in GPUI_DEFAULT_SIDEBAR_AGENTS {
            assert_eq!(
                gpui_sidebar_agent_icon(Some(agent.agent_id)),
                Some(agent.icon),
                "agent id {}",
                agent.agent_id
            );
            assert_eq!(
                gpui_sidebar_agent_icon(Some(agent.icon)),
                Some(agent.icon),
                "icon {}",
                agent.icon
            );
        }
    }

    /// A cloud session's tab carries its agent, so the chat gates take it: it has a chat agent and
    /// opens in Chat View even when the Default Agent View is Terminal.
    #[test]
    fn claude_cloud_tab_qualifies_for_chat() {
        let icon = gpui_sidebar_agent_icon(Some("claude-cloud"));
        assert_eq!(icon, Some(ghostex_gx_core::CLAUDE_CLOUD_AGENT_ID));
        assert_eq!(
            crate::app::session_chat_surfaces::session_chat_agent_for_icon(icon.unwrap()),
            Some(ghostex_gx_core::CLAUDE_CLOUD_AGENT_ID)
        );
        let mut settings = serde_json::Map::new();
        settings.insert("preferredAgentInterface".into(), "terminal".into());
        assert_eq!(
            gpui_effective_preferred_agent_interface_for_agent_icon(&settings, icon),
            GpuiPreferredAgentInterface::Chat
        );
    }
}
