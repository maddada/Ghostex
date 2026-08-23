// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the workspace/terminal/command-pane
// color and theme helper functions. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::sync::{Arc, atomic::Ordering};

use gpui::{
    AnyElement, Hsla, Image, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px, rgb, rgba,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_project_icon_image_from_data_url(value: &str) -> Option<Arc<Image>> {
    /*
    CDXC:GPUITitlebarProjectIcon 2026-07-04-03:00:
    The titlebar project icon is render-only and may come only from the explicit
    active-project `projectIconDataUrl` snapshot field. Decode the already
    bounded image data URL for the 16px titlebar slot without probing paths,
    fetching URLs, synthesizing initials, or persisting image bytes.
    */
    browser_favicon_image_from_data_url(value).map(|image| image.image)
}

pub(crate) fn tab_bar_button_hover_color() -> Hsla {
    rgb(0x222222).into()
}

pub(crate) fn workspace_background_color() -> Hsla {
    rgb(GPUI_WORKSPACE_BACKGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

pub(crate) fn source_view_background_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

/// One-shot startup read of the Ghostty config `background` color (macOS
/// parity: `ghostexRootView(defaultWorkspaceBackgroundColor:
/// ghosttyConfigColor("background") ?? .black)`). Runs before the GPUI window
/// opens; when the config carries no background value the fixed shell default
/// stays in place, matching the macOS `?? .black` contract. Live config reload
/// is intentionally out of scope for this slice.
pub(crate) fn initialize_workspace_background_color_from_ghostty_config() {
    #[cfg(target_os = "macos")]
    let background = ghostty_config_background_rgb_one_shot().unwrap_or(0x050505);
    #[cfg(not(target_os = "macos"))]
    let background: u32 = 0x050505;
    GPUI_GHOSTTY_WORKSPACE_BACKGROUND_RGB.store(u64::from(background), Ordering::Relaxed);
    GPUI_WORKSPACE_BACKGROUND_RGB.store(u64::from(background), Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
pub(crate) fn ghostty_config_background_rgb_one_shot() -> Option<u32> {
    terminal_ghostty_surface::load_default_ghostty_background_color()
        .map(|color| (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b))
}

pub(crate) fn gpui_settings_hex_rgb(value: Option<&serde_json::Value>) -> Option<u32> {
    let value = value?.as_str()?.trim();
    let hex = value.strip_prefix('#').unwrap_or(value);
    (hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

pub(crate) fn refresh_gpui_visual_settings(settings: &shared_settings::SharedSidebarSettingsSnapshot) {
    let object = settings.object();
    let configured_workspace = object
        .get("workspaceBackgroundColor")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .and_then(|_| gpui_settings_hex_rgb(object.get("workspaceBackgroundColor")))
        .map(|rgb| if rgb == 0 { 0x010101 } else { rgb });
    let workspace = configured_workspace
        .unwrap_or_else(|| GPUI_GHOSTTY_WORKSPACE_BACKGROUND_RGB.load(Ordering::Relaxed) as u32);
    GPUI_WORKSPACE_BACKGROUND_RGB.store(u64::from(workspace), Ordering::Relaxed);

    let custom_titlebar_enabled = object
        .get("customSidebarTitlebarColorsEnabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    /*
    CDXC:GPUITitlebarGradient 2026-07-22:
    The saved `customSidebarTitlebarBackgroundColor` hex is a legacy migration
    seed only — since the contrast-slider redesign the sidebar resolves the
    effective chrome background from `customSidebarTitlebarBackgroundDarkness-
    Percent` plus the tint (getSidebarTitlebarBackgroundForDarkness in
    packages/shared/ghostex-settings.ts). Reading the stale saved hex here made the
    Rust titlebar derive its color (and gradient stops) from a darker base
    than the sidebar actually renders. Mirror the TS resolution instead.
    */
    let titlebar_background = if custom_titlebar_enabled {
        resolved_custom_sidebar_titlebar_background(object)
    } else {
        0x0e0e0e
    };
    let titlebar_foreground = custom_titlebar_enabled
        .then(|| gpui_settings_hex_rgb(object.get("customSidebarTitlebarForegroundColor")))
        .flatten()
        .unwrap_or(0xffffff);
    GPUI_TITLEBAR_BACKGROUND_RGB.store(u64::from(titlebar_background), Ordering::Relaxed);
    /*
    CDXC:GPUITitlebarGradient 2026-07-22:
    The shared sidebar renders custom chrome as a fixed-strength gradient
    derived from the resolved titlebar background
    (getSidebarTitlebarGradientColors in packages/shared/ghostex-settings.ts), and the
    titlebar shares those exact stops horizontally: left = the sidebar's top
    stop (darker), right = the sidebar's bottom stop. A flat Rust titlebar
    therefore never matched the gradient sidebar. Mirror the TS derivation
    here so the GPUI titlebar strip fades with the same colors; when custom
    chrome is disabled the stops collapse to the flat titlebar color.
    */
    let (gradient_left, gradient_right) = if custom_titlebar_enabled {
        sidebar_titlebar_gradient_stops(titlebar_background)
    } else {
        (titlebar_background, titlebar_background)
    };
    GPUI_TITLEBAR_GRADIENT_LEFT_RGB.store(u64::from(gradient_left), Ordering::Relaxed);
    GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.store(u64::from(gradient_right), Ordering::Relaxed);
    GPUI_TITLEBAR_FOREGROUND_RGB.store(u64::from(titlebar_foreground), Ordering::Relaxed);
}

pub(crate) fn workspace_tab_drag_preview_color() -> Hsla {
    rgb(0x242424).opacity(0.94).into()
}

pub(crate) fn workspace_drop_feedback_border_color() -> Hsla {
    rgb(0x58b7ff).opacity(0.92).into()
}

pub(crate) fn workspace_drop_group_feedback_color() -> Hsla {
    rgb(0x58b7ff).opacity(0.12).into()
}

pub(crate) fn workspace_drop_split_feedback_color() -> Hsla {
    rgb(0x58b7ff).opacity(0.18).into()
}

pub(crate) fn workspace_drop_feedback_label_color(zone: WorkspaceDropZone) -> Hsla {
    match zone {
        WorkspaceDropZone::Center => rgb(0x122235).opacity(0.96).into(),
        WorkspaceDropZone::Left
        | WorkspaceDropZone::Right
        | WorkspaceDropZone::Top
        | WorkspaceDropZone::Bottom => rgb(0x0c2948).opacity(0.96).into(),
    }
}

pub(crate) fn workspace_drop_feedback_text_color() -> Hsla {
    rgb(0xe7f3ff).into()
}

pub(crate) fn workspace_tab_bar_color() -> Hsla {
    rgb(0x050608).opacity(0.96).into()
}

pub(crate) fn workspace_tab_background_color(visual_tone: WorkspaceTabLifecycleVisualTone) -> Hsla {
    let white_overlay_alpha = if visual_tone.uses_selected_treatment() {
        WORKSPACE_TAB_SELECTED_WHITE_OVERLAY_ALPHA
    } else {
        WORKSPACE_TAB_INACTIVE_WHITE_OVERLAY_ALPHA
    };
    workspace_tab_white_overlay_over_bar_color(white_overlay_alpha)
}

pub(crate) fn workspace_tab_white_overlay_over_bar_color(alpha: f32) -> Hsla {
    let channel =
        |base: u8| -> u32 { (base as f32 + (255.0 - base as f32) * alpha).round() as u32 };
    let red = channel(0x05);
    let green = channel(0x06);
    let blue = channel(0x08);
    rgb((red << 16) | (green << 8) | blue).into()
}

pub(crate) fn workspace_tab_reorder_insertion_marker_color() -> Hsla {
    rgb(0x70adff).opacity(0.95).into()
}

pub(crate) fn workspace_tab_action_cluster_color() -> Hsla {
    workspace_tab_action_button_color()
}

pub(crate) fn workspace_tab_action_button_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn workspace_tab_action_left_border_color() -> Hsla {
    rgb(0x252525).into()
}

pub(crate) fn workspace_tab_action_icon_color() -> Hsla {
    rgb(0xcfcfcf).into()
}

pub(crate) fn workspace_tab_border_color() -> Hsla {
    rgb(0x252525).into()
}

pub(crate) fn workspace_tab_text_color(visual_tone: WorkspaceTabLifecycleVisualTone) -> Hsla {
    if visual_tone.uses_selected_treatment() {
        workspace_tab_active_text_color()
    } else {
        workspace_tab_inactive_text_color()
    }
}

pub(crate) fn workspace_tab_active_text_color() -> Hsla {
    rgb(0xf5f5f5).opacity(0.98).into()
}

pub(crate) fn workspace_tab_inactive_text_color() -> Hsla {
    rgb(0xc7c7c7).opacity(0.82).into()
}

pub(crate) fn workspace_tab_terminal_icon_active_background(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.12).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.18).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.18).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.18).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.17).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.18).into()
        }
    }
}

