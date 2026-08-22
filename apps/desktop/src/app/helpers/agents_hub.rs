// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::{Context as _, Result};
use futures::StreamExt as _;
use gpui::{
    Action, AnyElement, AppContext as _, Bounds, Element, FontWeight, Hsla, Image, ImageFormat, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div, prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::{
    Selectable,
    v_flex,
};

use crate::app::helpers::*;
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
        "claude" => Some("agent-icons/claude.svg"),
        "codebuddy" => Some("agent-icons/codebuddy.svg"),
        "cursor-cli" => Some("agent-icons/cursor-cli.svg"),
        "codex" => Some("agent-icons/codex.svg"),
        "copilot" => Some("agent-icons/copilot.svg"),
        "factory-droid" => Some("agent-icons/factory-droid.svg"),
        "gemini" => Some("agent-icons/gemini.svg"),
        "grok-build" => Some("agent-icons/grok-build.svg"),
        "hermes-agent" => Some("agent-icons/hermes-agent.svg"),
        "kiro" => Some("agent-icons/kiro.svg"),
        "omp" => Some("agent-icons/omp.svg"),
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
        "claude" => 0xd97757,
        "codebuddy" => 0x72d6ff,
        "cursor-cli" => 0xedecec,
        "codex" => 0xffffff,
        "copilot" => 0xffffff,
        "factory-droid" => 0xff7a1a,
        "gemini" => 0x8b9aff,
        "grok-build" => 0xffffff,
        "hermes-agent" => 0xf3c46b,
        "kiro" => 0xa6e3ff,
        "omp" => 0xc8ff62,
        "opencode" => 0x6d96c0,
        "pi" => 0xc8ff62,
        "qoder" => 0xa991ff,
        "rovo-dev" => 0x4fc3a1,
        _ => 0xffffff,
    }
}

pub(crate) fn workspace_tab_agent_icon_opacity(visual_tone: WorkspaceTabLifecycleVisualTone) -> f32 {
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

pub(crate) fn agent_terminal_tab_status_inactive_opacity(tab_status: AgentTerminalTabStatus) -> f32 {
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
/// CDXC:GPUISessionTitleOverlay 2026-07-26:
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

pub(crate) fn gpui_status_pet_surface_color() -> Hsla {
    rgb(0x111315).opacity(0.94).into()
}

pub(crate) fn gpui_status_pet_surface_border_color() -> Hsla {
    rgb(0xffffff).opacity(0.16).into()
}

pub(crate) fn gpui_status_pet_surface_text_color() -> Hsla {
    rgb(0xf4f6f8).opacity(0.92).into()
}

pub(crate) fn gpui_status_pet_control_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).opacity(0.18).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).opacity(0.18).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).opacity(0.16).into(),
    }
}

pub(crate) fn gpui_status_pet_control_hover_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).opacity(0.26).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).opacity(0.26).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).opacity(0.24).into(),
    }
}

pub(crate) fn gpui_status_pet_status_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x95d7f6).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0xf59e0b).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x75d69a).into(),
    }
}

pub(crate) fn gpui_pet_overlay_activity_hover_color(status: GpuiStatusIndicatorStatus) -> Hsla {
    match status {
        GpuiStatusIndicatorStatus::Attention => rgb(0x15242c).opacity(0.96).into(),
        GpuiStatusIndicatorStatus::Working => rgb(0x2a2112).opacity(0.96).into(),
        GpuiStatusIndicatorStatus::Available => rgb(0x14251a).opacity(0.96).into(),
    }
}

pub(crate) fn gpui_pet_overlay_secondary_text_color() -> Hsla {
    rgb(0xd7dde3).opacity(0.68).into()
}

pub(crate) fn gpui_pet_overlay_label_background_color() -> Hsla {
    rgb(0x0b0d0f).opacity(0.72).into()
}

pub(crate) const fn gpui_pet_overlay_row_frames<const N: usize>(
    row_index: u8,
    frame_duration_ms: u64,
    final_frame_duration_ms: u64,
) -> [GpuiPetOverlayAnimationFrame; N] {
    let mut frames = [GpuiPetOverlayAnimationFrame {
        row_index,
        column_index: 0,
        duration_ms: frame_duration_ms,
    }; N];
    let mut index = 0;
    while index < N {
        frames[index] = GpuiPetOverlayAnimationFrame {
            row_index,
            column_index: index as u8,
            duration_ms: if index + 1 == N {
                final_frame_duration_ms
            } else {
                frame_duration_ms
            },
        };
        index += 1;
    }
    frames
}

pub(crate) fn gpui_pet_overlay_animation_state_for_surface(
    activities: &[GpuiPetOverlayActivityState],
    avatar_hovered: bool,
) -> GpuiPetOverlayAnimationState {
    if activities
        .iter()
        .any(|activity| activity.state == GpuiStatusIndicatorStatus::Attention)
    {
        return GpuiPetOverlayAnimationState::Review;
    }
    if activities
        .iter()
        .any(|activity| activity.state == GpuiStatusIndicatorStatus::Working)
    {
        return GpuiPetOverlayAnimationState::Running;
    }
    if avatar_hovered {
        return GpuiPetOverlayAnimationState::Jumping;
    }
    GpuiPetOverlayAnimationState::Idle
}

pub(crate) fn gpui_pet_overlay_animation_frame(
    state: GpuiPetOverlayAnimationState,
    started_at: Instant,
    now: Instant,
) -> GpuiPetOverlayAnimationFrame {
    let elapsed_ms = now
        .checked_duration_since(started_at)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    match state {
        GpuiPetOverlayAnimationState::Idle => {
            gpui_pet_overlay_loop_frame(&GPUI_PET_OVERLAY_IDLE_FRAMES, elapsed_ms, true)
        }
        GpuiPetOverlayAnimationState::Jumping => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_JUMPING_FRAMES, elapsed_ms)
        }
        GpuiPetOverlayAnimationState::Review => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_REVIEW_FRAMES, elapsed_ms)
        }
        GpuiPetOverlayAnimationState::Running => {
            gpui_pet_overlay_active_then_idle_frame(&GPUI_PET_OVERLAY_RUNNING_FRAMES, elapsed_ms)
        }
    }
}

pub(crate) fn gpui_pet_overlay_animation_frame_for_motion_preference(
    state: GpuiPetOverlayAnimationState,
    started_at: Instant,
    now: Instant,
    reduce_motion_enabled: bool,
) -> GpuiPetOverlayAnimationFrame {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    Reduce Motion must freeze the pet on a deterministic frame for the current semantic state while preserving the existing animated frame cadence when the OS setting is off. Keep this pure so ticker gating and frame choice can be verified without launching GPUI.
    */
    if reduce_motion_enabled {
        return gpui_pet_overlay_reduced_motion_frame(state);
    }
    gpui_pet_overlay_animation_frame(state, started_at, now)
}

pub(crate) fn gpui_pet_overlay_reduced_motion_frame(
    state: GpuiPetOverlayAnimationState,
) -> GpuiPetOverlayAnimationFrame {
    match state {
        GpuiPetOverlayAnimationState::Idle => GPUI_PET_OVERLAY_IDLE_FRAMES[0],
        GpuiPetOverlayAnimationState::Jumping => GPUI_PET_OVERLAY_JUMPING_FRAMES[0],
        GpuiPetOverlayAnimationState::Review => GPUI_PET_OVERLAY_REVIEW_FRAMES[0],
        GpuiPetOverlayAnimationState::Running => GPUI_PET_OVERLAY_RUNNING_FRAMES[0],
    }
}

pub(crate) fn gpui_pet_overlay_animation_ticker_should_run(
    pet_enabled: bool,
    reduce_motion_enabled: bool,
) -> bool {
    pet_enabled && !reduce_motion_enabled
}

pub(crate) fn gpui_pet_overlay_active_then_idle_frame(
    frames: &[GpuiPetOverlayAnimationFrame],
    elapsed_ms: u64,
) -> GpuiPetOverlayAnimationFrame {
    let active_duration = gpui_pet_overlay_animation_duration_ms(frames, false) * 3;
    if elapsed_ms < active_duration {
        return gpui_pet_overlay_loop_frame(frames, elapsed_ms, false);
    }
    gpui_pet_overlay_loop_frame(
        &GPUI_PET_OVERLAY_IDLE_FRAMES,
        elapsed_ms - active_duration,
        true,
    )
}

pub(crate) fn gpui_pet_overlay_loop_frame(
    frames: &[GpuiPetOverlayAnimationFrame],
    elapsed_ms: u64,
    slow_idle: bool,
) -> GpuiPetOverlayAnimationFrame {
    let Some(first_frame) = frames.first().copied() else {
        return GPUI_PET_OVERLAY_IDLE_FRAMES[0];
    };
    let duration = gpui_pet_overlay_animation_duration_ms(frames, slow_idle);
    if duration == 0 {
        return first_frame;
    }
    let mut remaining_ms = elapsed_ms % duration;
    for frame in frames {
        let frame_duration = gpui_pet_overlay_frame_duration_ms(*frame, slow_idle);
        if remaining_ms < frame_duration {
            return *frame;
        }
        remaining_ms = remaining_ms.saturating_sub(frame_duration);
    }
    first_frame
}

pub(crate) fn gpui_pet_overlay_animation_duration_ms(
    frames: &[GpuiPetOverlayAnimationFrame],
    slow_idle: bool,
) -> u64 {
    frames
        .iter()
        .map(|frame| gpui_pet_overlay_frame_duration_ms(*frame, slow_idle))
        .sum()
}

pub(crate) fn gpui_pet_overlay_frame_duration_ms(frame: GpuiPetOverlayAnimationFrame, slow_idle: bool) -> u64 {
    if slow_idle {
        frame
            .duration_ms
            .saturating_mul(GPUI_PET_OVERLAY_IDLE_SPEED_MULTIPLIER)
    } else {
        frame.duration_ms
    }
}

pub(crate) fn gpui_pet_overlay_pet_id_known(pet_id: &str) -> bool {
    gpui_pet_overlay_pet_display_name(pet_id).is_some()
}

pub(crate) fn gpui_pet_overlay_pet_display_name(pet_id: &str) -> Option<&'static str> {
    match pet_id {
        "boo" => Some("Boo"),
        "bsod" => Some("BSOD"),
        "codex" => Some("Codex"),
        "dewey" => Some("Dewey"),
        "fireball" => Some("Fireball"),
        "null-signal" => Some("Null Signal"),
        "rocky" => Some("Rocky"),
        "seedy" => Some("Seedy"),
        "stacky" => Some("Stacky"),
        _ => None,
    }
}

pub(crate) fn gpui_pet_overlay_spritesheet_image(pet_id: &str) -> Option<Arc<Image>> {
    match pet_id {
        "boo" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_BOO_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/boo-spritesheet-codexpethub-8a8161fb.webp"),
        )),
        "bsod" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_BSOD_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/bsod-spritesheet-v4-BRrRVy1T.webp"),
        )),
        "codex" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_CODEX_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/codex-spritesheet-v4-Bl6P89d_.webp"),
        )),
        "dewey" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_DEWEY_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/dewey-spritesheet-v4-gAYk_M9g.webp"),
        )),
        "fireball" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_FIREBALL_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/fireball-spritesheet-v4-BtU8R9Qp.webp"),
        )),
        "null-signal" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_NULL_SIGNAL_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/null-signal-spritesheet-v4-CCoTR-8t.webp"),
        )),
        "rocky" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_ROCKY_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/rocky-spritesheet-v4-3RlTi26B.webp"),
        )),
        "seedy" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_SEEDY_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/seedy-spritesheet-v4-CdlE_fn9.webp"),
        )),
        "stacky" => Some(gpui_pet_overlay_cached_spritesheet(
            &GPUI_PET_OVERLAY_STACKY_IMAGE,
            include_bytes!("../../../../../packages/core-ui/assets/pets/stacky-spritesheet-v4-CaUJd4fY.webp"),
        )),
        _ => None,
    }
}

