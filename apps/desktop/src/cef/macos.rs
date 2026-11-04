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
    cell::RefCell,
    ffi::{CString, c_double, c_int, c_void},
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

pub(crate) const CEF_FRAMEWORK_EXECUTABLE_ENV: &str = "GHOSTEX_CEF_FRAMEWORK_EXECUTABLE";
const CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH: &str =
    "Chromium Embedded Framework.framework/Chromium Embedded Framework";

unsafe extern "C" {
    fn GhostexGpuiReparentPaneNativeView(
        view: *mut c_void,
        parent: *mut c_void,
        from: *mut c_void,
    ) -> bool;
    fn GhostexGpuiSidebarRevealDispose(sidebar: *mut c_void);
    fn GhostexGpuiCEFPrepareApplication();
    fn GhostexGpuiCEFSystemUsesDarkPageAppearance() -> bool;
    fn GhostexGpuiCEFInstallApplicationHooks();
    fn GhostexGpuiCEFStartMessageLoop();
    fn GhostexGpuiCEFCancelMessageLoopStart();
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
    fn GhostexGpuiCEFSetNativeViewMotionHidden(
        native_view: *mut c_void,
        hidden: bool,
        fade_in_seconds: f64,
    );
    fn GhostexGpuiCEFOrderNativeViewFront(native_view: *mut c_void);
    fn GhostexGpuiCEFNativeViewContentIdentity(native_view: *mut c_void) -> u64;
    fn GhostexGpuiCEFRemoveNativeViewFromSuperview(native_view: *mut c_void);
    fn GhostexGpuiCEFPrepareNativeViewForFocus(native_view: *mut c_void);
    fn GhostexGpuiCEFSetNativeViewPinchZoomDisabled(native_view: *mut c_void, disabled: bool);
    fn GhostexGpuiCEFFocusNativeView(native_view: *mut c_void);
    fn GhostexGpuiCEFActivateNativeViewWindow(native_view: *mut c_void);
    fn GhostexGpuiCEFFocusGpuiRootView(native_view: *mut c_void);
    fn GhostexGpuiInstallFirstResponderObserverForNativeView(native_view: *mut c_void);
    fn GhostexGpuiNativeViewContainsResponder(
        root_native_view: *mut c_void,
        responder: *mut c_void,
    ) -> bool;
    fn GhostexGpuiCEFNativeViewOwnsFirstResponder(native_view: *mut c_void) -> bool;
    fn GhostexGpuiCEFReportSidebarPointerOutside();
    fn GhostexGpuiCEFRefreshSidebarPointerInside();
}

pub(super) fn reparent_pane_native_view(
    view: *mut c_void,
    parent: *mut c_void,
    from: *mut c_void,
) -> bool {
    unsafe { GhostexGpuiReparentPaneNativeView(view, parent, from) }
}

