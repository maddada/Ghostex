/*
CDXC:CefRuntime 2026-07-04:
macOS platform adapter for the shared windowed-CEF backend (cef/shell.rs).
This module owns only the truly per-OS pieces: loading the CEF framework
from the app bundle, the AppKit CefAppProtocol/message-pump shim glue, and
NSView child-view frame/visibility/focus operations. All browser/bridge/
runtime logic stays OS-agnostic in cef/shell.rs. Handles cross this seam as
opaque `*mut c_void`; only this file treats them as NSView pointers.
*/

use anyhow::{Context as _, Result};
use std::{
    ffi::{CString, c_double, c_int, c_longlong, c_void},
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

pub(crate) const CEF_FRAMEWORK_EXECUTABLE_ENV: &str = "GHOSTEX_CEF_FRAMEWORK_EXECUTABLE";
const CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH: &str =
    "Chromium Embedded Framework.framework/Chromium Embedded Framework";

unsafe extern "C" {
    fn GhostexGpuiCEFPrepareApplication();
    fn GhostexGpuiCEFSystemUsesDarkPageAppearance() -> bool;
    fn GhostexGpuiCEFInstallApplicationHooks();
    fn GhostexGpuiCEFInstallMessagePump();
    fn GhostexGpuiCEFInvalidateMessagePump();
    fn GhostexGpuiCEFScheduleMessagePumpWork(delay_ms: c_longlong);
    fn GhostexGpuiCEFSetNativeViewFrame(
        native_view: *mut c_void,
        x: c_double,
        y: c_double,
        width: c_double,
        height: c_double,
    );
    fn GhostexGpuiCEFLogResizeDiagnostic(
        browser_id: c_int,
        width: c_int,
        height: c_int,
        frame_us: u64,
        was_resized_us: u64,
        total_us: u64,
    );
    fn GhostexGpuiCEFSetNativeViewVisible(native_view: *mut c_void, visible: bool);
    fn GhostexGpuiCEFOrderNativeViewFront(native_view: *mut c_void);
    fn GhostexGpuiCEFRemoveNativeViewFromSuperview(native_view: *mut c_void);
    fn GhostexGpuiCEFPrepareNativeViewForFocus(native_view: *mut c_void);
    fn GhostexGpuiCEFSetNativeViewMouseFocusPassive(native_view: *mut c_void, passive: bool);
    fn GhostexGpuiCEFSetNativeViewPassiveFocusGrant(native_view: *mut c_void, granted: bool);
    fn GhostexGpuiCEFReturnFocusToGpuiRootFromNativeView(native_view: *mut c_void);
    fn GhostexGpuiCEFFocusNativeView(native_view: *mut c_void);
    fn GhostexGpuiCEFActivateNativeViewWindow(native_view: *mut c_void);
    fn GhostexGpuiCEFFocusGpuiRootView(native_view: *mut c_void);
    fn GhostexGpuiInstallFirstResponderObserverForNativeView(native_view: *mut c_void);
    fn GhostexGpuiNativeViewContainsResponder(
        root_native_view: *mut c_void,
        responder: *mut c_void,
    ) -> bool;
    fn GhostexGpuiCEFNativeViewOwnsFirstResponder(native_view: *mut c_void) -> bool;
    fn GhostexGpuiCEFSetSidebarPointerTrackingView(native_view: *mut c_void);
    fn GhostexGpuiCEFReportSidebarPointerOutside();
    fn GhostexGpuiCEFRefreshSidebarPointerInside();
}

/// Keeps the CEF framework loaded for the lifetime of the CEF runtime.
pub(super) struct PlatformCefRuntime {
    path: PathBuf,
}

pub(super) fn load_cef_runtime() -> Result<PlatformCefRuntime> {
    let executable = std::env::current_exe().context("failed to resolve GPUI executable path")?;
    let path = resolve_cef_framework_executable(&executable)?;
    let c_path = CString::new(path.as_os_str().as_bytes())
        .context("CEF framework path contains an embedded NUL byte")?;
    let loaded = unsafe { cef::load_library(Some(&*c_path.as_ptr().cast())) };
    if loaded != 1 {
        anyhow::bail!("CEF framework could not be loaded from {}", path.display());
    }
    Ok(PlatformCefRuntime { path })
}

impl Drop for PlatformCefRuntime {
    fn drop(&mut self) {
        if cef::unload_library() != 1 {
            eprintln!("could not unload CEF framework {}", self.path.display());
        }
    }
}

fn resolve_cef_framework_executable(executable: &Path) -> Result<PathBuf> {
    let executable_dir = executable
        .parent()
        .context("GPUI executable has no parent directory")?;
    for bundled in [
        executable_dir
            .join("../Frameworks")
            .join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH),
        executable_dir
            .join("../../..")
            .join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH),
    ] {
        if bundled.is_file() {
            return bundled.canonicalize().with_context(|| {
                format!("failed to resolve bundled CEF at {}", bundled.display())
            });
        }
    }

    let configured = std::env::var_os(CEF_FRAMEWORK_EXECUTABLE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("verified external CEF framework path was not configured")?;
    if !configured.is_file() {
        anyhow::bail!(
            "verified external CEF framework executable is missing at {}",
            configured.display()
        );
    }
    configured
        .canonicalize()
        .with_context(|| format!("failed to resolve external CEF at {}", configured.display()))
}

