// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the workspace-tab agent icon/status
// visuals, the loading spinner paint helper, and the first-prompt-title
// blocking overlay. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Bounds, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Styled as _, Window, div, px, rgb,
};
use gpui_component::v_flex;

use crate::*;

pub(crate) fn paint_agent_gui_loading_spinner(bounds: Bounds<Pixels>, window: &mut Window) {
    let center_x = bounds.left().as_f32() + bounds.size.width.as_f32() / 2.0;
    let center_y = bounds.top().as_f32() + bounds.size.height.as_f32() / 2.0;
    let radius = 7.0;
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f32();
    let start_angle = elapsed * std::f32::consts::TAU * 1.35;
    let sweep = std::f32::consts::PI * 1.35;
    let end_angle = start_angle + sweep;
    let radii = gpui::point(px(radius), px(radius));
    let mut path = gpui::PathBuilder::stroke(px(1.6));
    path.move_to(gpui::point(
        px(center_x + radius * start_angle.cos()),
        px(center_y + radius * start_angle.sin()),
    ));
    path.arc_to(
        radii,
        px(0.0),
        sweep > std::f32::consts::PI,
        true,
        gpui::point(
            px(center_x + radius * end_angle.cos()),
            px(center_y + radius * end_angle.sin()),
        ),
    );
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(0xffffff).opacity(0.72));
    }
}

pub(crate) fn agents_drop_feedback_border_color() -> Hsla {
    rgb(0xffffff).opacity(0.42).into()
}

pub(crate) fn agents_drop_group_feedback_color() -> Hsla {
    rgb(0xffffff).opacity(0.08).into()
}

pub(crate) fn agents_drop_split_feedback_color() -> Hsla {
    rgb(0xffffff).opacity(0.12).into()
}

pub(crate) fn workspace_tab_agent_icon_path(agent_icon: &str) -> Option<&'static str> {
    match agent_icon {
        "amp-cli" => Some("agent-icons/amp-cli.svg"),
        "antigravity-cli" => Some("agent-icons/antigravity-cli.svg"),
        "browser" => Some("agent-icons/browser.svg"),
        "campfire" => Some("agent-icons/campfire.svg"),
        "claude" => Some("agent-icons/claude.svg"),
        "codebuddy" => Some("agent-icons/codebuddy.svg"),
        "command-code" => Some("agent-icons/command-code.svg"),
        "cursor-cli" => Some("agent-icons/cursor-cli.svg"),
        "codex" => Some("agent-icons/codex.svg"),
        "copilot" => Some("agent-icons/copilot.svg"),
        "devin" => Some("agent-icons/devin.svg"),
        "factory-droid" => Some("agent-icons/factory-droid.svg"),
        "gemini" => Some("agent-icons/gemini.svg"),
        "grok-build" => Some("agent-icons/grok-build.svg"),
        "hermes-agent" => Some("agent-icons/hermes-agent.svg"),
        "kimi" => Some("agent-icons/kimi.svg"),
        "kiro" => Some("agent-icons/kiro.svg"),
        "omp" => Some("agent-icons/omp.svg"),
        "openclaude" => Some("agent-icons/openclaude.svg"),
        "opencode" => Some("agent-icons/opencode.svg"),
        "pi" => Some("agent-icons/pi.svg"),
        "qoder" => Some("agent-icons/qoder.svg"),
        "rovo-dev" => Some("agent-icons/rovo-dev.svg"),
        _ => None,
    }
}

pub(crate) fn workspace_tab_agent_svg_size(agent_icon: &str) -> f32 {
    match agent_icon {
        "amp-cli" => 12.0,
        "codex" | "copilot" | "gemini" | "pi" => 10.5,
        "claude" => 11.0,
        _ => 11.5,
    }
}

