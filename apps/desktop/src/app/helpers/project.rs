// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    env,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::Ordering,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::StreamExt as _;
use gpui::{
    AnyElement, AppContext as _, Asset, Element, Hsla, Image, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, prelude::FluentBuilder as _, px, rgb, rgba,
};
use gpui_component::Selectable;

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

pub(crate) fn project_editor_placeholder_border_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0x41d7b5).opacity(0.32).into(),
        TitlebarMode::Browser => rgb(0x58b7ff).opacity(0.32).into(),
        TitlebarMode::Kanban => rgb(0x8f7aff).opacity(0.34).into(),
        TitlebarMode::Automate => rgb(0xf0b84a).opacity(0.34).into(),
        TitlebarMode::Manage => rgb(0xff7ca8).opacity(0.34).into(),
        TitlebarMode::Agents => workspace_pane_border_color(),
    }
}

pub(crate) fn project_editor_placeholder_card_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0x0a352d).into(),
        TitlebarMode::Browser => rgb(0x0d2033).into(),
        TitlebarMode::Kanban => rgb(0x182253).into(),
        TitlebarMode::Automate => rgb(0x3d2c0b).into(),
        TitlebarMode::Manage => rgb(0x421831).into(),
        TitlebarMode::Agents => rgb(0x101010).into(),
    }
}

pub(crate) fn project_editor_placeholder_card_border_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0x41d7b5).opacity(0.36).into(),
        TitlebarMode::Browser => rgb(0x58b7ff).opacity(0.36).into(),
        TitlebarMode::Kanban => rgb(0x8f7aff).opacity(0.38).into(),
        TitlebarMode::Automate => rgb(0xf0b84a).opacity(0.38).into(),
        TitlebarMode::Manage => rgb(0xff7ca8).opacity(0.38).into(),
        TitlebarMode::Agents => workspace_pane_border_color(),
    }
}

pub(crate) fn project_editor_placeholder_badge_text_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0xc1fff1).opacity(0.96).into(),
        TitlebarMode::Browser => rgb(0xc9e6ff).opacity(0.96).into(),
        TitlebarMode::Kanban => rgb(0xdfdcff).opacity(0.96).into(),
        TitlebarMode::Automate => rgb(0xffe3ac).opacity(0.96).into(),
        TitlebarMode::Manage => rgb(0xffd3df).opacity(0.96).into(),
        TitlebarMode::Agents => rgb(0xffffff).opacity(0.82).into(),
    }
}

pub(crate) fn project_editor_placeholder_title_color(_mode: TitlebarMode) -> Hsla {
    rgb(0xffffff).opacity(0.94).into()
}

pub(crate) fn project_editor_placeholder_message_color(_mode: TitlebarMode) -> Hsla {
    rgb(0xf0f2f5).opacity(0.68).into()
}

pub(crate) fn project_editor_sleeping_placeholder_background_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0x031612).into(),
        TitlebarMode::Browser => rgb(0x050e17).into(),
        TitlebarMode::Kanban => rgb(0x090d22).into(),
        TitlebarMode::Automate => rgb(0x191304).into(),
        TitlebarMode::Manage => rgb(0x1a0812).into(),
        TitlebarMode::Agents => workspace_background_color(),
    }
}

pub(crate) fn project_editor_sleeping_placeholder_card_color(mode: TitlebarMode) -> Hsla {
    match mode {
        TitlebarMode::Source => rgb(0x071f1a).into(),
        TitlebarMode::Browser => rgb(0x0a1724).into(),
        TitlebarMode::Kanban => rgb(0x111735).into(),
        TitlebarMode::Automate => rgb(0x2b2008).into(),
        TitlebarMode::Manage => rgb(0x2a1020).into(),
        TitlebarMode::Agents => rgb(0x101010).into(),
    }
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

/// Mirrors the macOS host's per-surface `GhostexGhosttySearchState`: created by
/// START_SEARCH, updated by SEARCH_TOTAL/SEARCH_SELECTED, cleared by
/// END_SEARCH. Negative totals/selections from Ghostty mean "unknown".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiTerminalSearchState {
    pub(crate) needle: String,
    pub(crate) total: Option<u64>,
    pub(crate) selected: Option<u64>,
}

/// Which pane family a GPUI-engine terminal view reports events for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiEngineTerminalEventTarget {
    Agents(TerminalSessionId),
    Command(CommandSessionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTerminalAttachmentTarget {
    Terminal(GpuiEngineTerminalEventTarget),
    // Companion can display a shell id borrowed from a project-local remote
    // namespace. Retain the full shared workspace identity across the native
    // picker and upload so parked projects with the same numeric id cannot
    // redirect either the upload or its eventual paste.
    ProjectEditorCompanion {
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        session_key: GpuiWorkspaceTerminalSessionKey,
    },
}

impl GpuiTerminalAttachmentTarget {
    pub(crate) fn engine_target(&self) -> GpuiEngineTerminalEventTarget {
        match self {
            Self::Terminal(target) => *target,
            Self::ProjectEditorCompanion { slot_id, .. } => {
                GpuiEngineTerminalEventTarget::Agents(slot_id.session_id)
            }
        }
    }
}

pub(crate) fn gpui_workspace_open_target_detection_script() -> String {
    // Same probe script macOS builds in createWorkspaceOpenTargetDetectionScript
    // (native/sidebar/native-sidebar.tsx): a login shell resolves the user's
    // real PATH for `command -v`, and app bundles are checked by directory
    // before falling back to one mdfind query per name.
    let mut lines = vec![
        "set +e".to_string(),
        "ghostex_app_exists() {".to_string(),
        "  local app_name=\"$1\"".to_string(),
        "  local app_bundle=\"${app_name}.app\"".to_string(),
        "  local base".to_string(),
        "  for base in /Applications \"$HOME/Applications\" /System/Applications; do".to_string(),
        "    if [ -d \"$base/$app_bundle\" ]; then return 0; fi".to_string(),
        "  done".to_string(),
        "  local found".to_string(),
        "  found=$(/usr/bin/mdfind \"kMDItemFSName == '$app_bundle'cd && kMDItemContentType == 'com.apple.application-bundle'\" 2>/dev/null | /usr/bin/head -n 1)".to_string(),
        "  [ -n \"$found\" ]".to_string(),
        "}".to_string(),
    ];
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        for command in definition.commands {
            lines.push(format!(
                "if command -v {} >/dev/null 2>&1; then printf 'command\\t%s\\t%s\\n' {} {}; fi",
                gpui_shell_single_quote(command),
                gpui_shell_single_quote(definition.id),
                gpui_shell_single_quote(command),
            ));
        }
    }
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        for app_name in definition.macos_app_names {
            lines.push(format!(
                "if ghostex_app_exists {} ; then printf 'app\\t%s\\t%s\\n' {} {}; fi",
                gpui_shell_single_quote(app_name),
                gpui_shell_single_quote(definition.id),
                gpui_shell_single_quote(app_name),
            ));
        }
    }
    lines.join("\n")
}

pub(crate) fn gpui_detect_workspace_open_target_availability() -> Option<GpuiDetectedOpenTargetAvailability> {
    // None mirrors the macOS catch path (spawn failure or timeout: keep the
    // stored availability). A scan that ran but failed still returns the
    // finder-only result, exactly like the exitCode gate on macOS.
    let mut child = Command::new("/bin/zsh")
        .arg("-lc")
        .arg(gpui_workspace_open_target_detection_script())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + GPUI_OPEN_TARGET_DETECTION_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }

    let built_in_ids = gpui_built_in_open_target_ids();
    let mut available_ids: HashSet<String> = HashSet::from(["finder".to_string()]);
    let mut resolved_commands = HashMap::new();
    let mut resolved_app_names = HashMap::new();
    if status.code() == Some(0) {
        for line in stdout.split('\n') {
            let mut parts = line.split('\t');
            let (Some(kind), Some(target_id), Some(value)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if target_id.is_empty() || value.is_empty() || !built_in_ids.contains(target_id) {
                continue;
            }
            available_ids.insert(target_id.to_string());
            if kind == "command" && !resolved_commands.contains_key(target_id) {
                resolved_commands.insert(target_id.to_string(), value.to_string());
            }
            if kind == "app" && !resolved_app_names.contains_key(target_id) {
                resolved_app_names.insert(target_id.to_string(), value.to_string());
            }
        }
    }
    let available_target_ids = GPUI_BUILT_IN_OPEN_TARGETS
        .iter()
        .filter(|definition| available_ids.contains(definition.id))
        .map(|definition| definition.id.to_string())
        .collect();
    Some(GpuiDetectedOpenTargetAvailability {
        available_target_ids,
        resolved_commands,
        resolved_app_names,
    })
}

pub(crate) fn gpui_detected_open_target_availability_matches_stored(
    detected: &GpuiDetectedOpenTargetAvailability,
    stored: &GpuiOpenTargetAvailability,
) -> bool {
    // checkedAtMs is deliberately outside the comparison, matching
    // workspaceOpenTargetAvailabilityEquals on macOS, so an unchanged machine
    // never rewrites settings at startup.
    detected.available_target_ids.len() == stored.available_ids.len()
        && detected
            .available_target_ids
            .iter()
            .all(|id| stored.available_ids.contains(id))
        && detected.resolved_commands == stored.resolved_commands
        && detected.resolved_app_names == stored.resolved_app_names
}

