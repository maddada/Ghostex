// C4 light split: native focus/pointer delegate functions and
// edit-command/zoom dispatch -- the exact surface `cef/macos.rs`,
// `cef/windows.rs`, and `cef/linux_x11.rs` call into via `super::shell::*`.
// Pure move out of `cef/shell.rs`. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

pub(crate) fn active_cef_native_view() -> Option<usize> {
    match ACTIVE_CEF_NATIVE_VIEW.load(Ordering::Acquire) {
        0 => None,
        native_view => Some(native_view),
    }
}

pub(crate) fn set_active_cef_native_view(native_view: usize) {
    ACTIVE_CEF_NATIVE_VIEW.store(native_view, Ordering::Release);
}

pub(crate) fn set_cef_native_view_hidden(native_view: *mut c_void, hidden: bool) {
    if native_view.is_null() {
        return;
    }
    HIDDEN_CEF_NATIVE_VIEWS.with(|views| {
        if hidden {
            views.borrow_mut().insert(native_view as usize);
        } else {
            views.borrow_mut().remove(&(native_view as usize));
        }
    });
}

pub(crate) fn cef_native_view_is_hidden(native_view: *mut c_void) -> bool {
    if native_view.is_null() {
        return false;
    }
    HIDDEN_CEF_NATIVE_VIEWS.with(|views| views.borrow().contains(&(native_view as usize)))
}

pub fn prepare_application() {
    platform::prepare_application();
}

#[cfg(target_os = "macos")]
pub fn refresh_application_menu_hooks() {
    platform::install_application_hooks();
}

pub fn focus_native_view(native_view: *mut c_void) {
    platform::focus_native_view(native_view);
}

pub fn focus_gpui_root_view(native_view: *mut c_void) {
    platform::focus_gpui_root_view(native_view);
}

#[cfg(target_os = "windows")]
pub fn gpui_root_view_has_native_focus(native_view: *mut c_void) -> bool {
    platform::native_view_has_direct_focus(native_view)
}

#[cfg(target_os = "macos")]
pub fn install_first_responder_observer(native_view: *mut c_void) {
    platform::install_first_responder_observer(native_view);
}

#[cfg(target_os = "macos")]
pub fn set_sidebar_pointer_tracking_view(native_view: *mut c_void) {
    /*
    CDXC:GPUISidebarPointerTracking 2026-08-02:
    Registers the sidebar CEF child view with the AppKit sendEvent observer so
    Rust learns when the pointer crosses the sidebar's frame and when a
    mouse-down lands outside it (sticky-hover reset + context-menu dismissal).
    */
    platform::set_sidebar_pointer_tracking_view(native_view);
}

/*
CDXC:GPUISidebarPointerTracking 2026-08-20:
`data-native-pointer-inside` is only cheap because the AppKit observer keeps a
cache of what the page was last told and skips redundant writes on the
mouse-moved path. Window activation changes have to move through that same
cache instead of writing the page directly, or the cache and the page disagree
and the next real crossing is dropped as redundant.
*/
#[cfg(target_os = "macos")]
pub fn report_sidebar_pointer_outside() {
    platform::report_sidebar_pointer_outside();
}

#[cfg(target_os = "macos")]
pub fn refresh_sidebar_pointer_inside() {
    platform::refresh_sidebar_pointer_inside();
}

