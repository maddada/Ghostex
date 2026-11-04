//! Settings page commands: settings writes, app icon and background pickers, first launch, Ghostty and macOS settings links, sound previews, sidebar agents and actions, and per-project settings metadata.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_settings_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "updateSettings" => {
                self.handle_gpui_app_modal_update_settings_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "updateSettingsPatch" => {
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "openExternalUrl" => {
                self.receive_gpui_titlebar_resources_open_external_url_message(
                    &serde_json::Value::Object(command.clone()),
                );
            }
            "setAppIcon" => {
                self.handle_gpui_set_app_icon_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "pickTerminalBackgroundImageFile" => {
                self.handle_gpui_pick_terminal_background_image_message(cx);
            }
            "pickWindowGlassImageFile" => {
                self.handle_gpui_pick_window_glass_image_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "pickWindowGlassVideoFile" => {
                self.handle_gpui_pick_window_glass_video_message(
                    &serde_json::Value::Object(command.clone()),
                    cx,
                );
            }
            "pickFirstLaunchProjectFolder" => {
                self.handle_gpui_pick_first_launch_project_folder_message(cx);
            }
            "firstLaunchCreateProjectSession" => {
                self.handle_gpui_first_launch_create_project_session_message(command, cx);
            }
            "openGhosttySettingsDocs" => {
                self.open_gpui_trusted_url(
                    GPUI_GHOSTTY_SETTINGS_DOCS_URL,
                    "openGhosttySettingsDocs",
                    cx,
                );
            }
            "openAccessibilityPreferences" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
                    "openAccessibilityPreferences",
                    cx,
                );
            }
            "openScreenRecordingPreferences" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
                    "openScreenRecordingPreferences",
                    cx,
                );
            }
            // The Windows Developer Mode notice (settings_modal/tabs/windows_link_notice.rs).
            #[cfg(target_os = "windows")]
            "openWindowsDeveloperSettings" => {
                if let Err(message) = gpui_open_url(GPUI_WINDOWS_DEVELOPER_SETTINGS_URL) {
                    self.dispatch_gpui_settings_action_status(
                        "openWindowsDeveloperSettings",
                        false,
                        &message,
                        cx,
                    );
                }
            }
            #[cfg(target_os = "windows")]
            "openMacOSNotificationSettings" => {
                if let Err(message) = gpui_open_url(GPUI_WINDOWS_NOTIFICATION_SETTINGS_URL) {
                    self.dispatch_gpui_settings_action_status(
                        "openMacOSNotificationSettings",
                        false,
                        &message,
                        cx,
                    );
                }
            }
            #[cfg(not(target_os = "windows"))]
            "openMacOSNotificationSettings" => {
                self.open_gpui_macos_system_settings_url(
                    GPUI_MACOS_NOTIFICATION_SETTINGS_URL,
                    "openMacOSNotificationSettings",
                    cx,
                );
            }
            "requestMacOSNotificationPermission" => {
                self.request_gpui_macos_notification_permission(cx);
            }
            "playCompletionSoundPreview" => {
                self.play_gpui_completion_sound_preview(
                    command.get("sound").and_then(serde_json::Value::as_str),
                    cx,
                );
            }
            "testAgentTaskCompletion" => {
                self.test_gpui_agent_task_completion(cx);
            }
            "applyRecommendedGhosttySettings" => {
                self.update_gpui_ghostty_visible_settings(
                    shared_settings::apply_recommended_ghostty_visible_settings,
                    shared_settings::apply_recommended_ghostty_config_file,
                    cx,
                );
            }
            "resetGhosttySettingsToDefault" => {
                self.update_gpui_ghostty_visible_settings(
                    shared_settings::reset_ghostty_visible_settings_to_defaults,
                    shared_settings::reset_ghostty_config_file_to_defaults,
                    cx,
                );
            }
            "openGhosttyConfigFile" => {
                self.open_gpui_ghostty_config_file(cx);
            }
            "openGhostexTerminalConfigFile" => {
                self.open_gpui_ghostex_terminal_config_file(cx);
            }
            "importGhosttyConfig" => {
                self.import_gpui_ghostty_config(cx);
            }
            "saveSidebarAgent"
            | "deleteSidebarAgent"
            | "syncSidebarAgentOrder"
            | "setSidebarAgentsEnabled" => {
                self.handle_gpui_sidebar_agent_metadata_command(command, cx);
            }
            "saveSidebarCommand"
            | "deleteSidebarCommand"
            | "syncSidebarCommandOrder"
            | "saveGlobalSidebarCommand"
            | "deleteGlobalSidebarCommand"
            | "syncGlobalSidebarCommandOrder" => {
                self.handle_gpui_sidebar_command_metadata_command(command, cx);
            }
            "setProjectWorktreeCommand" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(command_text) = command
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::WorktreeCommand {
                        project_id,
                        command: command_text,
                    },
                    cx,
                );
            }
            "setProjectBeadsDisplayKey" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(display_key) = command
                    .get("displayKey")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::BeadsDisplayKey {
                        project_id,
                        display_key,
                    },
                    cx,
                );
            }
            "setProjectBeadsDirectory" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(directory) = command
                    .get("directory")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::BeadsDirectory {
                        project_id,
                        directory,
                    },
                    cx,
                );
            }
            "setProjectDocsDirectory" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                let Some(directory) = command
                    .get("directory")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                else {
                    return;
                };
                self.update_gpui_project_settings_metadata_in_background(
                    GpuiProjectSettingsMetadataUpdate::DocsDirectory {
                        project_id,
                        directory,
                    },
                    cx,
                );
            }
            _ => {}
        }
    }
}