pub(crate) fn gpui_workspace_open_target_availability_settings_value(
    detected: &GpuiDetectedOpenTargetAvailability,
) -> serde_json::Value {
    let checked_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let mut resolved_commands = serde_json::Map::new();
    let mut resolved_app_names = serde_json::Map::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        if let Some(command) = detected.resolved_commands.get(definition.id) {
            resolved_commands.insert(definition.id.to_string(), serde_json::json!(command));
        }
        if let Some(app_name) = detected.resolved_app_names.get(definition.id) {
            resolved_app_names.insert(definition.id.to_string(), serde_json::json!(app_name));
        }
    }
    serde_json::json!({
        "availableTargetIds": detected.available_target_ids,
        "checkedAtMs": checked_at_ms,
        "resolvedAppNames": resolved_app_names,
        "resolvedCommands": resolved_commands,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GpuiKeepAwakePowerSnapshot {
    pub(crate) battery_percent: Option<f64>,
    pub(crate) external_display_connected: bool,
    pub(crate) low_power_mode: Option<bool>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiKeepAwakePowerProbeOptions {
    pub(crate) include_battery: bool,
    pub(crate) include_external_display: bool,
    pub(crate) include_low_power_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiKeepAwakeWorkingSessionGraceState {
    pub(crate) previous_working_session_count: usize,
    pub(crate) grace_until: Option<Instant>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GpuiWorkspaceTerminalSessionKey {
    Local(GpuiLocalWorkspaceSessionKey),
    Remote(GpuiRemoteAttachSessionKey),
}

impl GpuiWorkspaceTerminalSessionKey {
    pub(crate) fn scoped_project_id(&self) -> String {
        match self {
            Self::Local(key) => key.project_id.clone(),
            Self::Remote(key) => gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            ),
        }
    }

    pub(crate) fn as_local(&self) -> Option<&GpuiLocalWorkspaceSessionKey> {
        match self {
            Self::Local(key) => Some(key),
            Self::Remote(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiLocalWorkspaceAttachTerminalPlan {
    pub(crate) agent_icon: Option<&'static str>,
    pub(crate) attach_command: String,
    pub(crate) command_id: Option<String>,
    pub(crate) persistence_session_created: Option<bool>,
    pub(crate) startup_text: Option<String>,
    pub(crate) startup_text_disposition: Option<String>,
    pub(crate) title: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) zmx_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAddProjectDialogOperation {
    Add,
    Browse,
    CancelCloneJob,
    CreateDirectory,
    DiscoverSourceControl,
    ListMachines,
    LookupRepository,
    PreviewClone,
    ReadCloneJob,
    StartClone,
}

impl GpuiAddProjectDialogOperation {
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "add" => Some(Self::Add),
            "browse" => Some(Self::Browse),
            "cancelCloneJob" => Some(Self::CancelCloneJob),
            "createDirectory" => Some(Self::CreateDirectory),
            "discoverSourceControl" => Some(Self::DiscoverSourceControl),
            "listMachines" => Some(Self::ListMachines),
            "lookupRepository" => Some(Self::LookupRepository),
            "previewClone" => Some(Self::PreviewClone),
            "readCloneJob" => Some(Self::ReadCloneJob),
            "startClone" => Some(Self::StartClone),
            _ => None,
        }
    }

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Browse => "browse",
            Self::CancelCloneJob => "cancelCloneJob",
            Self::CreateDirectory => "createDirectory",
            Self::DiscoverSourceControl => "discoverSourceControl",
            Self::ListMachines => "listMachines",
            Self::LookupRepository => "lookupRepository",
            Self::PreviewClone => "previewClone",
            Self::ReadCloneJob => "readCloneJob",
            Self::StartClone => "startClone",
        }
    }

    pub(crate) fn endpoint(self) -> Option<&'static str> {
        match self {
            Self::Add => Some("/api/addProjectPath"),
            Self::Browse => Some("/api/browseProjectDirectories"),
            Self::CancelCloneJob => Some("/api/cancelRepositoryCloneJob"),
            Self::CreateDirectory => Some("/api/createProjectDirectory"),
            Self::DiscoverSourceControl => Some("/api/discoverSourceControl"),
            Self::ListMachines => None,
            Self::LookupRepository => Some("/api/lookupRepository"),
            Self::PreviewClone => Some("/api/previewRepositoryClone"),
            Self::ReadCloneJob => Some("/api/readRepositoryCloneJob"),
            Self::StartClone => Some("/api/startRepositoryClone"),
        }
    }

    pub(crate) fn timeout(self) -> Duration {
        match self {
            Self::Add | Self::StartClone => GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT,
            Self::Browse => GPUI_ADD_PROJECT_DIALOG_BROWSE_TIMEOUT,
            Self::DiscoverSourceControl => GPUI_ADD_PROJECT_DIALOG_DISCOVERY_TIMEOUT,
            Self::LookupRepository | Self::PreviewClone => GPUI_ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT,
            Self::CancelCloneJob
            | Self::CreateDirectory
            | Self::ReadCloneJob
            | Self::ListMachines => GPUI_ADD_PROJECT_DIALOG_JOB_TIMEOUT,
        }
    }
}

pub(crate) fn gpui_add_project_dialog_params(
    operation: GpuiAddProjectDialogOperation,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut forwarded = serde_json::Map::new();
    match operation {
        GpuiAddProjectDialogOperation::ListMachines => {}
        GpuiAddProjectDialogOperation::Browse => {
            let partial_path =
                gpui_remote_path_like_string_from_command(params, "partialPath", true)?;
            forwarded.insert("partialPath".to_string(), serde_json::json!(partial_path));
            if let Some(cwd) = gpui_remote_path_like_string_from_command(params, "cwd", false) {
                forwarded.insert("cwd".to_string(), serde_json::json!(cwd));
            }
        }
        GpuiAddProjectDialogOperation::Add => {
            /*
            The daemon derives the project name from the resolved leaf folder,
            which is also what it does for a created-on-demand workspace root.
            Sending a client-side guess would only be able to disagree with it.
            */
            let path = gpui_remote_path_like_string_from_command(params, "path", false)?;
            forwarded.insert("path".to_string(), serde_json::json!(path));
            if params
                .get("createIfMissing")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("createIfMissing".to_string(), serde_json::json!(true));
            }
        }
        GpuiAddProjectDialogOperation::CreateDirectory => {
            /*
            CDXC:AddProjectNewFolder 2026-08-18:
            The new-folder request names an existing parent directory plus one
            bounded segment. The daemon re-validates both, so the bridge only
            has to keep the segment from carrying a path.
            */
            let parent_path =
                gpui_remote_path_like_string_from_command(params, "parentPath", false)?;
            let name = gpui_add_project_dialog_bounded_text(params, "name", 255)?;
            forwarded.insert("name".to_string(), serde_json::json!(name));
            forwarded.insert("parentPath".to_string(), serde_json::json!(parent_path));
        }
        GpuiAddProjectDialogOperation::DiscoverSourceControl => {}
        GpuiAddProjectDialogOperation::LookupRepository => {
            let provider = gpui_add_project_dialog_bounded_text(params, "provider", 64)?;
            let repository = gpui_add_project_dialog_bounded_text(params, "repository", 512)?;
            forwarded.insert("provider".to_string(), serde_json::json!(provider));
            forwarded.insert("repository".to_string(), serde_json::json!(repository));
        }
        GpuiAddProjectDialogOperation::PreviewClone | GpuiAddProjectDialogOperation::StartClone => {
            let remote_url = gpui_add_project_dialog_bounded_text(params, "remoteUrl", 4_096)?;
            let destination_path =
                gpui_remote_path_like_string_from_command(params, "destinationPath", false)?;
            forwarded.insert(
                "destinationPath".to_string(),
                serde_json::json!(destination_path),
            );
            forwarded.insert("remoteUrl".to_string(), serde_json::json!(remote_url));
            if let Some(branch_name) =
                gpui_add_project_dialog_bounded_text(params, "branchName", 1_024)
            {
                forwarded.insert("branchName".to_string(), serde_json::json!(branch_name));
            }
            if params
                .get("cloneMainOnly")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("cloneMainOnly".to_string(), serde_json::json!(true));
            }
            if params
                .get("shallowClone")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                forwarded.insert("shallowClone".to_string(), serde_json::json!(true));
            }
        }
        GpuiAddProjectDialogOperation::CancelCloneJob
        | GpuiAddProjectDialogOperation::ReadCloneJob => {
            let job_id = gpui_add_project_dialog_bounded_text(params, "jobId", 256)?;
            forwarded.insert("jobId".to_string(), serde_json::json!(job_id));
        }
    }
    Some(serde_json::Value::Object(forwarded))
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_add_project_dialog_translate_local_windows_paths(
    operation: GpuiAddProjectDialogOperation,
    mut params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:AddProjectWindowsWslPaths 2026-08-02:
    The Windows shell deliberately runs its one local gxserver inside WSL2.
    The shared dialog accepts Win32 drive and UNC paths on a Win32 machine,
    but those paths are not absolute to the Linux daemon. Translate only the
    filesystem fields for this computer before they cross the localhost RPC
    boundary. WSL-native paths, relative paths already resolved by the dialog,
    repository URLs, job ids, and every remote-machine request remain exactly
    as supplied; gxserver continues to store one canonical WSL project path.
    */
    let fields: &[&str] = match operation {
        GpuiAddProjectDialogOperation::Add => &["path"],
        GpuiAddProjectDialogOperation::Browse => &["partialPath", "cwd"],
        GpuiAddProjectDialogOperation::PreviewClone | GpuiAddProjectDialogOperation::StartClone => {
            &["destinationPath"]
        }
        GpuiAddProjectDialogOperation::CreateDirectory => &["parentPath"],
        GpuiAddProjectDialogOperation::CancelCloneJob
        | GpuiAddProjectDialogOperation::DiscoverSourceControl
        | GpuiAddProjectDialogOperation::ListMachines
        | GpuiAddProjectDialogOperation::LookupRepository
        | GpuiAddProjectDialogOperation::ReadCloneJob => &[],
    };
    let Some(object) = params.as_object_mut() else {
        return Err("The add-project request was invalid.".to_string());
    };
    for field in fields {
        let Some(path) = object
            .get(*field)
            .and_then(serde_json::Value::as_str)
            .filter(|path| gpui_add_project_dialog_is_windows_absolute_path(path))
        else {
            continue;
        };
        let translated = windows_terminal_backend::wsl_path_for_windows_path(Path::new(path))
            .map_err(|_| {
                "The selected Windows path could not be translated into WSL.".to_string()
            })?;
        object.insert((*field).to_string(), serde_json::Value::String(translated));
    }
    Ok(params)
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_add_project_dialog_is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || (bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\')))
}

pub(crate) fn gpui_add_project_dialog_local_platform() -> &'static str {
    /*
    CDXC:AddProject 2026-07-30:
    The dialog reads a machine's platform in `navigator.platform` spelling to
    decide the submit modifier label and whether Windows-style paths are legal,
    so report this computer in that vocabulary rather than Rust's target names.
    */
    if cfg!(target_os = "macos") {
        "MacIntel"
    } else if cfg!(target_os = "windows") {
        "Win32"
    } else {
        "Linux"
    }
}

pub(crate) fn gpui_add_project_dialog_bounded_text(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Option<String> {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= max_chars)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(|character| character.is_control()))
        .map(str::to_string)
}

pub(crate) fn gpui_workspace_project_key_allowed(value: &str) -> bool {
    gpui_remote_sidebar_project_id_allowed(value)
        || gpui_remote_project_reference_from_project_id(value).is_some()
}

/*
CDXC:GPUIRemoteBrowserTabs 2026-07-12:
Browser tab models are keyed by project id strings. Local projects use the
plain workspace id, and remote projects use their machine-scoped
`remote:<machine>:project:<id>` identity so their tabs park, persist, and
restore per remote project exactly like local ones.
*/
pub(crate) fn gpui_workspace_folder_picked_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceFolderPicked==='function'){{bridge.onWorkspaceFolderPicked(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceFolderPicks)?bridge.pendingWorkspaceFolderPicks:[];pending.push(payload);bridge.pendingWorkspaceFolderPicks=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_os_integration_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onOsIntegrationCommand==='function'){{bridge.onOsIntegrationCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingOsIntegrationCommands)?bridge.pendingOsIntegrationCommands:[];pending.push(payload);bridge.pendingOsIntegrationCommands=pending;}}}})(); undefined;"
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_path_is_script(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| matches!(extension.as_str(), "command" | "tool" | "sh"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

/// macOS `resolveExistingDirectoryForOpenRequest` parity: a requested cwd is
/// honored only if it is an existing directory; otherwise the quick terminal
/// roots at the home directory.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_resolved_terminal_cwd(cwd: Option<String>) -> String {
    let requested = cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(gpui_os_integration_expand_tilde_path);
    match requested {
        Some(path) if path.is_dir() => path.to_string_lossy().to_string(),
        _ => home_dir().to_string_lossy().to_string(),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_project_root_for_path(path: &Path) -> Option<PathBuf> {
    let path = gpui_os_integration_expand_tilde_path(path.to_string_lossy().as_ref());
    let metadata = std::fs::metadata(&path).ok()?;
    let base = if metadata.is_dir() {
        path
    } else {
        path.parent()?.to_path_buf()
    };
    Some(gpui_os_integration_git_root_for_path(&base).unwrap_or(base))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_git_root_for_path(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path.to_path_buf());
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// macOS `scriptRunCommand` parity: executable scripts run as `./name` from
/// their own directory; non-executable ones run through the user's shell.
#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_script_run_command(path: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let executable = std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);
    if executable {
        return format!("./{}", gpui_os_integration_shell_quote(&file_name));
    }
    let shell = std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());
    format!(
        "{} {}",
        gpui_os_integration_shell_quote(&shell),
        gpui_os_integration_shell_quote(path.to_string_lossy().as_ref())
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn gpui_workspace_terminal_bell_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalBell==='function'){{bridge.onWorkspaceTerminalBell(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalBells)?bridge.pendingWorkspaceTerminalBells:[];pending.push(payload);bridge.pendingWorkspaceTerminalBells=pending;}}}})(); undefined;"
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_workspace_terminal_title_changed_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalTitleChanged==='function'){{bridge.onWorkspaceTerminalTitleChanged(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalTitleChanges)?bridge.pendingWorkspaceTerminalTitleChanges:[];pending.push(payload);bridge.pendingWorkspaceTerminalTitleChanges=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceTerminalEscapePressed`.
pub(crate) fn gpui_workspace_terminal_escape_pressed_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalEscapePressed==='function'){{bridge.onWorkspaceTerminalEscapePressed(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalEscapePresses)?bridge.pendingWorkspaceTerminalEscapePresses:[];pending.push(payload);bridge.pendingWorkspaceTerminalEscapePresses=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel`.
pub(crate) fn gpui_workspace_first_prompt_title_generation_cancel_script(
    message: &serde_json::Value,
) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceFirstPromptTitleGenerationCancel==='function'){{bridge.onWorkspaceFirstPromptTitleGenerationCancel(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceFirstPromptTitleGenerationCancels)?bridge.pendingWorkspaceFirstPromptTitleGenerationCancels:[];pending.push(payload);bridge.pendingWorkspaceFirstPromptTitleGenerationCancels=pending;}}}})(); undefined;"
    )
}

// Bridge script for `ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge`.
pub(crate) fn gpui_workspace_session_attention_acknowledge_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceSessionAttentionAcknowledge==='function'){{bridge.onWorkspaceSessionAttentionAcknowledge(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceSessionAttentionAcknowledgements)?bridge.pendingWorkspaceSessionAttentionAcknowledgements:[];pending.push(payload);bridge.pendingWorkspaceSessionAttentionAcknowledgements=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_terminal_runtime_action_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalRuntimeAction==='function'){{bridge.onWorkspaceTerminalRuntimeAction(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalRuntimeActions)?bridge.pendingWorkspaceTerminalRuntimeActions:[];pending.push(payload);bridge.pendingWorkspaceTerminalRuntimeActions=pending;}}}})(); undefined;"
    )
}