pub(crate) fn workspace_tab_terminal_icon_inactive_background(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    if presentation_state.is_running() {
        rgb(0xffffff).opacity(0.06).into()
    } else {
        rgb(0xffffff).opacity(0.035).into()
    }
}

pub(crate) fn workspace_tab_terminal_icon_active_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.42).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.72).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.74).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.74).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.72).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.72).into()
        }
    }
}

pub(crate) fn workspace_tab_terminal_icon_inactive_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    if presentation_state.is_running() {
        rgb(0xffffff).opacity(0.24).into()
    } else {
        rgb(0xffffff).opacity(0.16).into()
    }
}

pub(crate) fn workspace_tab_terminal_icon_glyph_color(visual_tone: WorkspaceTabLifecycleVisualTone) -> Hsla {
    if visual_tone.uses_selected_treatment() {
        match visual_tone.presentation_state {
            TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.76).into(),
            TerminalSessionPresentationState::Sleeping => rgb(0xc7e4ff).opacity(0.86).into(),
            TerminalSessionPresentationState::Mounting => rgb(0xffe2a2).opacity(0.86).into(),
            TerminalSessionPresentationState::StartupFailed => rgb(0xffc2c2).opacity(0.86).into(),
            TerminalSessionPresentationState::RestoredUnmounted => {
                rgb(0xc4f2d2).opacity(0.84).into()
            }
            TerminalSessionPresentationState::PoppedOutPlaceholder => {
                rgb(0xffd0df).opacity(0.86).into()
            }
        }
    } else {
        rgb(0xffffff)
            .opacity(if visual_tone.uses_inactive_running_treatment() {
                0.42
            } else {
                debug_assert!(visual_tone.uses_subdued_non_running_treatment());
                0.24
            })
            .into()
    }
}

