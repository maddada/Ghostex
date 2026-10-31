// C1 wave-3 extraction: the AppKit/Objective-C FFI callback-target structs, thread-local callback registries, and pub extern "C" bridge fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeyboardRouteNativeEvent(
    gpui_root_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    keycode: u32,
    modifiers: u64,
    shortcut_characters: *const std::ffi::c_char,
    characters: *const std::ffi::c_char,
) -> std::ffi::c_int {
    let shortcut_characters = if shortcut_characters.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit owns the UTF-8 buffer for this synchronous callback.
        unsafe { std::ffi::CStr::from_ptr(shortcut_characters) }
            .to_string_lossy()
            .into_owned()
    };
    let characters = if characters.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit owns the UTF-8 buffer for this synchronous callback.
        unsafe { std::ffi::CStr::from_ptr(characters) }
            .to_string_lossy()
            .into_owned()
    };
    route_gpui_native_keyboard_event(
        gpui_root_view,
        action,
        keycode,
        modifiers,
        &shortcut_characters,
        &characters,
    ) as _
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiPerformTextInputHistoryCommand(
    gpui_root_view: *mut std::ffi::c_void,
    redo: std::ffi::c_int,
) -> std::ffi::c_int {
    dispatch_text_input_history_from_edit_menu(gpui_root_view, redo != 0) as _
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeyboardOwnerUsesRendererEditHotkeys(
    gpui_root_view: *mut std::ffi::c_void,
) -> std::ffi::c_int {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets
            .borrow()
            .get(&(gpui_root_view as usize))
            .is_some_and(|target| {
                matches!(
                    target.owner,
                    GpuiKeyboardOwner::FirstResponder(FirstResponderTarget::CefSurface(
                        FirstResponderCefSurface::ProjectWorkarea(
                            ProjectWorkareaCefSurfaceSlotKey::Source
                                | ProjectWorkareaCefSurfaceSlotKey::CodeWindow(_)
                                | ProjectWorkareaCefSurfaceSlotKey::Manage
                        )
                    ))
                )
            }) as std::ffi::c_int
    })
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeyboardOwnerUsesDocsEditorHotkeys(
    gpui_root_view: *mut std::ffi::c_void,
) -> std::ffi::c_int {
    GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
        targets
            .borrow()
            .get(&(gpui_root_view as usize))
            .is_some_and(|target| gpui_keyboard_owner_uses_docs_editor_hotkeys(target.owner))
            as std::ffi::c_int
    })
}

#[cfg(target_os = "macos")]
thread_local! {
    pub(crate) static GPUI_MENU_BAR_STATUS_CALLBACK_TARGET: RefCell<Option<GpuiMenuBarStatusCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SIDEBAR_POINTER_CALLBACK_TARGET: RefCell<Option<GpuiSidebarPointerCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET: RefCell<Option<GpuiSessionAttentionNotificationCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET: RefCell<Option<GpuiAccessibilityDisplayOptionsCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET: RefCell<Option<GpuiWorkspacePowerEventsCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SPARKLE_UPDATER_CALLBACK_TARGET: RefCell<Option<GpuiSparkleUpdaterCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_FIRST_RESPONDER_CALLBACK_TARGETS: RefCell<HashMap<usize, GpuiFirstResponderCallbackTarget>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_KEYBOARD_ROUTER_TARGETS: RefCell<HashMap<usize, GpuiKeyboardRouterCallbackTarget>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS: RefCell<HashMap<u32, usize>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS: RefCell<HashMap<usize, u32>> = RefCell::new(HashMap::new());
}

