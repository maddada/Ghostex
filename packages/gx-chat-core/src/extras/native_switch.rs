//! The "Switch to Native" strip chip and `•••` menu offer (`Q4768`–`Q4772`).
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! User picks (`native-announce`):
//! - Q4768 (`strip-chip`): compact "Switch to Native" chip in the chat pill strip for chat-first
//!   users on Terminal chats of Antigravity, Claude Code, and Codex, plus a manual row in `•••`.
//! - Q4770 (`per-agent-dismiss`): clicking `×` hides the offer per agent; manual switch stays in
//!   `•••`. The chip stays visible on Terminal chats (including coordinators) even when Native is
//!   already the default for new chats, until the chat itself is actually Native.
//! - Q4771 (`queue-when-done`): clicking mid-reply queues the switch until the reply finishes and
//!   shows a cancelable "Switches to Native when this reply ends" chip.
//! - Q4772 (`hide-threads-gate-remote`): hide the chip on coordinator threads (keep in `•••`); on
//!   remote computers only show when that computer's gxserver advertises `nativeModeSwitchCapable`.
//!
//! CDXC:AgentProviders 2026-11-25 DECISION:
//! User: "please show the reason in a tooltip so this pill isn't wide. pill should always say
//! Switching to Native (i)". A queued chip reads `Switching to Native` and an info icon; its
//! tooltip (desktop and web hover, the phone's tapped (i)) names what gxserver says the switch
//! waits on (`switchToNativeWait`, server/src/acp/channel.rs `SwitchWait`): the reply, background
//! work or queued messages, or an answer. Supersedes the always "when this reply ends" chip, which
//! was wrong once the reply was over and something else held the switch.

use crate::document::NativeSwitchOffer;
use crate::state::ChatState;

/// Canonical family and user-facing display name for Native-capable agents.
pub fn canonical_native_agent(raw: &str) -> Option<(&'static str, &'static str)> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" | "agy" => Some(("antigravity", "Antigravity")),
        "claude" | "claude-code" => Some(("claude", "Claude Code")),
        "codex" | "codex-cli" => Some(("codex", "Codex")),
        "opencode" | "opencode-cli" => Some(("opencode", "OpenCode")),
        "cursor" | "cursor-cli" | "cursor-agent" => Some(("cursor", "Cursor CLI")),
        "omp" | "omp-cli" | "oh-my-pi" => Some(("omp", "OMP")),
        "pi" | "pi-cli" => Some(("pi", "Pi")),
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
        .session_agent_id
        .as_deref()
        .and_then(canonical_native_agent)
        .or_else(|| {
            state
                .session
                .agent
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
    let queued_tooltip = queued.then(|| {
        let reason = queued_reason(state.session.switch_to_native_wait.as_deref());
        format!("{reason}\nClick × to cancel.")
    });
    // Keep the strip chip visible on Terminal chats (including coordinators) even when Native is
    // already the default for new chats, until this chat is actually Native (`non_cli_mode == true`)
    // or the user explicitly dismisses the chip per agent.
    let show_chip = queued || (!state.session.is_coordinator_thread && chat_first && !dismissed);

    Some(NativeSwitchOffer {
        family: family.to_string(),
        agent_name: agent_name.to_string(),
        show_chip,
        queued,
        queued_tooltip,
        native_default,
        show_menu_item: true,
    })
}

/// What a queued switch waits on, in the words the chip's tooltip and the queued toast use, from
/// gxserver's `switchToNativeWait` (chat state) or `wait` (the `switchSessionCliToAcp` answer).
/// SEE-ALSO: apps/mobile/app/src/chat/native/NativeChatScreen.tsx (`queuedSwitchReason`, the
/// phone's copy for its toast).
pub fn queued_reason(wait: Option<&str>) -> &'static str {
    match wait {
        Some("background") => "Switches to Native when background work ends.",
        Some("answer") => "Switches to Native after you answer.",
        // `"reply"`, or an older gxserver that does not say.
        _ => "Switches to Native when this reply ends.",
    }
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
