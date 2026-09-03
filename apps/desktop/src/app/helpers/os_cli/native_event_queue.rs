use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::Result;
use gpui_component::{WindowExt, notification::Notification};

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_shots_c_string(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_action_c_string(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    gpui_status_bridge_id_allowed(text.as_str()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_session_attention_notification_action_c_string(
    ptr: *const std::ffi::c_char,
) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    gpui_status_bridge_id_allowed(text.as_str()).then_some(text)
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_session_attention_notification_click(session_id: String) {
    let Some(target) = gpui_session_attention_notification_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                cx.activate(true);
                window.activate_window();
                this.dispatch_gpui_status_pet_activation(session_id.as_str(), cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_accessibility_display_options_changed(should_reduce_motion: bool) {
    let Some(target) = gpui_accessibility_display_options_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_pet_overlay_reduce_motion_enabled(should_reduce_motion, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_available_changed(available: bool) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_available(available, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_downloading_changed(downloading: bool) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_downloading(downloading, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_sparkle_update_download_progress_changed(progress: Option<f64>) {
    let Some(target) = gpui_sparkle_updater_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, _window, cx| {
                this.set_gpui_update_download_progress(progress, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_app_shot_capture(capture: GpuiAppShotCapture) {
    let Some(target) = gpui_app_shots_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            /*
            CDXC:AppShots 2026-06-26-04:18:
            Native App Shots callbacks must copy capture metadata at the FFI boundary and then enqueue a foreground GPUI update without borrowing `AsyncApp` across the returned future. This keeps the C callback non-blocking while preserving the existing Rust/sidebar capture contract.
            */
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                this.handle_gpui_native_app_shot_capture(capture, window, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn queue_gpui_app_shot_status(message: &'static str) {
    let Some(target) = gpui_app_shots_callback_target() else {
        return;
    };
    let app = target.app.clone();
    let mut async_app = target.async_app.clone();
    let foreground = target.async_app.foreground_executor().clone();
    foreground
        .spawn(async move {
            let _ = app.update_in(&mut async_app, |this, window, cx| {
                window.push_notification(Notification::warning(message), cx);
                this.dispatch_gpui_app_modal_toast("warning", "App Shot Failed", message, cx);
            });
        })
        .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_completion_sound_player(path: &Path) -> Result<(), String> {
    std::process::Command::new("/usr/bin/afplay")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "GPUI could not start the macOS sound preview player.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_spawn_completion_sound_player(_path: &Path) -> Result<(), String> {
    Err("GPUI sound preview is not available on this platform yet.".to_string())
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    pub(crate) fn GhostexGpuiRequestNotificationAuthorization() -> i32;
    pub(crate) fn GhostexGpuiDeliverSettingsTestNotification() -> i32;
    pub(crate) fn GhostexGpuiAccessibilityDisplayShouldReduceMotion() -> i32;
    pub(crate) fn GhostexGpuiInstallAccessibilityDisplayOptionsMonitor();
    pub(crate) fn GhostexGpuiRemoveAccessibilityDisplayOptionsMonitor();
    pub(crate) fn GhostexGpuiInstallWorkspacePowerEventsMonitor();
    pub(crate) fn GhostexGpuiRemoveWorkspacePowerEventsMonitor();
    pub(crate) fn GhostexGpuiDeliverSessionAttentionNotification(
        session_id: *const std::ffi::c_char,
        title: *const std::ffi::c_char,
        body: *const std::ffi::c_char,
        icon_data_url: *const std::ffi::c_char,
    ) -> i32;
    pub(crate) fn GhostexGpuiInstallAppShotsEventMonitors(shots_directory: *const std::ffi::c_char);
    pub(crate) fn GhostexGpuiRemoveAppShotsEventMonitors();
    pub(crate) fn GhostexGpuiSparkleUpdaterStart() -> i32;
    pub(crate) fn GhostexGpuiSparkleCheckForUpdates();
    pub(crate) fn GhostexGpuiSparkleProbeForUpdateInformation();
    pub(crate) fn GhostexGpuiShowStandardAboutPanel();
    pub(crate) fn GhostexGpuiSetLidSleepPreventionEnabled(
        enabled: i32,
        install_if_needed: i32,
    ) -> i32;
    pub(crate) fn GhostexGpuiHeartbeatLidSleepPrevention() -> i32;
    pub(crate) fn GhostexGpuiApplyMenuBarStatusItemWithProjects(
        attention_count: u64,
        working_count: u64,
        available_count: u64,
        projects: *const GpuiMenuBarStatusNativeProjectEntry,
        project_count: usize,
    );
    pub(crate) fn GhostexGpuiHideMenuBarStatusItem();
    pub(crate) fn GhostexGpuiSaveRemoteSshPassword(
        remote_machine_id: *const std::ffi::c_char,
        password_bytes: *const u8,
        password_len: usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiCopyRemoteSshPassword(
        remote_machine_id: *const std::ffi::c_char,
        password_bytes: *mut u8,
        password_capacity: usize,
        password_len: *mut usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiSaveRemoteGxserverToken(
        remote_machine_id: *const std::ffi::c_char,
        token_bytes: *const u8,
        token_len: usize,
    ) -> i32;
    pub(crate) fn GhostexGpuiRemoveToastPopupWindowChrome(native_view: *mut std::ffi::c_void);
    pub(crate) fn GhostexGpuiAttachToastPopupToMainWindow(
        toast_native_view: *mut std::ffi::c_void,
        main_native_view: *mut std::ffi::c_void,
    );
    pub(crate) fn GhostexGpuiPrepareTitlebarPopupWindow(native_view: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_set_lid_sleep_prevention_enabled(
    enabled: bool,
    install_if_needed: bool,
) -> bool {
    unsafe {
        GhostexGpuiSetLidSleepPreventionEnabled(enabled as i32, install_if_needed as i32) == 1
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_heartbeat_lid_sleep_prevention() -> bool {
    unsafe { GhostexGpuiHeartbeatLidSleepPrevention() == 1 }
}

pub(crate) fn gpui_completion_sound_path(file_name: &str) -> Option<PathBuf> {
    if !gpui_is_bundled_sound_file_name(file_name) {
        return None;
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(bundle_root) = find_app_bundle_root(&executable) {
            for directory in [
                bundle_root.join("Contents/Resources/Web/sounds"),
                bundle_root.join("Contents/Resources/sidebar/sounds"),
            ] {
                let candidate = directory.join(file_name);
                if gpui_is_file(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_sound = manifest_dir
        .parent()?
        .parent()?
        .join("media/sounds")
        .join(file_name);
    gpui_is_file(&repo_sound).then_some(repo_sound)
}

pub(crate) fn gpui_is_bundled_sound_file_name(file_name: &str) -> bool {
    file_name.ends_with(".mp3")
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && !file_name.contains("..")
}

pub(crate) fn gpui_normalize_completion_sound(sound: Option<&str>) -> &'static str {
    match sound.unwrap_or("arcade") {
        "ping" => "ping",
        "pingdouble" => "pingdouble",
        "glass" => "glass",
        "glimmer" => "glimmer",
        "shamisen" => "shamisen",
        "shamisenreverb" => "shamisenreverb",
        "arcade" => "arcade",
        "arcadeboost" => "arcadeboost",
        "confirmation-001" => "confirmation-001",
        "confirmation-002" => "confirmation-002",
        "confirmation-003" => "confirmation-003",
        "confirmation-004" => "confirmation-004",
        "notification-pop" => "notification-pop",
        "success-chime" => "success-chime",
        "high-up" => "high-up",
        "high-down" => "high-down",
        "low-three-tone" => "low-three-tone",
        "tone-1" => "tone-1",
        "three-tone-1" => "three-tone-1",
        "three-tone-2" => "three-tone-2",
        "two-tone-1" => "two-tone-1",
        "two-tone-2" => "two-tone-2",
        "power-up-5" => "power-up-5",
        "power-up-6" => "power-up-6",
        "power-up-8" => "power-up-8",
        "coin-collect" => "coin-collect",
        "phaser-up-5" => "phaser-up-5",
        "zap-two-tone" => "zap-two-tone",
        "voiceover-pack-male-mission-completed" => "voiceover-pack-male-mission-completed",
        "voiceover-pack-female-mission-completed" => "voiceover-pack-female-mission-completed",
        "voiceover-pack-male-you-win" => "voiceover-pack-male-you-win",
        "voiceover-pack-female-congratulations" => "voiceover-pack-female-congratulations",
        "flawless-victory" => "flawless-victory",
        _ => "arcade",
    }
}

pub(crate) fn gpui_completion_sound_file_name(sound: &str) -> &'static str {
    match gpui_normalize_completion_sound(Some(sound)) {
        "ping" => "ping.mp3",
        "pingdouble" => "pingdouble.mp3",
        "glass" => "glass.mp3",
        "glimmer" => "glimmer.mp3",
        "shamisen" => "shamisen.mp3",
        "shamisenreverb" => "shamisenreverb.mp3",
        "arcadeboost" => "arcadeboost.mp3",
        "confirmation-001" => "confirmation-001.mp3",
        "confirmation-002" => "confirmation-002.mp3",
        "confirmation-003" => "confirmation-003.mp3",
        "confirmation-004" => "confirmation-004.mp3",
        "notification-pop" => "notification-pop.mp3",
        "success-chime" => "success-chime.mp3",
        "high-up" => "high-up.mp3",
        "high-down" => "high-down.mp3",
        "low-three-tone" => "low-three-tone.mp3",
        "tone-1" => "tone-1.mp3",
        "three-tone-1" => "three-tone-1.mp3",
        "three-tone-2" => "three-tone-2.mp3",
        "two-tone-1" => "two-tone-1.mp3",
        "two-tone-2" => "two-tone-2.mp3",
        "power-up-5" => "power-up-5.mp3",
        "power-up-6" => "power-up-6.mp3",
        "power-up-8" => "power-up-8.mp3",
        "coin-collect" => "coin-collect.mp3",
        "phaser-up-5" => "phaser-up-5.mp3",
        "zap-two-tone" => "zap-two-tone.mp3",
        "voiceover-pack-male-mission-completed" => "voiceover-pack-male-mission-completed.mp3",
        "voiceover-pack-female-mission-completed" => "voiceover-pack-female-mission-completed.mp3",
        "voiceover-pack-male-you-win" => "voiceover-pack-male-you-win.mp3",
        "voiceover-pack-female-congratulations" => "voiceover-pack-female-congratulations.mp3",
        "flawless-victory" => "flawless-victory.mp3",
        _ => "arcade.mp3",
    }
}
