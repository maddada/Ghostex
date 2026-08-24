use std::{
    fs,
    path::{Path, PathBuf},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.
use std::cell::RefCell;

use gpui::{Bounds, Window, WindowBounds, px, size};

use crate::app::helpers::*;

pub(crate) const GPUI_WINDOW_FRAME_STATE_VERSION: u64 = 1;
pub(crate) const GPUI_WINDOW_FRAME_MIN_WIDTH: f32 = 800.0;
pub(crate) const GPUI_WINDOW_FRAME_MIN_HEIGHT: f32 = 600.0;

/// Window frame persistence (macOS `persistMainWindowChrome` /
/// `restoredInitialWindowFrame` parity): the frame is stored as a display
/// uuid plus a display-relative origin so a moved or removed monitor restores
/// onto an existing display instead of offscreen.
#[derive(Clone, PartialEq)]
pub(crate) struct GpuiWindowFrameState {
    pub(crate) state: String,
    pub(crate) display_uuid: String,
    pub(crate) relative_origin_x: f32,
    pub(crate) relative_origin_y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

thread_local! {
    // The latest observed frame lives process-locally so quit and
    // last-window-close persistence never need the (possibly already
    // dropped) app entity.
    pub(crate) static GPUI_LATEST_WINDOW_FRAME_STATE: RefCell<Option<GpuiWindowFrameState>> =
        const { RefCell::new(None) };
}

pub(crate) fn gpui_window_frame_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-window-frame-state.json")
}

pub(crate) fn gpui_window_frame_state_from_window(
    window: &Window,
    cx: &gpui::App,
) -> Option<GpuiWindowFrameState> {
    let display = window.display(cx)?;
    let display_origin = display.bounds().origin;
    let display_uuid = display.uuid().ok()?.to_string();
    let (state, bounds) = match window.window_bounds() {
        WindowBounds::Windowed(bounds) => ("windowed", bounds),
        WindowBounds::Maximized(bounds) => ("maximized", bounds),
        WindowBounds::Fullscreen(bounds) => ("fullscreen", bounds),
    };
    Some(GpuiWindowFrameState {
        state: state.to_string(),
        display_uuid,
        relative_origin_x: (bounds.origin.x - display_origin.x).as_f32(),
        relative_origin_y: (bounds.origin.y - display_origin.y).as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    })
}

pub(crate) fn record_gpui_window_frame_state(window: &Window, cx: &gpui::App) {
    let Some(state) = gpui_window_frame_state_from_window(window, cx) else {
        return;
    };
    GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| {
        *latest.borrow_mut() = Some(state);
    });
}

pub(crate) fn persist_gpui_window_frame_state() {
    let Some(state) = GPUI_LATEST_WINDOW_FRAME_STATE.with(|latest| latest.borrow().clone()) else {
        return;
    };
    write_gpui_window_frame_state_file(&gpui_window_frame_state_path(), &state);
}

pub(crate) fn write_gpui_window_frame_state_file(path: &Path, state: &GpuiWindowFrameState) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "displayUuid": state.display_uuid,
        "height": state.height,
        "relativeOriginX": state.relative_origin_x,
        "relativeOriginY": state.relative_origin_y,
        "state": state.state,
        "version": GPUI_WINDOW_FRAME_STATE_VERSION,
        "width": state.width,
    });
    let _ = fs::write(path, payload.to_string());
}

pub(crate) fn load_gpui_window_frame_state() -> Option<GpuiWindowFrameState> {
    load_gpui_window_frame_state_file(&gpui_window_frame_state_path())
}

pub(crate) fn load_gpui_window_frame_state_file(path: &Path) -> Option<GpuiWindowFrameState> {
    let text = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    if value.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_WINDOW_FRAME_STATE_VERSION)
    {
        return None;
    }
    let number = |key: &str| -> Option<f32> {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|number| number.is_finite())
            .map(|number| number as f32)
    };
    let state = value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .filter(|state| matches!(*state, "windowed" | "maximized" | "fullscreen"))?
        .to_string();
    Some(GpuiWindowFrameState {
        state,
        display_uuid: value
            .get("displayUuid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        relative_origin_x: number("relativeOriginX")?,
        relative_origin_y: number("relativeOriginY")?,
        width: number("width")?,
        height: number("height")?,
    })
}

