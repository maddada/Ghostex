use futures::StreamExt as _;
use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_reconnect_remote_machine_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
        Settings `reconnectRemoteMachine` must mirror the macOS app's Remote gxserver connect button: read the saved remote machine from shared Settings, start/read the remote daemon token over SSH, store only the token in Keychain, then open a checked localhost tunnel. The command may carry only the bounded machine id, install approval flag, and automatic-attempt flag; it must not carry host/user/path/token/password/command/output data from React.

        CDXC:GPUIRemoteMachines 2026-06-24-20:08:
        Approved install retries should surface the existing `installing` remote-machine state while Rust uploads/installs the bundled package, but React still provides no SSH details, package paths, commands, tokens, stdout/stderr, or daemon response authority.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote connect failed",
                "GPUI could not identify the remote machine to connect.",
                cx,
            );
            return;
        };
        let install_approved = command
            .get("installApproved")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let automatic = command
            .get("automatic")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let connect_generation =
            self.next_gpui_remote_gxserver_connect_generation(&remote_machine_id);
        if self
            .source_code_server_runtime
            .target
            .as_ref()
            .is_some_and(|target| {
                matches!(
                    &target.endpoint,
                    SourceCodeServerRuntimeEndpoint::Remote {
                        remote_machine_id: runtime_machine_id,
                        ..
                    } if runtime_machine_id == &remote_machine_id
                )
            })
        {
            self.stop_source_code_server_runtime(cx);
        }
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            &remote_machine_id,
        ) else {
            self.dispatch_gpui_remote_machine_status(remote_machine_id.as_str(), "invalid", cx);
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote connect failed",
                "The saved remote machine is missing or incomplete.",
                cx,
            );
            return;
        };
        if automatic && config.disabled {
            self.stop_gpui_remote_gxserver_connection(&remote_machine_id);
            self.dispatch_gpui_remote_machine_status(
                remote_machine_id.as_str(),
                "disconnected",
                cx,
            );
            return;
        }

        self.stop_gpui_remote_gxserver_connection(&remote_machine_id);
        let status_state = if install_approved {
            "installing"
        } else {
            GpuiRemoteGxserverConnectState::Connecting.wire_status_state()
        };
        self.dispatch_gpui_remote_machine_status(remote_machine_id.as_str(), status_state, cx);
        if !automatic {
            self.dispatch_gpui_app_modal_toast(
                "info",
                if install_approved {
                    "Installing remote gxserver"
                } else {
                    "Connecting remote gxserver"
                },
                if install_approved {
                    "GPUI is installing the remote gxserver package on the saved remote machine."
                } else {
                    "GPUI is connecting to the saved remote machine over SSH."
                },
                cx,
            );
        }
        let (progress_tx, mut progress_rx) = mpsc::unbounded::<GpuiRemoteGxserverConnectProgress>();
        let progress_remote_machine_id = remote_machine_id.clone();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = progress_rx.next().await {
                let should_continue = this
                    .update(cx, |this, cx| {
                        if !this.gpui_remote_gxserver_connect_generation_is_current(
                            progress_remote_machine_id.as_str(),
                            connect_generation,
                        ) {
                            return false;
                        }
                        if !this
                            .remote_machine_connect_states
                            .get(progress_remote_machine_id.as_str())
                            .is_some_and(|state| {
                                gpui_remote_gxserver_status_state_is_connect_progress(
                                    state.as_str(),
                                )
                            })
                        {
                            return false;
                        }
                        this.dispatch_gpui_remote_machine_status(
                            progress_remote_machine_id.as_str(),
                            progress.state.wire_status_state(),
                            cx,
                        );
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_connect_remote_gxserver(config, install_approved, Some(progress_tx))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_reconnect_remote_machine(
                    remote_machine_id,
                    connect_generation,
                    automatic,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn finish_gpui_reconnect_remote_machine(
        &mut self,
        remote_machine_id: String,
        connect_generation: u64,
        automatic: bool,
        mut result: GpuiRemoteGxserverConnectResult,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gpui_remote_gxserver_connect_generation_is_current(
            remote_machine_id.as_str(),
            connect_generation,
        ) {
            result.terminate_connection();
            return;
        }
        match result.state {
            GpuiRemoteGxserverConnectState::Connected => {
                if let Some(connection) = result.connection {
                    /*
                    Reconnects replace the machine's connection entry. The
                    outgoing connection owns an `ssh -N` tunnel child that
                    nothing kills on drop, so terminate it explicitly or every
                    reconnect leaks a live tunnel process.
                    */
                    if let Some(mut replaced) = self
                        .remote_gxserver_connections
                        .insert(remote_machine_id.clone(), connection)
                    {
                        replaced.terminate();
                    }
                    self.restart_gpui_remote_gxserver_presentation_stream(
                        remote_machine_id.clone(),
                        gpui_remote_gxserver_presentation_client_id(remote_machine_id.as_str()),
                        None,
                        cx,
                    );
                    self.clear_project_editor_companion_remote_attach_states_for_machine(
                        remote_machine_id.as_str(),
                    );
                    self.reattach_project_editor_companion_remote_terminal_after_reconnect(
                        remote_machine_id.as_str(),
                        connect_generation,
                        cx,
                    );
                    let reconnects_active_remote_docs = self
                        .latest_sidebar_project_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.active_project_id.as_ref())
                        .and_then(|project_id| {
                            gpui_remote_project_reference_from_project_id(project_id.0.as_str())
                        })
                        .is_some_and(|reference| reference.remote_machine_id == remote_machine_id);
                    if reconnects_active_remote_docs {
                        /*
                        The synthetic Docs resource handler captures the exact
                        tunnel target that owns it. Recreate only the active
                        remote Docs surface after reconnect so resource reads
                        use the replacement tunnel instead of a dead port/token.
                        */
                        self.remove_project_workarea_runtime_cef_surface(
                            ProjectWorkareaCefSurfaceSlotKey::Manage,
                            cx,
                        );
                        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                    }
                    self.ensure_source_code_server_runtime_for_current_context(cx);
                }
                self.dispatch_gpui_remote_machine_status(
                    remote_machine_id.as_str(),
                    "connected",
                    cx,
                );
                /*
                A connect may have installed or upgraded the remote package, so
                refresh the version Settings shows next to its Install/Update
                action instead of leaving the pre-connect answer on screen.
                */
                self.probe_gpui_remote_gxserver_install(remote_machine_id.clone(), cx);
                if !automatic {
                    self.dispatch_gpui_app_modal_toast(
                        "success",
                        "Remote gxserver connected",
                        "The remote gxserver tunnel is ready.",
                        cx,
                    );
                }
            }
            GpuiRemoteGxserverConnectState::InstallApprovalRequired => {
                self.dispatch_gpui_remote_machine_status(
                    remote_machine_id.as_str(),
                    "installApprovalRequired",
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Install approval required",
                    "gxserver is not installed on that machine. Approve the install to continue.",
                    cx,
                );
                self.open_gpui_remote_gxserver_install_modal(remote_machine_id, cx);
            }
            _ => {
                self.dispatch_gpui_remote_machine_status_with_message(
                    remote_machine_id.as_str(),
                    result.state.wire_status_state(),
                    Some(result.message.as_str()),
                    cx,
                );
                if !automatic {
                    self.dispatch_gpui_app_modal_toast(
                        result.state.toast_level(),
                        result.state.toast_title(),
                        result.message.as_str(),
                        cx,
                    );
                }
            }
        }
    }

    pub(crate) fn reattach_project_editor_companion_remote_terminal_after_reconnect(
        &mut self,
        remote_machine_id: &str,
        connection_generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.active_mode.is_project_editor_mode()
            || !self.project_editor_shell.left_companion_visible
        {
            return;
        }
        let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.project_editor_companion_active_terminal_key()
        else {
            return;
        };
        if key.remote_machine_id != remote_machine_id {
            return;
        }
        if !self.gpui_remote_gxserver_connect_generation_is_current(
            remote_machine_id,
            connection_generation,
        ) {
            return;
        }
        let Some(shell_session_id) = self.remote_attach_sessions.get(&key).copied() else {
            return;
        };
        let Some(slot_id) = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| slot_id.session_id == shell_session_id)
        else {
            return;
        };
        let attempt = GpuiProjectEditorCompanionRemoteAttachAttempt {
            connection_generation,
            remote_key: key.clone(),
        };
        if !self.project_editor_companion_remote_attach_attempt_is_current(slot_id, &attempt) {
            return;
        }
        self.project_editor_companion_remote_attach_states.insert(
            slot_id,
            GpuiProjectEditorCompanionRemoteAttachState::Preparing(attempt.clone()),
        );
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            self.record_project_editor_companion_remote_attach_unavailable(
                slot_id,
                attempt,
                "Reconnect the remote machine to show this terminal.".to_string(),
            );
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings_snapshot.object(), remote_machine_id)
        else {
            self.record_project_editor_companion_remote_attach_unavailable(
                slot_id,
                attempt,
                "The saved remote machine is missing required SSH settings.".to_string(),
            );
            return;
        };
        let reference = GpuiRemoteAttachSessionReference {
            remote_machine_id: key.remote_machine_id,
            project_id: key.project_id,
            session_id: key.session_id,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let prepare_reference = reference.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_remote_attach_terminal_plan(
                        &config,
                        &target,
                        &prepare_reference,
                        true,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.gpui_remote_gxserver_connect_generation_is_current(
                    reference.remote_machine_id.as_str(),
                    connection_generation,
                ) || !this
                    .project_editor_companion_remote_attach_states
                    .get(&slot_id)
                    .is_some_and(|state| state.attempt() == &attempt)
                    || !this.project_editor_companion_remote_attach_attempt_is_current(
                        slot_id, &attempt,
                    )
                {
                    if this
                        .project_editor_companion_remote_attach_states
                        .get(&slot_id)
                        .is_some_and(|state| state.attempt() == &attempt)
                    {
                        this.project_editor_companion_remote_attach_states
                            .remove(&slot_id);
                    }
                    return;
                }
                let expected_key = GpuiRemoteAttachSessionKey::from(&reference);
                if this.project_editor_companion_active_terminal_key()
                    != Some(GpuiWorkspaceTerminalSessionKey::Remote(expected_key))
                {
                    this.project_editor_companion_remote_attach_states
                        .remove(&slot_id);
                    return;
                }
                let plan = match result {
                    Ok(plan) => plan,
                    Err(message) => {
                        this.record_project_editor_companion_remote_attach_unavailable(
                            slot_id, attempt, message,
                        );
                        cx.notify();
                        return;
                    }
                };
                this.project_editor_companion_remote_attach_states
                    .remove(&slot_id);
                this.open_gpui_remote_attach_terminal(
                    reference,
                    plan,
                    None,
                    AgentsWorkspaceNewTerminalPlacement::Tab,
                    GpuiRemoteAttachOpenIntent::AttachExistingSession,
                    cx,
                );
            });
        })
        .detach();
    }
}
