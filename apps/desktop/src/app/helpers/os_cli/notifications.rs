use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

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
            Self::Authorized => "The operating system allows Ghostex notification banners.",
            Self::Provisional => {
                "The operating system allows provisional Ghostex notification banners."
            }
            Self::NotDetermined => "Notification permission has not been decided yet.",
            Self::Denied => {
                "The operating system is not allowing Ghostex notification banners. Use Open Notification Settings to allow notifications."
            }
            Self::Unsupported => {
                "System notification banners are not available in this GPUI build."
            }
            Self::Unknown => "Ghostex couldn't read notification permission status.",
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
                    "Played the completion sound and sent a notification test.",
                    true,
                )
            } else if completion_sound_enabled {
                (
                    "Sent a notification test. The completion sound preview reported a separate failure.",
                    true,
                )
            } else {
                ("Sent a notification test.", true)
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionDenied => {
            if completion_sound_enabled && played_sound {
                (
                    "Played the completion sound, but the operating system is not allowing Ghostex notification banners. Use Open Notification Settings to allow notifications.",
                    false,
                )
            } else {
                (
                    "The operating system is not allowing Ghostex notification banners. Use Open Notification Settings to allow notifications.",
                    false,
                )
            }
        }
        GpuiMacOSNotificationDeliveryResult::PermissionNotDetermined => (
            "The operating system did not return a notification permission decision. Use the notification permission button or Open Notification Settings.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Unsupported => (
            "System notification banners are not available in this GPUI build.",
            false,
        ),
        GpuiMacOSNotificationDeliveryResult::Failed => {
            ("Ghostex couldn't send a notification test.", false)
        }
        GpuiMacOSNotificationDeliveryResult::Unknown => (
            "Ghostex couldn't determine whether the notification test was sent.",
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
    CDXC:Settings 2026-06-24-11:59:
    GPUI sound preview may play only validated bundled completion sound filenames from the app bundle/sidebar resources or the repository media directory used by local GPUI runs. The command never accepts a path from React and falls back to an explicit unsupported status when the sound asset or platform playback command is unavailable.
    */
    let file_name = gpui_completion_sound_file_name(sound);
    let Some(sound_path) = gpui_completion_sound_path(file_name) else {
        return Err("Ghostex couldn't find a bundled completion sound asset.".to_string());
    };
    gpui_spawn_completion_sound_player(&sound_path)
}

pub(crate) const GPUI_COPY_SOUND_FILE_NAME: &str = "copy.mp3";

/// The not-moving sound: a session that became stuck (gx-core `Effect::SessionStuckRaised`).
pub(crate) const GPUI_NOT_MOVING_SOUND_FILE_NAME: &str = "not-moving.mp3";

/// CDXC:Coordinators 2026-11-05 DECISION:
/// User: a thread still not moving 8 minutes after Ghostex nudged it (and any other agent session
/// quiet for 8 minutes) plays "a NEW sound made for this", distinct from the completion sounds.
/// `not-moving.mp3` was synthesized for Ghostex with ffmpeg's `aevalsrc` (a soft "uh-oh": A#4
/// falling to a wobbling G4, three harmonics each, 0.95 s, -15 dB mean like the other pings), so
/// it is ours to ship. It plays unless agent sounds are off (Agent finished sound set to Off, or
/// the older `completionBellEnabled` false), whichever finished sound is picked, since it means
/// something else.
pub(crate) fn gpui_play_not_moving_sound() {
    let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let settings = settings_snapshot.object();
    if settings.get("completionBellEnabled") == Some(&serde_json::Value::Bool(false))
        || settings
            .get("completionSound")
            .and_then(serde_json::Value::as_str)
            == Some("off")
    {
        return;
    }
    let Some(sound_path) = gpui_completion_sound_path(GPUI_NOT_MOVING_SOUND_FILE_NAME) else {
        return;
    };
    let _ = gpui_spawn_completion_sound_player(&sound_path);
}

/// CDXC:Clipboard 2026-09-15 DECISION:
/// User: "I want this copy sound to actually play everywhere in the app when we copy something."
/// Every user-initiated copy in the desktop app funnels through this one function: the native GPUI copy sites, terminal copy on select and Cmd+C in both terminal engines, and every React page through the `playCopySound` bridge message.
/// It owns the bundled `copy.mp3` asset and the `copySound` setting (Settings > Notifications > Sounds), so no call site carries its own gate or audio element.
/// SEE-ALSO: packages/core-ui/copy-sound.ts (React entry point), apps/desktop/src/app/remote_conn/app_modal_bridge.rs, apps/desktop/src/app/session_chat/, apps/desktop/src/app/workspace_events.rs (bridge arms), apps/desktop/scripts/build-macos-app.sh (asset list).
pub(crate) fn gpui_play_copy_sound() {
    let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let enabled = settings_snapshot
        .object()
        .get("copySound")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let Some(sound_path) = gpui_completion_sound_path(GPUI_COPY_SOUND_FILE_NAME) else {
        return;
    };
    let _ = gpui_spawn_completion_sound_player(&sound_path);
}

/// CDXC:Clipboard 2026-09-22 DECISION:
/// User: "We need to add copy indicators for every place that you copy in the app. We need to show
/// a small tooltip whenever they copy that appears on screen that says 'Copied!'". Every copy site
/// gives its feedback through this one function, after writing the clipboard itself or through
/// `gpui_copy_to_clipboard`: the copy sound when that setting is on, and the "Copied!" bubble at
/// the pointer (`app::window::copied_indicator`). The React pages inside the app reach it through
/// the `playCopySound` bridge message they already post, so they need no second message.
pub(crate) fn gpui_copy_feedback(cx: &mut gpui::App) {
    gpui_play_copy_sound();
    crate::app::window::copied_indicator::show_copied_indicator(cx);
}

/// Writes `item` to the clipboard with the feedback every copy in the app gives. A lone picture
/// goes through [`gpui_copy_picture_to_clipboard`], which converts it to PNG and reports a copy
/// that did not land (a page's Copy image in the browser view comes here).
pub(crate) fn gpui_copy_to_clipboard(item: gpui::ClipboardItem, cx: &mut gpui::App) {
    if let [gpui::ClipboardEntry::Image(picture)] = item.entries() {
        let picture = picture.clone();
        crate::app::helpers::gpui_copy_picture_to_clipboard(&picture, cx);
        return;
    }
    cx.write_to_clipboard(item);
    gpui_copy_feedback(cx);
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::from_native_code(unsafe {
        GhostexGpuiRequestNotificationAuthorization()
    })
}

/// Windows asks no permission up front: this reports whether Windows lets Ghostex show toasts.
#[cfg(target_os = "windows")]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    gpui_windows_notification_status()
}

