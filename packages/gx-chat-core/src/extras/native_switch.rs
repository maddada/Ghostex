//! The "Switch to Native" strip chip and `•••` menu offer (`Q4768`–`Q4772`).
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! User picks (`native-announce`):
//! - Q4768 (`strip-chip`): compact "Switch to Native" chip in the chat pill strip for chat-first
//!   users on Terminal chats of Antigravity, Claude Code, and Codex, plus a manual row in `•••`.
//! - Q4770 (`per-agent-dismiss`): clicking `×` hides the offer per agent, and enabling Native for
//!   that agent retires the offer; manual switch stays in `•••`.
//! - Q4771 (`queue-when-done`): clicking mid-reply queues the switch until the reply finishes and
//!   shows a cancelable "Switching when done…" chip.
//! - Q4772 (`hide-threads-gate-remote`): hide the chip on coordinator threads (keep in `•••`); on
//!   remote computers only show when that computer's gxserver advertises `nativeModeSwitchCapable`.

use crate::document::NativeSwitchOffer;
use crate::state::ChatState;

/// Canonical family and user-facing display name for the three Native-capable agents.
pub fn canonical_native_agent(raw: &str) -> Option<(&'static str, &'static str)> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" | "agy" => Some(("antigravity", "Antigravity")),
        "claude" | "claude-code" => Some(("claude", "Claude Code")),
        "codex" | "codex-cli" => Some(("codex", "Codex")),
        _ => None,
    }
}

/// Computes the "Switch to Native" offer for this chat, or `None` when switching does not apply.
pub fn project(state: &ChatState) -> Option<NativeSwitchOffer> {
    if !state.session.native_mode_switch_capable
        || state.session.non_cli_mode
        || state.session.collab_guest.is_some()
    {
        return None;
    }
    let (family, agent_name) = state
        .session
        .agent
        .as_deref()
        .and_then(canonical_native_agent)
        .or_else(|| {
            state
                .session
                .session_agent_id
                .as_deref()
                .and_then(canonical_native_agent)
        })?;

    let is_remote = state
        .session
        .computer
        .computer
        .as_ref()
        .is_some_and(|c| c.remote);

    let native_default = if !is_remote {
        if let Some(modes) = state.core.agent_session_mode.as_ref() {
            modes
                .get(family)
                .is_some_and(|value| value.eq_ignore_ascii_case("non-cli"))
        } else {
            state.session.native_default_for_agent.unwrap_or(false)
        }
    } else {
        state.session.native_default_for_agent.unwrap_or(false)
    };

    let dismissed_by_settings = if !is_remote {
        if let Some(list) = state.core.native_switch_dismissed_agents.as_ref() {
            list.iter()
                .any(|entry| canonical_native_agent(entry).is_some_and(|(f, _)| f == family))
        } else {
            state.session.native_switch_dismissed.unwrap_or(false)
        }
    } else {
        state.session.native_switch_dismissed.unwrap_or(false)
    };
    let dismissed = dismissed_by_settings
        || state
            .extras
            .dismissed_native_switch_families
            .contains(family);

    let chat_first = if !is_remote {
        if state.core.preferred_agent_interface.is_some()
            || state.core.preferred_agent_interface_overrides.is_some()
        {
            let overrides = state.core.preferred_agent_interface_overrides.as_ref();
            let pref = state
                .session
                .session_agent_id
                .as_deref()
                .and_then(|id| overrides.and_then(|map| map.get(id)))
                .or_else(|| overrides.and_then(|map| map.get(family)))
                .or(state.core.preferred_agent_interface.as_ref())
                .map(String::as_str)
                .unwrap_or("chat");
            !pref.eq_ignore_ascii_case("terminal")
        } else {
            state.session.preferred_chat_first.unwrap_or(true)
        }
    } else {
        state.session.preferred_chat_first.unwrap_or(true)
    };

    let queued = state.session.switch_to_native_queued;
    let show_chip = queued
        || (!state.session.is_coordinator_thread && chat_first && !dismissed && !native_default);

    Some(NativeSwitchOffer {
        family: family.to_string(),
        agent_name: agent_name.to_string(),
        show_chip,
        queued,
        native_default,
        show_menu_item: true,
    })
}

/// Optimistically hides the strip chip for `family` in this chat while the host persists the dismissal.
pub fn handle_dismiss(state: &mut ChatState, family: &str) {
    if let Some((canonical, _)) = canonical_native_agent(family) {
        state
            .extras
            .dismissed_native_switch_families
            .insert(canonical.to_string());
    }
}