pub(crate) fn workspace_tab_status_dot_color(
    visual_tone: WorkspaceTabLifecycleVisualTone,
    tab_status: AgentTerminalTabStatus,
) -> Hsla {
    let is_active = visual_tone.uses_selected_treatment();
    match visual_tone.presentation_state {
        TerminalSessionPresentationState::Running => {
            workspace_tab_running_status_dot_color(tab_status, is_active)
        }
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff)
            .opacity(if is_active { 0.88 } else { 0.34 })
            .into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d)
            .opacity(if is_active { 0.90 } else { 0.36 })
            .into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b)
            .opacity(if is_active { 0.90 } else { 0.36 })
            .into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a)
            .opacity(if is_active { 0.88 } else { 0.34 })
            .into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => rgb(0xff7ca8)
            .opacity(if is_active { 0.88 } else { 0.34 })
            .into(),
    }
}

pub(crate) fn workspace_tab_running_status_dot_color(
    tab_status: AgentTerminalTabStatus,
    is_active: bool,
) -> Hsla {
    let color = rgb(agent_terminal_tab_status_color(tab_status));
    if is_active {
        color.into()
    } else {
        color
            .opacity(agent_terminal_tab_status_inactive_opacity(tab_status))
            .into()
    }
}

pub(crate) fn workspace_tab_state_badge_background(visual_tone: WorkspaceTabLifecycleVisualTone) -> Hsla {
    let is_active = visual_tone.uses_selected_treatment();
    match visual_tone.presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.0).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff)
            .opacity(if is_active { 0.18 } else { 0.07 })
            .into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d)
            .opacity(if is_active { 0.18 } else { 0.07 })
            .into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b)
            .opacity(if is_active { 0.18 } else { 0.07 })
            .into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a)
            .opacity(if is_active { 0.16 } else { 0.065 })
            .into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => rgb(0xff7ca8)
            .opacity(if is_active { 0.17 } else { 0.07 })
            .into(),
    }
}