pub(super) fn prepare_application() {
    unsafe {
        GhostexGpuiCEFPrepareApplication();
    }
}

pub(super) fn system_uses_dark_page_appearance() -> bool {
    unsafe { GhostexGpuiCEFSystemUsesDarkPageAppearance() }
}

pub(super) fn install_application_hooks() {
    unsafe { GhostexGpuiCEFInstallApplicationHooks() };
}

pub(super) fn install_message_pump(_cx: &gpui::App) {
    // The GPUI app context is unused here: dispatch_async onto the AppKit
    // main queue is the OS-level main-thread scheduler, so the pump needs no
    // gpui executor (unlike Linux, where gpui's foreground executor is the
    // only way into the main event loop).
    unsafe { GhostexGpuiCEFInstallMessagePump() };
}

pub(super) fn invalidate_message_pump() {
    unsafe { GhostexGpuiCEFInvalidateMessagePump() };
}

pub(super) fn schedule_message_pump_work(delay_ms: i64) {
    unsafe {
        GhostexGpuiCEFScheduleMessagePumpWork(delay_ms as c_longlong);
    }
}

pub(super) fn apply_platform_settings(settings: &mut cef::Settings) {
    let Some(framework_executable) = std::env::var_os(CEF_FRAMEWORK_EXECUTABLE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        // Bundled development builds keep CEF's default app-relative lookup.
        return;
    };
    let framework = framework_executable
        .parent()
        .expect("verified CEF framework executable has no parent directory");
    settings.framework_dir_path = cef::CefString::from(framework.to_string_lossy().as_ref());
}

pub(super) fn append_platform_command_line_switches(command_line: &mut cef::CommandLine) {
    use cef::ImplCommandLine as _;

    /*
    Chromium's MacAppCodeSignClone is a Chrome-updater safeguard: every browser
    launch APFS-clones the complete app bundle under /private/var/folders/.../X
    and relies on an exit helper to remove it. Ghostex updates through Sparkle,
    so the clone has no purpose here; crashes, forced development relaunches,
    and updater exits can skip Chromium's cleanup and retain gigabytes per
    launch until reboot. Disable creation at Chromium feature initialization
    instead of trying to sweep another process's potentially-live clone later.

    Preserve any disabled features already supplied by CEF or the caller.
    */
    const CODE_SIGN_CLONE_FEATURE: &str = "MacAppCodeSignClone";
    let switch_name = cef::CefString::from("disable-features");
    let existing = command_line.switch_value(Some(&switch_name));
    let existing = cef::CefString::from(&existing).to_string();
    if existing
        .split(',')
        .map(str::trim)
        .any(|feature| feature == CODE_SIGN_CLONE_FEATURE)
    {
        return;
    }
    let disabled_features = if existing.trim().is_empty() {
        CODE_SIGN_CLONE_FEATURE.to_string()
    } else {
        format!(
            "{},{}",
            existing.trim_end_matches(','),
            CODE_SIGN_CLONE_FEATURE
        )
    };
    command_line.append_switch_with_value(
        Some(&switch_name),
        Some(&cef::CefString::from(disabled_features.as_str())),
    );
}

pub(super) fn child_window_info(
    parent_native_view: *mut c_void,
    bounds: &cef::Rect,
) -> cef::WindowInfo {
    cef::WindowInfo::default().set_as_child(parent_native_view.cast(), bounds)
}

