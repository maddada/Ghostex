use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_native_app_shot_capture_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onNativeAppShotCaptured==='function'){{bridge.onNativeAppShotCaptured(payload);}}else{{const pending=Array.isArray(bridge.pendingNativeAppShots)?bridge.pendingNativeAppShots:[];pending.push(payload);bridge.pendingNativeAppShots=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_native_app_shot_prompt_result_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onNativeAppShotPromptResult==='function'){{bridge.onNativeAppShotPromptResult(payload);}}else{{const pending=Array.isArray(bridge.pendingNativeAppShotPromptResults)?bridge.pendingNativeAppShotPromptResults:[];pending.push(payload);bridge.pendingNativeAppShotPromptResults=pending;}}}})(); undefined;"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiMacOSNotificationAuthorizationStatus {
    Unsupported,
    Unknown,
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
}

impl GpuiMacOSNotificationAuthorizationStatus {
    pub(crate) fn from_native_code(code: i32) -> Self {
        match code {
            -1 => Self::Unsupported,
            1 => Self::NotDetermined,
            2 => Self::Denied,
            3 => Self::Authorized,
            4 => Self::Provisional,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn authorization_status(self) -> &'static str {
        match self {
            Self::Unsupported => "unavailable",
            Self::Unknown => "unknown",
            Self::NotDetermined => "notDetermined",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
            Self::Provisional => "provisional",
        }
    }

    pub(crate) fn available(self) -> bool {
        matches!(self, Self::Authorized | Self::Provisional)
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Authorized => "macOS allows Ghostex notification banners.",
            Self::Provisional => "macOS allows provisional Ghostex notification banners.",
            Self::NotDetermined => "macOS notification permission has not been decided yet.",
            Self::Denied => {
                "macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications."
            }
            Self::Unsupported => "macOS notification banners are not available in this GPUI build.",
            Self::Unknown => "GPUI could not read macOS notification permission status.",
        }
    }

    pub(crate) fn toast_level(self) -> &'static str {
        if self.available() {
            "success"
        } else {
            "warning"
        }
    }

    pub(crate) fn toast_title(self) -> &'static str {
        match self {
            Self::Authorized | Self::Provisional => "Notifications enabled",
            Self::Denied => "Notifications disabled",
            Self::NotDetermined => "Notification permission undecided",
            Self::Unsupported => "Notifications unavailable",
            Self::Unknown => "Notification status unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiMacOSNotificationDeliveryResult {
    Unsupported,
    Unknown,
    PermissionNotDetermined,
    PermissionDenied,
    Sent,
    Failed,
}