pub(crate) fn gpui_pet_overlay_cached_spritesheet(
    cache: &'static OnceLock<Arc<Image>>,
    bytes: &'static [u8],
) -> Arc<Image> {
    cache
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Webp, bytes.to_vec())))
        .clone()
}

pub(crate) fn gpui_bundled_agent_skill_name(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "browserUse" => Some("ghostex-browser-use"),
        "computerUse" => Some("ghostex-computer-use"),
        "embeddedBrowserUse" => Some("ghostex-embedded-browser-use"),
        "agentOrchestration" => Some("ghostex-agent-orchestration"),
        "fable56Orchestration" => Some("ghostex-fable-5.6-orchestration"),
        "findPrevSession" => Some("ghostex-find-prev-session"),
        "generateTitle" => Some("ghostex-auto-rename-session"),
        "moveCodexSession" => Some("ghostex-move-codex-session"),
        _ => None,
    }
}

pub(crate) const MANAGE_IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".ghostex",
    ".gradle",
    ".next",
    ".nuxt",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".tox",
    ".venv",
    ".vite",
    "DerivedData",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "storybook-static",
    "target",
    "tmp",
    "venv",
    "zig-out",
];

pub(crate) fn gpui_cef_html_entry_url(env_var: &str, entry_file_name: &str) -> Result<String> {
    if let Ok(value) = env::var(env_var) {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    let executable = env::current_exe().context("failed to resolve current executable")?;
    if let Some(bundle_root) = find_app_bundle_root(&executable) {
        let bundled = bundle_root
            .join("Contents/Resources/sidebar")
            .join(entry_file_name);
        if bundled.exists() {
            return Ok(file_url(&bundled));
        }
    }

    /*
    CDXC:GPUIWindowsAppModalBundle 2026-08-04:
    Packaged Windows and Linux builds stage every first-party CEF entry in
    dist/sidebar beside the executable, just like sidebar_url's packaged
    lookup. Resolve that directory before the compile-time checkout path so an
    installed Ghostex never loads modal-host, titlebar-host, Kanban, Manage, or
    Chat artifacts from the source tree that happened to build the binary.
    */
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Some(exe_dir) = executable.parent() {
        let packaged = exe_dir.join("dist/sidebar").join(entry_file_name);
        if packaged.exists() {
            return Ok(file_url(&packaged));
        }
    }

    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dist/sidebar")
        .join(entry_file_name);
    if local.exists() {
        return Ok(file_url(&local));
    }

    anyhow::bail!("GPUI CEF workarea bundle entry was not found")
}

/// The one-page document the first-launch tutorial player iframe points at.
/// Served from the app's synthetic https origin so YouTube's embed player has
/// a real embedding origin (CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19).
pub(crate) fn gpui_tutorial_video_player_document() -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}\
iframe{{border:0;display:block;height:100%;width:100%}}</style></head><body>\
<iframe src=\"{GHOSTEX_TUTORIAL_VIDEO_EMBED_URL}\" title=\"Ghostex introduction\" \
allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share\" \
allowfullscreen></iframe></body></html>"
    )
    .into_bytes()
}

pub(crate) fn gpui_app_modal_host_resource_scope() -> cef::ManageDocsResourceScope {
    cef::ManageDocsResourceScope::new_remote(Arc::new(|relative_path: &str| {
        (relative_path == GPUI_TUTORIAL_VIDEO_PLAYER_RESOURCE_PATH)
            .then(gpui_tutorial_video_player_document)
    }))
}

pub(crate) fn app_modal_host_url() -> Result<String> {
    gpui_cef_html_entry_url("GHOSTEX_GPUI_APP_MODAL_HOST_URL", "modal-host.html")
        .context("failed to resolve GPUI app-modal host bundle URL")
}

/// Where a command tab dropped into the Agents workspace should land.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandToAgentsDropPlacement {
    PaneBody(WorkspaceDropZone),
    TabStrip(usize),
}

pub(crate) fn apply_gpui_terminal_runtime_action_events(
    osc_states: &mut HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    events: Vec<terminal_ghostty_surface::GhosttyRuntimeActionEvent>,
) -> bool {
    use terminal_ghostty_surface::GhosttyRuntimeActionEvent;

    let mut runtime_state_changed = false;
    for event in events {
        match event {
            GhosttyRuntimeActionEvent::OpenUrl { .. } => {}
            GhosttyRuntimeActionEvent::RingBell => {
                let state = osc_states.entry(runtime_session_id).or_default();
                state.bell_count = state.bell_count.wrapping_add(1);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::SetTitle { title } => {
                if title == TEMP_REMOTE_LOCAL_READY_TITLE || title == TEMP_REMOTE_SSH_READY_TITLE {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        if title == TEMP_REMOTE_LOCAL_READY_TITLE {
                            "TEMP.remoteNewTerminal.localWrapperReady"
                        } else {
                            "TEMP.remoteNewTerminal.remoteCommandReady"
                        },
                        serde_json::json!({ "engine": "ghostty" }),
                    );
                }
                osc_states.entry(runtime_session_id).or_default().title = Some(title);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::Pwd { pwd } => {
                osc_states.entry(runtime_session_id).or_default().pwd = Some(pwd);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::MouseOverLink { url } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if state.hovered_link_url != url {
                    state.hovered_link_url = url;
                    runtime_state_changed = true;
                }
            }
            GhosttyRuntimeActionEvent::StartSearch { needle } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                match (&mut state.search, needle) {
                    (Some(search), Some(needle)) => search.needle = needle,
                    (Some(_), None) => {}
                    (search @ None, needle) => {
                        *search = Some(GpuiTerminalSearchState {
                            needle: needle.unwrap_or_default(),
                            ..GpuiTerminalSearchState::default()
                        });
                    }
                }
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::EndSearch => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if state.search.take().is_some() {
                    runtime_state_changed = true;
                }
            }
            GhosttyRuntimeActionEvent::SearchTotal { total } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if let Some(search) = &mut state.search {
                    if search.total != total {
                        search.total = total;
                        runtime_state_changed = true;
                    }
                }
            }
            GhosttyRuntimeActionEvent::SearchSelected { selected } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if let Some(search) = &mut state.search {
                    if search.selected != selected {
                        search.selected = selected;
                        runtime_state_changed = true;
                    }
                }
            }
        }
    }
    runtime_state_changed
}

pub(crate) fn gpui_open_path(path: &Path) -> Result<(), String> {
    gpui_spawn_os_open(path.as_os_str())
}

pub(crate) const GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX: &str = "custom:";

pub(crate) struct GpuiBuiltInOpenTargetDefinition {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) commands: &'static [&'static str],
    pub(crate) base_args: &'static [&'static str],
    // Detection probe names mirroring macOSAppNames in
    // packages/shared/workspace-open-targets.ts; keep both catalogs in sync.
    pub(crate) macos_app_names: &'static [&'static str],
}