pub(super) fn native_view_ptr(handle: cef::sys::cef_window_handle_t) -> *mut c_void {
    handle.cast()
}

pub(super) fn prepare_native_view_for_focus(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFPrepareNativeViewForFocus(native_view);
    }
}

pub(super) fn set_native_view_mouse_focus_passive(native_view: *mut c_void, passive: bool) {
    unsafe {
        GhostexGpuiCEFSetNativeViewMouseFocusPassive(native_view, passive);
    }
}

pub(super) fn set_native_view_passive_focus_grant(native_view: *mut c_void, granted: bool) {
    unsafe {
        GhostexGpuiCEFSetNativeViewPassiveFocusGrant(native_view, granted);
    }
}

pub(super) fn return_focus_to_gpui_root(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFReturnFocusToGpuiRootFromNativeView(native_view);
    }
}

pub(super) fn set_native_view_frame(
    native_view: *mut c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    _scale_factor: f32,
) {
    // AppKit child views are positioned in points (logical pixels); the
    // backing scale is AppKit's own concern, so the GPUI scale factor from
    // the shared shell is intentionally unused here.
    unsafe {
        GhostexGpuiCEFSetNativeViewFrame(
            native_view,
            x as c_double,
            y as c_double,
            width as c_double,
            height as c_double,
        );
    }
}

pub(super) fn log_resize_diagnostic(
    browser_id: i32,
    width: i32,
    height: i32,
    frame_us: u64,
    was_resized_us: u64,
    total_us: u64,
) {
    unsafe {
        GhostexGpuiCEFLogResizeDiagnostic(
            browser_id,
            width,
            height,
            frame_us,
            was_resized_us,
            total_us,
        );
    }
}

pub(super) fn set_native_view_visible(native_view: *mut c_void, visible: bool) {
    unsafe {
        GhostexGpuiCEFSetNativeViewVisible(native_view, visible);
    }
}

pub(super) fn focus_native_view(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFFocusNativeView(native_view);
    }
}

pub(super) fn activate_native_view_window(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFActivateNativeViewWindow(native_view);
    }
}

pub(super) fn focus_gpui_root_view(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFFocusGpuiRootView(native_view);
    }
}

pub(super) fn set_sidebar_pointer_tracking_view(native_view: *mut c_void) {
    unsafe { GhostexGpuiCEFSetSidebarPointerTrackingView(native_view) }
}

pub(super) fn report_sidebar_pointer_outside() {
    unsafe { GhostexGpuiCEFReportSidebarPointerOutside() }
}

pub(super) fn refresh_sidebar_pointer_inside() {
    unsafe { GhostexGpuiCEFRefreshSidebarPointerInside() }
}

pub(super) fn native_view_owns_first_responder(native_view: *mut c_void) -> bool {
    unsafe { GhostexGpuiCEFNativeViewOwnsFirstResponder(native_view) }
}

pub(super) fn order_native_view_front(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFOrderNativeViewFront(native_view);
    }
}

pub(super) fn release_native_view(native_view: *mut c_void) {
    /*
    CDXC:CefRuntime 2026-08-24:
    GhostexGpuiLifeSpanHandler::do_close returns handled so CEF never sends a
    native close to the host GPUI window. Per cef_life_span_handler.h's DoClose
    docs, the app is then still required to complete the close by proceeding
    with window/view-hierarchy tear-down; skipping it leaves the browser
    partially closed. This adapter used to no-op here on the claim that "CEF
    owns the child NSView lifecycle on macOS" — that claim was the leak: the
    CEF child view stayed a subview of the long-lived GPUI content view
    forever, on_before_close never fired, and every closed surface kept a live
    renderer subprocess (measured at tens of GiB over a long session).
    Removing the view from its superview drops its only strong reference and
    performs exactly the tear-down CEF is waiting for, mirroring what the Linux
    adapter does with its embed-host X window.
    */
    unsafe {
        GhostexGpuiCEFRemoveNativeViewFromSuperview(native_view);
    }
}

pub(super) fn install_first_responder_observer(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiInstallFirstResponderObserverForNativeView(native_view);
    }
}