pub(crate) fn workspace_tab_agent_icon_accent_color(agent_icon: &str) -> u32 {
    match agent_icon {
        "amp-cli" => 0xffffff,
        "antigravity-cli" => 0x749bff,
        "browser" => 0x82b7ff,
        "campfire" => 0xff8a3d,
        "claude" => 0xd97757,
        "codebuddy" => 0x72d6ff,
        "command-code" => 0x22d3ee,
        "cursor-cli" => 0xedecec,
        "codex" => 0xffffff,
        "copilot" => 0xffffff,
        "devin" => 0x3ea6ff,
        "factory-droid" => 0xff7a1a,
        "gemini" => 0x8b9aff,
        "grok-build" => 0xffffff,
        "hermes-agent" => 0xf3c46b,
        "kimi" => 0x7b6cf6,
        "kiro" => 0xa6e3ff,
        "omp" => 0xc8ff62,
        "openclaude" => 0xf0a68a,
        "opencode" => 0x6d96c0,
        "pi" => 0xc8ff62,
        "qoder" => 0xa991ff,
        "rovo-dev" => 0x4fc3a1,
        _ => 0xffffff,
    }
}

pub(crate) fn workspace_tab_agent_icon_opacity(
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> f32 {
    if visual_tone.uses_selected_treatment() {
        1.0
    } else if visual_tone.uses_inactive_running_treatment() {
        0.78
    } else {
        debug_assert!(visual_tone.uses_subdued_non_running_treatment());
        0.46
    }
}

pub(crate) fn workspace_tab_agent_icon_text_color(
    agent_icon: &str,
    visual_tone: WorkspaceTabLifecycleVisualTone,
) -> Hsla {
    rgb(workspace_tab_agent_icon_accent_color(agent_icon))
        .opacity(workspace_tab_agent_icon_opacity(visual_tone))
        .into()
}

pub(crate) fn agent_terminal_tab_status_color(tab_status: AgentTerminalTabStatus) -> u32 {
    match tab_status {
        AgentTerminalTabStatus::Idle => 0x67d886,
        AgentTerminalTabStatus::Working => 0xf59e0b,
        AgentTerminalTabStatus::Attention => 0x95d7f6,
        AgentTerminalTabStatus::DelayedSend => 0xffcf5a,
    }
}

pub(crate) fn agent_terminal_tab_status_inactive_opacity(
    tab_status: AgentTerminalTabStatus,
) -> f32 {
    match tab_status {
        AgentTerminalTabStatus::Idle => 0.58,
        AgentTerminalTabStatus::Working => 0.52,
        AgentTerminalTabStatus::Attention => 0.52,
        AgentTerminalTabStatus::DelayedSend => 0.50,
    }
}

/// Blocking pane overlay shown while gxserver generates a session's
/// first-prompt title, ported from the managed AppKit pane overlay.
///
/// CDXC:SessionTitles 2026-07-26:
/// While Ghostex generates a session title the terminal pane shows a centered
/// dimming overlay with white medium-weight status text and a smaller dim
/// Escape hint. The overlay is a normal absolutely-positioned child of the
/// body it covers, and it occludes pointer input for exactly that rectangle so
/// clicks cannot reach the suppressed terminal underneath. Keyboard input stays
/// with the terminal element, which consumes everything except Escape until
/// generation finishes or is cancelled.
pub(crate) fn render_agents_first_prompt_title_overlay(
    pane_id: WorkspacePaneId,
    session_id: TerminalSessionId,
) -> AnyElement {
    v_flex()
        .id(format!(
            "ghostex-gpui-terminal-first-prompt-title-overlay-{}-{}",
            pane_id.0, session_id.0
        ))
        .occlude()
        .absolute()
        .inset_0()
        .items_center()
        .justify_center()
        .bg(rgb(0x000000).opacity(0.58))
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0xffffff).opacity(0.96))
                .child("Generating title"),
        )
        .child(
            div()
                .mt(px(5.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(0xffffff).opacity(0.58))
                .child("(ESC to Cancel)"),
        )
        .into_any_element()
}
