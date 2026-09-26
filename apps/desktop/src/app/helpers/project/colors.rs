// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the workspace/terminal/command-pane
// color and theme helper functions. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::sync::{Arc, atomic::Ordering};

use gpui::{
    AnyElement, Hsla, Image, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    div, px, rgb, rgba,
};

use crate::app::helpers::*;
use crate::*;

static SHOW_ACTIVE_PANE_OUTLINE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static ACTIVE_PANE_OUTLINE_RGB: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3b82f6);

/// CDXC:Theming 2026-09-16 DECISION:
/// User: the stacked companion separator and resize drag-area backgrounds are #F3F4F6 in light mode, superseding the earlier #C9C9C9 choice.
pub(crate) const LIGHT_RESIZE_HANDLE_RGB: u32 = 0xf3f4f6;

pub(crate) fn show_active_pane_outline() -> bool {
    SHOW_ACTIVE_PANE_OUTLINE.load(Ordering::Relaxed)
}

pub(crate) fn gpui_project_icon_image_from_data_url(value: &str) -> Option<Arc<Image>> {
    /*
    CDXC:Titlebar 2026-07-04-03:00:
    The titlebar project icon is render-only and may come only from the explicit
    active-project `projectIconDataUrl` snapshot field. Decode the already
    bounded image data URL for the 16px titlebar slot without probing paths,
    fetching URLs, synthesizing initials, or persisting image bytes.
    */
    browser_favicon_image_from_data_url(value).map(|image| image.image)
}

pub(crate) fn tab_bar_button_hover_color() -> Hsla {
    chrome_color(0x222222, 0xe5e5e5).into()
}

