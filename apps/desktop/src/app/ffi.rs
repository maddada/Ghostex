// C1 wave-3 extraction: the AppKit/Objective-C FFI callback-target structs, thread-local callback registries, and pub extern "C" bridge fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;


#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_native_key_event_is_plain_escape_key_down(
    event: &ghostty_kit::ffi::ghostty_input_key_s,
) -> bool {
    event.keycode == GPUI_TERMINAL_ESCAPE_KEYCODE
        && (event.action == GPUI_TERMINAL_GHOSTTY_KEY_ACTION_PRESS
            || event.action == GPUI_TERMINAL_GHOSTTY_KEY_ACTION_REPEAT)
        && !event.composing
        && event.mods & GHOSTTY_TERMINAL_OBSERVED_SHORTCUT_MODS == 0
        && event.consumed_mods & GHOSTTY_TERMINAL_OBSERVED_SHORTCUT_MODS == 0
}


#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_native_key_event_is_modified_return_key_down(
    event: &ghostty_kit::ffi::ghostty_input_key_s,
) -> bool {
    matches!(
        event.keycode,
        GPUI_TERMINAL_RETURN_KEYCODE | GPUI_TERMINAL_KEYPAD_ENTER_KEYCODE
    ) && (event.action == GPUI_TERMINAL_GHOSTTY_KEY_ACTION_PRESS
        || event.action == GPUI_TERMINAL_GHOSTTY_KEY_ACTION_REPEAT)
        && event.mods & GHOSTTY_TERMINAL_OBSERVED_SHORTCUT_MODS != 0
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalNativeViewKeyTranslationMods(
    native_view: *mut std::ffi::c_void,
    mods: std::ffi::c_int,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
    The terminal host NSView asks Rust for Ghostty's modifier translation before constructing a key event, because option-as-alt and layout behavior belongs to the exact mounted surface config. This callback is synchronous, pointer-scoped, and returns the original modifiers when no real surface is registered.
    */
    terminal_ghostty_surface::native_key_translation_mods_for_view(native_view, mods)
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeyboardRouteNativeEvent(
    gpui_root_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    keycode: u32,
    modifiers: u64,
    characters_ignoring_modifiers: *const std::ffi::c_char,
    characters: *const std::ffi::c_char,
) -> std::ffi::c_int {
    let characters_ignoring_modifiers = if characters_ignoring_modifiers.is_null() {
        String::new()
    } else {
        // SAFETY: AppKit owns the UTF-8 buffer for this synchronous callback.
        unsafe { std::ffi::CStr::from_ptr(characters_ignoring_modifiers) }
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
    let should_probe = keycode == 0
        || characters.chars().count() > 1
        || characters_ignoring_modifiers.chars().count() > 1;
    if should_probe {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.nativeEventEntry",
            serde_json::json!({
                "action": action,
                "characters": support_logs::temporary_fluid_voice_text_shape(&characters),
                "charactersIgnoringModifiers":
                    support_logs::temporary_fluid_voice_text_shape(&characters_ignoring_modifiers),
                "keycode": keycode,
                "modifiers": modifiers,
                "rootViewPresent": !gpui_root_view.is_null(),
            }),
        );
    }
    let handled = route_gpui_native_keyboard_event(
        gpui_root_view,
        action,
        keycode,
        modifiers,
        &characters_ignoring_modifiers,
        &characters,
    );
    if should_probe {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.nativeEventResult",
            serde_json::json!({
                "action": action,
                "handled": handled,
                "keycode": keycode,
            }),
        );
    }
    handled as _
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
                                | ProjectWorkareaCefSurfaceSlotKey::Manage
                        ) | FirstResponderCefSurface::SessionChat(_)
                    ))
                )
            }) as std::ffi::c_int
    })
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiKeyboardOwnerIsSessionChat(
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
                        FirstResponderCefSurface::SessionChat(_)
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
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalHandleNativeKeyEvent(
    native_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    mods: std::ffi::c_int,
    consumed_mods: std::ffi::c_int,
    keycode: u32,
    text: *const std::ffi::c_char,
    unshifted_codepoint: u32,
    composing: std::ffi::c_int,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
    Native key events may cross from the exact AppKit host view to Ghostty only as transient primitives. The C string pointer is borrowed for this call only, and Rust resolves the host pointer through the runtime registry instead of using focused-surface fallback routing.
    */
    let event = ghostty_kit::ffi::ghostty_input_key_s {
        action,
        mods,
        consumed_mods,
        keycode,
        text,
        unshifted_codepoint,
        composing: composing != 0,
    };
    let should_dispatch_escape = gpui_terminal_native_key_event_is_plain_escape_key_down(&event);
    let should_log_modified_return =
        gpui_terminal_native_key_event_is_modified_return_key_down(&event);
    let target_diagnostic = should_log_modified_return
        .then(|| terminal_ghostty_surface::native_key_target_diagnostic_for_view(native_view))
        .flatten();
    let target_surface = target_diagnostic.map(|target| match target.surface_kind {
        0 => "agents",
        1 => "command",
        2 => "companion",
        _ => "unknown",
    });
    let accepted = terminal_ghostty_surface::send_native_key_event_for_view(native_view, event);
    if should_log_modified_return {
        let text_bytes = if text.is_null() {
            &[][..]
        } else {
            // SAFETY: the AppKit adapter lends a valid NUL-terminated string
            // for this synchronous callback only.
            unsafe { std::ffi::CStr::from_ptr(text) }.to_bytes()
        };
        let text_control_codepoint =
            (text_bytes.len() == 1 && text_bytes[0] < 0x20).then_some(u32::from(text_bytes[0]));
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalInput.modifiedReturnNativeRoute",
            serde_json::json!({
                "accepted": accepted,
                "action": action,
                "composing": composing != 0,
                "consumedMods": consumed_mods,
                "containerId": target_diagnostic.map(|target| target.container_id),
                "hasText": !text_bytes.is_empty(),
                "keycode": keycode,
                "mods": mods,
                "sessionId": target_diagnostic.map(|target| target.session_id),
                "surface": target_surface,
                "targetRegistered": target_diagnostic.is_some(),
                "textByteLength": text_bytes.len(),
                "textControlCodepoint": text_control_codepoint,
                "unshiftedCodepoint": unshifted_codepoint,
            }),
        );
    }
    if accepted {
        if should_dispatch_escape {
            queue_gpui_workspace_terminal_escape_pressed(native_view);
        }
        1
    } else {
        0
    }
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalNativeKeyEventIsBinding(
    native_view: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    mods: std::ffi::c_int,
    consumed_mods: std::ffi::c_int,
    keycode: u32,
    text: *const std::ffi::c_char,
    unshifted_codepoint: u32,
    composing: std::ffi::c_int,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeKeyBridge 2026-07-11:
    AppKit offers Command/Control key equivalents before keyDown. Let the exact
    mounted libghostty surface decide whether the native event is a binding so
    the host view can claim only terminal-owned chords and leave all other app
    key equivalents on the normal responder/menu path.
    */
    let event = ghostty_kit::ffi::ghostty_input_key_s {
        action,
        mods,
        consumed_mods,
        keycode,
        text,
        unshifted_codepoint,
        composing: composing != 0,
    };
    terminal_ghostty_surface::native_key_event_is_binding_for_view(native_view, event) as _
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalHandlePromptEditorShortcut(
    native_view: *mut std::ffi::c_void,
) -> std::ffi::c_int {
    queue_gpui_terminal_prompt_editor_shortcut(native_view) as _
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalInsertDroppedText(
    native_view: *mut std::ffi::c_void,
    text: *const std::ffi::c_char,
    len: usize,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalFileDropInsertion 2026-06-27-03:35:
    AppKit terminal file drops cross into Rust only as a borrowed byte slice for this synchronous call. Insert into the Ghostty surface registered for the exact mounted host view, return failure for null or empty input, and never store, log, persist, or reroute dropped text through focused-surface fallback, overlays, or hit-test routing.
    */
    let Some(text) = std::ptr::NonNull::new(text.cast_mut()) else {
        return 0;
    };
    if len == 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(text.as_ptr() as *const u8, len) };
    if terminal_ghostty_surface::send_native_dropped_text_for_view(native_view, bytes) {
        1
    } else {
        0
    }
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalInsertCommittedText(
    native_view: *mut std::ffi::c_void,
    text: *const std::ffi::c_char,
    len: usize,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeImeBridge 2026-06-27-03:46:
    AppKit committed IME text crosses into Rust only as borrowed bytes for this synchronous callback. Insert only into the Ghostty surface registered for the exact mounted host view, reject null or empty committed text, and never store, log, persist, or reroute typed text through focused-surface fallback.
    */
    let target_registered =
        terminal_ghostty_surface::native_key_target_diagnostic_for_view(native_view).is_some();
    let Some(text) = std::ptr::NonNull::new(text.cast_mut()) else {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.nativeCommittedText",
            serde_json::json!({
                "accepted": false,
                "reason": "nullText",
                "targetRegistered": target_registered,
                "text": support_logs::temporary_fluid_voice_bytes_shape(&[]),
            }),
        );
        return 0;
    };
    if len == 0 {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.fluidVoice.nativeCommittedText",
            serde_json::json!({
                "accepted": false,
                "reason": "emptyText",
                "targetRegistered": target_registered,
                "text": support_logs::temporary_fluid_voice_bytes_shape(&[]),
            }),
        );
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(text.as_ptr() as *const u8, len) };
    let accepted =
        terminal_ghostty_surface::send_native_committed_text_for_view(native_view, bytes);
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.gpui.fluidVoice.nativeCommittedText",
        serde_json::json!({
            "accepted": accepted,
            "targetRegistered": target_registered,
            "text": support_logs::temporary_fluid_voice_bytes_shape(bytes),
        }),
    );
    if accepted { 1 } else { 0 }
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalSetPreeditText(
    native_view: *mut std::ffi::c_void,
    text: *const std::ffi::c_char,
    len: usize,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeImeBridge 2026-06-27-03:46:
    AppKit marked-text preedit crosses into Rust only as borrowed bytes for this synchronous callback. Route by exact mounted host view, allow zero-length preedit to clear through Ghostty's null/zero preedit convention, and never store, log, persist, or reroute marked text through focused-surface fallback.
    */
    let bytes = if len == 0 {
        &[]
    } else {
        let Some(text) = std::ptr::NonNull::new(text.cast_mut()) else {
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.fluidVoice.nativePreeditText",
                serde_json::json!({
                    "accepted": false,
                    "reason": "nullNonEmptyText",
                    "text": support_logs::temporary_fluid_voice_bytes_shape(&[]),
                }),
            );
            return 0;
        };
        unsafe { std::slice::from_raw_parts(text.as_ptr() as *const u8, len) }
    };
    let target_registered =
        terminal_ghostty_surface::native_key_target_diagnostic_for_view(native_view).is_some();
    let accepted = terminal_ghostty_surface::set_native_preedit_text_for_view(native_view, bytes);
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.gpui.fluidVoice.nativePreeditText",
        serde_json::json!({
            "accepted": accepted,
            "targetRegistered": target_registered,
            "text": support_logs::temporary_fluid_voice_bytes_shape(bytes),
        }),
    );
    if accepted { 1 } else { 0 }
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiTerminalGetImePoint(
    native_view: *mut std::ffi::c_void,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) -> std::ffi::c_int {
    /*
    CDXC:GPUITerminalNativeImeBridge 2026-06-27-03:46:
    AppKit candidate-window placement may read only Ghostty's current IME point for the exact mounted host view. Return failure for missing output pointers or unregistered views instead of inventing cursor geometry or using focused-surface fallback.
    */
    if x.is_null() || y.is_null() || width.is_null() || height.is_null() {
        return 0;
    }
    let Some(point) = terminal_ghostty_surface::native_ime_point_for_view(native_view) else {
        return 0;
    };
    unsafe {
        *x = point.x;
        *y = point.y;
        *width = point.width;
        *height = point.height;
    }
    1
}