pub(crate) const GPUI_BUILT_IN_OPEN_TARGETS: &[GpuiBuiltInOpenTargetDefinition] = &[
    GpuiBuiltInOpenTargetDefinition {
        id: "cursor",
        label: "Cursor",
        commands: &["cursor"],
        base_args: &[],
        macos_app_names: &["Cursor"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "trae",
        label: "Trae",
        commands: &["trae"],
        base_args: &[],
        macos_app_names: &["Trae"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "kiro",
        label: "Kiro",
        commands: &["kiro"],
        base_args: &["ide"],
        macos_app_names: &["Kiro"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscode",
        label: "VS Code",
        commands: &["code"],
        base_args: &[],
        macos_app_names: &["Visual Studio Code"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscode-insiders",
        label: "VS Code Insiders",
        commands: &["code-insiders"],
        base_args: &[],
        macos_app_names: &["Visual Studio Code - Insiders"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "vscodium",
        label: "VSCodium",
        commands: &["codium"],
        base_args: &[],
        macos_app_names: &["VSCodium"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "zed",
        label: "Zed",
        commands: &["zed", "zeditor"],
        base_args: &[],
        macos_app_names: &["Zed", "Zed Preview"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "antigravity",
        label: "Antigravity",
        commands: &["agy-ide"],
        base_args: &[],
        macos_app_names: &["Antigravity"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "idea",
        label: "IntelliJ IDEA",
        commands: &["idea"],
        base_args: &[],
        macos_app_names: &["IntelliJ IDEA"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "aqua",
        label: "Aqua",
        commands: &["aqua"],
        base_args: &[],
        macos_app_names: &["Aqua"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "clion",
        label: "CLion",
        commands: &["clion"],
        base_args: &[],
        macos_app_names: &["CLion"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "datagrip",
        label: "DataGrip",
        commands: &["datagrip"],
        base_args: &[],
        macos_app_names: &["DataGrip"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "dataspell",
        label: "DataSpell",
        commands: &["dataspell"],
        base_args: &[],
        macos_app_names: &["DataSpell"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "goland",
        label: "GoLand",
        commands: &["goland"],
        base_args: &[],
        macos_app_names: &["GoLand"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "phpstorm",
        label: "PhpStorm",
        commands: &["phpstorm"],
        base_args: &[],
        macos_app_names: &["PhpStorm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "pycharm",
        label: "PyCharm",
        commands: &["pycharm"],
        base_args: &[],
        macos_app_names: &["PyCharm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rider",
        label: "Rider",
        commands: &["rider"],
        base_args: &[],
        macos_app_names: &["Rider"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rubymine",
        label: "RubyMine",
        commands: &["rubymine"],
        base_args: &[],
        macos_app_names: &["RubyMine"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "rustrover",
        label: "RustRover",
        commands: &["rustrover"],
        base_args: &[],
        macos_app_names: &["RustRover"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "webstorm",
        label: "WebStorm",
        commands: &["webstorm"],
        base_args: &[],
        macos_app_names: &["WebStorm"],
    },
    GpuiBuiltInOpenTargetDefinition {
        id: "finder",
        label: "Open Folder",
        commands: &[],
        base_args: &[],
        macos_app_names: &[],
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiOpenTarget {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) launch: GpuiOpenTargetLaunch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiOpenTargetLaunch {
    Finder,
    BuiltIn {
        default_command: Option<&'static str>,
        base_args: &'static [&'static str],
        resolved_command: Option<String>,
        resolved_app_name: Option<String>,
    },
    Custom {
        command: String,
        args: Vec<String>,
    },
}

pub(crate) struct GpuiOpenTargetAvailability {
    pub(crate) available_ids: HashSet<String>,
    pub(crate) resolved_commands: HashMap<String, String>,
    pub(crate) resolved_app_names: HashMap<String, String>,
}

pub(crate) fn gpui_visible_open_targets_from_current_settings() -> Vec<GpuiOpenTarget> {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_visible_open_targets_from_settings(settings.object())
}

pub(crate) fn gpui_visible_open_targets_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<GpuiOpenTarget> {
    /*
    CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
    GPUI titlebar Open In consumes the same shared Settings fields as React: hidden built-in ids, availability resolved ids/commands/app names, and normalized custom targets. Finder/Open Folder remains always available unless hidden, custom targets follow built-ins, and no project path, command text, URL, stdout/stderr, or user content is logged or persisted here.
    */
    let hidden_ids = gpui_open_target_hidden_ids(settings.get("workspaceOpenTargetHiddenIds"));
    let availability =
        gpui_open_target_availability(settings.get("workspaceOpenTargetAvailability"));
    let mut targets = Vec::new();
    for definition in GPUI_BUILT_IN_OPEN_TARGETS {
        if hidden_ids.contains(definition.id) {
            continue;
        }
        if definition.id != "finder" && !availability.available_ids.contains(definition.id) {
            continue;
        }
        targets.push(GpuiOpenTarget {
            id: definition.id.to_string(),
            label: definition.label.to_string(),
            launch: if definition.id == "finder" {
                GpuiOpenTargetLaunch::Finder
            } else {
                GpuiOpenTargetLaunch::BuiltIn {
                    default_command: definition.commands.first().copied(),
                    base_args: definition.base_args,
                    resolved_command: availability.resolved_commands.get(definition.id).cloned(),
                    resolved_app_name: availability.resolved_app_names.get(definition.id).cloned(),
                }
            },
        });
    }
    targets.extend(gpui_custom_open_targets(
        settings.get("customWorkspaceOpenTargets"),
    ));
    targets
}

pub(crate) fn gpui_open_target_hidden_ids(candidate: Option<&serde_json::Value>) -> HashSet<String> {
    let built_in_ids = gpui_built_in_open_target_ids();
    candidate
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| built_in_ids.contains(*id))
        .map(str::to_string)
        .collect()
}

pub(crate) fn gpui_open_target_availability(
    candidate: Option<&serde_json::Value>,
) -> GpuiOpenTargetAvailability {
    let built_in_ids = gpui_built_in_open_target_ids();
    let object = candidate.and_then(serde_json::Value::as_object);
    let mut available_ids = HashSet::from(["finder".to_string()]);
    if let Some(ids) = object
        .and_then(|object| object.get("availableTargetIds"))
        .and_then(serde_json::Value::as_array)
    {
        for id in ids
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| built_in_ids.contains(*id))
        {
            available_ids.insert(id.to_string());
        }
    }
    let resolved_commands = gpui_open_target_resolution_map(
        object.and_then(|object| object.get("resolvedCommands")),
        &available_ids,
    );
    let resolved_app_names = gpui_open_target_resolution_map(
        object.and_then(|object| object.get("resolvedAppNames")),
        &available_ids,
    );
    GpuiOpenTargetAvailability {
        available_ids,
        resolved_commands,
        resolved_app_names,
    }
}

pub(crate) fn gpui_open_target_resolution_map(
    candidate: Option<&serde_json::Value>,
    available_ids: &HashSet<String>,
) -> HashMap<String, String> {
    candidate
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(target_id, value)| {
            let value = value.as_str()?.trim();
            (available_ids.contains(target_id.as_str()) && !value.is_empty())
                .then(|| (target_id.clone(), value.to_string()))
        })
        .collect()
}

pub(crate) fn gpui_custom_open_targets(candidate: Option<&serde_json::Value>) -> Vec<GpuiOpenTarget> {
    let mut seen_ids = HashSet::new();
    let mut targets = Vec::new();
    let Some(entries) = candidate.and_then(serde_json::Value::as_array) else {
        return targets;
    };
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let label = object
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let command = object
            .get("command")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if label.is_empty() || command.is_empty() {
            continue;
        }
        let requested_id = object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let base_id = if requested_id.starts_with(GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX) {
            requested_id.to_string()
        } else {
            format!(
                "{}{}",
                GPUI_CUSTOM_OPEN_TARGET_ID_PREFIX,
                gpui_open_target_slug(label)
            )
        };
        let mut id = base_id.clone();
        for suffix in 2.. {
            if !seen_ids.contains(&id) {
                break;
            }
            id = format!("{base_id}-{suffix}");
        }
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let args = object
            .get("args")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        targets.push(GpuiOpenTarget {
            id,
            label: label.to_string(),
            launch: GpuiOpenTargetLaunch::Custom {
                command: command.to_string(),
                args,
            },
        });
    }
    targets
}

pub(crate) fn gpui_built_in_open_target_ids() -> HashSet<&'static str> {
    GPUI_BUILT_IN_OPEN_TARGETS
        .iter()
        .map(|target| target.id)
        .collect()
}

pub(crate) fn gpui_open_target_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in label.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "target".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) fn gpui_launch_open_target(target: &GpuiOpenTarget, project_path: &Path) -> Result<(), String> {
    /*
    CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
    Open In launch is bounded native process behavior: Finder/Open Folder uses the fixed OS opener, command targets use `/usr/bin/env` argv without shell splitting, macOS app-name launches use `/usr/bin/open -a`, child stdio is suppressed, and user paths/commands/errors are not copied into notifications or logs.
    */
    match &target.launch {
        GpuiOpenTargetLaunch::Finder => gpui_open_path(project_path)
            .map_err(|_| "Could not open the active project folder.".to_string()),
        GpuiOpenTargetLaunch::BuiltIn {
            default_command,
            base_args,
            resolved_command,
            resolved_app_name,
        } => {
            if let Some(command) = resolved_command
                .as_deref()
                .filter(|command| !command.trim().is_empty())
            {
                return gpui_spawn_open_target_command(command, *base_args, &[], project_path);
            }
            if let Some(app_name) = resolved_app_name
                .as_deref()
                .filter(|app_name| !app_name.trim().is_empty())
            {
                return gpui_spawn_open_target_app_name(app_name, project_path);
            }
            if let Some(command) = *default_command {
                return gpui_spawn_open_target_command(command, *base_args, &[], project_path);
            }
            Err("Could not launch the selected Open In target.".to_string())
        }
        GpuiOpenTargetLaunch::Custom { command, args } => {
            gpui_spawn_open_target_command(command, &[], args, project_path)
        }
    }
}

pub(crate) fn gpui_keep_awake_agents_working_session_count(workspace: &WorkspaceModel) -> usize {
    workspace
        .terminal_sessions
        .iter()
        .filter(|session| {
            session.presentation_state == TerminalSessionPresentationState::Running
                && session.activity == AgentTerminalActivity::Working
        })
        .count()
}

pub(crate) fn gpui_keep_awake_command_working_session_count(command_pane: &CommandPaneModel) -> usize {
    command_pane
        .flat_tab_ids()
        .into_iter()
        .filter(|(_group_id, session_id)| {
            command_pane.session(*session_id).is_some_and(|session| {
                !session.is_sleeping && session.activity == CommandTerminalActivity::Working
            })
        })
        .count()
}

pub(crate) fn gpui_keep_awake_command_delayed_send_session_count(command_pane: &CommandPaneModel) -> usize {
    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-27-01:05:
    Native `createTitlebarKeepAwakeSessionState` treats projected Delayed Send remaining-time fields as a Keep Awake hold input only for non-sleeping terminal sessions. Agents `delayed_send_active` is semantic tab chrome in GPUI, unlike native projected remaining-time fields, so only command tabs with timer-owned runtime Delayed Send state can start the titlebar power hold.
    */
    command_pane
        .flat_tab_ids()
        .into_iter()
        .filter(|(_group_id, session_id)| {
            command_pane.session(*session_id).is_some_and(|session| {
                !session.is_sleeping
                    && session.delayed_send_active
                    && session.delayed_send_timer_owned
            })
        })
        .count()
}

pub(crate) fn gpui_keep_awake_working_session_count(
    workspace: &WorkspaceModel,
    command_pane: &CommandPaneModel,
) -> usize {
    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-26-00:29:
    GPUI `keepAwakeWhileWorkingSessions` counts only real local terminal model facts equivalent to the macOS titlebar projection: Running Agents with `AgentTerminalActivity::Working` plus live command-tab sessions that are not sleeping and have `CommandTerminalActivity::Working`. Do not infer work from titles, command text, paths, status-file contents, terminal output, gxserver ids, or persisted private metadata.
    */
    gpui_keep_awake_agents_working_session_count(workspace)
        + gpui_keep_awake_command_working_session_count(command_pane)
}

pub(crate) fn gpui_keep_awake_refresh_working_session_grace_state(
    enabled: bool,
    previous_working_session_count: usize,
    current_working_session_count: usize,
    grace_until: Option<Instant>,
    now: Instant,
) -> GpuiKeepAwakeWorkingSessionGraceState {
    if !enabled {
        return GpuiKeepAwakeWorkingSessionGraceState {
            previous_working_session_count: 0,
            grace_until: None,
        };
    }

    let active_grace_until = grace_until.filter(|deadline| *deadline > now);
    let grace_until = if previous_working_session_count > 0 && current_working_session_count == 0 {
        now.checked_add(GPUI_KEEP_AWAKE_WORKING_SESSION_GRACE)
            .or(active_grace_until)
    } else {
        active_grace_until
    };

    GpuiKeepAwakeWorkingSessionGraceState {
        previous_working_session_count: current_working_session_count,
        grace_until,
    }
}

pub(crate) fn gpui_keep_awake_working_session_hold_active(
    settings: shared_settings::SharedKeepAwakeTitlebarSettings,
    observed_working_session_count: usize,
    grace_until: Option<Instant>,
    now: Instant,
) -> bool {
    settings.while_working_sessions
        && (observed_working_session_count > 0
            || grace_until.is_some_and(|deadline| now < deadline))
}

pub(crate) fn gpui_keep_awake_fire_at(
    started_at: Instant,
    duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
) -> Option<Instant> {
    (duration_minutes.minutes() > 0)
        .then(|| started_at + Duration::from_secs(duration_minutes.minutes() * 60))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_read_keep_awake_power_snapshot(
    options: GpuiKeepAwakePowerProbeOptions,
) -> Result<GpuiKeepAwakePowerSnapshot, String> {
    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
    Power automation probes must use fixed macOS command paths and only when the corresponding Settings rule can act. Parse stdout in memory into battery percent, Low Power Mode boolean, and display count; never log or persist command output, display names, paths, URLs, commands, settings payloads, or private terminal content.
    */
    let battery_output = if options.include_battery {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/bin/pmset",
            &["-g", "batt"],
        )?)
    } else {
        None
    };
    let low_power_output = if options.include_low_power_mode {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/bin/pmset",
            &["-g"],
        )?)
    } else {
        None
    };
    let display_output = if options.include_external_display {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/sbin/system_profiler",
            &["SPDisplaysDataType"],
        )?)
    } else {
        None
    };
    Ok(gpui_parse_keep_awake_power_snapshot(
        battery_output.as_deref(),
        low_power_output.as_deref(),
        display_output.as_deref(),
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_keep_awake_fixed_command_stdout(path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "power probe failed".to_string())?;
    if !output.status.success() {
        return Err("power probe failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn gpui_parse_keep_awake_power_snapshot(
    battery_output: Option<&str>,
    low_power_output: Option<&str>,
    display_output: Option<&str>,
) -> GpuiKeepAwakePowerSnapshot {
    GpuiKeepAwakePowerSnapshot {
        battery_percent: battery_output.and_then(gpui_parse_pmset_battery_percent),
        external_display_connected: display_output
            .map(gpui_system_profiler_external_display_connected)
            .unwrap_or(false),
        low_power_mode: low_power_output.and_then(gpui_parse_pmset_low_power_mode),
    }
}

pub(crate) fn gpui_parse_pmset_battery_percent(output: &str) -> Option<f64> {
    for line in output.lines() {
        let Some(percent_index) = line.find('%') else {
            continue;
        };
        let before_percent = &line[..percent_index];
        let digits_reversed: String = before_percent
            .chars()
            .rev()
            .skip_while(|ch| ch.is_ascii_whitespace())
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .collect();
        if digits_reversed.is_empty() {
            continue;
        }
        let digits: String = digits_reversed.chars().rev().collect();
        let Ok(percent) = digits.parse::<f64>() else {
            continue;
        };
        if percent.is_finite() {
            return Some(percent);
        }
    }
    None
}

pub(crate) fn gpui_parse_pmset_low_power_mode(output: &str) -> Option<bool> {
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some("lowpowermode")).then(|| parts.next() == Some("1"))
    })
}

pub(crate) fn gpui_system_profiler_external_display_connected(output: &str) -> bool {
    output
        .lines()
        .filter(|line| line.trim_start().starts_with("Resolution:"))
        .count()
        > 1
}

#[derive(Clone)]
pub(crate) struct GpuiAgentsHubProfileItem {
    pub(crate) agent_icon: &'static str,
    pub(crate) file_path: PathBuf,
    pub(crate) label: String,
    pub(crate) profile_path: PathBuf,
    pub(crate) target_path: Option<PathBuf>,
}

pub(crate) struct GpuiAgentsHubCatalogBuild {
    pub(crate) file_paths: HashSet<PathBuf>,
    pub(crate) message: serde_json::Value,
    pub(crate) open_paths: HashSet<PathBuf>,
}

pub(crate) struct GpuiAgentsHubCatalogBuilder {
    pub(crate) allowed_files: Vec<PathBuf>,
    pub(crate) allowed_roots: Vec<PathBuf>,
    pub(crate) file_paths: HashSet<PathBuf>,
    pub(crate) groups_by_tab: HashMap<&'static str, Vec<serde_json::Value>>,
    pub(crate) home: PathBuf,
    pub(crate) open_paths: HashSet<PathBuf>,
    pub(crate) seen_files: HashSet<PathBuf>,
}

impl GpuiAgentsHubCatalogBuilder {
    pub(crate) fn new(home: PathBuf) -> Self {
        let allowed_roots = gpui_agents_hub_allowed_roots(&home);
        let allowed_files = gpui_agents_hub_allowed_files(&home);
        Self {
            allowed_files,
            allowed_roots,
            file_paths: HashSet::new(),
            groups_by_tab: HashMap::new(),
            home,
            open_paths: HashSet::new(),
            seen_files: HashSet::new(),
        }
    }

    pub(crate) fn home_path(&self, parts: &[&str]) -> PathBuf {
        let mut path = self.home.clone();
        for part in parts {
            path.push(part);
        }
        path
    }

    pub(crate) fn add_group(
        &mut self,
        tab: &'static str,
        group_id: String,
        name: String,
        root_path: PathBuf,
        description: &'static str,
        files: Vec<PathBuf>,
        profiles: Vec<GpuiAgentsHubProfileItem>,
    ) {
        let resolved_files = files
            .into_iter()
            .filter_map(|candidate| self.file_item(&candidate, &root_path))
            .collect::<Vec<_>>();
        if resolved_files.is_empty() {
            return;
        }
        self.insert_open_path_if_allowed(&root_path);
        for profile in &profiles {
            self.insert_open_path_if_allowed(&profile.profile_path);
        }
        self.groups_by_tab.entry(tab).or_default().push(serde_json::json!({
            "description": description,
            "files": resolved_files,
            "id": group_id,
            "name": name,
            "path": gpui_path_string(&root_path),
            "profiles": profiles.into_iter().map(gpui_agents_hub_profile_json).collect::<Vec<_>>(),
        }));
    }

    pub(crate) fn file_item(&mut self, candidate_path: &Path, root_path: &Path) -> Option<serde_json::Value> {
        let resolved = self.valid_catalog_file(candidate_path)?;
        if self.seen_files.contains(&resolved) {
            return None;
        }
        self.seen_files.insert(resolved.clone());
        self.file_paths.insert(resolved.clone());
        if let Some(parent) = resolved.parent() {
            self.insert_open_path_if_allowed(parent);
        }
        let name = gpui_relative_path_name(&resolved, root_path)
            .unwrap_or_else(|| gpui_file_name_string(&resolved));
        Some(serde_json::json!({
            "id": gpui_agents_hub_file_id(&resolved),
            "language": gpui_agents_hub_language_for(&resolved),
            "name": name,
            "path": gpui_path_string(&resolved),
        }))
    }

    pub(crate) fn valid_catalog_file(&self, candidate_path: &Path) -> Option<PathBuf> {
        let resolved = fs::canonicalize(candidate_path).ok()?;
        if !self.is_allowed_path(&resolved) {
            return None;
        }
        let metadata = fs::metadata(&resolved).ok()?;
        if metadata.is_file() && metadata.len() <= GPUI_AGENTS_HUB_MAX_FILE_BYTES {
            Some(resolved)
        } else {
            None
        }
    }

    pub(crate) fn is_allowed_path(&self, resolved: &Path) -> bool {
        self.allowed_files.iter().any(|file| file == resolved)
            || self
                .allowed_roots
                .iter()
                .any(|root| gpui_path_is_relative_to(resolved, root))
    }

    pub(crate) fn insert_open_path_if_allowed(&mut self, candidate_path: &Path) {
        let Ok(resolved) = fs::canonicalize(candidate_path) else {
            return;
        };
        let Ok(metadata) = fs::metadata(&resolved) else {
            return;
        };
        if metadata.is_dir() && self.is_allowed_path(&resolved) {
            self.open_paths.insert(resolved);
        }
    }

    pub(crate) fn finish(mut self) -> GpuiAgentsHubCatalogBuild {
        for groups in self.groups_by_tab.values_mut() {
            groups.sort_by(|left, right| {
                let left_name = left
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let right_name = right
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                left_name.to_lowercase().cmp(&right_name.to_lowercase())
            });
        }
        let message = serde_json::json!({
            "generatedAt": gpui_status_generated_at(),
            "groupsByTab": {
                "configs": self.groups_by_tab.remove("configs").unwrap_or_default(),
                "hooks": self.groups_by_tab.remove("hooks").unwrap_or_default(),
                "mds": self.groups_by_tab.remove("mds").unwrap_or_default(),
                "skills": self.groups_by_tab.remove("skills").unwrap_or_default(),
            },
            "type": "agentsHubCatalog",
        });
        GpuiAgentsHubCatalogBuild {
            file_paths: self.file_paths,
            message,
            open_paths: self.open_paths,
        }
    }
}

pub(crate) fn gpui_agents_hub_catalog_message() -> serde_json::Value {
    gpui_agents_hub_catalog_build().message
}

pub(crate) fn gpui_agents_hub_catalog_build() -> GpuiAgentsHubCatalogBuild {
    /*
    CDXC:GPUIAgentsHubBridge 2026-06-24-12:26:
    GPUI Agents Hub scans the same machine-local agent/profile/skill/config roots as the macOS native helper, but catalog rows remain metadata-only. File bodies are deliberately omitted here so opening the Hub cannot bridge private instruction/config contents until the user selects a file.
    */
    let Some(home) = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return gpui_empty_agents_hub_catalog_build();
    };
    let mut builder = GpuiAgentsHubCatalogBuilder::new(home.clone());
    let main_target = builder.home_path(&[".agents", "main.md"]);
    let main_claude = gpui_agents_hub_profile(
        "claude",
        "Claude Code main",
        builder.home_path(&[".claude"]),
        builder.home_path(&[".claude", "CLAUDE.md"]),
        Some(main_target.clone()),
    );
    let main_codex = gpui_agents_hub_profile(
        "codex",
        "Codex main",
        builder.home_path(&[".codex"]),
        builder.home_path(&[".codex", "AGENTS.md"]),
        Some(main_target.clone()),
    );
    let mut profiles = vec![main_claude.clone()];
    for profile_path in gpui_agents_hub_list_directories(&builder.home_path(&[".claude-profiles"]))
    {
        let name = gpui_file_name_string(&profile_path);
        if !name.starts_with('.') && gpui_is_file(&profile_path.join("CLAUDE.md")) {
            profiles.push(gpui_agents_hub_profile(
                "claude",
                format!("Claude Code {name}"),
                profile_path.clone(),
                profile_path.join("CLAUDE.md"),
                Some(main_target.clone()),
            ));
        }
    }
    profiles.push(main_codex.clone());
    for profile_path in gpui_agents_hub_list_directories(&builder.home_path(&[".codex-profiles"])) {
        let name = gpui_file_name_string(&profile_path);
        if !name.starts_with('.')
            && (gpui_is_file(&profile_path.join("AGENTS.md"))
                || gpui_is_file(&profile_path.join("config.toml")))
        {
            profiles.push(gpui_agents_hub_profile(
                "codex",
                format!("Codex {name}"),
                profile_path.clone(),
                profile_path.join("AGENTS.md"),
                Some(main_target.clone()),
            ));
        }
    }
    let open_code = gpui_agents_hub_profile(
        "opencode",
        "OpenCode main",
        builder.home_path(&[".config", "opencode"]),
        builder.home_path(&[".config", "opencode", "opencode.json"]),
        None,
    );
    let pi_agent = gpui_agents_hub_profile(
        "pi",
        "Pi agent",
        builder.home_path(&[".pi", "agent"]),
        builder.home_path(&[".pi", "agent", "settings.json"]),
        None,
    );
    let linked_profiles = profiles.clone();

    let shared_agents_root = builder.home_path(&[".agents"]);
    builder.add_group(
        "mds",
        "md-shared-agents".to_string(),
        "Shared agent markdown".to_string(),
        shared_agents_root.clone(),
        "Shared instructions and best-practice markdown linked by agent profiles.",
        gpui_agents_hub_walk_files(&shared_agents_root, 1, |candidate| {
            gpui_agents_hub_extension(candidate) == ".md"
        }),
        linked_profiles.clone(),
    );
    let claude_profiles = profiles
        .iter()
        .filter(|profile| profile.agent_icon == "claude")
        .cloned()
        .collect::<Vec<_>>();
    builder.add_group(
        "mds",
        "md-claude-profiles".to_string(),
        "Claude profile instructions".to_string(),
        builder.home_path(&[".claude-profiles"]),
        "CLAUDE.md files owned by Claude profiles.",
        gpui_agents_hub_existing_files(
            claude_profiles
                .iter()
                .map(|item| item.file_path.clone())
                .collect(),
        ),
        claude_profiles,
    );
    let codex_profiles = profiles
        .iter()
        .filter(|profile| profile.agent_icon == "codex")
        .cloned()
        .collect::<Vec<_>>();
    builder.add_group(
        "mds",
        "md-codex-profiles".to_string(),
        "Codex profile instructions".to_string(),
        builder.home_path(&[".codex-profiles"]),
        "AGENTS.md files owned by Codex profiles.",
        gpui_agents_hub_existing_files(
            codex_profiles
                .iter()
                .map(|item| item.file_path.clone())
                .collect(),
        ),
        codex_profiles,
    );

    let skill_roots: Vec<(
        &str,
        PathBuf,
        &'static str,
        &'static str,
        Vec<GpuiAgentsHubProfileItem>,
    )> = vec![
        (
            "skill-shared-dot-agents",
            builder.home_path(&[".agents", "skills"]),
            "Shared skill installed under ~/.agents/skills.",
            "System skill installed in the shared agent skill folder.",
            linked_profiles.clone(),
        ),
        (
            "skill-legacy-shared-agents",
            home.join("agents").join("skills"),
            "Legacy shared skill installed under ~/agents/skills.",
            "System skill installed in the legacy shared agent skill folder.",
            linked_profiles.clone(),
        ),
        (
            "skill-claude",
            builder.home_path(&[".claude", "skills"]),
            "Claude Code global skill installed under ~/.claude/skills.",
            "Claude Code system skill installed under ~/.claude/skills.",
            vec![gpui_agents_hub_profile(
                "claude",
                "Claude Code skills",
                builder.home_path(&[".claude"]),
                builder.home_path(&[".claude", "skills"]),
                None,
            )],
        ),
        (
            "skill-codex",
            builder.home_path(&[".codex", "skills"]),
            "Codex global skill installed under ~/.codex/skills.",
            "Codex system skill installed under ~/.codex/skills.",
            vec![gpui_agents_hub_profile(
                "codex",
                "Codex skills",
                builder.home_path(&[".codex"]),
                builder.home_path(&[".codex", "skills"]),
                None,
            )],
        ),
        (
            "skill-cursor",
            builder.home_path(&[".cursor", "skills"]),
            "Cursor global skill installed under ~/.cursor/skills.",
            "Cursor system skill installed under ~/.cursor/skills.",
            vec![gpui_agents_hub_profile(
                "cursor-cli",
                "Cursor CLI skills",
                builder.home_path(&[".cursor"]),
                builder.home_path(&[".cursor", "skills"]),
                None,
            )],
        ),
        (
            "skill-opencode",
            builder.home_path(&[".config", "opencode", "skills"]),
            "OpenCode global skill installed under ~/.config/opencode/skills.",
            "OpenCode system skill installed under ~/.config/opencode/skills.",
            vec![gpui_agents_hub_profile(
                "opencode",
                "OpenCode skills",
                builder.home_path(&[".config", "opencode"]),
                builder.home_path(&[".config", "opencode", "skills"]),
                None,
            )],
        ),
        (
            "skill-pi",
            builder.home_path(&[".pi", "agent", "skills"]),
            "Pi global skill installed under ~/.pi/agent/skills.",
            "Pi system skill installed under ~/.pi/agent/skills.",
            vec![gpui_agents_hub_profile(
                "pi",
                "Pi skills",
                builder.home_path(&[".pi", "agent"]),
                builder.home_path(&[".pi", "agent", "skills"]),
                None,
            )],
        ),
        (
            "skill-gemini",
            builder.home_path(&[".gemini", "skills"]),
            "Gemini CLI global skill installed under ~/.gemini/skills.",
            "Gemini CLI system skill installed under ~/.gemini/skills.",
            vec![gpui_agents_hub_profile(
                "gemini",
                "Gemini CLI skills",
                builder.home_path(&[".gemini"]),
                builder.home_path(&[".gemini", "skills"]),
                None,
            )],
        ),
        (
            "skill-copilot",
            builder.home_path(&[".copilot", "skills"]),
            "GitHub Copilot global skill installed under ~/.copilot/skills.",
            "GitHub Copilot system skill installed under ~/.copilot/skills.",
            vec![gpui_agents_hub_profile(
                "copilot",
                "GitHub Copilot skills",
                builder.home_path(&[".copilot"]),
                builder.home_path(&[".copilot", "skills"]),
                None,
            )],
        ),
        (
            "skill-factory-droid",
            builder.home_path(&[".factory", "skills"]),
            "Factory Droid global skill installed under ~/.factory/skills.",
            "Factory Droid system skill installed under ~/.factory/skills.",
            vec![gpui_agents_hub_profile(
                "factory-droid",
                "Factory Droid skills",
                builder.home_path(&[".factory"]),
                builder.home_path(&[".factory", "skills"]),
                None,
            )],
        ),
        (
            "skill-antigravity-cli",
            builder.home_path(&[".gemini", "antigravity-cli", "skills"]),
            "Antigravity CLI global skill installed under ~/.gemini/antigravity-cli/skills.",
            "Antigravity CLI system skill installed under ~/.gemini/antigravity-cli/skills.",
            vec![gpui_agents_hub_profile(
                "antigravity-cli",
                "Antigravity CLI skills",
                builder.home_path(&[".gemini", "antigravity-cli"]),
                builder.home_path(&[".gemini", "antigravity-cli", "skills"]),
                None,
            )],
        ),
        (
            "skill-antigravity",
            builder.home_path(&[".gemini", "antigravity", "skills"]),
            "Antigravity global skill installed under ~/.gemini/antigravity/skills.",
            "Antigravity system skill installed under ~/.gemini/antigravity/skills.",
            vec![gpui_agents_hub_profile(
                "antigravity-cli",
                "Antigravity skills",
                builder.home_path(&[".gemini", "antigravity"]),
                builder.home_path(&[".gemini", "antigravity", "skills"]),
                None,
            )],
        ),
        (
            "skill-config-agents",
            builder.home_path(&[".config", "agents", "skills"]),
            "Universal agent skill installed under ~/.config/agents/skills.",
            "Universal agent system skill installed under ~/.config/agents/skills.",
            vec![gpui_agents_hub_profile(
                "amp-cli",
                "Universal agent skills",
                builder.home_path(&[".config", "agents"]),
                builder.home_path(&[".config", "agents", "skills"]),
                None,
            )],
        ),
        (
            "skill-hermes-agent",
            builder.home_path(&[".hermes", "skills"]),
            "Hermes Agent global skill installed under ~/.hermes/skills.",
            "Hermes Agent system skill installed under ~/.hermes/skills.",
            vec![gpui_agents_hub_profile(
                "hermes-agent",
                "Hermes Agent skills",
                builder.home_path(&[".hermes"]),
                builder.home_path(&[".hermes", "skills"]),
                None,
            )],
        ),
        (
            "skill-kiro",
            builder.home_path(&[".kiro", "skills"]),
            "Kiro CLI global skill installed under ~/.kiro/skills.",
            "Kiro CLI system skill installed under ~/.kiro/skills.",
            vec![gpui_agents_hub_profile(
                "kiro",
                "Kiro CLI skills",
                builder.home_path(&[".kiro"]),
                builder.home_path(&[".kiro", "skills"]),
                None,
            )],
        ),
        (
            "skill-codebuddy",
            builder.home_path(&[".codebuddy", "skills"]),
            "CodeBuddy global skill installed under ~/.codebuddy/skills.",
            "CodeBuddy system skill installed under ~/.codebuddy/skills.",
            vec![gpui_agents_hub_profile(
                "codebuddy",
                "CodeBuddy skills",
                builder.home_path(&[".codebuddy"]),
                builder.home_path(&[".codebuddy", "skills"]),
                None,
            )],
        ),
        (
            "skill-qoder",
            builder.home_path(&[".qoder", "skills"]),
            "Qoder global skill installed under ~/.qoder/skills.",
            "Qoder system skill installed under ~/.qoder/skills.",
            vec![gpui_agents_hub_profile(
                "qoder",
                "Qoder skills",
                builder.home_path(&[".qoder"]),
                builder.home_path(&[".qoder", "skills"]),
                None,
            )],
        ),
        (
            "skill-rovo-dev",
            builder.home_path(&[".rovodev", "skills"]),
            "Rovo Dev global skill installed under ~/.rovodev/skills.",
            "Rovo Dev system skill installed under ~/.rovodev/skills.",
            vec![gpui_agents_hub_profile(
                "rovo-dev",
                "Rovo Dev skills",
                builder.home_path(&[".rovodev"]),
                builder.home_path(&[".rovodev", "skills"]),
                None,
            )],
        ),
    ];
    for (id_prefix, root, description, system_description, profiles) in skill_roots {
        gpui_agents_hub_add_skill_root(
            &mut builder,
            id_prefix,
            &root,
            description,
            system_description,
            profiles,
        );
    }

    for plugins_root in [
        builder.home_path(&[".codex-profiles"]),
        builder.home_path(&[".claude-profiles"]),
    ] {
        for profile_dir in gpui_agents_hub_list_directories(&plugins_root) {
            if gpui_file_name_string(&profile_dir).starts_with('.') {
                continue;
            }
            let cache_root = profile_dir.join("plugins").join("cache");
            if !cache_root.exists() {
                continue;
            }
            let mut roots: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
            for plugin_file in gpui_agents_hub_walk_files(&cache_root, 7, |candidate| {
                gpui_file_name_string(candidate) == "SKILL.md"
                    || gpui_path_string(candidate).ends_with("/.codex-plugin/plugin.json")
                    || gpui_path_string(candidate).ends_with("/.claude-plugin/plugin.json")
            }) {
                if let Some(root) = gpui_agents_hub_plugin_root(&plugin_file) {
                    roots.entry(root).or_default().push(plugin_file);
                }
            }
            let mut roots = roots.into_iter().collect::<Vec<_>>();
            roots.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (root, files) in roots {
                let rel_name = gpui_relative_path_name(&root, &cache_root)
                    .unwrap_or_else(|| gpui_file_name_string(&root));
                builder.add_group(
                    "skills",
                    format!("skill-profile-{}", gpui_agents_hub_file_id(&root)),
                    rel_name,
                    root.clone(),
                    "Skill or plugin manifest installed inside an agent profile plugin cache.",
                    files,
                    gpui_agents_hub_profiles_for(
                        &root,
                        &home,
                        &profiles,
                        &linked_profiles,
                        &open_code,
                        &pi_agent,
                    ),
                );
            }
        }
    }

    let hooks_root = home.join("agents").join("hooks");
    let mut hooks_profiles = linked_profiles.clone();
    hooks_profiles.push(pi_agent.clone());
    builder.add_group(
        "hooks",
        "hooks-shared".to_string(),
        "Shared hooks".to_string(),
        hooks_root.clone(),
        "Shared hook scripts and documentation used by agent profiles.",
        gpui_agents_hub_walk_files(&hooks_root, 3, gpui_agents_hub_is_text_file),
        hooks_profiles,
    );
    builder.add_group(
        "hooks",
        "hooks-codex-main".to_string(),
        "Codex main hooks".to_string(),
        builder.home_path(&[".codex"]),
        "Global Codex hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".codex", "hooks.json"])]),
        vec![main_codex.clone()],
    );
    builder.add_group(
        "hooks",
        "hooks-codex-profiles".to_string(),
        "Codex profile hooks".to_string(),
        builder.home_path(&[".codex-profiles"]),
        "hooks.json files owned by Codex profiles.",
        gpui_agents_hub_walk_files(&builder.home_path(&[".codex-profiles"]), 2, |candidate| {
            gpui_file_name_string(candidate) == "hooks.json"
        }),
        profiles
            .iter()
            .filter(|item| item.agent_icon == "codex")
            .cloned()
            .collect(),
    );
    builder.add_group(
        "hooks",
        "hooks-cursor-main".to_string(),
        "Cursor hooks".to_string(),
        builder.home_path(&[".cursor"]),
        "Cursor Agent hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".cursor", "hooks.json"])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-antigravity-main".to_string(),
        "Antigravity hooks".to_string(),
        builder.home_path(&[".gemini", "config"]),
        "Antigravity hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[
            ".gemini",
            "config",
            "hooks.json",
        ])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-grok-main".to_string(),
        "Grok hooks".to_string(),
        builder.home_path(&[".grok", "hooks"]),
        "Grok hook configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[
            ".grok",
            "hooks",
            "ghostex-session.json",
        ])]),
        Vec::new(),
    );
    builder.add_group(
        "hooks",
        "hooks-pi-agent".to_string(),
        "Pi extensions".to_string(),
        builder.home_path(&[".pi", "agent"]),
        "Pi agent extension hooks and settings-adjacent TypeScript files.",
        gpui_agents_hub_walk_files(
            &builder.home_path(&[".pi", "agent", "extensions"]),
            2,
            |candidate| {
                matches!(
                    gpui_agents_hub_extension(candidate).as_str(),
                    ".ts" | ".js" | ".json"
                )
            },
        ),
        vec![pi_agent.clone()],
    );

    builder.add_group(
        "configs",
        "config-shared-agents".to_string(),
        "Shared agent config".to_string(),
        shared_agents_root.clone(),
        "Shared agent lock and setup files.",
        gpui_agents_hub_walk_files(&shared_agents_root, 1, |candidate| {
            gpui_file_name_string(candidate).ends_with(".json")
        }),
        linked_profiles.clone(),
    );
    builder.add_group(
        "configs",
        "config-claude-main".to_string(),
        "Claude main configs".to_string(),
        builder.home_path(&[".claude"]),
        "Global Claude Code settings and MCP configuration.",
        gpui_agents_hub_existing_files(vec![
            builder.home_path(&[".claude.json"]),
            builder.home_path(&[".claude", "settings.json"]),
            builder.home_path(&[".claude", "settings.local.json"]),
        ]),
        vec![main_claude.clone()],
    );
    for item in profiles.iter().filter(|profile| {
        profile.agent_icon == "claude"
            && gpui_path_string(&profile.profile_path).contains("-profiles")
    }) {
        let root = item.profile_path.clone();
        builder.add_group(
            "configs",
            format!("config-claude-{}", gpui_agents_hub_file_id(&root)),
            format!("Claude {} configs", gpui_file_name_string(&root)),
            root.clone(),
            "Claude profile-owned config and plugin registry files.",
            gpui_agents_hub_existing_files(vec![
                root.join(".claude.json"),
                root.join("settings.json"),
                root.join("settings.local.json"),
                root.join("policy-limits.json"),
                root.join("stats-cache.json"),
                root.join("plugins").join("installed_plugins.json"),
                root.join("plugins").join("known_marketplaces.json"),
                root.join("plugins").join("blocklist.json"),
            ]),
            vec![item.clone()],
        );
    }
    builder.add_group(
        "configs",
        "config-codex-main".to_string(),
        "Codex main configs".to_string(),
        builder.home_path(&[".codex"]),
        "Global Codex TOML configuration.",
        gpui_agents_hub_existing_files(vec![builder.home_path(&[".codex", "config.toml"])]),
        vec![main_codex.clone()],
    );
    for item in profiles.iter().filter(|profile| {
        profile.agent_icon == "codex"
            && gpui_path_string(&profile.profile_path).contains("-profiles")
    }) {
        let root = item.profile_path.clone();
        builder.add_group(
            "configs",
            format!("config-codex-{}", gpui_agents_hub_file_id(&root)),
            format!("Codex {} configs", gpui_file_name_string(&root)),
            root.clone(),
            "Codex profile-owned config, browser, and plugin registry files.",
            gpui_agents_hub_existing_files(vec![
                root.join("config.toml"),
                root.join(".codex-global-state.json"),
                root.join("browser").join("config.toml"),
                root.join("plugins").join("installed_plugins.json"),
                root.join("plugins").join("known_marketplaces.json"),
                root.join("plugins").join("blocklist.json"),
            ]),
            vec![item.clone()],
        );
    }
    builder.add_group(
        "configs",
        "config-opencode".to_string(),
        "OpenCode configs".to_string(),
        open_code.profile_path.clone(),
        "OpenCode JSON, package, and plugin files.",
        gpui_agents_hub_walk_files(&open_code.profile_path, 2, |candidate| {
            matches!(
                gpui_file_name_string(candidate).as_str(),
                "opencode.json" | "tui.json" | "package.json"
            ) || (gpui_file_name_string(candidate.parent().unwrap_or_else(|| Path::new("")))
                == "plugin"
                && gpui_agents_hub_extension(candidate) == ".js")
        }),
        vec![open_code.clone()],
    );
    builder.add_group(
        "configs",
        "config-pi".to_string(),
        "Pi configs".to_string(),
        pi_agent.profile_path.clone(),
        "Pi agent settings and local extension files.",
        gpui_agents_hub_walk_files(&pi_agent.profile_path, 2, |candidate| {
            matches!(
                gpui_agents_hub_extension(candidate).as_str(),
                ".json" | ".ts" | ".js"
            ) && gpui_file_name_string(candidate) != "auth.json"
        }),
        vec![pi_agent.clone()],
    );

    builder.finish()
}

pub(crate) fn gpui_agents_hub_add_skill_root(
    builder: &mut GpuiAgentsHubCatalogBuilder,
    id_prefix: &str,
    root: &Path,
    description: &'static str,
    system_description: &'static str,
    profiles: Vec<GpuiAgentsHubProfileItem>,
) {
    for skill_dir in gpui_agents_hub_list_directories(root) {
        let skill_name = gpui_file_name_string(&skill_dir);
        if skill_name.starts_with('.') && skill_name != ".system" {
            continue;
        }
        if skill_name == ".system" {
            for system_skill in gpui_agents_hub_list_directories(&skill_dir) {
                builder.add_group(
                    "skills",
                    format!(
                        "{id_prefix}-system-{}",
                        gpui_agents_hub_file_id(&system_skill)
                    ),
                    gpui_file_name_string(&system_skill),
                    system_skill.clone(),
                    system_description,
                    gpui_agents_hub_walk_files(&system_skill, 3, gpui_agents_hub_is_skill_file),
                    profiles.clone(),
                );
            }
            continue;
        }
        builder.add_group(
            "skills",
            format!("{id_prefix}-{}", gpui_agents_hub_file_id(&skill_dir)),
            skill_name,
            skill_dir.clone(),
            description,
            gpui_agents_hub_walk_files(&skill_dir, 3, gpui_agents_hub_is_skill_file),
            profiles.clone(),
        );
    }
}

pub(crate) fn gpui_empty_agents_hub_catalog_build() -> GpuiAgentsHubCatalogBuild {
    GpuiAgentsHubCatalogBuild {
        file_paths: HashSet::new(),
        message: serde_json::json!({
            "generatedAt": gpui_status_generated_at(),
            "groupsByTab": {
                "configs": [],
                "hooks": [],
                "mds": [],
                "skills": [],
            },
            "type": "agentsHubCatalog",
        }),
        open_paths: HashSet::new(),
    }
}

pub(crate) fn gpui_agents_hub_file_content_message(
    file_path: String,
    request_id: String,
) -> serde_json::Value {
    let normalized_file_path = file_path.trim().to_string();
    if normalized_file_path.is_empty() {
        return gpui_agents_hub_file_content_error(
            file_path,
            request_id,
            "Choose an Agents Hub file first.",
        );
    }
    let resolved = match gpui_agents_hub_validate_catalog_file_path(&normalized_file_path) {
        Ok(path) => path,
        Err(message) => {
            return gpui_agents_hub_file_content_error(normalized_file_path, request_id, &message);
        }
    };
    match fs::read_to_string(&resolved) {
        Ok(content) => serde_json::json!({
            "content": content,
            "filePath": gpui_path_string(&resolved),
            "requestId": request_id,
            "type": "agentsHubFileContent",
        }),
        Err(_) => gpui_agents_hub_file_content_error(
            gpui_path_string(&resolved),
            request_id,
            "Unable to load file contents.",
        ),
    }
}

pub(crate) fn gpui_agents_hub_file_content_error(
    file_path: String,
    request_id: String,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "errorMessage": message,
        "filePath": file_path,
        "requestId": request_id,
        "type": "agentsHubFileContent",
    })
}

pub(crate) fn gpui_save_agents_hub_file(
    file_path: String,
    content: String,
) -> Result<serde_json::Value, String> {
    let resolved = gpui_agents_hub_validate_catalog_file_path(&file_path)?;
    let Some(directory) = resolved.parent() else {
        return Err("Unable to save the selected Agents Hub file.".to_string());
    };
    let temp_path = directory.join(format!(
        ".ghostex-agents-hub-save.{}.tmp",
        system_time_epoch_millis_string(SystemTime::now())
    ));
    if fs::write(&temp_path, content.as_bytes()).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err("Unable to save the selected Agents Hub file.".to_string());
    }
    if fs::rename(&temp_path, &resolved).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err("Unable to save the selected Agents Hub file.".to_string());
    }
    Ok(gpui_agents_hub_catalog_message())
}