/*
CDXC:GPUISidebarPointerTracking 2026-08-02:
`data-native-pointer-inside` is a pure CSS state flag whose only writer is the
native pointer observer, so it is set directly on `document.body` rather than
through a page bridge: the attribute exists from the first paint, no page code
has to be mounted for the write to land, and an absent attribute is already the
correct "pointer position unknown, hover normally" state.
*/
pub(crate) fn gpui_workspace_terminal_lifecycle_request_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTerminalLifecycleRequest==='function'&&typeof bridge.postWorkspaceTerminalLifecycleResult==='function'){{bridge.onWorkspaceTerminalLifecycleRequest(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTerminalLifecycleRequests)?bridge.pendingWorkspaceTerminalLifecycleRequests:[];pending.push(payload);bridge.pendingWorkspaceTerminalLifecycleRequests=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_prepare_local_workspace_attach_terminal_plan(
    reference: &GpuiLocalWorkspaceSessionKey,
    intent: GpuiLocalWorkspaceAttachIntent,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(reference, None, intent)
}

pub(crate) fn gpui_prepare_local_workspace_attach_terminal_plan_with_startup_text(
    reference: &GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
    intent: GpuiLocalWorkspaceAttachIntent,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    Local GPUI sidebar session clicks follow macOS zmx attach sequencing: Rust asks localhost gxserver for wake/attach metadata, starts every missing zmx provider through gxserver (with queued startup text when present), then opens an awake Agents Running tab whose exact mount slot consumes the daemon-built attach command. CEF cannot provide commands, cwd, titles, paths, daemon bodies, tokens, stdout/stderr, or terminal content.

    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    Command-pane fresh Action launches pass their one-shot startup text as the
    explicit gxserver attach parameter, matching native's wake/attach/provider
    sequence. The text is consumed only by gxserver's startupTextDisposition path;
    it is not stored in launchSettings or used as the terminal process command.
    */
    let attach_params = gpui_local_workspace_attach_rpc_params(reference, startup_text);
    let mut result =
        gpui_gxserver_rpc_result(intent.rpc_path(), &attach_params, Duration::from_secs(15))?;
    let mut attach = gpui_local_workspace_attach_object(&result)?;
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    let mut startup_text_for_plan =
        gpui_local_workspace_attach_startup_text(attach).map(str::to_string);
    let mut startup_text_disposition_for_plan =
        gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string);

    if should_start_local_zmx_provider_before_gpui_attach(attach) {
        let provider_params =
            gpui_local_workspace_attach_rpc_params(reference, startup_text_for_plan.as_deref());
        gpui_gxserver_rpc_result(
            "/api/startSessionProvider",
            &provider_params,
            Duration::from_secs(30),
        )?;
        result = gpui_gxserver_rpc_result(
            "/api/attachSessionMetadata",
            &attach_params,
            Duration::from_secs(15),
        )?;
        attach = gpui_local_workspace_attach_object(&result)?;
        gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
        if startup_text_for_plan.is_none() {
            startup_text_for_plan =
                gpui_local_workspace_attach_startup_text(attach).map(str::to_string);
        }
        if startup_text_disposition_for_plan.is_none() {
            startup_text_disposition_for_plan =
                gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string);
        }
    }

    gpui_local_workspace_attach_terminal_plan_from_result(
        &result,
        startup_text_for_plan,
        startup_text_disposition_for_plan,
    )
}

pub(crate) fn gpui_local_workspace_attach_terminal_plan_from_result(
    result: &serde_json::Value,
    startup_text_for_plan: Option<String>,
    startup_text_disposition_for_plan: Option<String>,
) -> Result<GpuiLocalWorkspaceAttachTerminalPlan, String> {
    let attach = gpui_local_workspace_attach_object(result)?;
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    gpui_validate_local_workspace_attach_metadata(attach)?;
    let agent_icon = gpui_workspace_attach_agent_icon(attach);
    let attach_command = gpui_local_workspace_attach_string(attach, "attachCommand")
        .ok_or_else(|| "Session attach metadata is unavailable.".to_string())?
        .to_string();
    let command_id = attach
        .get("session")
        .and_then(serde_json::Value::as_object)
        .and_then(|session| gpui_trimmed_json_string_field(session, "commandId"))
        .and_then(valid_action_command_id);
    let persistence_session_created =
        gpui_local_workspace_attach_persistence_session_created(attach);
    let startup_text = startup_text_for_plan
        .or_else(|| gpui_local_workspace_attach_startup_text(attach).map(str::to_string));
    let startup_text_disposition = startup_text_disposition_for_plan.or_else(|| {
        gpui_local_workspace_attach_startup_text_disposition(attach).map(str::to_string)
    });
    let title = gpui_workspace_attach_title(attach);
    let working_directory = gpui_local_workspace_attach_string(attach, "cwd").map(str::to_string);
    let zmx_name = gpui_local_workspace_attach_string(attach, "zmxName").map(str::to_string);

    Ok(GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    })
}

pub(crate) fn gpui_command_terminal_create_session_params(
    input: &GpuiCommandTerminalCreateInput,
) -> serde_json::Value {
    /*
    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    GPUI command-pane creation mirrors native `createCommandTerminal`: create
    the gxserver row first with `surface:"commands"` and zmx provider metadata,
    then pass Action startup text through the attach RPC sequence rather than
    persisting it in session launchSettings or spawning it as the rendered
    terminal process command.
    */
    let mut launch_settings = serde_json::Map::new();
    launch_settings.insert(
        "surface".to_string(),
        serde_json::Value::String("commands".to_string()),
    );
    if let Some(command_title) = input.command_title.as_deref() {
        launch_settings.insert(
            "commandTitle".to_string(),
            serde_json::Value::String(command_title.to_string()),
        );
    }
    let mut params = serde_json::Map::new();
    if let Some(command_id) = input.command_id.as_deref() {
        params.insert(
            "commandId".to_string(),
            serde_json::Value::String(command_id.to_string()),
        );
    }
    params.insert(
        "cwd".to_string(),
        serde_json::Value::String(input.cwd.clone()),
    );
    params.insert(
        "kind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    params.insert(
        "launchSettings".to_string(),
        serde_json::Value::Object(launch_settings),
    );
    params.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("running".to_string()),
    );
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(input.project_id.clone()),
    );
    params.insert(
        "providerState".to_string(),
        serde_json::json!({
            "lifecycleState": "exists",
            "provider": "zmx",
        }),
    );
    params.insert(
        "runtimeSettings".to_string(),
        serde_json::json!({
            "sessionPersistenceProvider": "zmx",
            "titleSource": "user",
        }),
    );
    params.insert(
        "surface".to_string(),
        serde_json::Value::String("commands".to_string()),
    );
    params.insert(
        "title".to_string(),
        serde_json::Value::String(input.title.clone()),
    );
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_create_local_project_workspace_terminal(
    project_id: &str,
) -> Result<
    (
        GpuiLocalWorkspaceSessionKey,
        GpuiLocalWorkspaceAttachTerminalPlan,
    ),
    String,
> {
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return Err("The active project is unavailable.".to_string());
    }
    /*
    CDXC:GPUIWindowsTerminalStartup 2026-07-26:
    The selected WSL gxserver can create a fresh workspace terminal, start its
    never-reused zmx identity, and return the final require-existing attach
    plan in one operation. Windows uses that atomic path so New Terminal does
    not serialize three known-missing provider probes before materialization.
    macOS and Linux retain that established lifecycle.
    */
    #[cfg(target_os = "windows")]
    let result = {
        let mut params = serde_json::Map::new();
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        if gpui_current_zmx_prompt_editor_attach_mode_is_monaco() {
            params.insert(
                "promptEditor".to_string(),
                serde_json::Value::String("monaco".to_string()),
            );
        }
        gpui_gxserver_rpc_result(
            "/api/createWorkspaceTerminal",
            &serde_json::Value::Object(params),
            Duration::from_secs(30),
        )?
    };
    #[cfg(not(target_os = "windows"))]
    let result = gpui_gxserver_rpc_result(
        "/api/createSession",
        &serde_json::json!({
            "kind": "terminal",
            "lifecycleState": "running",
            "projectId": project_id,
            "surface": "workspace",
            "title": "Terminal",
        }),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create a companion terminal.".to_string())?;
    let created_project_id = gpui_trimmed_json_string_field(session, "projectId")
        .unwrap_or(project_id)
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return a companion terminal id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid companion terminal id.".to_string());
    }
    let key = GpuiLocalWorkspaceSessionKey {
        project_id: created_project_id,
        session_id,
    };
    #[cfg(target_os = "windows")]
    let plan = gpui_local_workspace_attach_terminal_plan_from_result(&result, None, None)?;
    #[cfg(not(target_os = "windows"))]
    let plan = gpui_prepare_local_workspace_attach_terminal_plan(
        &key,
        GpuiLocalWorkspaceAttachIntent::Attach,
    )?;
    Ok((key, plan))
}