pub(crate) fn workspace_background_color() -> Hsla {
    rgb(GPUI_WORKSPACE_BACKGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

pub(crate) fn source_view_background_color() -> Hsla {
    chrome_color(0x0e0e0e, 0xffffff).into()
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

pub(crate) fn refresh_gpui_visual_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) {
    let object = settings.object();
    CHROME_LIGHT_APPEARANCE.store(sidebar_uses_light_theme(object), Ordering::Relaxed);
    SHOW_ACTIVE_PANE_OUTLINE.store(
        object
            .get("showActivePaneOutline")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        Ordering::Relaxed,
    );
    ACTIVE_PANE_OUTLINE_RGB.store(
        gpui_settings_hex_rgb(object.get("workspaceActivePaneBorderColor")).unwrap_or(0x3b82f6),
        Ordering::Relaxed,
    );
    let terminal_settings = settings.gpui_terminal_engine_settings();
    let terminal_is_light = terminal_settings.uses_light_theme(gpui_system_uses_light_appearance());
    /*
    CDXC:Theming 2026-09-23 DECISION:
    User: the terminal background and the active tab follow the theme like the chat does, then
    "wtf does terminal color have to do with workarea theme??? pls make this more intuitive pls".
    The work area always paints the theme's content colour for the terminal's own appearance (the
    chrome in light mode, one step off it in dark), so the theme's tint and Background contrast
    always reach it, superseding the Ghostty config background and the light palette's background.
    The Terminal background setting no longer touches the work area: it follows the theme by
    default, and a chosen colour only replaces the colour behind terminal cells in dark mode
    (`GpuiTerminalConfig::apply_color_scheme`). Supersedes the 2026-09-22 rule that an explicit
    Terminal Background won for the whole dark work area.
    */
    let workspace = gpui_terminal_theme_background_rgb(object, terminal_is_light);
    GPUI_WORKSPACE_BACKGROUND_RGB.store(u64::from(workspace), Ordering::Relaxed);
    GPUI_TERMINAL_PADDING_BACKGROUND_RGB.store(
        if terminal_is_light {
            u64::from(workspace)
        } else {
            0
        },
        Ordering::Relaxed,
    );

    /*
    CDXC:Theming 2026-07-22:
    The saved `customSidebarTitlebarBackgroundColor` hex is a legacy migration
    seed only — since the contrast-slider redesign the sidebar resolves the
    effective chrome background from `customSidebarTitlebarBackgroundDarkness-
    Percent` plus the tint (getSidebarTitlebarBackgroundForDarkness in
    packages/shared/ghostex-settings.ts). Reading the stale saved hex here made the
    Rust titlebar derive its color (and gradient stops) from a darker base
    than the sidebar actually renders. Mirror the TS resolution instead.
    */
    let titlebar_background = resolved_custom_sidebar_titlebar_background(object);
    GPUI_MENU_BACKGROUND_RGB.store(
        sidebar_titlebar_menu_background_for_chrome(titlebar_background),
        Ordering::Relaxed,
    );
    let titlebar_foreground = if sidebar_uses_light_theme(object) {
        0x262626
    } else {
        gpui_settings_hex_rgb(object.get("customSidebarTitlebarForegroundColor"))
            .unwrap_or(0xffffff)
    };
    GPUI_TITLEBAR_BACKGROUND_RGB.store(u64::from(titlebar_background), Ordering::Relaxed);
    /*
    CDXC:Theming 2026-07-22:
    The shared sidebar renders custom chrome as a fixed-strength gradient
    derived from the resolved titlebar background
    (getSidebarTitlebarGradientColors in packages/shared/ghostex-settings.ts), and the
    titlebar shares those exact stops horizontally: left = the sidebar's top
    stop (darker), right = the sidebar's bottom stop. A flat Rust titlebar
    therefore never matched the gradient sidebar. Mirror the TS derivation
    here so the GPUI titlebar strip fades with the same colors; when custom
    chrome is disabled the stops collapse to the flat titlebar color.
    */
    let (gradient_left, gradient_right) = if sidebar_uses_light_theme(object) {
        (titlebar_background, titlebar_background)
    } else {
        sidebar_titlebar_gradient_stops(titlebar_background)
    };
    GPUI_TITLEBAR_GRADIENT_LEFT_RGB.store(u64::from(gradient_left), Ordering::Relaxed);
    GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.store(u64::from(gradient_right), Ordering::Relaxed);
    GPUI_TITLEBAR_FOREGROUND_RGB.store(u64::from(titlebar_foreground), Ordering::Relaxed);
    refresh_window_glass(object);
    crate::app::panel_motion::refresh_panel_motion_speed(object);
}

/// The theme's content colour for terminals of one appearance: the same rule the chat uses, so a
/// terminal, the chat next to it and the header over both sit on one surface.
pub(crate) fn gpui_terminal_theme_background_rgb(
    object: &serde_json::Map<String, serde_json::Value>,
    light: bool,
) -> u32 {
    work_area_background_for_variant(object, light)
}

/// The background a terminal engine paints for the current settings and system appearance.
pub(crate) fn gpui_terminal_theme_background(
    settings: &shared_settings::SharedGpuiTerminalEngineSettings,
) -> [u8; 3] {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let light = settings.uses_light_theme(gpui_system_uses_light_appearance());
    let rgb = gpui_terminal_theme_background_rgb(snapshot.object(), light);
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

pub(crate) fn workspace_tab_drag_preview_color() -> Hsla {
    chrome_color(0x242424, 0xf4f4f5).opacity(0.94).into()
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

/// CDXC:Theming 2026-09-13 WHY:
/// The terminal grid excludes its padding and width gutters, so the parent must paint the selected light terminal palette there too.
/// Cache this separately from app chrome so terminal overrides work, preserving the existing black padding in dark mode.
pub(crate) fn workspace_terminal_placeholder_color() -> Hsla {
    rgb(GPUI_TERMINAL_PADDING_BACKGROUND_RGB.load(Ordering::Relaxed) as u32).into()
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
    chrome_color(0x000000, 0xffffff).into()
}

pub(crate) fn terminal_search_bar_divider_color() -> Hsla {
    rgb(0x202020).into()
}

pub(crate) fn terminal_search_bar_background_color() -> Hsla {
    chrome_color(0x000000, 0xffffff).into()
}

pub(crate) fn terminal_search_bar_border_color() -> Hsla {
    chrome_color(0x252525, 0xd4d4d4).into()
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
    chrome_ink().opacity(239.0 / 255.0).into()
}

pub(crate) fn terminal_search_bar_count_color() -> Hsla {
    chrome_ink().opacity(184.0 / 255.0).into()
}

pub(crate) fn terminal_search_bar_button_color() -> Hsla {
    chrome_color(0xcfcfcf, 0x404040).into()
}

pub(crate) fn terminal_search_bar_button_background_color() -> Hsla {
    chrome_color(0x000000, 0xffffff).into()
}

pub(crate) fn terminal_search_bar_button_hover_color() -> Hsla {
    chrome_color(0x343434, 0xe5e5e5).into()
}

/// CDXC:Theming 2026-09-13 DECISION:
/// User: change the #090b0f background behind the terminals and Agents pane to #0d0d0d.
pub(crate) fn workspace_terminal_body_color(
    presentation_state: Option<TerminalSessionPresentationState>,
) -> Hsla {
    match presentation_state {
        // Under window glass the grid paints no default background either, so the pane shows the
        // frosted column all the way to its padding.
        Some(
            TerminalSessionPresentationState::Running
            | TerminalSessionPresentationState::Sleeping
            | TerminalSessionPresentationState::Mounting,
        ) if window_glass_active() => gpui::transparent_black(),
        Some(TerminalSessionPresentationState::Running) => workspace_terminal_placeholder_color(),
        Some(
            TerminalSessionPresentationState::Sleeping | TerminalSessionPresentationState::Mounting,
        ) => {
            if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
                rgb(0xffffff).into()
            } else {
                workspace_terminal_placeholder_color()
            }
        }
        Some(TerminalSessionPresentationState::StartupFailed) => {
            chrome_color(0x140908, 0xffffff).into()
        }
        Some(TerminalSessionPresentationState::RestoredUnmounted) => {
            chrome_color(0x08110d, 0xffffff).into()
        }
        Some(TerminalSessionPresentationState::PoppedOutPlaceholder) => {
            chrome_color(0x13090f, 0xffffff).into()
        }
        None => chrome_color(0x0d0d0d, 0xffffff).into(),
    }
}

pub(crate) fn workspace_terminal_placeholder_card_color(
    presentation_state: TerminalSessionPresentationState,
) -> Hsla {
    match presentation_state {
        TerminalSessionPresentationState::Running => chrome_color(0x000000, 0xffffff).into(),
        TerminalSessionPresentationState::Sleeping => chrome_color(0x101923, 0xf0f6fc).into(),
        TerminalSessionPresentationState::Mounting => chrome_color(0x1c160b, 0xfff9ed).into(),
        TerminalSessionPresentationState::StartupFailed => chrome_color(0x21100f, 0xfff2f1).into(),
        TerminalSessionPresentationState::RestoredUnmounted => {
            chrome_color(0x101b15, 0xf0f8f3).into()
        }
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            chrome_color(0x1d1118, 0xfcf1f6).into()
        }
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
        TerminalSessionPresentationState::Running => {
            chrome_color(0xffffff, 0x111111).opacity(0.82).into()
        }
        TerminalSessionPresentationState::Sleeping => {
            chrome_color(0xc9e6ff, 0x285b8c).opacity(0.96).into()
        }
        TerminalSessionPresentationState::Mounting => {
            chrome_color(0xffdf9a, 0x8a5c16).opacity(0.96).into()
        }
        TerminalSessionPresentationState::StartupFailed => {
            chrome_color(0xffc6c6, 0xb43d39).opacity(0.96).into()
        }
        TerminalSessionPresentationState::RestoredUnmounted => {
            chrome_color(0xc6f1d2, 0x287a48).opacity(0.94).into()
        }
        TerminalSessionPresentationState::PoppedOutPlaceholder => {
            chrome_color(0xffccdc, 0x9e3d65).opacity(0.96).into()
        }
    }
}

pub(crate) fn workspace_terminal_placeholder_title_color() -> Hsla {
    chrome_color(0xffffff, 0x111111).opacity(0.92).into()
}

pub(crate) fn workspace_terminal_placeholder_message_color() -> Hsla {
    chrome_color(0xe5e8ec, 0x111111).opacity(0.64).into()
}

pub(crate) fn workspace_terminal_placeholder_session_color() -> Hsla {
    chrome_color(0xe5e8ec, 0x111111).opacity(0.46).into()
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
    glass_divider(chrome_color(0x202020, 0xe5e5e5).into())
}

pub(crate) fn workspace_pane_focused_border_color() -> Hsla {
    rgb(ACTIVE_PANE_OUTLINE_RGB.load(Ordering::Relaxed)).into()
}

pub(crate) fn workspace_pane_attention_border_color() -> Hsla {
    rgb(0x95d7f6).into()
}

pub(crate) fn workspace_pane_border_color_for_state(state: WorkspacePaneBorderState) -> Hsla {
    match state {
        WorkspacePaneBorderState::Focused if show_active_pane_outline() => {
            workspace_pane_focused_border_color()
        }
        WorkspacePaneBorderState::Neutral | WorkspacePaneBorderState::Focused => {
            workspace_pane_border_color()
        }
        WorkspacePaneBorderState::Attention => workspace_pane_attention_border_color(),
    }
}

/// CDXC:Theming 2026-09-21 DECISION:
/// User: remove the border line around the Browser pane from its left and top; the view panel's own left border line, which runs from the top of the tab strip to the bottom of the panel, is the edge there, and nothing is drawn between the tab strip and the address bar.
/// This supersedes the 2026-09-14 decision that restored the left and top edges. The remaining right and bottom edges keep #d4d4d4 in light mode with dark mode unchanged, and still carry the focus/attention colours.
pub(crate) fn browser_pane_border_color_for_state(state: WorkspacePaneBorderState) -> Hsla {
    match state {
        WorkspacePaneBorderState::Neutral => glass_divider(chrome_color(0x202020, 0xd4d4d4).into()),
        WorkspacePaneBorderState::Focused if !show_active_pane_outline() => {
            glass_divider(chrome_color(0x202020, 0xd4d4d4).into())
        }
        _ => workspace_pane_border_color_for_state(state),
    }
}

pub(crate) fn project_editor_companion_border_color_for_state(
    state: WorkspacePaneBorderState,
) -> Hsla {
    match state {
        WorkspacePaneBorderState::Focused if show_active_pane_outline() => {
            workspace_pane_focused_border_color()
        }
        WorkspacePaneBorderState::Neutral | WorkspacePaneBorderState::Focused => {
            glass_divider(chrome_color(0x252525, 0xd4d4d4).into())
        }
        WorkspacePaneBorderState::Attention => workspace_pane_attention_border_color(),
    }
}

/// Rails rest in the neutral pane border colour so the two pane borders and the rail between them read as one line (see the Workarea decision on `SIDEBAR_DIVIDER_WIDTH`).
pub(crate) fn workspace_split_handle_color() -> Hsla {
    workspace_pane_border_color()
}

/// CDXC:Theming 2026-09-22 DECISION:
/// User: the top of the side panel (the view panel's tab strip and shell backing) follows the theme
/// and matches the Docs title bar, so it is the resolved chrome colour itself. This supersedes the
/// same-day step off the chrome (half as bright dark, 60% toward white light), which washed the
/// tint out in light mode. The command pane chrome and the terminal tab bar share it.
pub(crate) fn project_editor_shell_background_color() -> Hsla {
    titlebar_background()
}

pub(crate) fn project_editor_companion_divider_background_color() -> Hsla {
    project_editor_companion_border_color_for_state(WorkspacePaneBorderState::Neutral)
}

pub(crate) fn maximized_view_panel_top_line_color() -> Hsla {
    glass_divider(chrome_color(0x252525, 0xe5e5e5).into())
}

pub(crate) fn command_pane_chrome_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:19:
    Native command-panel chrome and command titlebars use an opaque black background. Keep GPUI command chrome on black instead of the generic dark titlebar gray so tabs, tab-add, and panel actions sit on the same base as macOS.

    CDXC:Theming 2026-09-22 DECISION:
    User: the command pane's tabs bar follows the theme too. It shares the side panel shell's step off
    the resolved chrome (half as bright on dark chrome, 60% toward white on light), superseding the fixed
    black / white above so a tinted preset carries its hue into the command chrome.
    */
    project_editor_shell_background_color()
}

pub(crate) fn command_pane_strip_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:19:
    The collapsed command strip is native command titlebar chrome with side margins, so its background stays black like expanded command titlebars.
    */
    command_pane_chrome_color()
}