pub(crate) fn gpui_agents_hub_open_path_in_finder(path: String) -> Result<(), String> {
    /*
    CDXC:GPUIAgentsHubBridge 2026-06-24-12:26:
    Agents Hub opener commands accept only paths proven by the current catalog builder: catalog files, their containing directories, group roots, and profile roots. React-supplied arbitrary paths, URLs, shell commands, query strings, environment values, and file contents are never forwarded to the OS opener.
    */
    let normalized = path.trim();
    if normalized.is_empty() {
        return Err("Choose an Agents Hub path first.".to_string());
    }
    let resolved = fs::canonicalize(normalized)
        .map_err(|_| "The selected Agents Hub path is unavailable.".to_string())?;
    let metadata = fs::metadata(&resolved)
        .map_err(|_| "The selected Agents Hub path is unavailable.".to_string())?;
    if metadata.is_file() {
        let file = gpui_agents_hub_validate_catalog_file_path(normalized)?;
        let parent = file
            .parent()
            .ok_or_else(|| "The selected Agents Hub file has no containing folder.".to_string())?;
        return gpui_open_path(parent);
    }
    let catalog = gpui_agents_hub_catalog_build();
    if metadata.is_dir() && catalog.open_paths.contains(&resolved) {
        gpui_open_path(&resolved)
    } else {
        Err("The selected Agents Hub path is not available from the current catalog.".to_string())
    }
}