pub(crate) fn workspace_tab_state_badge_text_color(visual_tone: WorkspaceTabLifecycleVisualTone) -> Hsla {
    let is_active = visual_tone.uses_selected_treatment();
    match visual_tone.presentation_state {
        TerminalSessionPresentationState::Running => workspace_tab_text_color(visual_tone),
        TerminalSessionPresentationState::Sleeping => rgb(0xc9e6ff)
            .opacity(if is_active { 0.92 } else { 0.44 })
            .into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffdf9a)
            .opacity(if is_active { 0.92 } else { 0.44 })
            .into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xffc6c6)
            .opacity(if is_active { 0.92 } else { 0.44 })
            .into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0xc6f1d2)
            .opacity(if is_active { 0.90 } else { 0.42 })
            .into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => rgb(0xffccdc)
            .opacity(if is_active { 0.92 } else { 0.44 })
            .into(),
    }
}

pub(crate) fn workspace_tab_close_active_color() -> Hsla {
    rgb(0xffffff).opacity(0.76).into()
}

pub(crate) fn workspace_tab_close_inactive_color() -> Hsla {
    rgb(0xffffff).opacity(0.46).into()
}

pub(crate) fn workspace_tab_close_hover_color() -> Hsla {
    tab_bar_button_hover_color()
}

pub(crate) fn workspace_terminal_placeholder_color() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn terminal_search_count_label(search: &GpuiTerminalSearchState) -> String {
    if search.needle.trim().is_empty() {
        return String::new();
    }
    if search.total == Some(0) {
        return "N/A".to_string();
    }
    match (search.selected, search.total) {
        (Some(selected), Some(total)) => format!("{}/{}", selected + 1, total),
        (Some(selected), None) => format!("{}/?", selected + 1),
        (None, Some(total)) => format!("-/{total}"),
        (None, None) => String::new(),
    }
}

pub(crate) fn terminal_search_bar_row_color() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn terminal_search_bar_divider_color() -> Hsla {
    rgb(0x202020).into()
}

pub(crate) fn terminal_search_bar_background_color() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn terminal_search_bar_border_color() -> Hsla {
    rgb(0x252525).into()
}

/// The same yellow the sidebar's queued-prompt badge uses, so one queue never
/// looks like two different things in two places.
pub(crate) fn terminal_queued_prompts_dot_color() -> Hsla {
    rgb(0xf6c945).into()
}

/// The sidebar's error red (`.session-status-dot-anchored[data-lifecycle-state
/// ="error"]`), which the queued-prompt badge also switches to when a row has
/// failed, so a stalled queue reads the same in the sidebar and in the pane.
pub(crate) fn terminal_queued_prompts_failed_dot_color() -> Hsla {
    rgb(0xff6b6b).into()
}

pub(crate) fn terminal_queued_prompts_text_color() -> Hsla {
    rgba(0xffffffe0).into()
}

pub(crate) fn terminal_queued_prompts_background_color() -> Hsla {
    rgb(0x1b1b1b).into()
}

pub(crate) fn terminal_queued_prompts_hover_color() -> Hsla {
    rgb(0x2a2a2a).into()
}

pub(crate) fn terminal_queued_prompts_border_color() -> Hsla {
    rgb(0x323232).into()
}

pub(crate) fn terminal_search_bar_text_color() -> Hsla {
    rgba(0xffffffef).into()
}

pub(crate) fn terminal_search_bar_count_color() -> Hsla {
    rgba(0xffffffb8).into()
}

pub(crate) fn terminal_search_bar_button_color() -> Hsla {
    rgb(0xcfcfcf).into()
}

