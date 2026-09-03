use crate::app::helpers::*;
use crate::app::hotkeys::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_app_modal_update_settings_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Settings 2026-06-24-11:14:
        The shared React Settings modal posts normalized `updateSettings` objects through the app-modal host. GPUI must persist only object-shaped payloads through the shared settings service, reject malformed or non-object messages without panicking, then use the returned in-memory snapshot to refresh runtime settings and rehydrate the open modal host so the saved revision/object cannot drift.
        */
        let Some(settings_object) = message
            .get("settings")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        let previous_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let previous_agent_settings = previous_settings_snapshot.gxserver_agent_settings();
        let previous_settings_object = previous_settings_snapshot.object().clone();
        #[cfg(target_os = "windows")]
        let previous_windows_wsl_distribution = previous_settings_object
            .get("windowsWslDistribution")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut settings_object = settings_object.clone();
        // Only explicit remote-machine UI and sidebar ordering saves may replace the
        // saved machine list; broad Settings saves keep the stored value. Mirrors
        // canSettingsUpdateSourceChangeRemoteMachines in packages/shared/ghostex-settings.ts.
        let source_can_change_remote_machines = matches!(
            message.get("source").and_then(serde_json::Value::as_str),
            Some("settings:remoteMachines") | Some("sidebar:remoteMachineOrder")
        );
        if !source_can_change_remote_machines {
            match previous_settings_object.get("remoteMachines") {
                Some(previous_remote_machines) => {
                    settings_object.insert(
                        "remoteMachines".to_string(),
                        previous_remote_machines.clone(),
                    );
                }
                None => {
                    settings_object.remove("remoteMachines");
                }
            }
        }
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };

        let next_agent_settings = write_result.snapshot.gxserver_agent_settings();
        let ghostty_config_backed_setting_keys_changed =
            shared_settings::ghostty_terminal_config_backed_setting_keys_changed(
                &previous_settings_object,
                write_result.snapshot.object(),
            );
        cx.bind_keys(gpui_configured_hotkey_unbinds_from_settings(
            &previous_settings_snapshot,
        ));
        self.sync_gpui_ghostty_config_file_after_settings_save(
            &ghostty_config_backed_setting_keys_changed,
            &write_result.snapshot,
            cx,
        );
        // Config-backed terminal settings must reach Ghostty's managed file
        // before live GPUI terminals reload it. The old order reloaded the
        // previous theme and only wrote the new theme afterwards.
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.sync_gpui_gxserver_agent_settings_after_save(
            previous_agent_settings,
            next_agent_settings,
            cx,
        );
        self.sync_gpui_portless_settings_after_save(
            &previous_settings_object,
            write_result.snapshot.object(),
            cx,
        );
        self.sync_gpui_removed_remote_machine_passwords_after_settings_save(
            &previous_settings_object,
            write_result.snapshot.object(),
            cx,
        );
        self.sync_gpui_disabled_remote_machine_connections_after_settings_save(
            write_result.snapshot.object(),
            cx,
        );
        let previous_app_icon_source_id =
            app_icon::source_id_from_settings(&previous_settings_object);
        let next_app_icon_source_id =
            app_icon::source_id_from_settings(write_result.snapshot.object());
        if previous_app_icon_source_id != next_app_icon_source_id {
            let _ = app_icon::apply_persisted_source_id(&next_app_icon_source_id);
        }
        #[cfg(target_os = "windows")]
        {
            let next_windows_wsl_distribution = write_result
                .snapshot
                .object()
                .get("windowsWslDistribution")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if previous_windows_wsl_distribution != next_windows_wsl_distribution {
                /*
                WSL distribution changes apply to newly spawned ConPTY
                terminals. Drop only the process-memory distro selection/token
                and bootstrap the new distro; existing terminal processes are
                not killed or silently migrated.
                */
                windows_terminal_backend::reset();
                self.replay_sidebar_gxserver_bootstrap(cx);
                self.start_gpui_local_gxserver_bootstrap(cx);
            }
        }
    }

    /// Granular Settings controls save through `updateSettingsPatch` (the modal's
    /// `onPatch` path), not bulk `updateSettings`. Merging the patch onto the
    /// current stored snapshot — not the modal's `baseRevision` view — is what
    /// makes concurrent saves safe, matching macOS `saveSidebarSettingsPatch`.
    pub(crate) fn handle_gpui_app_modal_update_settings_patch_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(patch_object) = message.get("patch").and_then(serde_json::Value::as_object) else {
            return;
        };
        let mut merged_settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        for (key, value) in patch_object {
            merged_settings.insert(key.clone(), value.clone());
        }
        let merged_message = serde_json::json!({
            "settings": merged_settings,
            "source": message.get("source").cloned().unwrap_or(serde_json::Value::Null),
        });
        self.handle_gpui_app_modal_update_settings_message(&merged_message, cx);
    }

    pub(crate) fn sync_gpui_disabled_remote_machine_connections_after_settings_save(
        &mut self,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        for remote_machine_id in gpui_disabled_remote_machine_ids(next_settings) {
            if !self
                .remote_gxserver_connections
                .contains_key(remote_machine_id.as_str())
            {
                continue;
            }
            self.stop_gpui_remote_gxserver_connection(remote_machine_id.as_str());
            self.dispatch_gpui_remote_machine_status(
                remote_machine_id.as_str(),
                "disconnected",
                cx,
            );
        }
    }

    pub(crate) fn handle_gpui_save_remote_machine_password_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-06-24-13:36:
        Remote Machine password saves are explicit one-shot Settings commands. GPUI may accept the transient password only from this command, validate the bounded remote machine id, write or delete macOS Keychain first, and persist only the `sshPasswordSaved` marker after Keychain success.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "GPUI could not save the SSH password for this remote machine.",
                cx,
            );
            return;
        };
        let Some(password) = command
            .get("password")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "GPUI could not save the SSH password for this remote machine.",
                cx,
            );
            return;
        };

        if !gpui_settings_object_has_remote_machine_id(
            shared_settings::shared_sidebar_settings_snapshot().object(),
            &remote_machine_id,
        ) {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "The remote machine changed before GPUI could update its saved password.",
                cx,
            );
            return;
        }

        let has_password = !password.is_empty();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let keychain_result = background
                .spawn({
                    let remote_machine_id = remote_machine_id.clone();
                    async move {
                        let result = gpui_save_remote_machine_password_to_keychain(
                            &remote_machine_id,
                            password.as_str(),
                        );
                        drop(password);
                        result
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_remote_machine_password_save(
                    remote_machine_id,
                    has_password,
                    keychain_result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn finish_gpui_remote_machine_password_save(
        &mut self,
        remote_machine_id: String,
        has_password: bool,
        keychain_result: GpuiRemoteSshPasswordKeychainResult,
        cx: &mut gpui::Context<Self>,
    ) {
        match keychain_result {
            GpuiRemoteSshPasswordKeychainResult::Success => {}
            GpuiRemoteSshPasswordKeychainResult::Unsupported => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "Remote SSH password storage is only available on macOS.",
                    cx,
                );
                return;
            }
            GpuiRemoteSshPasswordKeychainResult::Failed => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "macOS Keychain could not update the SSH password.",
                    cx,
                );
                return;
            }
        }

        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        if !gpui_set_remote_machine_password_marker(
            &mut settings_object,
            &remote_machine_id,
            has_password,
        ) {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                gpui_remote_machine_password_failure_title(has_password),
                "The remote machine changed before GPUI could update its saved password.",
                cx,
            );
            return;
        }

        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                gpui_remote_machine_password_failure_title(has_password),
                "GPUI could not update Settings after the Keychain operation.",
                cx,
            );
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.dispatch_gpui_app_modal_toast(
            "success",
            if has_password {
                "SSH password saved"
            } else {
                "SSH password removed"
            },
            if has_password {
                "The password is stored in macOS Keychain."
            } else {
                "The Keychain password was removed."
            },
            cx,
        );
    }

    pub(crate) fn handle_gpui_probe_remote_gxserver_install_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-19:
        Settings asks whether a saved remote machine already runs gxserver so
        its action can read Install or Update and show the installed version.
        The command carries only the bounded machine id; SSH details come from
        the shared Settings snapshot exactly like reconnect.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        self.probe_gpui_remote_gxserver_install(remote_machine_id, cx);
    }

    pub(crate) fn probe_gpui_remote_gxserver_install(
        &mut self,
        remote_machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.app_modal_window.is_none() {
            /*
            Nothing can render the answer while no app modal is open, so skip
            the SSH round trip instead of probing every saved machine on the
            automatic startup connect pass.
            */
            return;
        }
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            &remote_machine_id,
        ) else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let probe = background
                .spawn(async move { gpui_probe_remote_gxserver_install(config) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut message = serde_json::json!({
                    "installed": probe.installed,
                    "remoteMachineId": remote_machine_id,
                    "type": "remoteGxserverInstallState",
                });
                if let Some(version) = probe.version
                    && let Some(object) = message.as_object_mut()
                {
                    object.insert("version".to_string(), serde_json::Value::String(version));
                }
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }
}