pub(super) fn dispose_sidebar_hover_reveal(sidebar: *mut c_void) {
    unsafe { GhostexGpuiSidebarRevealDispose(sidebar) }
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

/// LaunchServices already activates the running app on a second launch.
pub(super) fn activate_running_app() {}

pub(super) fn install_application_hooks() {
    unsafe { GhostexGpuiCEFInstallApplicationHooks() };
}

/// macOS runs Chromium's own message loop (see `start_cef`), so there is no external pump to
/// install before `cef::initialize`.
pub(super) fn install_message_pump(_cx: &gpui::App) {}

thread_local! {
    static PENDING_CEF_START: RefCell<Option<Box<dyn FnOnce() -> anyhow::Result<()>>>> =
        const { RefCell::new(None) };
}

/// CDXC:CefRuntime 2026-10-17 WHY:
/// macOS runs Chromium's own message loop instead of CEF's external message pump, because the external pump (MessagePumpExternal::DirectRunWork) runs Chromium's idle step only when no delayed task is queued, which in a running browser is almost never. Chrome's autofill popup waits for that idle step before it accepts a suggestion, so clicking a saved value or pressing Enter on it did nothing (GitHub #240). Chromium's macOS pump runs the idle step from its before-wait observer, but only while one of its run loops is on the stack, so `cef::run_message_loop` is entered once, nested inside GPUI's NSApplication loop, and runs for the rest of the process. Supersedes the 2026-06-14 external pump on macOS; Windows and Linux keep it and its #240 bug (their in-app alternative, `multi_threaded_message_loop`, moves every CEF callback off the main thread).
/// `cef::initialize` runs in the same run-loop callout that enters the loop (GhostexGpuiCEFStartMessageLoop in the shim says which callout). Initialization creates Chromium's pump, whose run-loop observers count every loop entry and exit and CHECK-crash (an empty work-item stack in PopWorkItemScope) on an exit they never saw enter. Ghostex starts CEF lazily from inside a running loop pass, so the first version (f7c7177cf), which initialized in a GPUI task and entered the loop on a later timer, crashed on the first web view of every launch the moment that pass ended. Ghostty's crash handler caught the trap, so the process exited with status 6 and macOS wrote no crash report. With both steps in one callout, the only loop passes the observers see are the ones Chromium's loop starts itself. The 2026-06-14 run-loop trap came from a 10 ms NSTimer in every mode calling CefDoMessageLoopWork without the external pump, which nested a Chromium event loop on every tick; here the loop starts once, in the default mode only.
pub(super) fn start_cef(start: Box<dyn FnOnce() -> anyhow::Result<()>>) -> anyhow::Result<()> {
    PENDING_CEF_START.with(|pending| *pending.borrow_mut() = Some(start));
    unsafe { GhostexGpuiCEFStartMessageLoop() };
    Ok(())
}

pub(super) fn invalidate_message_pump() {
    PENDING_CEF_START.with(|pending| pending.borrow_mut().take());
    unsafe { GhostexGpuiCEFCancelMessageLoopStart() };
}

/// Chromium's own pump wakes itself whenever a task is posted, so nothing needs to be scheduled.
pub(super) fn schedule_message_pump_work(_delay_ms: i64) {}

pub(super) fn apply_platform_settings(settings: &mut cef::Settings) {
    settings.external_message_pump = 0;
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

pub(super) fn set_native_view_pinch_zoom_disabled(native_view: *mut c_void, disabled: bool) {
    unsafe {
        GhostexGpuiCEFSetNativeViewPinchZoomDisabled(native_view, disabled);
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

pub(super) fn set_native_view_motion_hidden(
    native_view: *mut c_void,
    hidden: bool,
    fade_in_seconds: f64,
) {
    unsafe {
        GhostexGpuiCEFSetNativeViewMotionHidden(native_view, hidden, fade_in_seconds);
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

pub(super) fn native_view_content_identity(native_view: *mut c_void) -> u64 {
    if native_view.is_null() {
        return 0;
    }
    unsafe { GhostexGpuiCEFNativeViewContentIdentity(native_view) }
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

/// The shim's run-loop callout: initializes CEF and runs Chromium's message loop for the rest of
/// the process.
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFRunMessageLoop() {
    let Some(start) = PENDING_CEF_START.with(|pending| pending.borrow_mut().take()) else {
        return;
    };
    if let Err(error) = start() {
        crate::support_logs::append(
            crate::support_logs::GpuiSupportLog::HostLifecycle,
            "gpui.cef.startFailed",
            serde_json::json!({ "pid": std::process::id(), "error": format!("{error:#}") }),
        );
        super::shell::record_deferred_start_failure(&error);
        return;
    }
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::HostLifecycle,
        "gpui.cef.messageLoopStarted",
        serde_json::json!({ "pid": std::process::id() }),
    );
    cef::run_message_loop();
    // Nothing quits this loop while Ghostex runs, and the next exit of the loop pass below it
    // trips Chromium's observer CHECK, so record it before returning.
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::HostLifecycle,
        "gpui.cef.messageLoopEndedError",
        serde_json::json!({
            "pid": std::process::id(),
            "error": "Chromium's message loop returned while Ghostex was running",
        }),
    );
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
pub extern "C" fn GhostexGpuiCEFRefreshSystemPageAppearances() {
    super::shell::refresh_browser_page_appearances();
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView(
    native_view: *mut c_void,
) -> c_int {
    super::shell::refresh_system_page_appearance_for_native_view(native_view)
}

/*
CDXC:Hotkeys 2026-10-27 WHY:
GitHub #326: in the Code view, Up, Down, Page Up, Page Down, Home and End typed 0x1E 0x1F 0x0B 0x0C 0x01 0x04 into the file, and each one typed into the Command Palette's box re-filtered the list, so the arrows never moved its selection. Those are the codes the keyboard layout gives the keys (the same on the British and US layouts), carried by every key event the system makes. When the macOS input method commits that code as text, Chromium's insertText: hands it to the page as committed text, which skips Blink's rule that a key never types a control character. Keys that really type a control character (Enter 0x0D, Tab 0x09, Backspace 0x08, Escape 0x1B, Ctrl+letter) reach the page through Chromium's own key event, which this never touches, so dropping a commit made only of such codes changes nothing a user can type. Tab, line feed and carriage return stay allowed: dictation commits a new line as text.
*/
fn text_is_stray_key_character(units: &[u16]) -> bool {
    !units.is_empty()
        && units.iter().all(|&unit| {
            matches!(unit, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F | 0xF700..=0xF8FF)
        })
}

#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiCEFTextIsStrayKeyCharacter(units: *const u16, len: usize) -> c_int {
    if units.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: AppKit owns the UTF-16 buffer for this synchronous call.
    let units = unsafe { std::slice::from_raw_parts(units, len) };
    text_is_stray_key_character(units) as c_int
}

#[cfg(test)]
mod tests {
    use super::text_is_stray_key_character;

    // What each key's event carries, read with UCKeyTranslate from
    // com.apple.keylayout.British and com.apple.keylayout.US (identical for these keys).
    const LAYOUT_KEY_CODES: &[(&str, u16)] = &[
        ("Up", 0x1E),
        ("Down", 0x1F),
        ("Left", 0x1C),
        ("Right", 0x1D),
        ("Page Up", 0x0B),
        ("Page Down", 0x0C),
        ("Home", 0x01),
        ("End", 0x04),
        ("Forward Delete", 0x7F),
        ("Backspace", 0x08),
        ("Escape", 0x1B),
        ("F1", 0x10),
        ("Ctrl+A", 0x01),
    ];

    #[test]
    fn navigation_and_control_key_codes_are_never_committed_as_text() {
        for layout in ["British", "US"] {
            for (key, code) in LAYOUT_KEY_CODES {
                assert!(
                    text_is_stray_key_character(&[*code]),
                    "{layout} {key} {code:#04x}"
                );
            }
        }
        // AppKit's function-key characters (NSUpArrowFunctionKey .. the 0xF8FF end of the range).
        for code in [
            0xF700, 0xF701, 0xF729, 0xF72B, 0xF72C, 0xF72D, 0xF704, 0xF8FF,
        ] {
            assert!(text_is_stray_key_character(&[code]), "{code:#06x}");
        }
        // The six bytes from the report, committed together.
        assert!(text_is_stray_key_character(&[
            0x1E, 0x1F, 0x0B, 0x0C, 0x01, 0x04
        ]));
    }

    #[test]
    fn real_text_is_still_committed() {
        for (label, text) in [
            ("Enter", "\r"),
            ("Tab", "\t"),
            ("dictated new line", "\n"),
            ("letter", "a"),
            ("British pound", "£"),
            ("space", " "),
            ("IME commit", "愛"),
            ("emoji", "🙂"),
            ("text ending in a new line", "ok\n"),
        ] {
            let units: Vec<u16> = text.encode_utf16().collect();
            assert!(!text_is_stray_key_character(&units), "{label}");
        }
        assert!(!text_is_stray_key_character(&[]));
        // A control code inside real text is the page's business, not a stray key.
        assert!(!text_is_stray_key_character(&[0x61, 0x1E]));
    }
}