#[cfg(target_os = "macos")]
thread_local! {
    pub(crate) static GPUI_APP_SHOTS_CALLBACK_TARGET: RefCell<Option<GpuiAppShotsCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_MENU_BAR_STATUS_CALLBACK_TARGET: RefCell<Option<GpuiMenuBarStatusCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SIDEBAR_POINTER_CALLBACK_TARGET: RefCell<Option<GpuiSidebarPointerCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET: RefCell<Option<GpuiSessionAttentionNotificationCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET: RefCell<Option<GpuiAccessibilityDisplayOptionsCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_WORKSPACE_POWER_EVENTS_CALLBACK_TARGET: RefCell<Option<GpuiWorkspacePowerEventsCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_SPARKLE_UPDATER_CALLBACK_TARGET: RefCell<Option<GpuiSparkleUpdaterCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_OS_INTEGRATION_CALLBACK_TARGET: RefCell<Option<GpuiOsIntegrationCallbackTarget>> = const { RefCell::new(None) };
    pub(crate) static GPUI_FIRST_RESPONDER_CALLBACK_TARGETS: RefCell<HashMap<usize, GpuiFirstResponderCallbackTarget>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_TERMINAL_KEY_EVENT_CALLBACK_TARGETS: RefCell<HashMap<usize, GpuiTerminalKeyEventCallbackTarget>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_KEYBOARD_ROUTER_TARGETS: RefCell<HashMap<usize, GpuiKeyboardRouterCallbackTarget>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_KEYBOARD_PRESSED_WINDOW_OWNERS: RefCell<HashMap<u32, usize>> = RefCell::new(HashMap::new());
    pub(crate) static GPUI_FIRST_RESPONDER_PROGRAMMATIC_DEPTHS: RefCell<HashMap<usize, u32>> = RefCell::new(HashMap::new());
    // Launch-time ghostex:// / file-open URLs can arrive before the app entity
    // exists (macOS `pendingOSIntegrationCommands` parity); buffer them until
    // the callback target registers.
    pub(crate) static GPUI_PENDING_OS_INTEGRATION_URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}