impl GpuiMacOSNotificationDeliveryResult {
    pub(crate) fn from_native_code(code: i32) -> Self {
        match code {
            -1 => Self::Unsupported,
            1 => Self::PermissionNotDetermined,
            2 => Self::PermissionDenied,
            3 => Self::Sent,
            4 => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn permission_status(self) -> Option<GpuiMacOSNotificationAuthorizationStatus> {
        match self {
            Self::Unsupported => Some(GpuiMacOSNotificationAuthorizationStatus::Unsupported),
            Self::PermissionNotDetermined => {
                Some(GpuiMacOSNotificationAuthorizationStatus::NotDetermined)
            }
            Self::PermissionDenied => Some(GpuiMacOSNotificationAuthorizationStatus::Denied),
            Self::Unknown => Some(GpuiMacOSNotificationAuthorizationStatus::Unknown),
            Self::Sent | Self::Failed => None,
        }
    }
}

pub(crate) fn gpui_notification_permission_status_message(
    status: GpuiMacOSNotificationAuthorizationStatus,
) -> serde_json::Value {
    serde_json::json!({
        "authorizationStatus": status.authorization_status(),
        "available": status.available(),
        "generatedAt": gpui_status_generated_at(),
        "message": status.message(),
        "type": "notificationPermissionStatus",
    })
}

pub(crate) fn gpui_macos_notification_test_action_message(
    result: GpuiMacOSNotificationDeliveryResult,
    completion_sound_enabled: bool,
    played_sound: bool,
) -> (&'static str, bool) {
    match result {
        GpuiMacOSNotificationDeliveryResult::Sent => {
            if completion_sound_enabled && played_sound {
                (
                    "Played the completion sound and sent a macOS notification test.",
                    true,
                )
            } else if completion_sound_enabled {
                (
                    "Sent a macOS notification test. The completion sound preview reported a separate failure.",
                    true,
                )
            } else {
                ("Sent a macOS notification test.", true)
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionDenied => {
            if completion_sound_enabled && played_sound {
                (
                    "Played the completion sound, but macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications.",
                    false,
                )
            } else {
                (
                    "macOS is not allowing Ghostex notification banners. Use Open macOS Notification Settings to allow notifications.",
                    false,
                )
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionNotDetermined => (
            "macOS did not return a notification permission decision. Use the notification permission button or Open macOS Notification Settings.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Unsupported => (
            "macOS notification banners are not available in this GPUI build.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Failed => {
            ("GPUI could not send a macOS notification test.", false)
        }
        GpuiMacOSNotificationDeliveryResult::Unknown => (
            "GPUI could not determine whether the macOS notification test was sent.",
            false,
        ),
    }
}

pub(crate) fn gpui_sound_preview_status_message(ok: bool, message: &str) -> serde_json::Value {
    serde_json::json!({
        "generatedAt": gpui_status_generated_at(),
        "message": message,
        "ok": ok,
        "type": "soundPreviewStatus",
    })
}

pub(crate) fn gpui_ghostex_folder_stats_error_message(message: &str) -> serde_json::Value {
    serde_json::json!({
        "errorMessage": message,
        "folderPath": gpui_path_string(&shared_settings::ghostex_storage_paths().data_dir),
        "folders": [],
        "generatedAt": gpui_status_generated_at(),
        "totalBytes": 0,
        "type": "ghostexFolderStats",
    })
}

pub(crate) fn gpui_play_completion_sound(sound: &str) -> Result<(), String> {
    /*
    CDXC:GPUISettingsActionBridge 2026-06-24-11:59:
    GPUI sound preview may play only validated bundled completion sound filenames from the app bundle/sidebar resources or the repository media directory used by local GPUI runs. The command never accepts a path from React and falls back to an explicit unsupported status when the sound asset or platform playback command is unavailable.
    */
    let file_name = gpui_completion_sound_file_name(sound);
    let Some(sound_path) = gpui_completion_sound_path(file_name) else {
        return Err("GPUI could not find a bundled completion sound asset.".to_string());
    };
    gpui_spawn_completion_sound_player(&sound_path)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::from_native_code(unsafe {
        GhostexGpuiRequestNotificationAuthorization()
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    GpuiMacOSNotificationDeliveryResult::from_native_code(unsafe {
        GhostexGpuiDeliverSettingsTestNotification()
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    Read only the macOS Reduce Motion boolean from the AppKit shim. Unknown or unsupported native results intentionally mean "animation allowed" so GPUI does not add fake fallback state or persist accessibility preferences.
    */
    unsafe { GhostexGpuiAccessibilityDisplayShouldReduceMotion() == 1 }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    false
}

pub(crate) fn gpui_macos_attention_notifications_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("showMacOSAttentionNotifications")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    let Ok(session_id) = std::ffi::CString::new(candidate.session_id.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let Ok(title) = std::ffi::CString::new(candidate.title.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let Ok(body) = std::ffi::CString::new(candidate.body.as_str()) else {
        return GpuiMacOSNotificationDeliveryResult::Failed;
    };
    let icon_data_url = match candidate.icon_data_url.as_deref() {
        Some(value) => match std::ffi::CString::new(value) {
            Ok(value) => Some(value),
            Err(_) => return GpuiMacOSNotificationDeliveryResult::Failed,
        },
        None => None,
    };
    GpuiMacOSNotificationDeliveryResult::from_native_code(unsafe {
        GhostexGpuiDeliverSessionAttentionNotification(
            session_id.as_ptr(),
            title.as_ptr(),
            body.as_ptr(),
            icon_data_url
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
        )
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    _candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_gpui_menu_bar_status_item(state: &GpuiSidebarSessionStatusIndicatorsState) {
    if let Some(visible_state) = gpui_menu_bar_status_item_visible_state(state) {
        let (_project_owners, project_entries) = gpui_menu_bar_status_native_projects(state);
        unsafe {
            GhostexGpuiApplyMenuBarStatusItemWithProjects(
                visible_state.attention_count,
                visible_state.working_count,
                visible_state.available_count,
                project_entries.as_ptr(),
                project_entries.len(),
            );
        }
    } else {
        hide_gpui_menu_bar_status_item();
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_gpui_menu_bar_status_item(_state: &GpuiSidebarSessionStatusIndicatorsState) {}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_native_projects(
    state: &GpuiSidebarSessionStatusIndicatorsState,
) -> (
    Vec<GpuiMenuBarStatusNativeProjectOwner>,
    Vec<GpuiMenuBarStatusNativeProjectEntry>,
) {
    let project_owners = state
        .projects
        .iter()
        .filter_map(gpui_menu_bar_status_native_project_owner)
        .collect::<Vec<_>>();
    let project_entries = project_owners
        .iter()
        .map(|project| GpuiMenuBarStatusNativeProjectEntry {
            project_id: project.project_id.as_ptr(),
            title: project.title.as_ptr(),
            sessions: project.entries.as_ptr(),
            session_count: project.entries.len(),
        })
        .collect::<Vec<_>>();
    (project_owners, project_entries)
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_app_shots_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUIAppShots 2026-06-25-23:07:
    The macOS App Shots monitor is process-global, but its callback target is the live GPUI root entity only. Register and remove it with the root lifecycle so native flags monitors cannot route captures to stale windows or fallback targets.
    */
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiAppShotsCallbackTarget { app, async_app });
    });
    let images_directory = shared_settings::ghostex_storage_paths().images_dir();
    let Ok(images_directory) = std::ffi::CString::new(gpui_path_string(&images_directory)) else {
        return;
    };
    unsafe {
        GhostexGpuiInstallAppShotsEventMonitors(images_directory.as_ptr());
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_app_shots_callback_target() {
    unsafe {
        GhostexGpuiRemoveAppShotsEventMonitors();
    }
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_shots_callback_target() -> Option<GpuiAppShotsCallbackTarget> {
    GPUI_APP_SHOTS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_session_attention_notification_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUISettingsNotifications 2026-06-26-06:56:
    macOS attention notification responses are process-global UserNotifications callbacks, but GPUI should route clicks only while a live root app target is registered. The callback target carries no notification content and only dispatches the copied bounded session id through the existing status/pet activation path.
    */
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() =
            Some(GpuiSessionAttentionNotificationCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_session_attention_notification_callback_target() {
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_session_attention_notification_callback_target()
-> Option<GpuiSessionAttentionNotificationCallbackTarget> {
    GPUI_SESSION_ATTENTION_NOTIFICATION_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_accessibility_display_options_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    NSWorkspace accessibility display-option notifications are process-global, but Reduce Motion should update only the live GPUI root. Register the callback target with app lifecycle and carry only the current boolean preference into the pet animation gate.
    */
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() =
            Some(GpuiAccessibilityDisplayOptionsCallbackTarget { app, async_app });
    });
    unsafe {
        GhostexGpuiInstallAccessibilityDisplayOptionsMonitor();
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_accessibility_display_options_callback_target() {
    unsafe {
        GhostexGpuiRemoveAccessibilityDisplayOptionsMonitor();
    }
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_accessibility_display_options_callback_target()
-> Option<GpuiAccessibilityDisplayOptionsCallbackTarget> {
    GPUI_ACCESSIBILITY_DISPLAY_OPTIONS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_sparkle_updater_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    // Sparkle delegate callbacks are process-global main-thread calls; route
    // them only while a live GPUI root is registered, mirroring the other
    // native callback targets.
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiSparkleUpdaterCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_sparkle_updater_callback_target() {
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_sparkle_updater_callback_target() -> Option<GpuiSparkleUpdaterCallbackTarget> {
    GPUI_SPARKLE_UPDATER_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_os_integration_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiOsIntegrationCallbackTarget { app, async_app });
    });
    let pending = GPUI_PENDING_OS_INTEGRATION_URLS.with(|urls| urls.borrow_mut().split_off(0));
    if !pending.is_empty() {
        queue_gpui_os_integration_urls(pending);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_os_integration_callback_target() {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_os_integration_callback_target() -> Option<GpuiOsIntegrationCallbackTarget> {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_first_responder_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().insert(
            gpui_root_view as usize,
            GpuiFirstResponderCallbackTarget { app, async_app },
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_first_responder_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
) {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS.with(|targets| {
        targets.borrow_mut().remove(&(gpui_root_view as usize));
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_first_responder_callback_target(
    gpui_root_view: *mut std::ffi::c_void,
) -> Option<GpuiFirstResponderCallbackTarget> {
    GPUI_FIRST_RESPONDER_CALLBACK_TARGETS
        .with(|targets| targets.borrow().get(&(gpui_root_view as usize)).cloned())
}