/// Linux notification daemons ask no permission; delivery itself reports a missing `notify-send`.
#[cfg(target_os = "linux")]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::Authorized
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(crate) fn gpui_request_macos_notification_permission()
-> GpuiMacOSNotificationAuthorizationStatus {
    GpuiMacOSNotificationAuthorizationStatus::Unsupported
}

/// Why this computer cannot show Ghostex's agent alerts, for the onboarding's "Ping me when an
/// agent needs me" row; `None` when nothing is known to be wrong.
///
/// CDXC:Notifications 2026-10-28 WHY:
/// The onboarding turned alerts on with no check outside macOS. Linux shows them through
/// `notify-send`, which minimal installs leave out, and Windows can have Ghostex's notifications
/// switched off; both can be read without sending anything. macOS asks for permission itself the
/// first time an alert is sent, so it reports nothing here.
pub(crate) fn gpui_attention_notification_problem() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if crate::app::helpers::gpui_which_command("notify-send").is_none() {
            return Some(
                "This computer can't show alerts yet: install libnotify (the notify-send command)."
                    .to_string(),
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        if gpui_windows_notification_status() == GpuiMacOSNotificationAuthorizationStatus::Denied {
            return Some(
                "Windows has notifications from Ghostex turned off: turn them on in Settings > System > Notifications."
                    .to_string(),
            );
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    GpuiMacOSNotificationDeliveryResult::from_native_code(unsafe {
        GhostexGpuiDeliverSettingsTestNotification()
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    gpui_deliver_windows_test_notification()
}

#[cfg(target_os = "linux")]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    gpui_deliver_linux_notification(
        "Agent task complete",
        "This is a Ghostex notification test.",
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(crate) fn gpui_deliver_macos_settings_test_notification() -> GpuiMacOSNotificationDeliveryResult
{
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    /*
    CDXC:StatusPet 2026-06-26-07:31:
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

#[cfg(target_os = "windows")]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    gpui_deliver_windows_notification(candidate)
}

// Linux has no variant: session and Browser page banners go through GPUI there
// (gpui_show_linux_session_attention_notification).
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(crate) fn gpui_deliver_macos_session_attention_notification(
    _candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    GpuiMacOSNotificationDeliveryResult::Unsupported
}

/// CDXC:Notifications 2026-10-05 WHY:
/// Linux shows system notifications through the desktop's freedesktop notification daemon via `notify-send` (libnotify), which every major desktop ships. A missing `notify-send` reports the banners as unavailable. Session attention and Browser page banners no longer come through here (see `gpui_show_linux_session_attention_notification`); the Settings test banner still does.
#[cfg(target_os = "linux")]
pub(crate) fn gpui_deliver_linux_notification(
    title: &str,
    body: &str,
) -> GpuiMacOSNotificationDeliveryResult {
    let status = std::process::Command::new("notify-send")
        .args(["--app-name=Ghostex", "--", title, body])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => GpuiMacOSNotificationDeliveryResult::Sent,
        Ok(_) => GpuiMacOSNotificationDeliveryResult::Failed,
        Err(_) => GpuiMacOSNotificationDeliveryResult::Unsupported,
    }
}

/// CDXC:Notifications 2026-10-28 WHY:
/// Clicking a Ghostex banner on Linux did nothing, where Windows and macOS bring Ghostex up on that session: `notify-send` is fire-and-forget. A session banner now goes over the freedesktop D-Bus interface through GPUI (`gpui_linux` system_notifications.rs, notify-rust), which offers the daemon's "default" action and waits for it on a thread per banner; the click comes back here with the session id as the tag and takes the same path as a Windows toast click (`activate_system_notification`).
#[cfg(target_os = "linux")]
pub(crate) fn gpui_show_linux_session_attention_notification(
    candidate: GpuiSessionAttentionNotificationCandidate,
    cx: &gpui::App,
) {
    cx.show_system_notification(gpui::SystemNotification {
        tag: candidate.session_id.into(),
        title: candidate.title.into(),
        body: candidate.body.into(),
        actions: Vec::new(),
    });
}

/// Banner clicks reach the lead window: GPUI hands them to one app-wide handler, which a new lead
/// replaces, ending the previous lead's loop with its sender.
#[cfg(target_os = "linux")]
pub(crate) fn register_gpui_linux_notification_click_target(
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    use futures::StreamExt as _;
    let (sender, mut receiver) = futures::channel::mpsc::unbounded::<String>();
    cx.set_app_identity("ghostex", "Ghostex");
    cx.on_system_notification_response(move |response, _cx| {
        if response.action_id.is_none() {
            sender.unbounded_send(response.tag.to_string()).ok();
        }
    });
    cx.spawn(async move |this, cx| {
        while let Some(target) = receiver.next().await {
            if this
                .update_in(cx, |this, window, cx| {
                    this.activate_system_notification(target, window, cx);
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

/// What the menu bar status item last showed, and whose state it showed (see
/// `apply_gpui_menu_bar_status_item`).
#[cfg(target_os = "macos")]
struct GpuiMenuBarStatusItemShown {
    /// The address of the lead window's status state that was applied, standing for that window.
    owner: usize,
    badge: Option<GpuiMenuBarStatusItemState>,
    projects: Vec<GpuiStatusIndicatorProjectState>,
}

#[cfg(target_os = "macos")]
thread_local! {
    static GPUI_MENU_BAR_STATUS_ITEM_SHOWN: std::cell::RefCell<Option<GpuiMenuBarStatusItemShown>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether two project lists draw the same Running Agents rows: the ids, titles, statuses, order
/// and last-active times the dropdown receives, not the project icons or question marks it never
/// shows.
#[cfg(target_os = "macos")]
fn gpui_menu_bar_status_rows_match(
    shown: &[GpuiStatusIndicatorProjectState],
    next: &[GpuiStatusIndicatorProjectState],
) -> bool {
    shown.len() == next.len()
        && shown.iter().zip(next).all(|(shown, next)| {
            shown.project_id == next.project_id
                && shown.title == next.title
                && shown.sessions.len() == next.sessions.len()
                && shown
                    .sessions
                    .iter()
                    .zip(&next.sessions)
                    .all(|(shown, next)| {
                        shown.session_id == next.session_id
                            && shown.title == next.title
                            && shown.last_active_at == next.last_active_at
                            && shown.status == next.status
                            && shown.order == next.order
                    })
        })
}

/// CDXC:StatusPet 2026-10-23 WHY:
/// Every change of the sidebar's status indicators used to rebuild the status item's badge image
/// and its Running Agents rows, although most changes (a section count, a project icon, a question
/// mark) are things the item never shows. It is applied again only when its badge or its rows
/// would differ, or when another window became the lead: the closing lead hides the item
/// (`hide_gpui_menu_bar_status_item` in core.rs), and its successor's state lives elsewhere.
#[cfg(target_os = "macos")]
pub(crate) fn apply_gpui_menu_bar_status_item(state: &GpuiSidebarSessionStatusIndicatorsState) {
    let owner = state as *const GpuiSidebarSessionStatusIndicatorsState as usize;
    let badge = gpui_menu_bar_status_item_visible_state(state);
    let unchanged = GPUI_MENU_BAR_STATUS_ITEM_SHOWN.with_borrow(|shown| {
        shown.as_ref().is_some_and(|shown| {
            shown.owner == owner
                && shown.badge == badge
                && (badge.is_none()
                    || gpui_menu_bar_status_rows_match(&shown.projects, &state.projects))
        })
    });
    if unchanged {
        return;
    }
    GPUI_MENU_BAR_STATUS_ITEM_SHOWN.with_borrow_mut(|shown| {
        *shown = Some(GpuiMenuBarStatusItemShown {
            owner,
            badge,
            projects: if badge.is_some() {
                state.projects.clone()
            } else {
                Vec::new()
            },
        });
    });
    if let Some(visible_state) = badge {
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
pub(crate) fn register_gpui_session_attention_notification_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:Notifications 2026-06-26-06:56:
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
    CDXC:StatusPet 2026-06-26-07:31:
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

pub(crate) fn unregister_gpui_os_integration_callback_target() {
    GPUI_OS_INTEGRATION_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

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