#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiAppShotsCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
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


#[cfg(target_os = "macos")]
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
pub(crate) struct GpuiTerminalKeyEventCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
}


#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct GpuiKeyboardRouterCallbackTarget {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) async_app: gpui::AsyncApp,
    pub(crate) owner: GpuiKeyboardOwner,
    pub(crate) owner_generation: u64,
    pub(crate) window_keyboard_id: u64,
    pub(crate) pressed_keys: HashMap<u32, GpuiCapturedKeyRoute>,
}


#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
pub(crate) struct GpuiAppShotCapture {
    pub(crate) app_name: String,
    pub(crate) bundle_identifier: Option<String>,
    pub(crate) image_path: String,
    pub(crate) window_title: Option<String>,
    pub(crate) window_width: Option<i32>,
    pub(crate) window_height: Option<i32>,
    pub(crate) trigger: Option<String>,
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiAppShotsSettingsEnabled() -> std::ffi::c_int {
    if shared_settings::shared_sidebar_settings_snapshot()
        .app_shots_settings()
        .enabled
    {
        1
    } else {
        0
    }
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiAppShotsSettingsHotkey() -> std::ffi::c_int {
    shared_settings::shared_sidebar_settings_snapshot()
        .app_shots_settings()
        .hotkey
        .native_code()
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiAppShotsCaptureSucceeded(
    app_name: *const std::ffi::c_char,
    bundle_identifier: *const std::ffi::c_char,
    image_path: *const std::ffi::c_char,
    window_title: *const std::ffi::c_char,
    window_width: i32,
    window_height: i32,
    trigger: *const std::ffi::c_char,
) {
    let Some(app_name) = gpui_app_shots_c_string(app_name) else {
        queue_gpui_app_shot_status("App Shot failed.");
        return;
    };
    let Some(image_path) = gpui_app_shots_c_string(image_path) else {
        queue_gpui_app_shot_status("App Shot failed.");
        return;
    };
    let capture = GpuiAppShotCapture {
        app_name,
        bundle_identifier: gpui_app_shots_c_string(bundle_identifier),
        image_path,
        window_title: gpui_app_shots_c_string(window_title),
        window_width: (window_width > 0).then_some(window_width),
        window_height: (window_height > 0).then_some(window_height),
        trigger: gpui_app_shots_c_string(trigger),
    };
    queue_gpui_app_shot_capture(capture);
}


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiAppShotsCaptureFailed(_message: *const std::ffi::c_char) {
    queue_gpui_app_shot_status("Could not capture an App Shot.");
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


#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiSparkleUpdateDownloadProgressChanged(
    has_progress: std::ffi::c_int,
    progress: f64,
) {
    queue_gpui_sparkle_update_download_progress_changed((has_progress == 1).then_some(progress));
}