#[cfg(target_os = "macos")]
pub fn native_view_contains_responder(
    root_native_view: *mut c_void,
    responder: *mut c_void,
) -> bool {
    platform::native_view_contains_responder(root_native_view, responder)
}
/*
CDXC:GPUICefEditCommands 2026-07-09:
Cut/Copy/Paste join Select All as bridged edit commands because GPUI's
window-level key dispatch consumes Cmd-chords before AppKit can deliver
them to CEF child views, so settings, modal-host, sidebar, and browser
pages never receive the standard clipboard shortcuts. The raw values are
the ABI contract with the AppKit shim (GpuiCefAppKitHooks.m); both sides
must stay in sync.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CefEditCommand {
    Cut,
    Copy,
    Paste,
}

impl CefEditCommand {
    pub(crate) fn from_raw(raw: c_int) -> Option<Self> {
        match raw {
            1 => Some(Self::Cut),
            2 => Some(Self::Copy),
            3 => Some(Self::Paste),
            _ => None,
        }
    }
}

/*
CDXC:GPUICefPaneZoomShortcuts 2026-07-14:
The AppKit CEF responder subclass forwards only the standard page-zoom
commands for Browser, main project-workarea, and Session Chat native views.
The raw values are the narrow ABI contract with GpuiCefAppKitHooks.m. Sidebar,
modal, titlebar, and companion CEF views are deliberately absent from the
keyboard zoom registry even though they share the browser registry used by
editing.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CefZoomCommand {
    In,
    Out,
    Reset,
}

impl CefZoomCommand {
    pub(crate) fn from_raw(raw: c_int) -> Option<Self> {
        match raw {
            1 => Some(Self::In),
            2 => Some(Self::Out),
            3 => Some(Self::Reset),
            _ => None,
        }
    }
}

pub(crate) fn edit_command_in_browser(browser: &cef::Browser, command: CefEditCommand) -> bool {
    let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) else {
        return false;
    };
    match command {
        CefEditCommand::Cut => frame.cut(),
        CefEditCommand::Copy => frame.copy(),
        CefEditCommand::Paste => frame.paste(),
    }
    true
}

/*
CDXC:GPUICefPlatformSeam 2026-07-04:
The select-all/active-view helpers stay in shared code because the registry
they consult is shared, but the entry points that reach them are per-OS:
macOS exports them to the AppKit responder-chain shim (cef/macos.rs), while
Windows/Chromium routes Ctrl+A to the focused browser HWND natively and
needs no external dispatch hook.
*/
pub(crate) fn select_all_for_native_view(native_view: *mut c_void) -> c_int {
    if native_view.is_null() {
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);

    /*
    CDXC:GPUICefEditCommands 2026-08-18:
    Taking native focus here is only for the case where GPUI chrome still owns
    the first responder while the page holds the caret. When Chromium already
    owns the keyboard, re-running the focus handoff walks the responder out of
    the render widget and back, and Blink reports that round trip to the page
    as a blur/focus pair — which commits and closes any field that saves on
    blur, such as the sidebar group rename. Select All must never disturb the
    focus that already exists.
    */
    if let Some(host) = browser.host() {
        let host_view = platform::native_view_ptr(host.window_handle());
        if !platform::native_view_owns_first_responder(host_view) {
            platform::focus_native_view(host_view);
            host.set_focus(1);
        }
    }

    if select_all_in_browser(&browser) {
        1
    } else {
        0
    }
}

pub(crate) fn select_all_for_active_native_view() -> c_int {
    let native_view = active_cef_native_view();
    let Some(native_view) = native_view else {
        return 0;
    };
    select_all_for_native_view(native_view as *mut c_void)
}

/*
CDXC:GPUICefEditCommands 2026-07-09:
Unlike Select All, clipboard commands are destructive to shared clipboard
state, so the AppKit shim resolves the target by walking the key window's
actual first responder instead of the last-active CEF view registry; a
stale active view (e.g. after clicking into a native Ghostty terminal)
must never receive a mirrored Cmd+C/X/V.
*/
pub(crate) fn edit_command_for_native_view(
    native_view: *mut c_void,
    command: CefEditCommand,
) -> c_int {
    if native_view.is_null() {
        crate::support_logs::append(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.cef.editCommandNativeViewRoute",
            serde_json::json!({
                "command": format!("{command:?}"),
                "matchedBrowser": false,
                "nativeViewWasNull": true,
            }),
        );
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);

    if let Some(host) = browser.host() {
        host.set_focus(1);
    }

    let handled = edit_command_in_browser(&browser, command);
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.editCommandNativeViewRoute",
        serde_json::json!({
            "command": format!("{command:?}"),
            "matchedBrowser": true,
            "browserId": browser.identifier(),
            "isPopup": browser.is_popup() != 0,
            "handled": handled,
        }),
    );
    if handled { 1 } else { 0 }
}

