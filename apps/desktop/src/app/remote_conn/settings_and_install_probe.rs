use crate::app::helpers::*;
use crate::app::hotkeys::*;
use crate::*;

/*
CDXC:RemoteMachines 2026-11-08 WHY:
Windows repro of GitHub #508: Add machine saved the machine, the reconnect ladder connected it at
once (a newly enabled machine connects, reconnect_ladder.rs), and that connect ran without the
password typed in the same dialog, whose save lands a moment later. It failed `authFailed` in 0.3 s
("The machine rejected the saved SSH username, password or key"), which is never retried, so the
user saw a refused login for a password they had just typed. A password save now marks the machine
while it runs: a connect that started without it is started again once it lands, and one that fails
`authFailed` meanwhile waits for it instead of reporting the refusal.
*/
thread_local! {
    /// Machines whose SSH password save is running (the app's main thread only).
    static PASSWORD_SAVES_RUNNING: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Machines whose connect failed `authFailed` while their password was being saved.
    static CONNECT_AFTER_PASSWORD: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// A connect of `machine_id` failed `authFailed`: `true` (and it waits) when the machine's password
/// is being saved right now, so the refusal is for a login made without it.
pub(crate) fn wait_for_password_being_saved(machine_id: &str) -> bool {
    let running = PASSWORD_SAVES_RUNNING.with_borrow(|running| running.contains(machine_id));
    if running {
        CONNECT_AFTER_PASSWORD.with_borrow_mut(|waiting| waiting.insert(machine_id.to_string()));
    }
    running
}

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
        // CDXC:Onboarding 2026-09-15 WHY:
        // The new chat tips card saves read IDs directly (native_chat/new_chat_tips.rs), while React's normalized settings omit this key or carry an older copy.
        // Preserve the latest native value so a later sidebar or Settings save cannot make read tips unread again.
        match previous_settings_object
            .get(crate::app::native_chat::new_chat_tips::NEW_CHAT_TIPS_READ_SETTINGS_KEY)
        {
            Some(read_ids) => {
                settings_object.insert(
                    crate::app::native_chat::new_chat_tips::NEW_CHAT_TIPS_READ_SETTINGS_KEY
                        .to_string(),
                    read_ids.clone(),
                );
            }
            None => {
                settings_object.remove(
                    crate::app::native_chat::new_chat_tips::NEW_CHAT_TIPS_READ_SETTINGS_KEY,
                );
            }
        }
        // Only explicit remote-machine UI and sidebar ordering saves may replace the
        // saved machine list; broad Settings saves keep the stored value. Mirrors
        // canSettingsUpdateSourceChangeRemoteMachines in packages/shared/ghostex-settings.ts (deleted 2026-10-01).
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
        let previous_hotkey_unbinds =
            gpui_configured_hotkey_unbinds_from_settings(&previous_settings_snapshot, cx);
        cx.bind_keys(previous_hotkey_unbinds);
        let previous_shell_hotkey_unbinds =
            crate::app::shell_hotkeys::gpui_shell_hotkey_unbinds_from_settings(
                &previous_settings_snapshot,
                cx,
            );
        cx.bind_keys(previous_shell_hotkey_unbinds);
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
        self.sync_gpui_removed_remote_machine_passwords_after_settings_save(
            &previous_settings_object,
            write_result.snapshot.object(),
            cx,
        );
        self.sync_gpui_disabled_remote_machine_connections_after_settings_save(
            write_result.snapshot.object(),
            cx,
        );
        if super::connected_computers::gpui_remote_machine_agent_access_changed(
            &previous_settings_object,
            write_result.snapshot.object(),
        ) {
            self.renew_connected_computers(true, cx);
        }
        for remote_machine_id in gpui_remote_machines_with_changed_connection(
            &previous_settings_object,
            write_result.snapshot.object(),
        ) {
            self.stop_gpui_remote_gxserver_connection(remote_machine_id.as_str());
            let mut command = serde_json::Map::new();
            command.insert(
                "remoteMachineId".to_string(),
                serde_json::Value::String(remote_machine_id),
            );
            self.handle_gpui_reconnect_remote_machine_message(&command, cx);
        }
        let previous_app_icon_source_id =
            app_icon::source_id_from_settings(&previous_settings_object);
        let next_app_icon_source_id =
            app_icon::source_id_from_settings(write_result.snapshot.object());
        if previous_app_icon_source_id != next_app_icon_source_id {
            let _ = app_icon::apply_persisted_source_id(&next_app_icon_source_id);
        }
        #[cfg(target_os = "windows")]
        {
            self.prompt_windows_environment_restart_after_settings_save(
                &previous_settings_object,
                write_result.snapshot.object(),
                cx,
            );
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
                self.start_gpui_local_gxserver_bootstrap(true, cx);
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
        for remote_machine_id in gpui_remote_machine_connections_to_stop(
            next_settings,
            self.remote_gxserver_connections.keys(),
        ) {
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
        // A machine that is turned off or removed gives up its Easy Connect forwarder too.
        gpui_stop_easy_connect_forwards_of_unsaved_machines(next_settings);
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
                "Ghostex couldn't save the SSH password for this remote machine.",
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
                "Ghostex couldn't save the SSH password for this remote machine.",
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
        PASSWORD_SAVES_RUNNING.with_borrow_mut(|running| running.insert(remote_machine_id.clone()));
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
        PASSWORD_SAVES_RUNNING.with_borrow_mut(|running| running.remove(&remote_machine_id));
        let connect_waited =
            CONNECT_AFTER_PASSWORD.with_borrow_mut(|waiting| waiting.remove(&remote_machine_id));
        if connect_waited
            && (!has_password
                || !matches!(
                    keychain_result,
                    GpuiRemoteSshPasswordKeychainResult::Success
                ))
        {
            // No password landed: the refused login it waited on is reported after all.
            self.dispatch_gpui_remote_machine_status_with_message(
                remote_machine_id.as_str(),
                GpuiRemoteGxserverConnectState::AuthFailed.wire_status_state(),
                None,
                cx,
            );
        }
        match keychain_result {
            GpuiRemoteSshPasswordKeychainResult::Success => {}
            GpuiRemoteSshPasswordKeychainResult::Unsupported => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "Remote SSH password storage is unavailable in this build.",
                    cx,
                );
                return;
            }
            GpuiRemoteSshPasswordKeychainResult::Failed => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "Secure system storage could not update the SSH password.",
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
                "Could not update Settings after changing the saved SSH password.",
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
                "The password is stored in secure system storage."
            } else {
                "The SSH password was removed from secure system storage."
            },
            cx,
        );
        // A refused login is never retried automatically (`reconnect_ladder.rs`), so the new
        // password is what tries again; a connect still signing in started without it, so it
        // starts again with it (the newer connect makes the older one's answer ignored).
        if has_password
            && (connect_waited
                || self
                    .remote_machine_connect_states
                    .get(remote_machine_id.as_str())
                    .is_some_and(|state| state == "authFailed" || state == "connecting"))
        {
            self.remote_reconnect_from_sidebar(&remote_machine_id, cx);
        }
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
        if self.app_modal_window.is_none()
            && !self
                .native_app_modal_kind()
                .is_some_and(|kind| kind.is_settings_modal_entry())
        {
            /*
            Nothing can render the answer while no app modal is open (the React
            host or the native Settings modal), so skip the SSH round trip instead
            of probing every saved machine on the automatic startup connect pass.
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
                    "managed": probe.managed,
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