pub(super) fn native_view_contains_responder(
    root_native_view: *mut c_void,
    responder: *mut c_void,
) -> bool {
    unsafe { GhostexGpuiNativeViewContainsResponder(root_native_view, responder) }
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFDoMessageLoopWork() {
    cef::do_message_loop_work();
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFHandleSelectAllForNativeView(native_view: *mut c_void) -> c_int {
    /*
    CDXC:Hotkeys 2026-06-14-17:25:
    Native AppKit command dispatch can reach CEF's NSView even when GPUI still remembers the address input as its focused element. Keep a main-thread native-view to cef-rs browser registry so the standard selectAll: command can call Chromium's Frame::select_all for the focused page field instead of selecting GPUI chrome.
    */
    super::shell::select_all_for_native_view(native_view)
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFHandleSelectAllForActiveNativeView() -> c_int {
    super::shell::select_all_for_active_native_view()
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFHandleEditCommandForNativeView(
    native_view: *mut c_void,
    command: c_int,
) -> c_int {
    /*
    CDXC:Hotkeys 2026-07-09:
    Cut/Copy/Paste use the same native-view-to-browser registry as Select
    All so the AppKit responder-chain shim can route standard clipboard
    commands to Chromium's Frame edit actions for whichever CEF surface
    (settings, modal-host, sidebar, browser page) owns the first responder.
    */
    let Some(command) = super::shell::CefEditCommand::from_raw(command) else {
        return 0;
    };
    super::shell::edit_command_for_native_view(native_view, command)
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFLogClipboardRoute(
    command: c_int,
    bridged_during_dispatch: c_int,
    responder_walk_handled: c_int,
    responder_class: *const std::ffi::c_char,
) {
    let responder_class = if responder_class.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit supplies object_getClassName storage that remains
        // valid for this synchronous diagnostic callback.
        unsafe { std::ffi::CStr::from_ptr(responder_class) }
            .to_string_lossy()
            .into_owned()
    };
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.clipboardAppKitRoute",
        serde_json::json!({
            "command": command,
            "bridgedDuringDispatch": bridged_during_dispatch != 0,
            "responderWalkHandled": responder_walk_handled != 0,
            "responderClass": responder_class,
        }),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFLogDevToolsWindowActivation(
    window_is_key: c_int,
    responder_inside_native_view: c_int,
    responder_class: *const std::ffi::c_char,
) {
    let responder_class = if responder_class.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit supplies object_getClassName storage that remains
        // valid for this synchronous diagnostic callback.
        unsafe { std::ffi::CStr::from_ptr(responder_class) }
            .to_string_lossy()
            .into_owned()
    };
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.devToolsWindowActivation",
        serde_json::json!({
            "windowIsKey": window_is_key != 0,
            "responderInsideNativeView": responder_inside_native_view != 0,
            "responderClass": responder_class,
        }),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFHandleZoomCommandForNativeView(
    native_view: *mut c_void,
    command: c_int,
) -> c_int {
    let Some(command) = super::shell::CefZoomCommand::from_raw(command) else {
        return 0;
    };
    super::shell::zoom_command_for_native_view(native_view, command)
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFMarkNativeViewFocused(native_view: *mut c_void) -> c_int {
    super::shell::mark_native_view_focused(native_view)
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFLogNativeMouseDown(
    native_view: *mut c_void,
    event_window_x: c_double,
    event_window_y: c_double,
    frame_window_x: c_double,
    frame_window_y: c_double,
    frame_width: c_double,
    frame_height: c_double,
    parent_bounds_width: c_double,
    parent_bounds_height: c_double,
    hidden: c_int,
    responder_class: *const std::ffi::c_char,
) {
    let responder_class = if responder_class.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit supplies object_getClassName storage that remains
        // valid for this synchronous diagnostic callback.
        unsafe { std::ffi::CStr::from_ptr(responder_class) }
            .to_string_lossy()
            .into_owned()
    };
    super::shell::log_native_mouse_down(
        native_view,
        event_window_x,
        event_window_y,
        frame_window_x,
        frame_window_y,
        frame_width,
        frame_height,
        parent_bounds_width,
        parent_bounds_height,
        hidden != 0,
        responder_class,
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFClearActiveNativeView() {
    super::shell::clear_active_native_view_registry()
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView(
    native_view: *mut c_void,
) -> c_int {
    super::shell::refresh_system_page_appearance_for_native_view(native_view)
}