pub(crate) fn terminal_search_bar_button_background_color() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn terminal_search_bar_button_hover_color() -> Hsla {
    rgb(0x343434).into()
}

pub(crate) fn workspace_terminal_body_color(
    presentation_state: Option<TerminalSessionPresentationState>,
) -> Hsla {
    match presentation_state {
        Some(TerminalSessionPresentationState::Running) => workspace_terminal_placeholder_color(),
        Some(TerminalSessionPresentationState::Sleeping) => rgb(0x000000).into(),
        Some(TerminalSessionPresentationState::Mounting) => rgb(0x000000).into(),
        Some(TerminalSessionPresentationState::StartupFailed) => rgb(0x140908).into(),
        Some(TerminalSessionPresentationState::RestoredUnmounted) => rgb(0x08110d).into(),
        Some(TerminalSessionPresentationState::PoppedOutPlaceholder) => rgb(0x13090f).into(),
        None => rgb(0x090b0f).into(),
    }
}

pub(crate) fn workspace_terminal_placeholder_card_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0x000000).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x101923).into(),
        TerminalSessionPresentationState::Mounting => rgb(0x1c160b).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0x21100f).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x101b15).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => rgb(0x1d1118).into(),
    }
}

pub(crate) fn workspace_terminal_placeholder_border_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0x242424).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.22).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.22).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.24).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.20).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.22).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_badge_background(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.10).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.18).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.18).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.18).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.16).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.18).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_badge_text_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.82).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0xc9e6ff).opacity(0.96).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffdf9a).opacity(0.96).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xffc6c6).opacity(0.96).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0xc6f1d2).opacity(0.94).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xffccdc).opacity(0.96).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_title_color() -> Hsla {
    rgb(0xffffff).opacity(0.92).into()
}

pub(crate) fn workspace_terminal_placeholder_message_color() -> Hsla {
    rgb(0xe5e8ec).opacity(0.64).into()
}

pub(crate) fn workspace_terminal_placeholder_session_color() -> Hsla {
    rgb(0xe5e8ec).opacity(0.46).into()
}

pub(crate) fn workspace_terminal_placeholder_action_border_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    workspace_terminal_placeholder_border_color(presentation_state)
}

pub(crate) fn workspace_terminal_placeholder_action_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.06).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.11).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.11).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.11).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.10).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.11).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_action_hover_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => rgb(0xffffff).opacity(0.09).into(),
        TerminalSessionPresentationState::Sleeping => rgb(0x6bb7ff).opacity(0.16).into(),
        TerminalSessionPresentationState::Mounting => rgb(0xffc14d).opacity(0.16).into(),
        TerminalSessionPresentationState::StartupFailed => rgb(0xff6b6b).opacity(0.16).into(),
        TerminalSessionPresentationState::RestoredUnmounted => rgb(0x75d69a).opacity(0.15).into(),
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            rgb(0xff7ca8).opacity(0.16).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_action_text_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    workspace_terminal_placeholder_badge_text_color(presentation_state)
}

pub(crate) fn workspace_pane_border_color() -> Hsla {
    rgb(0x202020).into()
}

pub(crate) fn workspace_pane_focused_border_color() -> Hsla {
    rgb(0x6d6d6d).into()
}

pub(crate) fn workspace_pane_attention_border_color() -> Hsla {
    rgb(0x95d7f6).into()
}

pub(crate) fn workspace_pane_border_color_for_state(state: WorkspacePaneBorderState) -> Hsla {
    match state {
        WorkspacePaneBorderState::Neutral => workspace_pane_border_color(),
        WorkspacePaneBorderState::Focused => workspace_pane_focused_border_color(),
        WorkspacePaneBorderState::Attention => workspace_pane_attention_border_color(),
    }
}

pub(crate) fn project_editor_companion_border_color_for_state(state: WorkspacePaneBorderState) -> Hsla {
    match state {
        WorkspacePaneBorderState::Neutral => rgb(0x252525).into(),
        WorkspacePaneBorderState::Focused => workspace_pane_focused_border_color(),
        WorkspacePaneBorderState::Attention => workspace_pane_attention_border_color(),
    }
}

