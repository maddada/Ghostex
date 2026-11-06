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
        CDXC:RemoteMachines 2026-06-24-14:34:
        Settings `reconnectRemoteMachine` must mirror the macOS app's Remote gxserver connect button: read the saved remote machine from shared Settings, start/read the remote daemon token over SSH, store only the token in Keychain, then open a checked localhost tunnel. The command may carry only the bounded machine id, install approval flag, and automatic-attempt flag; it must not carry host/user/path/token/password/command/output data from React.

        CDXC:RemoteMachines 2026-06-24-20:08:
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
                "Ghostex couldn't identify the remote machine to connect.",
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
        if !self.remote_reconnect_admit(&remote_machine_id, automatic) {
            return;
        }
        let connect_generation =
            self.next_gpui_remote_gxserver_connect_generation(&remote_machine_id);
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let config = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            &remote_machine_id,
        );
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
            self.refresh_source_code_server_runtime_child(cx);
            // CDXC:CodeEditor 2026-09-23 WHY:
            // Code owns a separate SSH connection. Restarting it for an automatic API-only reconnect races the old listener's shutdown and can accept that old listener as the new launch's readiness.
            let preserve_code = automatic
                && !install_approved
                && self.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Ready
                && config.as_ref().is_some_and(|config| {
                    !config.disabled && self.source_code_server_runtime.target.as_ref().is_some_and(|target| {
                        matches!(&target.endpoint, SourceCodeServerRuntimeEndpoint::Remote { machine_config, .. } if machine_config == config)
                    })
                });
            if !preserve_code {
                self.stop_source_code_server_runtime(cx);
            }
        }
        let Some(config) = config else {
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
            // CDXC:Notices 2026-11-06: the toasts name the computer, never "GPUI" or "gxserver" (Q1410, Q1412).
            let machine = saved_machine_name(&remote_machine_id);
            let (title, description) = if install_approved {
                (
                    format!("Setting up Ghostex on {machine}…"),
                    "This takes a minute the first time.",
                )
            } else {
                (format!("Connecting to {machine}…"), "")
            };
            self.dispatch_gpui_app_modal_toast("info", &title, description, cx);
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
                    // Its Ghostex version and update, for the machine tab and Settings > Remote (app_update.rs).
                    self.refresh_remote_app_update(remote_machine_id.clone(), cx);
                    // Agents here can use the machine, and the machine's agents this computer.
                    self.renew_connected_computers(true, cx);
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
                    if reconnects_active_remote_docs {
                        /*
                        CDXC:RemoteMachines 2026-08-29:
                        The titlebar Actions snapshot for a remote project is
                        read from the machine that owns it, so any refresh that
                        ran while the tunnel was down came back empty. The
                        active project id is unchanged by a reconnect, so the
                        active-project path will not re-run that read — do it
                        here, where the tunnel just became usable.
                        */
                        self.refresh_titlebar_actions_in_background(cx);
                    }
                    self.ensure_source_code_server_runtime_for_current_context(cx);
                    // Restored chat panes cannot create their page until the remote bootstrap is available.
                    self.reconcile_agents_pane_surfaces(cx);
                }
                self.dispatch_gpui_remote_machine_status(
                    remote_machine_id.as_str(),
                    "connected",
                    cx,
                );
                if self
                    .browser_tabs
                    .tabs
                    .iter()
                    .any(|tab| tab.remote_machine_id.as_deref() == Some(remote_machine_id.as_str()))
                {
                    self.ensure_remote_browser_tunnel(&remote_machine_id, true, cx);
                }
                /*
                A connect may have installed or upgraded the remote package, so
                refresh the version Settings shows next to its Install/Update
                action instead of leaving the pre-connect answer on screen.
                */
                self.probe_gpui_remote_gxserver_install(remote_machine_id.clone(), cx);
                if !automatic {
                    let machine = saved_machine_name(&remote_machine_id);
                    self.dispatch_gpui_app_modal_toast(
                        "success",
                        &format!("Connected to {machine}"),
                        "",
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
}

/// The name the remote computer was saved under in Settings, or "the other computer".
fn saved_machine_name(machine_id: &str) -> String {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .and_then(|machines| {
            machines.iter().find(|machine| {
                machine.get("id").and_then(serde_json::Value::as_str) == Some(machine_id)
            })
        })
        .and_then(|machine| machine.get("name").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "the other computer".to_string())
}