pub(crate) fn zoom_command_for_native_view(
    native_view: *mut c_void,
    command: CefZoomCommand,
) -> c_int {
    if native_view.is_null()
        || !KEYBOARD_ZOOM_CEF_NATIVE_VIEWS
            .with(|views| views.borrow().contains(&(native_view as usize)))
    {
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };
    let Some(host) = browser.host() else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);
    host.set_focus(1);
    match command {
        CefZoomCommand::In => host.zoom(ZoomCommand::IN),
        CefZoomCommand::Out => host.zoom(ZoomCommand::OUT),
        CefZoomCommand::Reset => host.zoom(ZoomCommand::RESET),
    }
    1
}

/*
CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
App-owned focus grant/release for the mouse-focus-passive sidebar surface.
"focused" repeats the exact sequence every sanctioned grant already uses
(mark the explicit active view, then native first responder, then Chromium
focus) so the OnSetFocus arbitration recognizes it as app-owned. "blurred"
releases only if the sidebar actually owns the first responder, handing the
keyboard back to the GPUI root so the previously focused terminal types
again without requiring a click.
*/
pub(crate) fn handle_sidebar_editable_focus(browser: Option<&mut cef::Browser>, payload: &str) {
    let focused = match payload {
        "focused" => true,
        "blurred" => false,
        _ => return,
    };
    let Some(host) = browser.and_then(|browser| browser.host()) else {
        return;
    };
    let native_view = platform::native_view_ptr(host.window_handle());
    if native_view.is_null() {
        return;
    }

    let owned_first_responder = platform::native_view_owns_first_responder(native_view);
    if focused {
        SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.store(native_view as usize, Ordering::Release);
        set_active_cef_native_view(native_view as usize);
        platform::set_native_view_passive_focus_grant(native_view, true);
        platform::focus_native_view(native_view);
        host.set_focus(1);
    } else {
        let _ = SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.compare_exchange(
            native_view as usize,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        platform::set_native_view_passive_focus_grant(native_view, false);
        if owned_first_responder {
            host.set_focus(0);
            platform::return_focus_to_gpui_root(native_view);
        }
    }
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.sidebarEditableFocus",
        serde_json::json!({
            "focused": focused,
            "ownedFirstResponder": owned_first_responder,
        }),
    );
}

pub(crate) fn mark_native_view_focused(native_view: *mut c_void) -> c_int {
    if native_view.is_null() {
        return 0;
    }

    let is_cef_view = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().contains_key(&(native_view as usize)));
    if !is_cef_view {
        return 0;
    }

    set_active_cef_native_view(native_view as usize);
    1
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn log_native_mouse_down(
    native_view: *mut c_void,
    event_window_x: f64,
    event_window_y: f64,
    frame_window_x: f64,
    frame_window_y: f64,
    frame_width: f64,
    frame_height: f64,
    parent_bounds_width: f64,
    parent_bounds_height: f64,
    hidden: bool,
    responder_class: String,
) {
    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let event_inside_native_frame = event_window_x >= frame_window_x
        && event_window_y >= frame_window_y
        && event_window_x < frame_window_x + frame_width
        && event_window_y < frame_window_y + frame_height;
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.nativeMouseDown",
        serde_json::json!({
            "browserId": browser.as_ref().map(cef::Browser::identifier),
            "isPopup": browser.as_ref().is_some_and(|browser| browser.is_popup() != 0),
            "eventWindow": [event_window_x, event_window_y],
            "nativeFrameWindow": [frame_window_x, frame_window_y, frame_width, frame_height],
            "parentBounds": [parent_bounds_width, parent_bounds_height],
            "eventInsideNativeFrame": event_inside_native_frame,
            "hidden": hidden,
            "responderClass": responder_class,
        }),
    );
}

pub(crate) fn clear_active_native_view_registry() {
    clear_active_native_view();
}