pub(crate) fn gpui_local_workspace_attach_rpc_params(
    reference: &GpuiLocalWorkspaceSessionKey,
    startup_text: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(reference.project_id.clone()),
    );
    params.insert(
        "sessionId".to_string(),
        serde_json::Value::String(reference.session_id.clone()),
    );
    if let Some(startup_text) = startup_text {
        params.insert(
            "startupText".to_string(),
            serde_json::Value::String(startup_text.to_string()),
        );
    }
    if gpui_current_zmx_prompt_editor_attach_mode_is_monaco() {
        params.insert(
            "promptEditor".to_string(),
            serde_json::Value::String("monaco".to_string()),
        );
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_local_workspace_attach_object(
    result: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, String> {
    result
        .get("attach")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Session attach metadata is unavailable.".to_string())
}

pub(crate) fn gpui_validate_local_workspace_attach_metadata(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    gpui_validate_local_workspace_attach_not_restore_blocked(attach)?;
    if attach.get("provider").and_then(serde_json::Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|provider| provider.get("lifecycleState"))
            .and_then(serde_json::Value::as_str)
            != Some("exists")
    {
        return Err(
            "gxserver did not confirm the zmx provider exists before terminal attach.".to_string(),
        );
    }
    if gpui_local_workspace_attach_string(attach, "attachCommand").is_none() {
        return Err("Session attach metadata is unavailable.".to_string());
    }
    if gpui_local_workspace_attach_has_terminal_ready_startup_text(attach)
        && gpui_local_workspace_attach_persistence_session_created(attach) == Some(false)
    {
        return Err(
            "gxserver did not confirm the session provider started before terminal attach."
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn gpui_validate_local_workspace_attach_not_restore_blocked(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    if attach.get("restoreBlocked").is_some() {
        return Err(
            "Session restore is blocked because its working directory is unavailable.".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn should_start_local_zmx_provider_before_gpui_attach(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    /*
    CDXC:GPUIZmxProviderOwnership 2026-07-15:
    A GPUI attach must never be the operation that creates a missing zmx
    provider. gxserver owns provider startup for both restored agents and
    blank terminal rows because that path installs the prompt-editor wrapper
    before the interactive shell starts. The current attach client still
    decides Monaco versus the machine editor through its advertised zmx
    capability; provider initialization does not make Monaco durable.
    */
    attach.get("provider").and_then(serde_json::Value::as_str) == Some("zmx")
        && attach
            .get("providerState")
            .and_then(|provider| provider.get("lifecycleState"))
            .and_then(serde_json::Value::as_str)
            == Some("missing")
}

pub(crate) fn gpui_local_workspace_attach_startup_text(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&str> {
    attach
        .get("startupText")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_local_workspace_attach_startup_text_disposition(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&str> {
    attach
        .get("startupTextDisposition")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_local_workspace_attach_has_terminal_ready_startup_text(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    gpui_local_workspace_attach_startup_text_disposition(attach) == Some("queueAfterTerminalReady")
        && gpui_local_workspace_attach_startup_text(attach).is_some()
}

pub(crate) fn gpui_local_workspace_attach_persistence_session_created(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<bool> {
    attach
        .get("persistenceSessionCreated")
        .and_then(serde_json::Value::as_bool)
}

pub(crate) fn gpui_local_workspace_attach_string<'a>(
    attach: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    attach
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
}

pub(crate) fn gpui_workspace_attach_title(attach: &serde_json::Map<String, serde_json::Value>) -> String {
    attach
        .get("session")
        .and_then(|session| session.get("title"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| {
            !title.is_empty()
                && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !title.contains('\0')
                && !title.chars().any(char::is_control)
        })
        .unwrap_or("Terminal")
        .to_string()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_gpui_local_workspace_attach_terminal_in_new_leaf(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    requested_pane_id: WorkspacePaneId,
    placement: AgentsWorkspaceNewTerminalPlacement,
    key: GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<(WorkspacePaneId, TerminalSessionId), &'static str> {
    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the session attach terminal command.")?;

    let created = match placement {
        AgentsWorkspaceNewTerminalPlacement::Tab => {
            return Err("GPUI cannot open a tab placement as a new workspace leaf.");
        }
        AgentsWorkspaceNewTerminalPlacement::SplitRight => {
            workspace.split_mounting_session_to_right_of_pane(requested_pane_id)
        }
        AgentsWorkspaceNewTerminalPlacement::SplitBelow => {
            workspace.split_mounting_session_below_pane(requested_pane_id)
        }
        AgentsWorkspaceNewTerminalPlacement::BottomRow => workspace
            .resolve_action_pane_id(requested_pane_id)
            .map(|resolved_pane_id| {
                workspace.focus_pane(resolved_pane_id);
                workspace.append_mounting_session_bottom_row()
            }),
    };
    let Some((pane_id, session_id)) = created else {
        return Err("GPUI could not create a workspace pane for the session.");
    };
    let gxserver_session_id = key.session_id.clone();
    let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    else {
        return Err("GPUI could not find the terminal tab for the session.");
    };
    session.title = title;
    session.agent_icon = agent_icon;
    session.zmx_session_name = zmx_name;
    session.set_presentation_state_with_startup_eligibility(
        TerminalSessionPresentationState::Running,
        false,
    );
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        },
        payload,
    );
    local_workspace_session_mappings.insert(key, session_id);
    local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
    Ok((pane_id, session_id))
}

pub(crate) fn insert_gpui_local_workspace_attach_terminal(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    requested_pane_id: WorkspacePaneId,
    force_requested_pane_placement: bool,
    key: GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<(WorkspacePaneId, TerminalSessionId), &'static str> {
    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the session attach terminal command.")?;

    let gxserver_session_id = key.session_id.clone();
    if let Some(session_id) = local_workspace_session_mappings.get(&key).copied() {
        let existing_pane_id = workspace
            .pane_id_for_session(session_id)
            .filter(|pane_id| workspace.session_belongs_to_pane(*pane_id, session_id));
        if let Some(mut pane_id) = existing_pane_id {
            if pane_id != requested_pane_id && force_requested_pane_placement {
                if !workspace.group_tab_into_pane(pane_id, requested_pane_id, session_id) {
                    return Err("GPUI could not move the mapped terminal into the target group.");
                }
                pane_id = requested_pane_id;
            }
            let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
            let mount_slot_id = AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            };
            launch_payload_source.insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                mount_slot_id,
                payload,
            );
            let Some(session) = workspace
                .terminal_sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            else {
                return Err("GPUI could not find the mapped terminal tab for the session.");
            };
            session.title = title;
            session.agent_icon = agent_icon;
            session.zmx_session_name = zmx_name;
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Running,
                false,
            );
            workspace.select_tab(pane_id, session_id);
            local_workspace_session_mappings.insert(key, session_id);
            local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
            return Ok((pane_id, session_id));
        }

        local_workspace_session_mappings.remove(&key);
        local_app_shot_session_mappings
            .retain(|_, mapped_session_id| *mapped_session_id != session_id);
    }

    if workspace.find_leaf(requested_pane_id).is_none() {
        return Err("GPUI could not find the target pane for the session.");
    }
    let Some((pane_id, session_id)) =
        workspace.add_running_session_to_pane(requested_pane_id, title, agent_icon)
    else {
        return Err("GPUI could not create a terminal tab for the session.");
    };
    if let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.zmx_session_name = zmx_name;
    }
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(session_id);
    let mount_slot_id = AgentsTerminalBodyMountSlotId {
        pane_id,
        session_id,
    };
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        mount_slot_id,
        payload,
    );
    local_workspace_session_mappings.insert(key, session_id);
    local_app_shot_session_mappings.insert(gxserver_session_id, session_id);
    Ok((pane_id, session_id))
}

pub(crate) fn attach_gpui_surfaced_local_workspace_terminal(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    local_app_shot_session_mappings: &mut HashMap<String, TerminalSessionId>,
    pane_id: WorkspacePaneId,
    key: &GpuiLocalWorkspaceSessionKey,
    plan: GpuiLocalWorkspaceAttachTerminalPlan,
) -> Result<TerminalSessionId, &'static str> {
    /*
    CDXC:GPUIWorkspaceSessionReattach 2026-07-24:
    Startup reattachment for a surfaced non-focused terminal is an ownership
    repair, not a selection action. Require the canonical gxserver mapping to
    still identify the active Running terminal in this exact rendered pane,
    then seed only that slot's process-local launch payload. Do not select the
    tab, change the focused pane, publish sidebar focus, or derive a fallback
    target if the restored layout changed while metadata was loading.
    */
    let shell_session_id = local_workspace_session_mappings
        .get(key)
        .copied()
        .ok_or("GPUI could not find the surfaced terminal mapping.")?;
    if workspace.pane_id_for_session(shell_session_id) != Some(pane_id)
        || workspace.active_session_in_pane(pane_id) != Some(shell_session_id)
    {
        return Err("GPUI surfaced terminal placement changed before attach completed.");
    }
    let Some(session) = workspace
        .terminal_sessions
        .iter_mut()
        .find(|session| session.id == shell_session_id)
    else {
        return Err("GPUI could not find the surfaced terminal tab.");
    };
    if session.presentation_state != TerminalSessionPresentationState::Running {
        return Err("GPUI surfaced terminal is not attachable.");
    }

    let GpuiLocalWorkspaceAttachTerminalPlan {
        agent_icon,
        attach_command,
        command_id: _,
        persistence_session_created,
        startup_text,
        startup_text_disposition,
        title,
        working_directory,
        zmx_name,
    } = plan;
    let initial_input = if startup_text_disposition.as_deref() == Some("queueAfterTerminalReady")
        && persistence_session_created == Some(true)
    {
        startup_text
    } else {
        None
    };
    let payload = AgentsTerminalExplicitLaunchPayload {
        working_directory,
        command: Some(attach_command),
        env_vars: Vec::new(),
        initial_input,
        wait_after_command: false,
    };
    payload
        .to_ghostty_launch_payload()
        .map_err(|_| "GPUI could not prepare the surfaced session attach command.")?;

    session.title = title;
    session.agent_icon = agent_icon;
    session.zmx_session_name = zmx_name;
    let runtime_session_id = runtime_sessions.ensure_runtime_session_id(shell_session_id);
    launch_payload_source.insert_explicit_payload_for_mount_slot(
        runtime_session_id,
        AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        },
        payload,
    );
    local_app_shot_session_mappings.insert(key.session_id.clone(), shell_session_id);
    Ok(shell_session_id)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_project_owner(
    project: &GpuiStatusIndicatorProjectState,
) -> Option<GpuiMenuBarStatusNativeProjectOwner> {
    let project_id = std::ffi::CString::new(project.project_id.as_str()).ok()?;
    let title = std::ffi::CString::new(project.title.as_str()).ok()?;
    let sessions = project
        .sessions
        .iter()
        .filter_map(gpui_menu_bar_status_native_session_owner)
        .collect::<Vec<_>>();
    let entries = sessions
        .iter()
        .map(|session| session.entry)
        .collect::<Vec<_>>();
    Some(GpuiMenuBarStatusNativeProjectOwner {
        project_id,
        title,
        sessions,
        entries,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_session_owner(
    session: &GpuiStatusIndicatorSessionState,
) -> Option<GpuiMenuBarStatusNativeSessionOwner> {
    let session_id = std::ffi::CString::new(session.session_id.as_str()).ok()?;
    let title = std::ffi::CString::new(session.title.as_str()).ok()?;
    let last_active_at = session
        .last_active_at
        .as_ref()
        .and_then(|timestamp| std::ffi::CString::new(timestamp.as_str()).ok());
    let entry = GpuiMenuBarStatusNativeSessionEntry {
        session_id: session_id.as_ptr(),
        title: title.as_ptr(),
        last_active_at: last_active_at
            .as_ref()
            .map_or(std::ptr::null(), |timestamp| timestamp.as_ptr()),
        status: gpui_menu_bar_status_native_kind(session.status),
        order: session.order,
    };
    Some(GpuiMenuBarStatusNativeSessionOwner {
        session_id,
        title,
        last_active_at,
        entry,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_kind(status: GpuiStatusIndicatorStatus) -> i32 {
    match status {
        GpuiStatusIndicatorStatus::Attention => 0,
        GpuiStatusIndicatorStatus::Working => 1,
        GpuiStatusIndicatorStatus::Available => 2,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn hide_gpui_menu_bar_status_item() {
    unsafe {
        GhostexGpuiHideMenuBarStatusItem();
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn hide_gpui_menu_bar_status_item() {}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_workspace_power_events_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiWorkspacePowerEventsCallbackTarget { app, async_app });
    });
    unsafe {
        GhostexGpuiInstallWorkspacePowerEventsMonitor();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_workspace_power_events_callback_target() {
    unsafe {
        GhostexGpuiRemoveWorkspacePowerEventsMonitor();
    }
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_workspace_power_events_callback_target() -> Option<GpuiWorkspacePowerEventsCallbackTarget> {
    GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_workspace_terminal_escape_pressed(native_view: *mut std::ffi::c_void) {
    let Some(target) = gpui_terminal_key_event_callback_target_for_native_view(native_view) else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let native_view = native_view as usize;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.dispatch_gpui_workspace_terminal_escape_pressed_for_native_view(
                    native_view as *mut std::ffi::c_void,
                    cx,
                );
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_terminal_prompt_editor_shortcut(native_view: *mut std::ffi::c_void) -> bool {
    let Some(target) = gpui_terminal_key_event_callback_target_for_native_view(native_view) else {
        return false;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    let native_view = native_view as usize;
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.handle_native_terminal_prompt_editor_shortcut(
                    native_view as *mut std::ffi::c_void,
                    cx,
                );
            });
        })
        .detach();
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_os_integration_urls(urls: Vec<String>) {
    let Some(target) = gpui_os_integration_callback_target() else {
        GPUI_PENDING_OS_INTEGRATION_URLS.with(|pending| pending.borrow_mut().extend(urls));
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.receive_gpui_os_integration_urls(urls, window, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_menu_bar_status_project_click(project_id: String) {
    let Some(target) = gpui_menu_bar_status_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.activate_window();
                this.dispatch_gpui_menu_bar_project_activation(project_id.as_str(), cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_menu_bar_status_session_click(project_id: String, session_id: String) {
    let Some(target) = gpui_menu_bar_status_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.activate_window();
                this.dispatch_gpui_menu_bar_session_activation(
                    project_id.as_str(),
                    session_id.as_str(),
                    cx,
                );
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_workspace_did_wake() {
    let Some(target) = gpui_workspace_power_events_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.validate_gpui_remote_gxserver_connections(true, cx);
            });
        })
        .detach();
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiProjectSettingsMetadataUpdate {
    WorktreeCommand {
        project_id: String,
        command: String,
    },
    BeadsDisplayKey {
        project_id: String,
        display_key: String,
    },
    BeadsDirectory {
        project_id: String,
        directory: String,
    },
    DocsDirectory {
        project_id: String,
        directory: String,
    },
    SidebarCommands {
        project_id: String,
        commands: Vec<GpuiStoredSidebarCommand>,
        command_order: Vec<String>,
        deleted_default_command_ids: Vec<String>,
    },
}

impl GpuiProjectSettingsMetadataUpdate {
    pub(crate) fn project_id(&self) -> &str {
        match self {
            Self::WorktreeCommand { project_id, .. }
            | Self::BeadsDisplayKey { project_id, .. }
            | Self::BeadsDirectory { project_id, .. }
            | Self::DocsDirectory { project_id, .. }
            | Self::SidebarCommands { project_id, .. } => project_id,
        }
    }
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects_or_presentation(
    domain_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    gpui_project_settings_projects_from_domain_projects_or_else(domain_projects, || {
        gpui_read_gxserver_presentation_snapshot()
            .ok()
            .and_then(|snapshot| {
                let snapshot = snapshot.as_object()?;
                json_array_field(snapshot, "projects").map(|projects| {
                    gpui_project_settings_projects_from_presentation_projects(projects)
                })
            })
            .unwrap_or_default()
    })
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects_or_else<F>(
    domain_projects: &[serde_json::Value],
    presentation_projects: F,
) -> Vec<serde_json::Value>
where
    F: FnOnce() -> Vec<serde_json::Value>,
{
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:02:
    Project Settings parks only explicit boolean `isRecentProject: true` domain rows; string, number, false, missing, and malformed values do not become Recent Projects. If all domain rows are explicit recent or otherwise unusable, keep the presentation fallback path available rather than fabricating Settings rows.
    */
    let project_settings_projects =
        gpui_project_settings_projects_from_domain_projects(domain_projects);
    if !project_settings_projects.is_empty() {
        return project_settings_projects;
    }

    presentation_projects()
}

pub(crate) fn gpui_project_settings_projects_from_domain_projects(
    domain_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    domain_projects
        .iter()
        .filter_map(gpui_project_settings_project_from_domain_project)
        .collect::<Vec<_>>()
}

pub(crate) fn gpui_project_settings_projects_from_presentation_projects(
    presentation_projects: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    presentation_projects
        .iter()
        .filter_map(gpui_project_settings_project_from_presentation_project)
        .collect::<Vec<_>>()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRecentProjectMutation {
    Remove,
    Restore,
}

impl GpuiRecentProjectMutation {
    pub(crate) fn endpoint(self) -> &'static str {
        match self {
            Self::Remove => "/api/removeRecentProject",
            Self::Restore => "/api/restoreRecentProject",
        }
    }
}

pub(crate) fn gpui_recent_projects_result_message(request: &GpuiRecentProjectsRequest) -> serde_json::Value {
    /*
    Recent Projects follows the same transient app-modal response path as
    Previous Sessions. The owning gxserver remains the only persistence
    authority; failed local or remote reads return an empty contract-shaped
    result without exposing transport details or daemon response bodies.
    */
    let recent_projects = match request.machine_id.as_deref() {
        None => gpui_gxserver_recent_projects(Duration::from_secs(10)),
        Some(machine_id) => request
            .remote_target
            .as_ref()
            .map(|target| {
                gpui_recent_projects_from_remote_gxserver(
                    target,
                    machine_id,
                    request.machine_name.as_deref(),
                )
            })
            .unwrap_or_default(),
    };
    let mut message = serde_json::json!({
        "recentProjects": recent_projects,
        "type": "recentProjectsResult",
    });
    if let Some(machine_id) = request.machine_id.as_ref() {
        message["machineId"] = serde_json::json!(machine_id);
    }
    message
}

pub(crate) fn gpui_recent_project_mutation_and_result(
    mutation: GpuiRecentProjectMutation,
    project_id: String,
    request: GpuiRecentProjectsRequest,
) -> (bool, serde_json::Value) {
    let mutated = match request.machine_id.as_deref() {
        None => gpui_gxserver_rpc_result(
            mutation.endpoint(),
            &serde_json::json!({ "projectId": project_id }),
            Duration::from_secs(10),
        )
        .is_ok(),
        Some(_) => request.remote_target.as_ref().is_some_and(|target| {
            gpui_remote_gxserver_rpc_result(
                target,
                mutation.endpoint(),
                &serde_json::json!({ "projectId": project_id }),
                Duration::from_secs(10),
            )
            .is_ok()
        }),
    };
    let result = gpui_recent_projects_result_message(&request);
    (mutated, result)
}

pub(crate) fn gpui_project_settings_project_from_domain_project(
    project: &serde_json::Value,
) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    /*
    CDXC:GPUIRecentProjects 2026-06-25-18:50:
    GPUI app-modal normal project lists must mirror macOS by excluding explicit parked Recent Projects from `/api/listProjects` instead of deriving Settings project rows from those parked rows. Only `isRecentProject: true` is parked; zero-session normal projects and rows without the flag remain settings projects.
    */
    if gpui_gxserver_project_row_is_explicit_recent_project(project) {
        return None;
    }
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let name = gpui_trimmed_json_string_field(project, "name")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let git_config = project
        .get("gitConfig")
        .and_then(serde_json::Value::as_object);
    let project_board_config = project
        .get("projectBoardConfig")
        .and_then(serde_json::Value::as_object);
    let worktree = project
        .get("worktree")
        .and_then(serde_json::Value::as_object);

    let mut item = serde_json::Map::new();
    item.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeParentProjectId",
        worktree.and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId")),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeCommand",
        git_config.and_then(|config| gpui_trimmed_json_string_field(config, "worktreeCommand")),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "beadsDisplayKey",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDisplayKey"))
            .or_else(|| {
                git_config
                    .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDisplayKey"))
            }),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "beadsDirectory",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "beadsDirectory")),
    );
    /*
    CDXC:DocsRootDirectory 2026-08-09:
    The per-project Docs root rides in the same per-project config object the
    Beads directory already uses, so Settings -> Projects keeps one storage seam
    and the feature needs no new domain field, column, or migration.
    */
    gpui_insert_optional_nonempty_string(
        &mut item,
        "docsDirectory",
        project_board_config
            .and_then(|config| gpui_trimmed_json_string_field(config, "docsDirectory")),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_project_settings_project_from_presentation_project(
    project: &serde_json::Value,
) -> Option<serde_json::Value> {
    let project = project.as_object()?;
    let project_id = gpui_trimmed_json_string_field(project, "projectId")?;
    let name = gpui_trimmed_json_string_field(project, "title")?;
    let path = gpui_trimmed_json_string_field(project, "path")?;
    let mut item = serde_json::Map::new();
    item.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    item.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    item.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    gpui_insert_optional_nonempty_string(
        &mut item,
        "worktreeParentProjectId",
        project
            .get("worktree")
            .and_then(serde_json::Value::as_object)
            .and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId")),
    );
    Some(serde_json::Value::Object(item))
}

pub(crate) fn gpui_update_project_settings_metadata(
    update: GpuiProjectSettingsMetadataUpdate,
) -> Result<(), String> {
    let project_id = gpui_trimmed_nonempty_str(Some(update.project_id()))
        .ok_or_else(|| "Missing gxserver project id.".to_string())?
        .to_string();
    let project = gpui_find_gxserver_project_by_id(&project_id)?;
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id),
    );

    match update {
        GpuiProjectSettingsMetadataUpdate::WorktreeCommand { command, .. } => {
            let mut git_config = gpui_clone_json_object_field(&project, "gitConfig");
            git_config.insert(
                "worktreeCommand".to_string(),
                gpui_settings_metadata_string_or_null(&command),
            );
            params.insert(
                "gitConfig".to_string(),
                serde_json::Value::Object(git_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::BeadsDisplayKey { display_key, .. } => {
            let display_key = gpui_settings_beads_display_key_or_null(&display_key);
            let mut git_config = gpui_clone_json_object_field(&project, "gitConfig");
            git_config.insert("beadsDisplayKey".to_string(), display_key.clone());
            params.insert(
                "gitConfig".to_string(),
                serde_json::Value::Object(git_config),
            );

            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert("beadsDisplayKey".to_string(), display_key);
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::BeadsDirectory { directory, .. } => {
            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert(
                "beadsDirectory".to_string(),
                gpui_settings_metadata_string_or_null(&directory),
            );
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::DocsDirectory { directory, .. } => {
            let mut project_board_config =
                gpui_clone_json_object_field(&project, "projectBoardConfig");
            project_board_config.insert(
                "docsDirectory".to_string(),
                gpui_settings_metadata_string_or_null(&directory),
            );
            params.insert(
                "projectBoardConfig".to_string(),
                serde_json::Value::Object(project_board_config),
            );
        }
        GpuiProjectSettingsMetadataUpdate::SidebarCommands {
            commands,
            command_order,
            deleted_default_command_ids,
            ..
        } => {
            params.insert(
                "customCommands".to_string(),
                gpui_stored_sidebar_commands_value(&commands),
            );
            params.insert(
                "customCommandOrder".to_string(),
                gpui_string_array_value(&command_order),
            );
            params.insert(
                "deletedDefaultCommandIds".to_string(),
                gpui_string_array_value(&deleted_default_command_ids),
            );
        }
    }

    let result = gpui_gxserver_rpc_result(
        "/api/updateProject",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )?;
    if result
        .get("project")
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        Ok(())
    } else {
        Err("gxserver returned an invalid project update result.".to_string())
    }
}

pub(crate) fn gpui_active_project_id_from_snapshot(snapshot: Option<&GpuiProjectSnapshot>) -> Option<&str> {
    snapshot
        .and_then(|snapshot| snapshot.active_project_id.as_ref())
        .map(|project_id| project_id.0.as_str())
}

pub(crate) fn gpui_project_snapshot_is_quick_automations_overview(
    snapshot: Option<&GpuiProjectSnapshot>,
) -> bool {
    gpui_active_project_id_from_snapshot(snapshot) == Some(GPUI_QUICK_AUTOMATIONS_PROJECT_ID)
}

pub(crate) fn automate_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
    runtime_settings: &cef::SidebarRuntimeSettingsSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
    Automate mirrors macOS `createProjectAutomateEditorUrl`: use the bundled Kanban/tasks CEF page, the explicit project identity params, the automate-mode project editor id, and `surface=automations`. Projectless contexts, missing project path, or missing automateBoardId must stay on the placeholder instead of synthesizing an Automate URL.

    CDXC:GPUIAutomateStable 2026-07-26:
    Project-scoped Automate is no longer an experimental GPUI feature. Mark that first-party workarea explicitly so the shared page does not apply the Show Beta Features content gate or experimental label. Quick Automations Overview keeps its existing experimental startup seed.
    */
    if !snapshot.feature_availability.automate {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.0.clone();
    if active_project_id == GPUI_QUICK_AUTOMATIONS_PROJECT_ID {
        /*
        CDXC:GPUIQuickAutomationsOverview 2026-07-08:
        Mirror macOS `createQuickAutomationsProjectEditorUrl` in `native/sidebar/native-sidebar.tsx`: the quick-automations project is a real Automate overview surface with empty `projectPath`, all-project scope, and the same Show Beta Features seed. Its identity is the project id, so it must not require an in-memory project path or be rejected by the projectless guard.
        */
        let surface_id = snapshot.surface_ids.automate_board_id.as_ref()?.clone();
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
        return ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(
            append_url_query_params_with_percent_encoded_spaces(
                base_url,
                &[
                    (
                        "projectName",
                        GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE.to_string(),
                    ),
                    ("projectPath", String::new()),
                    ("projectId", GPUI_QUICK_AUTOMATIONS_PROJECT_ID.to_string()),
                    ("projectEditorId", surface_id),
                    ("surface", "automations".to_string()),
                    ("scope", "all".to_string()),
                    (
                        "beadsDisplayKey",
                        GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE.to_string(),
                    ),
                    (
                        "showBetaFeatures",
                        if runtime_settings.show_beta_features {
                            "true"
                        } else {
                            "false"
                        }
                        .to_string(),
                    ),
                ],
            ),
        );
    }
    if snapshot.is_quick_projectless {
        return None;
    }
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .to_string();
    let surface_id = snapshot.surface_ids.automate_board_id.as_ref()?.clone();
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        base_url,
        &[
            ("projectName", snapshot.display_name.clone()),
            ("projectPath", project_path),
            ("projectId", active_project_id),
            ("projectEditorId", surface_id),
            ("beadsDisplayKey", snapshot.display_name.clone()),
            ("surface", "automations".to_string()),
            ("automationExperimental", "false".to_string()),
            (
                "showBetaFeatures",
                if runtime_settings.show_beta_features {
                    "true"
                } else {
                    "false"
                }
                .to_string(),
            ),
        ],
    ))
}

pub(crate) fn run_project_beads_bridge_request_for_context(
    payload: &str,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    match project_beads_bridge_result_for_request(&request, context) {
        Ok(response) => response,
        Err(error) => project_beads_bridge_error_response(&request_id, &error),
    }
}

pub(crate) fn project_beads_bridge_result_for_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<serde_json::Value, String> {
    let context = context.ok_or_else(|| "No active Kanban project is available.".to_string())?;
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    let action = manage_request_string(request, "action").unwrap_or_default();
    if action == "generateTitle" {
        return gpui_project_beads_generate_title(request, context, &request_id);
    }

    let gxserver_action = project_beads_gxserver_action_for_board_action(&action)?;
    let mut params = serde_json::Map::new();
    params.insert(
        "action".to_string(),
        serde_json::Value::String(gxserver_action.to_string()),
    );
    params.insert(
        "projectBoardScope".to_string(),
        serde_json::Value::Bool(true),
    );
    params.insert(
        "projectPath".to_string(),
        serde_json::Value::String(context.project_path.clone()),
    );
    let request_project_id = manage_request_string(request, "projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let (Some(request_project_id), Some(active_project_id)) =
        (request_project_id.as_ref(), context.project_id.as_ref())
    {
        if request_project_id != active_project_id {
            return Err("Project board request was not sent by this active project.".to_string());
        }
    }
    if let Some(project_id) = request_project_id.or_else(|| context.project_id.clone()) {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id),
        );
    }

    for key in [
        "comment",
        "dependsOnId",
        "depType",
        "description",
        "issueId",
        "label",
        "priority",
        "query",
        "status",
        "title",
        "value",
    ] {
        if let Some(value) = manage_request_string(request, key) {
            params.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
    if let Some(estimate) = request.get("estimate").filter(|value| value.is_number()) {
        params.insert("estimate".to_string(), estimate.clone());
    }
    if let Some(labels) = request.get("labels").and_then(serde_json::Value::as_array) {
        let labels = labels
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|label| serde_json::Value::String(label.to_string()))
            .collect::<Vec<_>>();
        params.insert("labels".to_string(), serde_json::Value::Array(labels));
    }

    let params = serde_json::Value::Object(params);
    if context.remote_machine_id.is_some() {
        let target = context
            .remote_target
            .as_ref()
            .ok_or_else(|| "The remote gxserver is not connected.".to_string())?;
        let result = gpui_remote_gxserver_rpc_result(
            target,
            "/api/runBeadsAction",
            &params,
            Duration::from_secs(60),
        )?;
        return Ok(project_beads_bridge_response_from_result(
            &result,
            &request_id,
        ));
    }
    let (status_code, body) =
        gxserver_post_typed_operation("/api/runBeadsAction", &params, Duration::from_secs(60))?;
    project_beads_bridge_response_from_gxserver(status_code, &body, &request_id)
}

pub(crate) fn project_beads_bridge_response_from_result(
    result: &serde_json::Value,
    request_id: &str,
) -> serde_json::Value {
    let exit_code = result
        .get("exitCode")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(1);
    serde_json::json!({
        "error": null,
        "exitCode": exit_code,
        "requestId": request_id,
        "stderr": result.get("stderr").and_then(serde_json::Value::as_str).unwrap_or(""),
        "stdout": result.get("stdout").and_then(serde_json::Value::as_str).unwrap_or(""),
    })
}

pub(crate) fn project_beads_bridge_error_response(request_id: &str, error: &str) -> serde_json::Value {
    serde_json::json!({
        "error": error,
        "exitCode": 127,
        "requestId": request_id,
        "stderr": error,
        "stdout": "",
    })
}

pub(crate) fn gpui_project_beads_generate_title(
    request: &serde_json::Value,
    context: &ProjectBoardBridgeRuntimeContext,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    // Scenario-gated (native.project.board) title-generation diagnostics:
    // agent id, prompt size, duration, and outcome only — never prompt text,
    // generated titles, paths, or process output.
    let agent_id = manage_request_string(request, "agentId").unwrap_or_default();
    let prompt_chars = manage_request_string(request, "prompt")
        .map(|prompt| prompt.chars().count())
        .unwrap_or(0);
    let started_at = Instant::now();
    support_logs::append(
        support_logs::GpuiSupportLog::ProjectBoard,
        "gpui.projectBoard.generateTitleStarted",
        serde_json::json!({ "agentId": agent_id, "promptChars": prompt_chars }),
    );
    let result = gpui_project_beads_generate_title_inner(request, context, request_id);
    support_logs::append(
        support_logs::GpuiSupportLog::ProjectBoard,
        if result.is_ok() {
            "gpui.projectBoard.generateTitleFinished"
        } else {
            "gpui.projectBoard.generateTitleFailed"
        },
        serde_json::json!({
            "durationMs": started_at.elapsed().as_millis() as u64,
            "ok": result.is_ok(),
        }),
    );
    result
}

pub(crate) fn gpui_project_beads_generate_title_inner(
    request: &serde_json::Value,
    context: &ProjectBoardBridgeRuntimeContext,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    /*
    macOS `projectBeadsGenerateTitle` parity (TerminalWorkspaceView.swift):
    board ticket title generation is a local prompt-agent subprocess, not a
    gxserver endpoint. The selected/default prompt agent runs once through a
    login zsh with the title prompt fed as a heredoc, and the first non-empty
    stdout line becomes the sanitized 39-char board title.
    */
    let prompt = manage_request_string(request, "prompt")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Missing required Beads field: prompt".to_string())?;
    let agent_command = manage_request_string(request, "agentCommand");
    let agent_id = manage_request_string(request, "agentId");
    let cwd = std::path::PathBuf::from(&context.project_path);
    if !cwd.is_dir() {
        return Err(format!(
            "Project path does not exist: {}",
            context.project_path
        ));
    }
    let generation_command = gpui_project_beads_prompt_generation_command(
        agent_command.as_deref(),
        agent_id.as_deref(),
    )?;
    let source_text: String = prompt.chars().take(4_000).collect();
    let generation_prompt = format!(
        "Write a concise session title that summarizes the user's text.\n\
         Return plain text only.\n\
         Rules:\n\
         - keep it specific and scannable\n\
         - must be fewer than 60 characters\n\
         - do not use quotes, markdown, or commentary\n\
         - do not end with punctuation\n\
         - focus on the task, bug, feature, or topic\n\
         \n\
         User text:\n\
         {source_text}\n\
         \n\
         Output handling:\n\
         - Produce only the final session title.\n\
         - Do not wrap the result in backticks.\n\
         - Print only the final result to stdout."
    );
    let delimiter = format!(
        "ghostex_SESSION_TITLE_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    );
    let command =
        format!("{generation_command} <<'{delimiter}'\n{generation_prompt}\n{delimiter}\n");
    let mut process = std::process::Command::new("/bin/zsh");
    process
        .arg("-lc")
        .arg(&command)
        .current_dir(&cwd)
        .env("GHOSTEX_INTERNAL_PROMPT_GENERATION", "1")
        .env("GHOSTEX_INTERNAL_TITLE_GENERATION", "1")
        .env(
            "PATH",
            gpui_project_beads_title_generation_path(std::env::var("PATH").ok().as_deref()),
        );
    for key in GPUI_PROJECT_BEADS_TITLE_GENERATION_STRIPPED_ENV_KEYS {
        process.env_remove(key);
    }
    // macOS waits on the process with no deadline (the page's own 60s bridge
    // timeout owns UI responsiveness); the beads bridge already runs on the
    // background executor.
    let output = process
        .output()
        .map_err(|_| "Could not launch the prompt-agent title generation process.".to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            "Prompt-agent title generation failed.".to_string()
        } else {
            stderr.to_string()
        });
    }
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "Prompt-agent title generation returned an empty title.".to_string())?;
    let sanitized = gpui_project_beads_sanitize_generated_title(line);
    if sanitized.is_empty() {
        return Err("Prompt-agent title generation returned an empty title.".to_string());
    }
    let title: String = sanitized.chars().take(39).collect();
    Ok(serde_json::json!({
        "error": null,
        "exitCode": 0,
        "requestId": request_id,
        "stderr": "",
        "stdout": serde_json::json!({ "title": title }).to_string(),
    }))
}

pub(crate) const GPUI_PROJECT_BEADS_TITLE_GENERATION_STRIPPED_ENV_KEYS: [&str; 22] = [
    // macOS `projectBoardInternalPromptGenerationEnvironmentKeys`: internal
    // prompt-agent work must not inherit Ghostex session-binding environment,
    // so hooks cannot turn the background job into a restorable user session.
    "GHOSTEX_AGENT",
    "GHOSTEX_GLOBAL_SESSION_REF",
    "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
    "GHOSTEX_GXSERVER_BASE_URL",
    "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
    "GHOSTEX_NATIVE_SESSION_ID",
    "GHOSTEX_SESSION_ID",
    "GHOSTEX_SESSION_STATE_FILE",
    "GHOSTEX_WORKSPACE_ID",
    "GHOSTEX_WORKSPACE_ROOT",
    "VSMUX_AGENT",
    "VSMUX_SESSION_ID",
    "VSMUX_SESSION_STATE_FILE",
    "VSMUX_WORKSPACE_ID",
    "VSMUX_WORKSPACE_ROOT",
    "ZMX_SESSION",
    "ZMX_SESSION_PREFIX",
    "ghostex_AGENT",
    "ghostex_SESSION_ID",
    "ghostex_SESSION_STATE_FILE",
    "ghostex_WORKSPACE_ID",
    "ghostex_WORKSPACE_ROOT",
];

pub(crate) fn gpui_project_beads_prompt_generation_command(
    agent_command: Option<&str>,
    agent_id: Option<&str>,
) -> Result<String, String> {
    // macOS `projectBeadsPromptGenerationCommand` parity, including the
    // ephemeral Codex exec profile so a title prompt can never become a
    // restorable Codex transcript.
    const CODEX_EXEC_ARGS: &str = "exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort=\"low\"'";
    let normalized_agent_id = agent_id
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    let command = agent_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if normalized_agent_id.is_empty() {
        return Ok(format!("codex {CODEX_EXEC_ARGS}"));
    }
    if let Some(command) = command {
        return Ok(match normalized_agent_id.as_str() {
            "codex" => format!("{command} {CODEX_EXEC_ARGS}"),
            "cursor" => format!(
                "{command} --print --mode ask --trust --model cursor-grok-4.5-low --output-format text"
            ),
            "claude" => format!("{command} -p --model haiku --effort low"),
            "gemini" => format!("{command} -p"),
            _ => command.to_string(),
        });
    }
    match normalized_agent_id.as_str() {
        "codex" => Ok(format!("codex {CODEX_EXEC_ARGS}")),
        "claude" => Ok("claude -p --model haiku --effort low".to_string()),
        "cursor" => Ok(
            "cursor-agent --print --mode ask --trust --model cursor-grok-4.5-low --output-format text"
                .to_string(),
        ),
        "gemini" => Ok("gemini -p".to_string()),
        _ => Err(format!(
            "{normalized_agent_id} does not support background title generation."
        )),
    }
}

pub(crate) fn gpui_project_beads_title_generation_path(existing: Option<&str>) -> String {
    // macOS `projectBoardNativeProcessPath`: common tool directories lead so
    // agent CLIs resolve even under minimal login-shell PATH files.
    let home = std::env::var("HOME").unwrap_or_default();
    let default_entries = [
        format!("{home}/.local/share/mise/shims"),
        format!("{home}/.local/bin"),
        format!("{home}/.asdf/shims"),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ];
    let mut seen = std::collections::HashSet::new();
    default_entries
        .into_iter()
        .chain(existing.unwrap_or_default().split(':').map(str::to_string))
        .filter(|entry| {
            let normalized = entry.trim().to_string();
            !normalized.is_empty() && seen.insert(normalized)
        })
        .collect::<Vec<_>>()
        .join(":")
}

pub(crate) fn gpui_project_beads_sanitize_generated_title(line: &str) -> String {
    let stripped = line
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | '`'));
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_end_matches(|character| matches!(character, '.' | '…'))
        .trim()
        .to_string()
}

pub(crate) fn gpui_normalized_project_path_for_comparison(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

pub(crate) fn gpui_add_project_dialog_rpc_result(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let (status_code, body) = match target {
        Some(target) => gpui_remote_gxserver_post_typed_operation(target, path, params, timeout)
            .map_err(|_| "The remote machine did not answer.".to_string())?,
        None => gxserver_post_typed_operation(path, params, timeout)
            .map_err(|_| "gxserver is not reachable.".to_string())?,
    };
    if !(200..300).contains(&status_code) {
        return Err(gpui_add_project_dialog_error_message(&body));
    }
    parse_gpui_gxserver_rpc_result(&body)
        .map_err(|_| "gxserver returned an unexpected response.".to_string())
}

pub(crate) fn gpui_add_project_dialog_restore_recent_project(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    add_result: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:AddProjectRestoreRecent 2026-08-12:
    `/api/addProjectPath` is intentionally idempotent and returns an existing
    path registration unchanged. When that registration is parked in Recent
    Projects, Add Project must perform the same authoritative restore mutation
    as clicking its Recent Projects row before reporting success. Otherwise the
    dialog closes around a still-hidden `isRecentProject: true` project and the
    user sees a silent no-op.
    */
    let Some(project) = add_result
        .get("project")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(add_result);
    };
    if !gpui_gxserver_project_row_is_explicit_recent_project(project) {
        return Ok(add_result);
    }
    let project_id = gpui_trimmed_json_string_field(project, "projectId")
        .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
        .ok_or_else(|| "gxserver returned an unexpected response.".to_string())?;
    gpui_add_project_dialog_rpc_result(
        target,
        "/api/restoreRecentProject",
        &serde_json::json!({ "projectId": project_id }),
        timeout,
    )
}

pub(crate) fn gpui_add_project_dialog_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| {
            message
                .chars()
                .filter(|character| !character.is_control())
                .take(GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
                .collect::<String>()
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "gxserver rejected the request.".to_string())
}

pub(crate) fn gpui_repository_clone_rpc_result(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    match target {
        Some(target) => gpui_remote_gxserver_rpc_result(target, path, params, timeout),
        None => gpui_gxserver_rpc_result(path, params, timeout),
    }
}

pub(crate) fn gpui_command_pane_side_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> GpuiCommandPaneSide {
    match settings.command_pane_side() {
        shared_settings::SharedCommandPaneSide::Bottom => GpuiCommandPaneSide::Bottom,
        shared_settings::SharedCommandPaneSide::Right => GpuiCommandPaneSide::Right,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiStatusIndicatorProjectState {
    pub(crate) icon_data_url: Option<String>,
    pub(crate) project_id: String,
    pub(crate) sessions: Vec<GpuiStatusIndicatorSessionState>,
    pub(crate) title: String,
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
What the tab strip needs to draw one Global Action button and to ask the sidebar
runtime to run it: a bounded id, a display name for the tooltip, and an optional
icon slug. Deliberately no command text, URL, cwd, or run state — the click
sends the id back through the existing Action selector bridge, which resolves the
trusted definition on the sidebar side, so a compromised renderer payload cannot
put an executable string in front of the user.
*/
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuiMenuBarStatusNativeProjectEntry {
    pub(crate) project_id: *const std::ffi::c_char,
    pub(crate) title: *const std::ffi::c_char,
    pub(crate) sessions: *const GpuiMenuBarStatusNativeSessionEntry,
    pub(crate) session_count: usize,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct GpuiMenuBarStatusNativeSessionOwner {
    pub(crate) session_id: std::ffi::CString,
    pub(crate) title: std::ffi::CString,
    pub(crate) last_active_at: Option<std::ffi::CString>,
    pub(crate) entry: GpuiMenuBarStatusNativeSessionEntry,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub(crate) struct GpuiMenuBarStatusNativeProjectOwner {
    pub(crate) project_id: std::ffi::CString,
    pub(crate) title: std::ffi::CString,
    pub(crate) sessions: Vec<GpuiMenuBarStatusNativeSessionOwner>,
    pub(crate) entries: Vec<GpuiMenuBarStatusNativeSessionEntry>,
}

pub(crate) fn gpui_status_indicator_project_from_value(
    value: &serde_json::Value,
) -> Result<GpuiStatusIndicatorProjectState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["projectId", "title", "sessions", "iconDataUrl"])?;
    let sessions = object
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .filter(|sessions| sessions.len() <= GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT)
        .ok_or(())?
        .iter()
        .map(gpui_status_indicator_session_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GpuiStatusIndicatorProjectState {
        icon_data_url: gpui_status_optional_icon_data_url_field(object, "iconDataUrl")?,
        project_id: gpui_status_id_field(object, "projectId")?,
        sessions,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_status_indicator_session_from_value(
    value: &serde_json::Value,
) -> Result<GpuiStatusIndicatorSessionState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(
        object,
        &[
            "sessionId",
            "status",
            "title",
            "sidebarOrder",
            "lastActiveAt",
        ],
    )?;
    Ok(GpuiStatusIndicatorSessionState {
        last_active_at: gpui_status_optional_title_field(object, "lastActiveAt")?,
        order: object
            .get("sidebarOrder")
            .and_then(serde_json::Value::as_u64)
            .ok_or(())?,
        session_id: gpui_status_id_field(object, "sessionId")?,
        status: gpui_status_field(object, "status")?,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_menu_bar_project_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onMenuBarProjectActivation==='function'){{bridge.onMenuBarProjectActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingMenuBarProjectActivations)?bridge.pendingMenuBarProjectActivations:[];pending.push(payload);bridge.pendingMenuBarProjectActivations=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_menu_bar_session_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onMenuBarSessionActivation==='function'){{bridge.onMenuBarSessionActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingMenuBarSessionActivations)?bridge.pendingMenuBarSessionActivations:[];pending.push(payload);bridge.pendingMenuBarSessionActivations=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_command_palette_session_focus_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onCommandPaletteSessionFocus==='function'){{bridge.onCommandPaletteSessionFocus(payload);}}else{{const pending=Array.isArray(bridge.pendingCommandPaletteSessionFocusRequests)?bridge.pendingCommandPaletteSessionFocusRequests:[];pending.push(payload);bridge.pendingCommandPaletteSessionFocusRequests=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_tab_session_selected_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onWorkspaceTabSessionSelected==='function'){{bridge.onWorkspaceTabSessionSelected(payload);}}else{{const pending=Array.isArray(bridge.pendingWorkspaceTabSessionSelections)?bridge.pendingWorkspaceTabSessionSelections:[];pending.push(payload);bridge.pendingWorkspaceTabSessionSelections=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_workspace_terminal_rename_command_input(
    command: GpuiWorkspaceTerminalRenameCommandKind,
    title: &str,
) -> String {
    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    This is the only rename path that turns the validated title into terminal input. It must remain a fixed `/rename <title>`, Pi `/name <title>`, or Hermes Agent `/title <title>` command chosen by the validated enum selector for the already-resolved Agents surface and must not add shell escaping, logging, persistence, fallback commands, or renderer-selected text.
    */
    match command {
        GpuiWorkspaceTerminalRenameCommandKind::Name => format!("/name {title}"),
        GpuiWorkspaceTerminalRenameCommandKind::Rename => format!("/rename {title}"),
        GpuiWorkspaceTerminalRenameCommandKind::Title => format!("/title {title}"),
    }
}

pub(crate) fn gpui_project_git_changed_file_paths(project_id: &str) -> Result<HashSet<String>, String> {
    let status = gpui_gxserver_git_action_result(project_id, "statusPorcelain")?;
    if gpui_typed_operation_exit_code(&status) != Some(0) {
        return Err("GPUI could not refresh changed files.".to_string());
    }
    let mut files = HashSet::new();
    gpui_collect_git_status_porcelain_paths(gpui_typed_operation_stdout(&status), &mut files);

    let diff = gpui_gxserver_git_action_result(project_id, "diffNumstat")?;
    if gpui_typed_operation_exit_code(&diff) == Some(0) {
        gpui_collect_git_numstat_paths(gpui_typed_operation_stdout(&diff), &mut files);
    }

    let untracked = gpui_gxserver_git_action_result(project_id, "listUntracked")?;
    if gpui_typed_operation_exit_code(&untracked) == Some(0) {
        gpui_collect_git_zero_delimited_paths(gpui_typed_operation_stdout(&untracked), &mut files);
    }
    Ok(files)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceEditorLaunchKind {
    DirectPath,
    VscodeCompatible,
    ZedCompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiWorkspaceEditorTarget {
    pub(crate) command: &'static str,
    pub(crate) app_names: &'static [&'static str],
    pub(crate) launch_kind: GpuiWorkspaceEditorLaunchKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCustomWorkspaceEditorCommand {
    pub(crate) executable: GpuiCustomWorkspaceEditorExecutable,
    pub(crate) args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCustomWorkspaceEditorExecutable {
    AbsolutePath(PathBuf),
    PathSearch(String),
}

pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_APP_NAMES: &[&str] = &["Visual Studio Code"];
pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES: &[&str] = &["Visual Studio Code - Insiders"];
pub(crate) const GPUI_WORKSPACE_EDITOR_CODIUM_APP_NAMES: &[&str] = &["VSCodium"];
pub(crate) const GPUI_WORKSPACE_EDITOR_CURSOR_APP_NAMES: &[&str] = &["Cursor"];
pub(crate) const GPUI_WORKSPACE_EDITOR_WINDSURF_APP_NAMES: &[&str] = &["Windsurf"];
pub(crate) const GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES: &[&str] = &["Zed", "Zed Preview"];
pub(crate) const GPUI_WORKSPACE_EDITOR_SUBLIME_APP_NAMES: &[&str] = &["Sublime Text"];

pub(crate) const GPUI_WORKSPACE_EDITOR_VSCODE_TARGET: GpuiWorkspaceEditorTarget = GpuiWorkspaceEditorTarget {
    command: "code",
    app_names: GPUI_WORKSPACE_EDITOR_VSCODE_APP_NAMES,
    launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
};

pub(crate) const GPUI_WORKSPACE_EDITOR_ZED_TARGET: GpuiWorkspaceEditorTarget = GpuiWorkspaceEditorTarget {
    command: "zed",
    app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
    launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
};

pub(crate) fn gpui_open_project_path_for_native_ide_action(
    action: GpuiSidebarNativeProjectPathAction,
    project_path: &Path,
) -> Result<(), String> {
    match action {
        GpuiSidebarNativeProjectPathAction::OpenActiveWorkspaceProjectInVscode => {
            gpui_open_project_path_in_editor_target(
                GPUI_WORKSPACE_EDITOR_VSCODE_TARGET,
                project_path,
            )
        }
        GpuiSidebarNativeProjectPathAction::OpenActiveWorkspaceProjectInZed => {
            gpui_open_project_path_in_editor_target(GPUI_WORKSPACE_EDITOR_ZED_TARGET, project_path)
        }
        GpuiSidebarNativeProjectPathAction::OpenWorkspaceProjectInIde => {
            gpui_open_project_path_in_default_editor(project_path)
        }
        _ => Err("Configured editor is not available for GPUI project open.".to_string()),
    }
}

pub(crate) fn gpui_open_project_path_in_default_editor(project_path: &Path) -> Result<(), String> {
    /*
    CDXC:GPUISidebarProjectPathActions 2026-06-24-13:49:
    Generic GPUI project IDE opens are native-owned Settings behavior. The sidebar action supplies only a gxserver project id; this launcher supports the normalized built-in default editor commands with fixed argv or fixed macOS app names, suppresses stdio, and reports only generic failure text.

    CDXC:GPUISidebarProjectPathActions 2026-06-24-13:57:
    Custom default editor command support is intentionally narrower than a shell: parse Settings-owned text into literal argv, reject shell syntax/placeholders, require an executable found by PATH or absolute executable path, append the gxserver-resolved project path as argv, suppress child stdio, and return generic UI failures without exposing command text or paths.
    */
    let settings = shared_settings::shared_sidebar_settings_snapshot().external_editor_settings();
    if settings.default_editor_command() == shared_settings::SharedDefaultEditorCommand::Other
        && settings.editor_command().trim() != shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND
    {
        return gpui_open_project_path_in_custom_default_editor(
            settings.editor_command(),
            project_path,
        );
    }
    let target = gpui_workspace_editor_target_from_settings(&settings)?;
    gpui_open_project_path_in_editor_target(target, project_path)
}

pub(crate) fn gpui_open_project_path_in_editor_target(
    target: GpuiWorkspaceEditorTarget,
    project_path: &Path,
) -> Result<(), String> {
    if gpui_command_exists_on_path(target.command) {
        return gpui_spawn_workspace_editor_command(target, project_path);
    }
    for app_name in target.app_names {
        if gpui_macos_named_app_exists(app_name) {
            return gpui_spawn_open_target_app_name(app_name, project_path)
                .map_err(|_| "Configured editor could not open that project.".to_string());
        }
    }
    Err("Configured editor is not available for GPUI project open.".to_string())
}

pub(crate) fn gpui_workspace_editor_target_from_settings(
    settings: &shared_settings::SharedDefaultEditorSettings,
) -> Result<GpuiWorkspaceEditorTarget, String> {
    match settings.default_editor_command() {
        shared_settings::SharedDefaultEditorCommand::Code => {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::CodeInsiders => {
            Ok(GpuiWorkspaceEditorTarget {
                command: "code-insiders",
                app_names: GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES,
                launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
            })
        }
        shared_settings::SharedDefaultEditorCommand::Codium => Ok(GpuiWorkspaceEditorTarget {
            command: "codium",
            app_names: GPUI_WORKSPACE_EDITOR_CODIUM_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Cursor => Ok(GpuiWorkspaceEditorTarget {
            command: "cursor",
            app_names: GPUI_WORKSPACE_EDITOR_CURSOR_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Windsurf => Ok(GpuiWorkspaceEditorTarget {
            command: "windsurf",
            app_names: GPUI_WORKSPACE_EDITOR_WINDSURF_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Zed => Ok(GPUI_WORKSPACE_EDITOR_ZED_TARGET),
        shared_settings::SharedDefaultEditorCommand::Zeditor => Ok(GpuiWorkspaceEditorTarget {
            command: "zeditor",
            app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Subl => Ok(GpuiWorkspaceEditorTarget {
            command: "subl",
            app_names: GPUI_WORKSPACE_EDITOR_SUBLIME_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::DirectPath,
        }),
        shared_settings::SharedDefaultEditorCommand::Other
            if settings.editor_command().trim()
                == shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND =>
        {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::Other => {
            Err("Configured editor is not available for GPUI project open.".to_string())
        }
    }
}

pub(crate) fn gpui_open_project_path_in_custom_default_editor(
    editor_command: &str,
    project_path: &Path,
) -> Result<(), String> {
    let command = gpui_parse_custom_workspace_editor_command(editor_command)?;
    gpui_spawn_custom_workspace_editor_command(&command, project_path)
}

pub(crate) fn gpui_parse_custom_workspace_editor_command(
    editor_command: &str,
) -> Result<GpuiCustomWorkspaceEditorCommand, String> {
    /*
    CDXC:GPUISidebarProjectPathActions 2026-06-24-13:57:
    This parser is not a shell compatibility layer. It accepts only Settings-owned argv text, uses quotes/backslashes only to form literal tokens, and rejects shell control or expansion syntax so GPUI project opens never execute arbitrary custom command snippets.
    */
    let trimmed = editor_command.trim();
    if trimmed.is_empty()
        || gpui_custom_workspace_editor_command_has_unsupported_shell_syntax(trimmed)
    {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    let mut argv = gpui_split_custom_workspace_editor_argv(trimmed)?;
    let executable = argv.remove(0);
    if executable.trim().is_empty()
        || executable.contains('\\')
        || executable.chars().any(char::is_control)
    {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }

    let executable_path = PathBuf::from(&executable);
    let executable = if executable_path.is_absolute() {
        if !gpui_is_executable_file(&executable_path) {
            return Err("Configured editor is not available for GPUI project open.".to_string());
        }
        GpuiCustomWorkspaceEditorExecutable::AbsolutePath(executable_path)
    } else {
        if executable.contains('/') || executable.chars().any(char::is_whitespace) {
            return Err(
                "Custom default editor command is not supported for GPUI project open.".to_string(),
            );
        }
        if !gpui_command_exists_on_path(&executable) {
            return Err("Configured editor is not available for GPUI project open.".to_string());
        }
        GpuiCustomWorkspaceEditorExecutable::PathSearch(executable)
    };

    Ok(GpuiCustomWorkspaceEditorCommand {
        executable,
        args: argv,
    })
}

pub(crate) fn gpui_custom_workspace_editor_command_has_unsupported_shell_syntax(value: &str) -> bool {
    value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '|' | '&'
                    | ';'
                    | '<'
                    | '>'
                    | '`'
                    | '$'
                    | '*'
                    | '?'
                    | '%'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '#'
            )
    })
}

pub(crate) fn gpui_split_custom_workspace_editor_argv(command: &str) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut token_started = false;
    let mut chars = command.chars().peekable();

    while let Some(character) = chars.next() {
        match quote {
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                } else {
                    current.push(character);
                }
            }
            Some('"') => {
                if character == '"' {
                    quote = None;
                } else if character == '\\' {
                    let Some(next) = chars.next() else {
                        return Err(
                            "Custom default editor command is not supported for GPUI project open."
                                .to_string(),
                        );
                    };
                    if next == '"' || next == '\\' {
                        current.push(next);
                    } else {
                        current.push('\\');
                        current.push(next);
                    }
                } else {
                    current.push(character);
                }
            }
            Some(_) => unreachable!("custom editor parser uses only quote delimiters"),
            None => {
                if character.is_whitespace() {
                    if token_started {
                        argv.push(std::mem::take(&mut current));
                        token_started = false;
                    }
                } else if character == '\'' || character == '"' {
                    quote = Some(character);
                    token_started = true;
                } else if character == '\\' {
                    let Some(next) = chars.next() else {
                        return Err(
                            "Custom default editor command is not supported for GPUI project open."
                                .to_string(),
                        );
                    };
                    current.push(next);
                    token_started = true;
                } else {
                    current.push(character);
                    token_started = true;
                }
            }
        }
    }

    if quote.is_some() {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    if token_started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err(
            "Custom default editor command is not supported for GPUI project open.".to_string(),
        );
    }
    Ok(argv)
}

pub(crate) fn gpui_project_context_from_env_bridge_value(value: Option<&str>) -> GpuiProjectContext {
    GpuiProjectContext::from_project_is_quick_bridge(value == Some("true"))
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_contract_project_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &[
            "activeProjectId",
            "displayName",
            "projectIconDataUrl",
            "projectPath",
            "selectionOwnerProjectId",
            "isQuickProjectless",
            "workareaAvailability",
            "surfaceIds",
        ],
    )?;

    let active_project_id = required_nullable_contract_string_field(
        object,
        "activeProjectId",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?
    .map(GpuiProjectId);
    let selection_owner_project_id = required_nullable_contract_string_field(
        object,
        "selectionOwnerProjectId",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?
    .map(GpuiProjectId);
    let display_name = required_contract_string_field(
        object,
        "displayName",
        GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
    )?;
    let project_icon_data_url =
        optional_contract_icon_data_url_field(object, "projectIconDataUrl")?;
    let in_memory_project_path = required_nullable_contract_string_field(
        object,
        "projectPath",
        GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS,
    )?
    .map(PathBuf::from);
    let is_quick_projectless = required_contract_bool_field(object, "isQuickProjectless")?;
    let feature_availability = object
        .get("workareaAvailability")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(gpui_project_feature_availability_from_contract_value)?;
    let surface_ids = object
        .get("surfaceIds")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(gpui_project_surface_ids_from_contract_value)?;
    let is_quick_automations_overview = active_project_id
        .as_ref()
        .is_some_and(|project_id| project_id.0 == GPUI_QUICK_AUTOMATIONS_PROJECT_ID);

    if !feature_availability.source && !is_quick_automations_overview {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }
    if surface_ids.has_ids_for_unavailable_features(feature_availability) {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }

    if is_quick_projectless {
        if active_project_id.is_some()
            || selection_owner_project_id.is_some()
            || project_icon_data_url.is_some()
            || in_memory_project_path.is_some()
            || surface_ids.has_any()
            || !feature_availability.is_quick_projectless_compatible()
        {
            return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
        }
    } else if active_project_id.is_none() || selection_owner_project_id.is_none() {
        return Err(GpuiProjectSnapshotContractError::InconsistentProjectContext);
    }

    Ok(GpuiProjectSnapshot {
        active_project_id,
        selection_owner_project_id,
        display_name,
        project_icon_data_url,
        in_memory_project_path,
        is_quick_projectless,
        feature_availability,
        surface_ids,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_project_feature_availability_from_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectScopedFeatureAvailability, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &["source", "browser", "kanban", "automate", "manage"],
    )?;
    Ok(GpuiProjectScopedFeatureAvailability {
        source: required_contract_bool_field(object, "source")?,
        browser: required_contract_bool_field(object, "browser")?,
        kanban: required_contract_bool_field(object, "kanban")?,
        automate: required_contract_bool_field(object, "automate")?,
        manage: required_contract_bool_field(object, "manage")?,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_project_surface_ids_from_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSurfaceIds, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(
        object,
        &[
            "sourceWorkareaId",
            "kanbanBoardId",
            "automateBoardId",
            "manageWorkspaceId",
        ],
    )?;
    Ok(GpuiProjectSurfaceIds {
        source_workarea_id: optional_contract_string_field(
            object,
            "sourceWorkareaId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        kanban_board_id: optional_contract_string_field(
            object,
            "kanbanBoardId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        automate_board_id: optional_contract_string_field(
            object,
            "automateBoardId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
        manage_workspace_id: optional_contract_string_field(
            object,
            "manageWorkspaceId",
            GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS,
        )?,
    })
}

#[allow(dead_code)]
pub(crate) fn gpui_contract_object(
    value: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, GpuiProjectSnapshotContractError> {
    value
        .as_object()
        .ok_or(GpuiProjectSnapshotContractError::ExpectedObject)
}

#[allow(dead_code)]
pub(crate) fn reject_unexpected_contract_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), GpuiProjectSnapshotContractError> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(GpuiProjectSnapshotContractError::UnexpectedKey);
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn required_contract_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, GpuiProjectSnapshotContractError> {
    object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(|value| {
            value
                .as_bool()
                .ok_or(GpuiProjectSnapshotContractError::MalformedField)
        })
}

#[allow(dead_code)]
pub(crate) fn required_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<String, GpuiProjectSnapshotContractError> {
    object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)
        .and_then(|value| contract_string(value, max_chars))
}

#[allow(dead_code)]
pub(crate) fn required_nullable_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    let value = object
        .get(key)
        .ok_or(GpuiProjectSnapshotContractError::MissingField)?;
    if value.is_null() {
        Ok(None)
    } else {
        contract_string(value, max_chars).map(Some)
    }
}

#[allow(dead_code)]
pub(crate) fn optional_contract_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => contract_string(value, max_chars).map(Some),
    }
}

#[allow(dead_code)]
pub(crate) fn optional_contract_icon_data_url_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, GpuiProjectSnapshotContractError> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_status_icon_data_url_allowed(value))
            .map(str::to_string)
            .map(Some)
            .ok_or(GpuiProjectSnapshotContractError::MalformedField),
    }
}

#[allow(dead_code)]
pub(crate) fn contract_string(
    value: &serde_json::Value,
    max_chars: usize,
) -> Result<String, GpuiProjectSnapshotContractError> {
    let value = value
        .as_str()
        .ok_or(GpuiProjectSnapshotContractError::MalformedField)?;
    if value.trim().is_empty() || value.chars().count() > max_chars {
        return Err(GpuiProjectSnapshotContractError::MalformedField);
    }
    Ok(value.to_string())
}

pub(crate) fn next_available_gpui_local_workspace_lifecycle_request_id(
    next_request_id: u64,
    mut is_pending: impl FnMut(u64) -> bool,
) -> Option<(u64, u64)> {
    next_available_gpui_local_workspace_lifecycle_request_id_in_range(
        next_request_id,
        GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_ID_MAX,
        &mut is_pending,
    )
}

pub(crate) fn next_available_gpui_local_workspace_lifecycle_request_id_in_range(
    next_request_id: u64,
    request_id_max: u64,
    mut is_pending: impl FnMut(u64) -> bool,
) -> Option<(u64, u64)> {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:44:
    Rust-to-sidebar Sleep/Wake acks are matched only by request id, so id allocation must never overwrite a still-pending request after wraparound. Local-first Close also receives an id for its ignored cleanup acknowledgement. Skip live pending ids and fail dispatch only if the bounded id space is exhausted.
    */
    let request_id_max = request_id_max.max(1);
    let first_request_id = next_request_id.clamp(1, request_id_max);
    let mut request_id = first_request_id;
    loop {
        let next_candidate = if request_id >= request_id_max {
            1
        } else {
            request_id + 1
        };
        if !is_pending(request_id) {
            return Some((request_id, next_candidate));
        }
        request_id = next_candidate;
        if request_id == first_request_id {
            return None;
        }
    }
}

pub(crate) fn project_editor_auto_sleep_duration(
    mode: TitlebarMode,
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> Option<Duration> {
    let target = match mode {
        TitlebarMode::Source => shared_settings::SharedSettingsAutoSleepTarget::CodeEditor,
        TitlebarMode::Browser => shared_settings::SharedSettingsAutoSleepTarget::Browser,
        TitlebarMode::Kanban | TitlebarMode::Automate | TitlebarMode::Manage => {
            shared_settings::SharedSettingsAutoSleepTarget::ProjectEditor
        }
        TitlebarMode::Agents => return None,
    };
    settings.auto_sleep_duration(target)
}

pub(crate) fn native_chrome_settings_path() -> PathBuf {
    if let Ok(override_path) = env::var("ghostex_SETTINGS_PATH") {
        if !override_path.trim().is_empty() {
            return PathBuf::from(override_path);
        }
    }

    let Some(home) = env::var_os("HOME") else {
        return PathBuf::from("settings.json");
    };
    let app_support = PathBuf::from(home).join("Library/Application Support");
    let candidates = [
        app_support.join("com.madda.ghostex.host/state/settings.json"),
        app_support.join("dev.maddada.ghostex/dev/state/settings.json"),
        app_support.join("com.ghostex.host/state/settings.json"),
    ];
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

pub(crate) fn ghostex_state_root() -> PathBuf {
    shared_settings::ghostex_storage_paths().state_dir.clone()
}

pub(crate) fn gpui_workspace_shell_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-workspace-shell-state.json")
}