pub(crate) fn gpui_agents_hub_source_open_target(file_path: String) -> Result<PendingSourceFileOpen, String> {
    /*
    The app-modal renderer supplies only a catalog file candidate. Resolve it
    again against the current Rust-owned catalog and root Source at the file's
    containing directory; no external command or saved editor preference is
    involved.
    */
    let resolved = gpui_agents_hub_validate_catalog_file_path(&file_path)?;
    let Some(project_path) = resolved.parent().map(Path::to_path_buf) else {
        return Err("The selected Agents Hub file has no containing folder.".to_string());
    };
    Ok(PendingSourceFileOpen {
        file_path: resolved,
        project_path,
    })
}

pub(crate) fn gpui_agents_hub_validate_catalog_file_path(file_path: &str) -> Result<PathBuf, String> {
    let normalized = file_path.trim();
    if normalized.is_empty() {
        return Err("Choose an Agents Hub file first.".to_string());
    }
    let resolved = fs::canonicalize(normalized)
        .map_err(|_| "The selected Agents Hub file is unavailable.".to_string())?;
    let catalog = gpui_agents_hub_catalog_build();
    if catalog.file_paths.contains(&resolved) {
        Ok(resolved)
    } else {
        Err("The selected Agents Hub file is not available from the current catalog.".to_string())
    }
}