/// Restores the persisted frame with the macOS multi-monitor rules: prefer
/// the stored display by uuid, fall back to the primary display, clamp the
/// size to the display and a minimum, and keep the origin inside the display.
pub(crate) fn restored_gpui_window_bounds(cx: &gpui::App) -> Option<WindowBounds> {
    let state = load_gpui_window_frame_state()?;
    restored_gpui_window_bounds_from_state(
        state,
        GPUI_WINDOW_FRAME_MIN_WIDTH,
        GPUI_WINDOW_FRAME_MIN_HEIGHT,
        cx,
    )
}

pub(crate) fn restored_gpui_window_bounds_from_state(
    state: GpuiWindowFrameState,
    min_width: f32,
    min_height: f32,
    cx: &gpui::App,
) -> Option<WindowBounds> {
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|display| {
            display
                .uuid()
                .ok()
                .is_some_and(|uuid| uuid.to_string() == state.display_uuid)
        })
        .cloned()
        .or_else(|| cx.primary_display())
        .or_else(|| displays.first().cloned())?;
    let display_bounds = display.bounds();
    let width = px(state
        .width
        .max(min_width)
        .min(display_bounds.size.width.as_f32()));
    let height = px(state
        .height
        .max(min_height)
        .min(display_bounds.size.height.as_f32()));
    let max_x = (display_bounds.size.width - width).max(px(0.0));
    let max_y = (display_bounds.size.height - height).max(px(0.0));
    let origin = gpui::point(
        display_bounds.origin.x + px(state.relative_origin_x).clamp(px(0.0), max_x),
        display_bounds.origin.y + px(state.relative_origin_y).clamp(px(0.0), max_y),
    );
    let bounds = Bounds::new(origin, size(width, height));
    Some(match state.state.as_str() {
        "maximized" => WindowBounds::Maximized(bounds),
        "fullscreen" => WindowBounds::Fullscreen(bounds),
        _ => WindowBounds::Windowed(bounds),
    })
}

pub(crate) fn gpui_first_run_onboarding_state_path() -> PathBuf {
    ghostex_state_root().join("gpui-first-run-onboarding-state.json")
}

/// Keep first-run onboarding in GPUI-owned state rather than coupling native
/// onboarding lifecycle to any CEF profile or page-local storage.
#[derive(Clone, Default)]
pub(crate) struct GpuiFirstRunOnboardingState {
    pub(crate) tips_and_tricks_seen: bool,
    pub(crate) highlighted_features_seen_revision: Option<String>,
    pub(crate) first_launch_setup_seen_revision: Option<String>,
    pub(crate) os_integration_onboarding_seen: bool,
    pub(crate) first_launch_setup_complete: bool,
    pub(crate) windows_terminal_setup_complete: bool,
}

/// CDXC:GPUIFirstRunOnboardingOnce 2026-08-18: the markers whose surfaces are
/// user-visible, so they are written only after that surface is on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiFirstRunOnboardingMarker {
    FirstLaunchSetupSeen,
    OsIntegrationOnboardingSeen,
}

pub(crate) fn load_gpui_first_run_onboarding_state() -> GpuiFirstRunOnboardingState {
    let Some(value) = fs::read_to_string(gpui_first_run_onboarding_state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return GpuiFirstRunOnboardingState::default();
    };
    GpuiFirstRunOnboardingState {
        tips_and_tricks_seen: value
            .get("tipsAndTricksSeen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        highlighted_features_seen_revision: value
            .get("highlightedFeaturesSeenRevision")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        first_launch_setup_seen_revision: value
            .get("firstLaunchSetupSeenRevision")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        os_integration_onboarding_seen: value
            .get("osIntegrationOnboardingSeen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        first_launch_setup_complete: value
            .get("firstLaunchSetupComplete")
            // Written under the Windows-only name until 2026-08-19.
            .or_else(|| value.get("windowsFirstLaunchSetupComplete"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        windows_terminal_setup_complete: value
            .get("windowsTerminalSetupComplete")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

pub(crate) fn persist_gpui_first_run_onboarding_state(state: &GpuiFirstRunOnboardingState) {
    let path = gpui_first_run_onboarding_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "firstLaunchSetupSeenRevision": state.first_launch_setup_seen_revision,
        "highlightedFeaturesSeenRevision": state.highlighted_features_seen_revision,
        "osIntegrationOnboardingSeen": state.os_integration_onboarding_seen,
        "tipsAndTricksSeen": state.tips_and_tricks_seen,
        "firstLaunchSetupComplete": state.first_launch_setup_complete,
        "windowsTerminalSetupComplete": state.windows_terminal_setup_complete,
    });
    let _ = fs::write(path, payload.to_string());
}