pub(crate) fn command_pane_panel_separator_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:19:
    Native command-panel boundaries use the workspace separator line #1e1e1e for the panel edge, separate from focused pane outlines and titlebar command separators.
    */
    glass_divider(chrome_color(0x1e1e1e, 0xd4d4d4).into())
}

pub(crate) fn command_pane_border_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:19:
    Native inactive command terminal pane outlines use #111111, not the translucent command titlebar separator. Keep the inactive command group outline distinct from titlebar chrome.
    */
    glass_divider(chrome_color(0x111111, 0xe5e5e5).into())
}

pub(crate) fn command_pane_side_edge_color() -> Hsla {
    glass_divider(chrome_color(0x252525, 0xd4d4d4).into())
}

pub(crate) fn command_pane_hidden_border_color() -> Hsla {
    /*
    CDXC:FocusRouting 2026-06-25-18:02:
    Pinned native command panels set inactive command borders to nil. Use a transparent GPUI border color instead of removing the border frame so split command groups keep stable layout while matching the hidden inactive outline.
    */
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn command_pane_focused_border_color() -> Hsla {
    workspace_pane_focused_border_color()
}

pub(crate) fn command_pane_tab_background_color(is_active: bool, is_sleeping: bool) -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-14:36:
    Match macOS `compositedWorkspaceTabColor` for command-role tabs instead of using generic GPUI dark fills. The channel math keeps the active and inactive tab backgrounds tied to the native AppKit source values.

    CDXC:SessionSleep 2026-06-25-14:39:
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
    CDXC:CommandPane 2026-06-25-14:36:
    Native command tabs do not brighten the tab fill on hover; hover state only affects the drawn trailing status/close affordance.
    */
    command_pane_tab_background_color(is_active, is_sleeping)
}

