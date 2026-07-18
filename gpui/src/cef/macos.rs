/*
CDXC:GPUICefPlatformSeam 2026-07-04:
macOS platform adapter for the shared windowed-CEF backend (cef/shell.rs).
This module owns only the truly per-OS pieces: loading the CEF framework
from the app bundle, the AppKit CefAppProtocol/message-pump shim glue, and
NSView child-view frame/visibility/focus operations. All browser/bridge/
runtime logic stays OS-agnostic in cef/shell.rs. Handles cross this seam as
opaque `*mut c_void`; only this file treats them as NSView pointers.
*/

use anyhow::{Context as _, Result};
use std::ffi::{c_double, c_int, c_longlong, c_void};

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
    fn GhostexGpuiCEFPrepareNativeViewForFocus(native_view: *mut c_void);
    fn GhostexGpuiCEFFocusNativeView(native_view: *mut c_void);
    fn GhostexGpuiCEFActivateNativeViewWindow(native_view: *mut c_void);
    fn GhostexGpuiCEFFocusGpuiRootView(native_view: *mut c_void);
    fn GhostexGpuiInstallFirstResponderObserverForNativeView(native_view: *mut c_void);
    fn GhostexGpuiNativeViewContainsResponder(
        root_native_view: *mut c_void,
        responder: *mut c_void,
    ) -> bool;
    fn GhostexGpuiCEFNativeViewOwnsFirstResponder(native_view: *mut c_void) -> bool;
}

/// Keeps the CEF framework loaded for the lifetime of the CEF runtime.
pub(super) struct PlatformCefRuntime {
    _loader: cef::library_loader::LibraryLoader,
}

pub(super) fn load_cef_runtime() -> Result<PlatformCefRuntime> {
    let executable = std::env::current_exe().context("failed to resolve GPUI executable path")?;
    let loader = cef::library_loader::LibraryLoader::new(&executable, false);
    if !loader.load() {
        anyhow::bail!("CEF framework could not be loaded from the app bundle");
    }
    Ok(PlatformCefRuntime { _loader: loader })
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

pub(super) fn apply_platform_settings(_settings: &mut cef::Settings) {
    // macOS discovers helper apps from the bundle layout; nothing to add.
}

pub(super) fn append_platform_command_line_switches(_command_line: &mut cef::CommandLine) {
    // macOS needs no OS-specific Chromium switches beyond the shared set in
    // cef/shell.rs; Ozone platform selection is a Linux-only concern.
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

pub(super) fn native_view_owns_first_responder(native_view: *mut c_void) -> bool {
    unsafe { GhostexGpuiCEFNativeViewOwnsFirstResponder(native_view) }
}

pub(super) fn order_native_view_front(native_view: *mut c_void) {
    unsafe {
        GhostexGpuiCEFOrderNativeViewFront(native_view);
    }
}

pub(super) fn release_native_view(_native_view: *mut c_void) {
    // CEF owns the child NSView lifecycle on macOS; only the Linux adapter
    // holds per-surface embed-host state that needs explicit teardown.
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
    CDXC:GPUICefEditCommands 2026-06-14-17:25:
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
    CDXC:GPUICefEditCommands 2026-07-09:
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