pub(crate) fn workspace_split_handle_color() -> Hsla {
    rgb(0x0c0c0c).into()
}

pub(crate) fn workspace_split_separator_color() -> Hsla {
    rgb(0x333333).opacity(0.0).into()
}

pub(crate) fn project_editor_shell_background_color() -> Hsla {
    rgb(0x050505).into()
}

pub(crate) fn project_editor_companion_divider_background_color() -> Hsla {
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn project_editor_companion_divider_line_color() -> Hsla {
    rgb(0x000000).opacity(0.0).into()
}









pub(crate) fn command_pane_chrome_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    Native command-panel chrome and command titlebars use an opaque black background. Keep GPUI command chrome on black instead of the generic dark titlebar gray so tabs, tab-add, and panel actions sit on the same base as macOS.
    */
    rgb(0x000000).into()
}

pub(crate) fn command_pane_strip_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    The collapsed command strip is native command titlebar chrome with side margins, so its background stays black like expanded command titlebars.
    */
    command_pane_chrome_color()
}

pub(crate) fn command_pane_panel_separator_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    Native command-panel boundaries use the workspace separator line #1e1e1e for the panel edge, separate from focused pane outlines and titlebar command separators.
    */
    rgb(0x1e1e1e).into()
}

pub(crate) fn command_pane_border_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    Native inactive command terminal pane outlines use #111111, not the translucent command titlebar separator. Keep the inactive command group outline distinct from titlebar chrome.
    */
    rgb(0x111111).into()
}

pub(crate) fn command_pane_side_edge_color() -> Hsla {
    rgb(0x252525).into()
}

pub(crate) fn command_pane_hidden_border_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneFocus 2026-06-25-18:02:
    Pinned native command panels set inactive command borders to nil. Use a transparent GPUI border color instead of removing the border frame so split command groups keep stable layout while matching the hidden inactive outline.
    */
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn command_pane_focused_border_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneChrome 2026-06-25-13:19:
    Native focused terminal pane chrome is a neutral #737373 outline at 95% alpha, while #95d7f6 remains reserved for attention/done status. Do not tint command focus blue.
    */
    rgb(0x737373).opacity(0.95).into()
}

pub(crate) fn command_pane_tab_background_color(is_active: bool, is_sleeping: bool) -> Hsla {
    /*
    CDXC:GPUICommandTabBackground 2026-06-25-14:36:
    Match macOS `compositedWorkspaceTabColor` for command-role tabs instead of using generic GPUI dark fills. The channel math keeps the active and inactive tab backgrounds tied to the native AppKit source values.

    CDXC:GPUICommandTabSleepVisuals 2026-06-25-14:39:
    Native command-role sleeping tabs keep the active fill when selected and use the parked 3.2% inactive overlay only as inactive siblings.
    */
    let overlay_alpha = if is_active {
        COMMAND_PANE_TAB_ACTIVE_OVERLAY_ALPHA
    } else if is_sleeping {
        COMMAND_PANE_TAB_SLEEPING_INACTIVE_OVERLAY_ALPHA
    } else {
        COMMAND_PANE_TAB_INACTIVE_OVERLAY_ALPHA
    };
    command_pane_native_composited_tab_color(overlay_alpha)
}

pub(crate) fn command_pane_tab_hover_background_color(is_active: bool, is_sleeping: bool) -> Hsla {
    /*
    CDXC:GPUICommandTabBackground 2026-06-25-14:36:
    Native command tabs do not brighten the tab fill on hover; hover state only affects the drawn trailing status/close affordance.
    */
    command_pane_tab_background_color(is_active, is_sleeping)
}

pub(crate) fn command_pane_native_composited_tab_color(overlay_alpha: f32) -> Hsla {
    let channel =
        |base: u8| -> u32 { (base as f32 + (255.0 - base as f32) * overlay_alpha).round() as u32 };
    let red = channel(COMMAND_PANE_TAB_BACKGROUND_BASE_RED);
    let green = channel(COMMAND_PANE_TAB_BACKGROUND_BASE_GREEN);
    let blue = channel(COMMAND_PANE_TAB_BACKGROUND_BASE_BLUE);
    rgb((red << 16) | (green << 8) | blue).into()
}