pub(crate) fn command_pane_native_composited_tab_color(overlay_alpha: f32) -> Hsla {
    let light = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
    /*
    CDXC:Theming 2026-09-23 DECISION:
    User: the command pane's active tab and its + button must "fit better with the bar behind them". Under window glass the bar paints nothing, so a tab is only its overlay (a wash of the ink) and the icon buttons are clear with a wash on hover, instead of opaque fills baked over the chrome colour.
    */
    if window_glass_active() {
        return rgb(if light { 0x000000 } else { 0xffffff })
            .opacity(overlay_alpha)
            .into();
    }
    // The tab overlays composite over the themed command chrome (see command_pane_chrome_color)
    // rather than the fixed AppKit base, so the tabs carry the same tint as their bar.
    let base: gpui::Rgba = project_editor_shell_background_color().into();
    let channel = |value: f32| -> f32 {
        let value = value * 255.0;
        if light {
            value * (1.0 - overlay_alpha)
        } else {
            value + (255.0 - value) * overlay_alpha
        }
    };
    rgb(sidebar_titlebar_pack_rgb([
        channel(base.r),
        channel(base.g),
        channel(base.b),
    ]))
    .into()
}

pub(crate) fn command_pane_tab_title_text_color(is_active: bool, is_sleeping: bool) -> Hsla {
    /*
    CDXC:SessionSleep 2026-06-25-14:39:
    Command-role tab titles use selected-label white for both active and inactive tabs, but inactive sleeping tabs multiply title alpha by the native 0.48 parked-session treatment. Active sleeping tabs keep full selected label opacity.
    */
    let sleep_alpha_multiplier = if is_sleeping && !is_active {
        COMMAND_PANE_TAB_TITLE_SLEEPING_INACTIVE_ALPHA_MULTIPLIER
    } else {
        1.0
    };
    chrome_color(0xf5f5f5, 0x262626)
        .opacity(0.98 * sleep_alpha_multiplier)
        .into()
}