pub(crate) fn gpui_agents_hub_allowed_roots(home: &Path) -> Vec<PathBuf> {
    [
        home.join(".agents"),
        home.join("agents"),
        home.join(".claude"),
        home.join(".claude-profiles"),
        home.join(".codebuddy"),
        home.join(".config").join("agents"),
        home.join(".codex"),
        home.join(".codex-profiles"),
        home.join(".copilot"),
        home.join(".cursor"),
        home.join(".factory"),
        home.join(".gemini"),
        home.join(".grok").join("hooks"),
        home.join(".hermes"),
        home.join(".kiro"),
        home.join(".config").join("opencode"),
        home.join(".pi").join("agent"),
        home.join(".qoder"),
        home.join(".rovodev"),
    ]
    .into_iter()
    .filter_map(|path| fs::canonicalize(path).ok())
    .collect()
}

pub(crate) fn gpui_agents_hub_allowed_files(home: &Path) -> Vec<PathBuf> {
    [home.join(".claude.json")]
        .into_iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect()
}

pub(crate) fn gpui_agents_hub_profile(
    agent_icon: &'static str,
    label: impl Into<String>,
    profile_path: PathBuf,
    file_path: PathBuf,
    target_path: Option<PathBuf>,
) -> GpuiAgentsHubProfileItem {
    GpuiAgentsHubProfileItem {
        agent_icon,
        file_path,
        label: label.into(),
        profile_path,
        target_path,
    }
}

