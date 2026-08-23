// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds macOS notification permission/test-notification/completion-sound preview plus the Portless settings sync, state refresh, setup prompt, and admin-action handlers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn request_gpui_macos_notification_permission(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISettingsNotifications 2026-06-24-12:44:
        The GPUI Settings notification permission action mirrors the native Settings boundary: read macOS authorization, request only alert permission when it is notDetermined, report denied as a system-settings repair path, and never fake success with an in-app notification fallback.
        */
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let status = background
                .spawn(async move { gpui_request_macos_notification_permission() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_notification_permission_status_message(status),
                    cx,
                );
                this.dispatch_gpui_settings_action_status(
                    "requestMacOSNotificationPermission",
                    status.available(),
                    status.message(),
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    status.toast_level(),
                    status.toast_title(),
                    status.message(),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn deliver_gpui_macos_settings_test_notification(
        &mut self,
        completion_sound_enabled: bool,
        played_sound: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_deliver_macos_settings_test_notification() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(status) = result.permission_status() {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                        gpui_notification_permission_status_message(status),
                        cx,
                    );
                }
                let (message, available) = gpui_macos_notification_test_action_message(
                    result,
                    completion_sound_enabled,
                    played_sound,
                );
                this.dispatch_gpui_settings_action_status(
                    "testAgentTaskCompletion",
                    available,
                    message,
                    cx,
                );
                if result == GpuiMacOSNotificationDeliveryResult::Sent {
                    this.dispatch_gpui_app_modal_toast(
                        "success",
                        "Test notification sent",
                        message,
                        cx,
                    );
                } else {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Test notification unavailable",
                        message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn play_gpui_completion_sound_preview(
        &mut self,
        sound: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let normalized_sound = gpui_normalize_completion_sound(sound);
        match gpui_play_completion_sound(normalized_sound) {
            Ok(()) => true,
            Err(message) => {
                self.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_sound_preview_status_message(false, &message),
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Sound preview unavailable",
                    &message,
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn test_gpui_agent_task_completion(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISettingsNotifications 2026-06-24-12:44:
        Settings test-agent-completion parity is intentionally bounded to the local test action. Continue the existing completion-sound preview when enabled, then send one generic no-sound macOS banner only when the Settings notification toggle is enabled; do not route real session attention notifications, names, paths, command text, URLs, or click-to-focus metadata through this path.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings = settings_snapshot.object();
        let completion_sound_enabled =
            json_bool_field(settings, "completionBellEnabled").unwrap_or(true);
        let notifications_enabled =
            json_bool_field(settings, "showMacOSAttentionNotifications").unwrap_or(true);
        let mut played_sound = false;
        if completion_sound_enabled {
            played_sound = self.play_gpui_completion_sound_preview(
                json_string_field(settings, "completionSound"),
                cx,
            );
        }

        if notifications_enabled {
            self.deliver_gpui_macos_settings_test_notification(
                completion_sound_enabled,
                played_sound,
                cx,
            );
            return;
        }

        if completion_sound_enabled {
            let (available, message) = if played_sound {
                (
                    true,
                    "Played the configured completion sound. macOS attention notifications are disabled in Settings.",
                )
            } else {
                (
                    false,
                    "The completion sound preview failed and macOS attention notifications are disabled in Settings.",
                )
            };
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                available,
                message,
                cx,
            );
        } else if !played_sound {
            let message = "Current Settings have completion sounds and macOS attention notifications disabled.";
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                false,
                message,
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Completion alerts are disabled",
                message,
                cx,
            );
        }
    }

    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn sync_gpui_portless_settings_after_save(
        &mut self,
        previous_settings: &serde_json::Map<String, serde_json::Value>,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
        Saved Settings changes in GPUI must synchronize Portless enabled/protocol state to gxserver through `/api/updatePortlessState`, matching the shared modal's production behavior without running privileged Portless admin scripts from this slice.

        CDXC:GPUISettingsPortlessBridge 2026-06-24-11:48:
        Portless Settings fan-out must stay bounded to shared contract values: missing or malformed `portlessEnabled` behaves as enabled, protocol is restricted to HTTP/HTTPS, and saved changes emit only `setEnabled` plus enabled-only `setProtocol` metadata RPCs so local Settings saves cannot carry paths, URLs, commands, tokens, or native admin payloads.
        */
        let previous_enabled = gpui_settings_portless_enabled(previous_settings);
        let next_enabled = gpui_settings_portless_enabled(next_settings);
        if previous_enabled != next_enabled {
            // macOS `syncPortlessEnabledSetting`: enabling re-arms the setup
            // prompt for this run; disabling suppresses it.
            if next_enabled {
                self.portless_setup_prompt_suppressed_until_restart = false;
            } else {
                self.suppress_gpui_portless_setup_prompt_for_this_run();
            }
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetEnabled {
                    enabled: next_enabled,
                },
                cx,
            );
        }

        let previous_protocol = gpui_settings_portless_protocol(previous_settings);
        let next_protocol = gpui_settings_portless_protocol(next_settings);
        if next_enabled && previous_protocol != next_protocol {
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetProtocol {
                    protocol: next_protocol,
                },
                cx,
            );
        }
    }

    pub(crate) fn sync_gpui_removed_remote_machine_passwords_after_settings_save(
        &mut self,
        previous_settings: &serde_json::Map<String, serde_json::Value>,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachinesSettings 2026-06-24-13:36:
        Normal GPUI Settings saves mirror native-sidebar cleanup for deleted Remote Machines: after the settings object is already saved, remove Keychain passwords for disappeared machines that carried `sshPasswordSaved === true`. This cleanup is silent and best-effort, carries only bounded ids, and never blocks the settings save or logs machine/user/host/password details.
        */
        let removed_ids =
            gpui_removed_remote_machine_password_ids(previous_settings, next_settings);
        if removed_ids.is_empty() {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            background
                .spawn(async move {
                    for remote_machine_id in removed_ids {
                        let _ =
                            gpui_save_remote_machine_password_to_keychain(&remote_machine_id, "");
                    }
                })
                .await;
        })
        .detach();
    }

    pub(crate) fn update_gpui_portless_state_in_background(
        &mut self,
        update: GpuiPortlessStateUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (portless_state, sidebar_state_message) = background
                .spawn(async move {
                    let portless_state = gpui_update_portless_gxserver_state(update).ok();
                    let sidebar_state_message =
                        gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
                            portless_state.clone(),
                            active_project_id.as_deref(),
                        );
                    (portless_state, sidebar_state_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
                if let Some(portless_state) = portless_state {
                    this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
                }
            });
        })
        .detach();
    }

    /// GPUI equivalent of macOS `maybeOpenPortlessSetupPrompt`: macOS
    /// re-evaluates on every sidebar HUD publish, while GPUI owns Portless
    /// state in Rust and evaluates after daemon bootstrap and after every
    /// Portless state update. Suppression is memory-only for this app run
    /// after Postpone, Cancel, Disable, or launching an admin action.
    pub(crate) fn maybe_open_gpui_portless_setup_prompt(
        &mut self,
        portless_state: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            return;
        }
        if self.active_portless_setup_prompt_mode.is_some()
            || self.portless_setup_prompt_suppressed_until_restart
        {
            return;
        }
        // GPUI hosts one app-modal window; auto-opening the prompt would
        // replace whatever modal the user has open (macOS stacks child
        // windows), so an occupied host defers the prompt to a later check.
        if self.app_modal_window.is_some() {
            /*
            CDXC:GPUIPortlessPromptDeferral 2026-08-18:
            "Later check" used to mean "whenever some other Portless state
            update happens to run again", which for a prompt resolved during
            startup meant never: the prompt was dropped for the whole run.
            Remember the deferral so closing the user's modal re-runs the
            check, and keep the run-scoped suppression untouched so Postpone,
            Cancel, and Disable still stop it for good.
            */
            self.portless_setup_prompt_pending_modal_close = true;
            return;
        }
        let settings_portless_enabled = gpui_settings_portless_enabled(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let Some((mode, protocol)) =
            gpui_resolve_portless_setup_prompt(settings_portless_enabled, portless_state)
        else {
            return;
        };
        self.active_portless_setup_prompt_mode = Some(mode);
        let modal = GpuiAppModalKind::PortlessSetup;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "mode": mode.as_str(),
            "protocol": protocol.as_str(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }

    pub(crate) fn start_gpui_portless_setup_prompt_check(&mut self, cx: &mut gpui::Context<Self>) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            self.suppress_gpui_portless_setup_prompt_for_this_run();
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetEnabled { enabled: false },
                cx,
            );
            return;
        }
        if self.active_portless_setup_prompt_mode.is_some()
            || self.portless_setup_prompt_suppressed_until_restart
        {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let portless_state = background
                .spawn(async { gpui_sidebar_portless_state_with_presentation() })
                .await;
            let Some(portless_state) = portless_state else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
            });
        })
        .detach();
    }

    pub(crate) fn suppress_gpui_portless_setup_prompt_for_this_run(&mut self) {
        self.portless_setup_prompt_suppressed_until_restart = true;
        self.active_portless_setup_prompt_mode = None;
        self.portless_setup_prompt_pending_modal_close = false;
    }

    /// CDXC:GPUIPortlessPromptDeferral 2026-08-18: the app-modal host is free
    /// again, so a prompt that was deferred because the user had a modal open
    /// gets its check re-run instead of being lost for the rest of the run.
    pub(crate) fn resume_deferred_gpui_portless_setup_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.portless_setup_prompt_pending_modal_close {
            return;
        }
        self.portless_setup_prompt_pending_modal_close = false;
        self.start_gpui_portless_setup_prompt_check(cx);
    }

    pub(crate) fn handle_gpui_set_portless_enabled_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command.get("enabled").and_then(serde_json::Value::as_bool) != Some(false) {
            return;
        }
        // The setup prompt's Disable button (macOS
        // `setPortlessEnabledFromSetupPrompt` suppresses before saving).
        self.suppress_gpui_portless_setup_prompt_for_this_run();
        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings_object.insert(
            "portlessEnabled".to_string(),
            serde_json::Value::Bool(false),
        );
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.update_gpui_portless_state_in_background(
            GpuiPortlessStateUpdate::SetEnabled { enabled: false },
            cx,
        );
    }

    pub(crate) fn handle_gpui_portless_admin_action_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            return;
        }
        let Some(action) = command
            .get("action")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_portless_admin_action)
        else {
            return;
        };
        /*
        CDXC:GPUIPortlessAdminBridge 2026-06-24-14:28:
        GPUI Settings/setup Portless admin commands now run through a fixed macOS helper equivalent to the reviewed Swift PortlessAdminClient. Accept only bounded action/protocol/requestId metadata, use the bundled Web/code-server Node plus Web/portless CLI runtime, record only sanitized result fields in gxserver, and refresh the app-modal HUD without logging scripts, paths, URLs, stdout/stderr, command text, tokens, environment values, project data, or user content.
        */
        let Some(request_id) = gpui_portless_admin_request_id(command) else {
            return;
        };
        let protocol = command
            .get("protocol")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_portless_protocol);
        // macOS suppresses the setup prompt when a non-remove admin action
        // launches (`runTrackedPortlessAdminAction`) and again when one fails
        // (`recordPortlessAdminResultInGxserver`); launch-time suppression
        // covers both.
        if action != GpuiPortlessAdminAction::Remove {
            self.suppress_gpui_portless_setup_prompt_for_this_run();
        }
        self.run_gpui_portless_admin_action_in_background(action, protocol, request_id, cx);
    }

    pub(crate) fn run_gpui_portless_admin_action_in_background(
        &mut self,
        action: GpuiPortlessAdminAction,
        protocol: Option<GpuiPortlessProtocol>,
        request_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (result, portless_state, sidebar_state_message) = background
                .spawn(async move {
                    let result = gpui_run_portless_admin_action(action, protocol, request_id);
                    let portless_state = gpui_update_portless_gxserver_state(
                        GpuiPortlessStateUpdate::RecordAdminResult {
                            action: result.action,
                            ok: result.ok,
                            protocol: result.protocol,
                        },
                    )
                    .ok()
                    .map(|state| gpui_portless_state_with_admin_result(state, &result));
                    let sidebar_state_message =
                        gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
                            portless_state.clone(),
                            active_project_id.as_deref(),
                        );
                    (result, portless_state, sidebar_state_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(result.message(), cx);
                if let Some(portless_state) = portless_state {
                    this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn update_gpui_project_settings_metadata_in_background(
        &mut self,
        update: GpuiProjectSettingsMetadataUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsProjectMetadata 2026-06-24-11:59:
        GPUI Settings project metadata edits are gxserver-owned. Apply worktree command, Beads display-key, and Beads directory changes through `/api/updateProject`, then rehydrate the open modal from real gxserver/shared Settings state instead of mutating local fake project rows or shelling out.
        */
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    let _ = gpui_update_project_settings_metadata(update);
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }

}