pub(crate) fn command_pane_tab_separator_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-14:17:
    macOS command tab separators use calibrated white at 10% alpha, separate from the heavier command-pane structural border color.
    */
    chrome_color(0xffffff, 0x000000).opacity(0.10).into()
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

pub(crate) fn command_pane_tab_status_indicator_color(
    tab_status: CommandTerminalTabStatus,
) -> Hsla {
    let color = rgb(command_terminal_tab_status_color(tab_status));
    color
        .opacity(command_terminal_tab_status_indicator_opacity(tab_status))
        .into()
}

pub(crate) fn command_terminal_tab_status_has_indicator(
    tab_status: CommandTerminalTabStatus,
) -> bool {
    !matches!(tab_status, CommandTerminalTabStatus::Idle)
}

pub(crate) fn command_terminal_tab_status_indicator_visible(
    tab_status: CommandTerminalTabStatus,
    tab_hovered: bool,
) -> bool {
    /*
    CDXC:SessionStatus 2026-06-25-13:18:
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

pub(crate) fn command_terminal_tab_status_indicator_opacity(
    tab_status: CommandTerminalTabStatus,
) -> f32 {
    match tab_status {
        CommandTerminalTabStatus::DelayedSend => 0.96,
        CommandTerminalTabStatus::Idle
        | CommandTerminalTabStatus::Working
        | CommandTerminalTabStatus::Attention => 1.0,
    }
}

/// CDXC:CommandPane 2026-09-23 DECISION:
/// User: the command pane's panel buttons (Keep open, Minimize) "need to match the look of the + button in the command pane tabs bar". Their cluster takes the + button's own fill, so under window glass it is clear like the + button instead of a solid block behind them.
pub(crate) fn command_pane_control_cluster_color() -> Hsla {
    command_pane_control_button_color()
}

pub(crate) fn command_pane_control_button_color() -> Hsla {
    glass_clear(chrome_color(0x0e0e0e, 0xfafafa).into())
}

pub(crate) fn command_pane_control_text_color() -> Hsla {
    chrome_color(0xcfcfcf, 0x404040).into()
}

pub(crate) fn command_pane_control_hover_color() -> Hsla {
    if window_glass_active() {
        return chrome_ink().opacity(0.08).into();
    }
    tab_bar_button_hover_color()
}

pub(crate) fn command_pane_sticky_active_tab_button_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:34:
    Native sticky active-tab navigation shares the command tab-bar icon-button background with Pin, Minimize, and inline New Terminal.
    */
    command_pane_control_button_color()
}