pub(crate) fn gpui_agents_hub_profile_json(profile: GpuiAgentsHubProfileItem) -> serde_json::Value {
    let mut value = serde_json::json!({
        "agentIcon": profile.agent_icon,
        "filePath": gpui_path_string(&profile.file_path),
        "label": profile.label,
        "profilePath": gpui_path_string(&profile.profile_path),
    });
    if let Some(target_path) = profile.target_path {
        value["targetPath"] = serde_json::Value::String(gpui_path_string(&target_path));
    }
    value
}

pub(crate) fn gpui_agents_hub_profiles_for(
    candidate_path: &Path,
    home: &Path,
    profiles: &[GpuiAgentsHubProfileItem],
    linked_profiles: &[GpuiAgentsHubProfileItem],
    open_code: &GpuiAgentsHubProfileItem,
    pi_agent: &GpuiAgentsHubProfileItem,
) -> Vec<GpuiAgentsHubProfileItem> {
    let shared_agents_root = home.join(".agents");
    let shared_skills_root = home.join("agents");
    if gpui_path_is_relative_to(candidate_path, &shared_agents_root)
        || gpui_path_is_relative_to(candidate_path, &shared_skills_root)
    {
        let mut result = linked_profiles.to_vec();
        if gpui_path_is_relative_to(candidate_path, &home.join("agents").join("hooks")) {
            result.push(pi_agent.clone());
        }
        return result;
    }
    for profile in profiles {
        if gpui_path_is_relative_to(candidate_path, &profile.profile_path) {
            return vec![profile.clone()];
        }
    }
    if gpui_path_is_relative_to(candidate_path, &open_code.profile_path) {
        return vec![open_code.clone()];
    }
    if gpui_path_is_relative_to(candidate_path, &pi_agent.profile_path) {
        return vec![pi_agent.clone()];
    }
    Vec::new()
}

pub(crate) fn gpui_agents_hub_existing_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|path| gpui_is_file(path))
        .collect()
}

pub(crate) fn gpui_agents_hub_list_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            fs::metadata(path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

pub(crate) fn gpui_agents_hub_walk_files<F>(root: &Path, max_depth: usize, predicate: F) -> Vec<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    let mut files = Vec::new();
    gpui_agents_hub_walk_files_inner(root, 0, max_depth, &predicate, &mut files);
    files.sort();
    files
}

pub(crate) fn gpui_agents_hub_walk_files_inner<F>(
    current: &Path,
    depth: usize,
    max_depth: usize,
    predicate: &F,
    files: &mut Vec<PathBuf>,
) where
    F: Fn(&Path) -> bool,
{
    let mut entries = fs::read_dir(current)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in &entries {
        let path = entry.path();
        if fs::metadata(&path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
            && predicate(&path)
        {
            files.push(path);
        }
    }
    if depth >= max_depth {
        return;
    }
    for entry in entries {
        let path = entry.path();
        let name = gpui_file_name_string(&path);
        if gpui_agents_hub_ignored_directory_name(&name) {
            continue;
        }
        if fs::metadata(&path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            gpui_agents_hub_walk_files_inner(&path, depth + 1, max_depth, predicate, files);
        }
    }
}

pub(crate) fn gpui_agents_hub_ignored_directory_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "dist"
            | "build"
            | "out"
            | "coverage"
            | ".cache"
            | "cache"
            | "__pycache__"
            | "sessions"
            | "projects"
            | "todos"
            | "telemetry"
            | "usage-data"
            | "ambient-suggestions"
            | "memories_2026-04-24"
            | "logs"
            | "tmp"
            | ".tmp"
    )
}

pub(crate) fn gpui_agents_hub_is_skill_file(candidate: &Path) -> bool {
    gpui_file_name_string(candidate) == "SKILL.md"
        || matches!(
            gpui_agents_hub_extension(candidate).as_str(),
            ".json" | ".yaml" | ".yml" | ".sh" | ".py" | ".js" | ".ts"
        )
}

pub(crate) fn gpui_agents_hub_is_text_file(candidate: &Path) -> bool {
    matches!(
        gpui_agents_hub_extension(candidate).as_str(),
        ".md"
            | ".json"
            | ".jsonl"
            | ".toml"
            | ".yaml"
            | ".yml"
            | ".sh"
            | ".ts"
            | ".js"
            | ".mjs"
            | ".py"
            | ".txt"
    )
}

pub(crate) fn gpui_agents_hub_plugin_root(plugin_file: &Path) -> Option<PathBuf> {
    let components = plugin_file.components().collect::<Vec<_>>();
    let file_name = gpui_file_name_string(plugin_file);
    if file_name == "SKILL.md" {
        for (index, component) in components.iter().enumerate() {
            if component.as_os_str() == std::ffi::OsStr::new("skills")
                && index + 1 < components.len()
            {
                return Some(gpui_path_from_components(&components[..=index + 1]));
            }
        }
    }
    for (index, component) in components.iter().enumerate() {
        if component.as_os_str() == std::ffi::OsStr::new(".codex-plugin")
            || component.as_os_str() == std::ffi::OsStr::new(".claude-plugin")
        {
            return Some(gpui_path_from_components(&components[..index]));
        }
    }
    None
}

pub(crate) fn gpui_path_from_components(components: &[std::path::Component<'_>]) -> PathBuf {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component.as_os_str());
    }
    path
}

pub(crate) fn gpui_agents_hub_language_for(file_path: &Path) -> &'static str {
    let name = gpui_file_name_string(file_path);
    let extension = gpui_agents_hub_extension(file_path);
    if name.ends_with(".yaml") || name.ends_with(".yml") {
        "yaml"
    } else {
        match extension.as_str() {
            ".json" | ".jsonl" => "json",
            ".toml" => "toml",
            ".sh" => "shell",
            ".py" => "python",
            ".ts" | ".tsx" => "typescript",
            ".js" | ".mjs" | ".cjs" => "javascript",
            ".md" => "markdown",
            _ => "plaintext",
        }
    }
}

pub(crate) fn gpui_agents_hub_file_id(path: &Path) -> String {
    let mut id = String::new();
    let mut previous_dash = false;
    for character in gpui_path_string(path).to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            previous_dash = false;
        } else if !previous_dash && !id.is_empty() {
            id.push('-');
            previous_dash = true;
        }
        if id.len() >= 180 {
            break;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id
}

pub(crate) fn gpui_agents_hub_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_lowercase()))
        .unwrap_or_default()
}

pub(crate) fn gpui_file_name_string(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn gpui_path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn gpui_relative_path_name(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(gpui_path_string)
        .filter(|value| !value.is_empty())
}

pub(crate) fn gpui_path_is_relative_to(candidate: &Path, root: &Path) -> bool {
    candidate == root || candidate.strip_prefix(root).is_ok()
}

pub(crate) fn gpui_settings_action_status_message(
    action: &str,
    available: bool,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "available": available,
        "generatedAt": gpui_status_generated_at(),
        "message": message,
        "type": "settingsActionStatus",
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_create_local_project_workspace_agent(
    project_id: &str,
    agent_id: &str,
) -> Result<
    (
        GpuiLocalWorkspaceSessionKey,
        GpuiLocalWorkspaceAttachTerminalPlan,
    ),
    String,
> {
    if !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_agent_id_allowed(agent_id)
    {
        return Err("The selected project agent is unavailable.".to_string());
    }
    let result = gpui_gxserver_rpc_result(
        "/api/createAgentSession",
        &serde_json::json!({
            "agentId": agent_id,
            "projectId": project_id,
            "requireLaunchCommand": true,
            "surface": "workspace",
        }),
        Duration::from_secs(15),
    )?;
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not create an agent session.".to_string())?;
    let created_project_id = gpui_trimmed_json_string_field(session, "projectId")
        .unwrap_or(project_id)
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "gxserver did not return an agent session id.".to_string())?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("gxserver returned an invalid agent session id.".to_string());
    }
    let key = GpuiLocalWorkspaceSessionKey {
        project_id: created_project_id,
        session_id,
    };
    match gpui_prepare_local_workspace_attach_terminal_plan(
        &key,
        GpuiLocalWorkspaceAttachIntent::Attach,
    ) {
        Ok(plan) => Ok((key, plan)),
        Err(message) => {
            gpui_close_command_terminal_gxserver_session(&key);
            Err(message)
        }
    }
}

pub(crate) fn gpui_workspace_attach_agent_icon(
    attach: &serde_json::Map<String, serde_json::Value>,
) -> Option<&'static str> {
    let session = attach.get("session").and_then(serde_json::Value::as_object);
    let candidate = session
        .and_then(|session| json_string_field(session, "agentIcon"))
        .or_else(|| session.and_then(|session| json_string_field(session, "agentName")))
        .or_else(|| session.and_then(|session| json_string_field(session, "agentId")))
        .or_else(|| json_string_field(attach, "agentIcon"))
        .or_else(|| json_string_field(attach, "agentName"))
        .or_else(|| json_string_field(attach, "agentId"));
    gpui_sidebar_agent_icon(candidate)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentsWorkspaceNewTerminalPlacement {
    Tab,
    SplitRight,
    SplitBelow,
    BottomRow,
}