pub(crate) fn command_pane_tab_title_text_color(is_active: bool, is_sleeping: bool) -> Hsla {
    /*
    CDXC:GPUICommandTabSleepVisuals 2026-06-25-14:39:
    Command-role tab titles use selected-label white for both active and inactive tabs, but inactive sleeping tabs multiply title alpha by the native 0.48 parked-session treatment. Active sleeping tabs keep full selected label opacity.
    */
    let sleep_alpha_multiplier = if is_sleeping && !is_active {
        COMMAND_PANE_TAB_TITLE_SLEEPING_INACTIVE_ALPHA_MULTIPLIER
    } else {
        1.0
    };
    rgb(0xf5f5f5).opacity(0.98 * sleep_alpha_multiplier).into()
}

pub(crate) fn command_pane_tab_separator_color() -> Hsla {
    /*
    CDXC:GPUICommandTabSeparators 2026-06-25-14:17:
    macOS command tab separators use calibrated white at 10% alpha, separate from the heavier command-pane structural border color.
    */
    rgb(0xffffff).opacity(0.10).into()
}

pub(crate) fn command_pane_tab_status_indicator_element(
    element_id: impl Into<String>,
    tab_status: CommandTerminalTabStatus,
) -> AnyElement {
    let indicator_color = command_pane_tab_status_indicator_color(tab_status);
    match tab_status {
        CommandTerminalTabStatus::DelayedSend => div()
            .id(element_id.into())
            .absolute()
            .right(px(COMMAND_PANE_TAB_DELAYED_SEND_ICON_TRAILING_PADDING))
            .top(px(COMMAND_PANE_TAB_DELAYED_SEND_ICON_TOP_OFFSET))
            .flex()
            .size(px(COMMAND_PANE_TAB_DELAYED_SEND_ICON_SIZE))
            .items_center()
            .justify_center()
            .text_color(indicator_color)
            .child(titlebar_svg_icon(
                COMMAND_ICON_CLOCK,
                COMMAND_PANE_TAB_DELAYED_SEND_ICON_SIZE,
                indicator_color,
            ))
            .into_any_element(),
        CommandTerminalTabStatus::Working | CommandTerminalTabStatus::Attention => div()
            .id(element_id.into())
            .absolute()
            .right(px(COMMAND_PANE_TAB_STATUS_INDICATOR_TRAILING_PADDING))
            .top(px(COMMAND_PANE_TAB_STATUS_INDICATOR_TOP_OFFSET))
            .size(px(COMMAND_PANE_TAB_STATUS_INDICATOR_SIZE))
            .rounded_full()
            .bg(indicator_color)
            .into_any_element(),
        CommandTerminalTabStatus::Idle => {
            div().id(element_id.into()).size(px(0.0)).into_any_element()
        }
    }
}

pub(crate) fn command_pane_tab_status_indicator_color(tab_status: CommandTerminalTabStatus) -> Hsla {
    let color = rgb(command_terminal_tab_status_color(tab_status));
    color
        .opacity(command_terminal_tab_status_indicator_opacity(tab_status))
        .into()
}

pub(crate) fn command_terminal_tab_status_has_indicator(tab_status: CommandTerminalTabStatus) -> bool {
    !matches!(tab_status, CommandTerminalTabStatus::Idle)
}

pub(crate) fn command_terminal_tab_status_indicator_visible(
    tab_status: CommandTerminalTabStatus,
    tab_hovered: bool,
) -> bool {
    /*
    CDXC:GPUICommandTabStatus 2026-06-25-13:18:
    Native command tabs hide working/attention/Delayed Send status chrome while the tab is hovered so the inline close affordance owns the trailing slot. Title reservation remains status-based, not hover-based, to avoid reflow.
    */
    command_terminal_tab_status_has_indicator(tab_status) && !tab_hovered
}

pub(crate) fn command_terminal_tab_status_title_trailing_reserved_width(
    tab_status: CommandTerminalTabStatus,
) -> f32 {
    if command_terminal_tab_status_has_indicator(tab_status) {
        COMMAND_PANE_TAB_STATUS_TITLE_RESERVED_WIDTH
    } else {
        COMMAND_PANE_TAB_TITLE_TRAILING_PADDING
    }
}