pub(crate) fn command_pane_sticky_active_tab_icon_color() -> Hsla {
    command_pane_control_text_color()
}

pub(crate) fn command_pane_sticky_active_tab_border_color() -> Hsla {
    chrome_color(0x2a2a2a, 0xd4d4d4).into()
}

pub(crate) fn command_pane_split_handle_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-09-19 WHY:
    Inactive command pane borders are hidden, so the 2px split rail is the one visible line between command panes and rests in the panel separator colour. This replaces the transparent five-pixel rails that relied on pane borders for separation.
    */
    command_pane_panel_separator_color()
}

pub(crate) fn command_terminal_placeholder_color() -> Hsla {
    glass_clear(workspace_terminal_placeholder_color())
}

pub(crate) fn command_pane_delayed_send_badge_background_color() -> Hsla {
    rgb(0x0d0d0d).opacity(0.78).into()
}

pub(crate) fn command_pane_delayed_send_badge_border_color() -> Hsla {
    chrome_color(0xffffff, 0x000000).opacity(0.12).into()
}

pub(crate) fn command_pane_delayed_send_badge_text_color() -> Hsla {
    rgb(0xf6c945).into()
}

pub(crate) fn gpui_combined_presentation_session_id(project_id: &str, session_id: &str) -> String {
    format!(
        "combined-session:{}:{}",
        gpui_encode_uri_component(project_id),
        gpui_encode_uri_component(session_id)
    )
}

pub(crate) fn gpui_combined_presentation_session_key(
    value: &str,
) -> Option<GpuiLocalWorkspaceSessionKey> {
    let payload = value.strip_prefix("combined-session:")?;
    let (project_id, session_id) = payload.split_once(':')?;
    let project_id = gpui_percent_decoded_id_part(project_id)?;
    let session_id = gpui_percent_decoded_id_part(session_id)?;
    Some(GpuiLocalWorkspaceSessionKey {
        project_id,
        session_id,
    })
}