/*
CDXC:GPUIRemoteNewTerminal 2026-08-18:
`open_gpui_remote_attach_terminal` serves two different intents and they need
different completion rules. Attaching a session that already existed must not
yank the workspace back if the user focused something else while the SSH plan
was prepared in the background. A session this very action just created on the
remote daemon has nothing to drift away from: dropping it there produced no tab
and no toast, and left an orphaned terminal row on the remote machine, which is
exactly what made the sidebar project-header Create Terminal button look inert.
*/
pub(crate) fn gpui_uninstall_bundled_agent_skills() -> Result<String, String> {
    /*
    CDXC:GPUISettingsAgentSkills 2026-06-24-12:56:
    The bundled skill uninstall action is intentionally narrower than a generic skills cleanup. Remove only the catalog-owned shared Ghostex skill names under `~/.agents/skills`, handle missing folders as already-uninstalled state, and do not follow React-provided paths or delete other user/system skills.
    */
    let mut removed_count = 0usize;
    for skill_name in GPUI_BUNDLED_GHOSTEX_AGENT_SKILL_NAMES {
        removed_count += usize::from(gpui_uninstall_bundled_agent_skill(skill_name)?);
    }
    if removed_count == 0 {
        Ok("No bundled Ghostex agent skills were installed. Current integration status was refreshed.".to_string())
    } else {
        Ok(
            "Bundled Ghostex agent skills uninstalled. You can install them again from Settings."
                .to_string(),
        )
    }
}

pub(crate) fn gpui_uninstall_bundled_agent_skill(skill_name: &str) -> Result<bool, String> {
    if !GPUI_BUNDLED_GHOSTEX_AGENT_SKILL_NAMES.contains(&skill_name) {
        return Err(
            "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
                .to_string(),
        );
    }
    let skill_path = gpui_home_dir()
        .join(".agents")
        .join("skills")
        .join(skill_name);
    let metadata = match fs::symlink_metadata(&skill_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => {
            return Err(
                "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
                    .to_string(),
            );
        }
    };
    let remove_result = if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(&skill_path)
    } else if metadata.is_dir() {
        fs::remove_dir_all(&skill_path)
    } else {
        fs::remove_file(&skill_path)
    };
    remove_result.map(|_| true).map_err(|_| {
        "Bundled Ghostex skill uninstall failed. Current integration status was refreshed."
            .to_string()
    })
}

pub(crate) const GPUI_AGENT_HOOK_PRIORITY_STATUS_AGENT_IDS: [&str; 4] = ["codex", "claude", "opencode", "pi"];

pub(crate) fn gpui_ordered_agent_hook_status_agent_ids(requested: Option<Vec<String>>) -> Vec<String> {
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayStatusItemState {
    pub(crate) count: u64,
    pub(crate) status: GpuiStatusIndicatorStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayActivityState {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) state: GpuiStatusIndicatorStatus,
    pub(crate) title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiPetOverlayAnimationState {
    Idle,
    Jumping,
    Review,
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiPetOverlayAnimationFrame {
    pub(crate) column_index: u8,
    pub(crate) duration_ms: u64,
    pub(crate) row_index: u8,
}

pub(crate) const GPUI_PET_OVERLAY_IDLE_FRAMES: [GpuiPetOverlayAnimationFrame; 6] = [
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 0,
        duration_ms: 280,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 1,
        duration_ms: 110,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 2,
        duration_ms: 110,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 3,
        duration_ms: 140,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 4,
        duration_ms: 140,
    },
    GpuiPetOverlayAnimationFrame {
        row_index: 0,
        column_index: 5,
        duration_ms: 320,
    },
];
pub(crate) const GPUI_PET_OVERLAY_JUMPING_FRAMES: [GpuiPetOverlayAnimationFrame; 5] =
    gpui_pet_overlay_row_frames::<5>(4, 140, 280);
pub(crate) const GPUI_PET_OVERLAY_REVIEW_FRAMES: [GpuiPetOverlayAnimationFrame; 6] =
    gpui_pet_overlay_row_frames::<6>(8, 150, 280);
pub(crate) const GPUI_PET_OVERLAY_RUNNING_FRAMES: [GpuiPetOverlayAnimationFrame; 6] =
    gpui_pet_overlay_row_frames::<6>(7, 120, 220);

pub(crate) static GPUI_PET_OVERLAY_BOO_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_BSOD_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_CODEX_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_DEWEY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_FIREBALL_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_NULL_SIGNAL_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_ROCKY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_SEEDY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
pub(crate) static GPUI_PET_OVERLAY_STACKY_IMAGE: OnceLock<Arc<Image>> = OnceLock::new();

pub(crate) fn gpui_pet_overlay_status_item_from_value(
    value: &serde_json::Value,
) -> Result<GpuiPetOverlayStatusItemState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["status", "count"])?;
    Ok(GpuiPetOverlayStatusItemState {
        count: gpui_status_count_field(object, "count")?,
        status: gpui_status_field(object, "status")?,
    })
}

pub(crate) fn gpui_pet_overlay_activity_from_value(
    value: &serde_json::Value,
) -> Result<GpuiPetOverlayActivityState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["id", "projectId", "state", "title"])?;
    Ok(GpuiPetOverlayActivityState {
        project_id: gpui_status_id_field(object, "projectId")?,
        session_id: gpui_status_id_field(object, "id")?,
        state: gpui_status_field(object, "state")?,
        title: gpui_status_title_field(object, "title")?,
    })
}

pub(crate) fn gpui_pet_overlay_selected_pet_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    let pet_id = gpui_status_id_field(object, key)?;
    if gpui_pet_overlay_pet_id_known(pet_id.as_str()) {
        Ok(pet_id)
    } else {
        Err(())
    }
}

pub(crate) fn reject_unexpected_gpui_status_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed_keys: &[&str],
) -> Result<(), ()> {
    if object
        .keys()
        .any(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(());
    }
    Ok(())
}

pub(crate) fn gpui_status_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or(())
}

pub(crate) fn gpui_status_count_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<u64, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value <= u64::from(u16::MAX))
        .ok_or(())
}

pub(crate) fn gpui_status_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<GpuiStatusIndicatorStatus, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiStatusIndicatorStatus::from_str)
        .ok_or(())
}

pub(crate) fn gpui_status_id_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_bridge_id_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, ()> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_status_title_allowed(value))
        .map(str::to_string)
        .ok_or(())
}

pub(crate) fn gpui_status_optional_title_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(_) => gpui_status_title_field(object, key).map(Some),
    }
}

pub(crate) fn gpui_status_optional_icon_data_url_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ()> {
    /*
    CDXC:GPUISettingsNotifications 2026-06-26-07:22:
    GPUI session-attention icons may enter Rust only as bounded image data URLs on sanitized status project rows. Reject paths, http(s) URLs, control characters, oversized strings, malformed headers, and unexpected keys so native notification attachment creation never receives a fallback file/URL source or raw renderer payload.
    */
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_status_icon_data_url_allowed(value))
            .map(str::to_string)
            .map(Some)
            .ok_or(()),
    }
}

pub(crate) fn gpui_status_bridge_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_status_icon_data_url_allowed(value: &str) -> bool {
    if value.is_empty()
        || value.len() > GPUI_STATUS_INDICATOR_ICON_DATA_URL_MAX_CHARS
        || !value.is_ascii()
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return false;
    }
    let Some(comma_index) = value.find(',') else {
        return false;
    };
    let header = value[..comma_index].to_ascii_lowercase();
    let payload = &value[comma_index + 1..];
    header.starts_with("data:image/")
        && header.contains(";base64")
        && gpui_status_icon_base64_payload_allowed(payload)
}

pub(crate) fn gpui_status_icon_base64_payload_allowed(payload: &str) -> bool {
    if payload.is_empty() || payload.len() % 4 != 0 {
        return false;
    }
    let padding_index = payload.find('=').unwrap_or(payload.len());
    let padding_len = payload.len().saturating_sub(padding_index);
    padding_len <= 2
        && payload[padding_index..].bytes().all(|byte| byte == b'=')
        && payload[..padding_index]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
}

pub(crate) fn gpui_status_title_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_pet_overlay_activity_activation_session_id(activity: &GpuiPetOverlayActivityState) -> &str {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-05:30:
    Pet activity cards activate the exact sanitized activity `id` supplied by the sidebar bridge. The GPUI click payload must not include project ids, titles, paths, URLs, commands, logs, or terminal content, and it must not derive a different target from the visible card text.
    */
    activity.session_id.as_str()
}

pub(crate) fn gpui_status_indicator_aggregate_activation_session_id(
    state: &GpuiSidebarSessionStatusIndicatorsState,
    status: GpuiStatusIndicatorStatus,
    focused_session_id: Option<&str>,
) -> Option<String> {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
    Aggregate GPUI status clicks must mirror macOS ordering from the sanitized sidebar payload: sort same-status candidates by lastActiveAt descending, then sidebarOrder. If the focused session is already the top candidate and another candidate exists, cycle to the next candidate instead of reselecting the same row.
    */
    let mut candidates = state
        .projects
        .iter()
        .flat_map(|project| project.sessions.iter())
        .filter(|session| session.status == status)
        .collect::<Vec<_>>();
    candidates.sort_by(gpui_status_indicator_activation_candidate_order);
    if candidates.is_empty() {
        return None;
    }
    if let Some(focused_session_id) = focused_session_id {
        if candidates.len() > 1 {
            if let Some(index) = candidates.iter().position(|session| {
                gpui_status_indicator_session_matches_focus(
                    session.session_id.as_str(),
                    focused_session_id,
                )
            }) {
                return Some(
                    candidates[(index + 1) % candidates.len()]
                        .session_id
                        .clone(),
                );
            }
        }
    }
    Some(candidates[0].session_id.clone())
}

pub(crate) fn gpui_status_indicator_activation_candidate_order(
    left: &&GpuiStatusIndicatorSessionState,
    right: &&GpuiStatusIndicatorSessionState,
) -> std::cmp::Ordering {
    let left_timestamp = left.last_active_at.as_deref().unwrap_or("");
    let right_timestamp = right.last_active_at.as_deref().unwrap_or("");
    right_timestamp
        .cmp(left_timestamp)
        .then_with(|| left.order.cmp(&right.order))
}

pub(crate) fn gpui_status_indicator_session_matches_focus(session_id: &str, focused_session_id: &str) -> bool {
    if session_id == focused_session_id {
        return true;
    }
    if gpui_local_combined_session_original_id(session_id)
        .as_deref()
        .is_some_and(|original_session_id| original_session_id == focused_session_id)
    {
        return true;
    }
    gpui_local_combined_session_original_id(focused_session_id)
        .as_deref()
        .is_some_and(|original_session_id| original_session_id == session_id)
}

pub(crate) fn gpui_local_combined_session_original_id(session_id: &str) -> Option<String> {
    const PREFIX: &str = "combined-session:";
    let payload = session_id.strip_prefix(PREFIX)?;
    let (_project_id, encoded_session_id) = payload.split_once(':')?;
    let decoded =
        browser_favicon_percent_decode(encoded_session_id, GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
            .and_then(|bytes| String::from_utf8(bytes).ok())?;
    (!decoded.is_empty()
        && decoded.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !decoded.chars().any(char::is_control))
    .then_some(decoded)
}

pub(crate) fn gpui_status_pet_activation_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onStatusPetActivation==='function'){{bridge.onStatusPetActivation(payload);}}else{{const pending=Array.isArray(bridge.pendingStatusPetActivations)?bridge.pendingStatusPetActivations:[];pending.push(payload);bridge.pendingStatusPetActivations=pending;}}}})(); undefined;"
    )
}