pub(crate) fn command_terminal_tab_status_color(tab_status: CommandTerminalTabStatus) -> u32 {
    match tab_status {
        CommandTerminalTabStatus::Idle => 0x58b7ff,
        CommandTerminalTabStatus::Working => 0xf59e0b,
        CommandTerminalTabStatus::Attention => 0x95d7f6,
        CommandTerminalTabStatus::DelayedSend => 0xf59e0b,
    }
}

pub(crate) fn command_terminal_tab_status_indicator_opacity(tab_status: CommandTerminalTabStatus) -> f32 {
    match tab_status {
        CommandTerminalTabStatus::DelayedSend => 0.96,
        CommandTerminalTabStatus::Idle
        | CommandTerminalTabStatus::Working
        | CommandTerminalTabStatus::Attention => 1.0,
    }
}

pub(crate) fn command_pane_control_cluster_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn command_pane_control_button_color() -> Hsla {
    rgb(0x0e0e0e).into()
}

pub(crate) fn command_pane_control_text_color() -> Hsla {
    rgb(0xcfcfcf).into()
}

pub(crate) fn command_pane_control_hover_color() -> Hsla {
    tab_bar_button_hover_color()
}

pub(crate) fn command_pane_sticky_active_tab_button_color() -> Hsla {
    /*
    CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
    Native sticky active-tab navigation shares the command tab-bar icon-button background with Pin, Minimize, and inline New Terminal.
    */
    command_pane_control_button_color()
}

pub(crate) fn command_pane_sticky_active_tab_icon_color() -> Hsla {
    command_pane_control_text_color()
}

pub(crate) fn command_pane_sticky_active_tab_border_color() -> Hsla {
    rgb(0x2a2a2a).into()
}

pub(crate) fn command_pane_split_handle_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneResize 2026-06-25-13:19:
    Native pane split rails are transparent five-pixel hit regions; pane borders provide visible separation until hover feedback appears.
    */
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn command_pane_split_separator_color() -> Hsla {
    /*
    CDXC:GPUICommandPaneResize 2026-06-25-13:19:
    Command split handles should not draw a persistent center separator because native resize rails are transparent in their normal state.
    */
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn command_terminal_placeholder_color() -> Hsla {
    rgb(0x000000).into()
}

pub(crate) fn command_pane_sleeping_placeholder_wake_label_color() -> Hsla {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
    Native AppKit uses calibrated white 0.55 for the sleeping placeholder wake label; keep the GPUI label on the equivalent neutral gray instead of reusing brighter tab or state-placeholder text colors.
    */
    rgb(0x8c8c8c).into()
}

pub(crate) fn command_pane_delayed_send_badge_background_color() -> Hsla {
    rgb(0x0d0d0d).opacity(0.78).into()
}

pub(crate) fn command_pane_delayed_send_badge_border_color() -> Hsla {
    rgb(0xffffff).opacity(0.12).into()
}

pub(crate) fn command_pane_delayed_send_badge_text_color() -> Hsla {
    rgb(0xf6c945).into()
}

pub(crate) fn gpui_combined_presentation_project_group_id(project_id: &str) -> String {
    format!("combined-project:{}", gpui_encode_uri_component(project_id))
}

pub(crate) fn gpui_combined_presentation_session_id(project_id: &str, session_id: &str) -> String {
    format!(
        "combined-session:{}:{}",
        gpui_encode_uri_component(project_id),
        gpui_encode_uri_component(session_id)
    )
}

pub(crate) fn gpui_combined_presentation_session_key(value: &str) -> Option<GpuiLocalWorkspaceSessionKey> {
    let payload = value.strip_prefix("combined-session:")?;
    let (project_id, session_id) = payload.split_once(':')?;
    let project_id = gpui_percent_decoded_id_part(project_id)?;
    let session_id = gpui_percent_decoded_id_part(session_id)?;
    Some(GpuiLocalWorkspaceSessionKey {
        project_id,
        session_id,
    })
}