thread_local! {
    pub(crate) static GPUI_OS_INTEGRATION_CALLBACK_TARGET: RefCell<Option<GpuiOsIntegrationCallbackTarget>> = const { RefCell::new(None) };
    // Launch-time ghostex:// / file-open URLs can arrive before the app entity
    // exists (macOS `pendingOSIntegrationCommands` parity); buffer them until
    // the callback target registers.
    pub(crate) static GPUI_PENDING_OS_INTEGRATION_URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiMenuBarStatusCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiSidebarPointerCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiSessionAttentionNotificationCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiAccessibilityDisplayOptionsCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiWorkspacePowerEventsCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiSparkleUpdaterCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[derive(Clone)]
pub(crate) struct GpuiOsIntegrationCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiFirstResponderCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiKeyboardRouterCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
    pub(crate) owner: GpuiKeyboardOwner,
    pub(crate) terminal_model_picker_session: Option<TerminalSessionId>,
    pub(crate) owner_generation: u64,
    pub(crate) window_keyboard_id: u64,
    pub(crate) pressed_keys: HashMap<u32, GpuiCapturedKeyRoute>,
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiMenuBarStatusProjectClicked(project_id: *const std::ffi::c_char) {
    let Some(project_id) = gpui_menu_bar_status_action_c_string(project_id) else {
        return;
    };
    queue_gpui_menu_bar_status_project_click(project_id);
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiMenuBarStatusSessionClicked(
    project_id: *const std::ffi::c_char,
    session_id: *const std::ffi::c_char,
) {
    let Some(project_id) = gpui_menu_bar_status_action_c_string(project_id) else {
        return;
    };
    let Some(session_id) = gpui_menu_bar_status_action_c_string(session_id) else {
        return;
    };
    queue_gpui_menu_bar_status_session_click(project_id, session_id);
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSessionAttentionNotificationClicked(
    session_id: *const std::ffi::c_char,
) {
    let Some(session_id) = gpui_session_attention_notification_action_c_string(session_id) else {
        return;
    };
    queue_gpui_session_attention_notification_click(session_id);
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiAccessibilityDisplayOptionsChanged(
    should_reduce_motion: std::ffi::c_int,
) {
    queue_gpui_accessibility_display_options_changed(should_reduce_motion == 1);
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiWorkspaceDidWake() {
    queue_gpui_workspace_did_wake();
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUpdateAvailableChanged(available: std::ffi::c_int) {
    queue_gpui_sparkle_update_available_changed(available == 1);
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUpdateDownloadingChanged(downloading: std::ffi::c_int) {
    queue_gpui_sparkle_update_downloading_changed(downloading == 1);
}

/// The version of the update Sparkle found, for another device asking this computer (app_update_remote_control.rs).
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUpdateVersionFound(version: *const std::ffi::c_char) {
    if version.is_null() {
        return;
    }
    // SAFETY: Sparkle's bridge passes a NUL-terminated UTF-8 string valid for this call.
    let version = unsafe { std::ffi::CStr::from_ptr(version) }
        .to_string_lossy()
        .trim()
        .to_string();
    if version.is_empty() || version.len() > 64 {
        return;
    }
    if let Ok(mut slot) =
        crate::app::os_integration::app_update_remote_control::SPARKLE_AVAILABLE_VERSION.lock()
    {
        *slot = Some(version);
    }
}

/// How an install another device asked for is going (`GhostexGpuiSparkleInstallUnattended`).
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUnattendedUpdateEvent(
    kind: std::ffi::c_int,
    text: *const std::ffi::c_char,
) {
    use crate::app::os_integration::app_update_remote_control as remote;
    let text = (!text.is_null())
        // SAFETY: Sparkle's bridge passes a NUL-terminated UTF-8 string valid for this call.
        .then(|| {
            unsafe { std::ffi::CStr::from_ptr(text) }
                .to_string_lossy()
                .to_string()
        });
    match kind {
        2 => remote::note_remote_install_failed("Ghostex is already up to date."),
        3 => remote::note_remote_install_failed(
            text.filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| "The update failed on this computer.".to_string()),
        ),
        4 => remote::note_remote_install_installing(),
        5 => remote::note_remote_install_ended(),
        _ => {}
    }
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUpdateDownloadProgressChanged(
    has_progress: std::ffi::c_int,
    progress: f64,
) {
    queue_gpui_sparkle_update_download_progress_changed((has_progress == 1).then_some(progress));
}
