use std::time::Duration;

use gpui::ClipboardItem;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn connected_gpui_remote_previous_session_sources(
        &self,
    ) -> Vec<GpuiRemotePreviousSessionSource> {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(machines) = settings_snapshot
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        machines
            .iter()
            .filter_map(|machine| {
                let remote_machine_id = gpui_remote_machine_id_from_value(machine)?;
                let target =
                    self.gpui_remote_gxserver_request_target(remote_machine_id.as_str())?;
                let machine_name = machine
                    .as_object()
                    .and_then(|machine| gpui_remote_machine_string_field(machine, "name"));
                Some(GpuiRemotePreviousSessionSource {
                    machine_name,
                    remote_machine_id,
                    target,
                })
            })
            .collect()
    }

    pub(crate) fn handle_gpui_remote_session_native_action(
        &mut self,
        message: GpuiSidebarNativeProjectPathActionMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(reference) =
            gpui_remote_attach_session_reference_from_project_id(message.project_id.as_str())
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "GPUI could not identify the remote session.",
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        // A Cloud Box is no saved machine; its pane attaches through its gxserver
        // (helpers/remote/cloud_box_attach.rs).
        let config = if ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(
            reference.remote_machine_id.as_str(),
        )
        .is_some()
        {
            Some(gpui_cloud_box_machine_config(&reference.remote_machine_id))
        } else {
            gpui_remote_machine_config_from_settings(
                settings_snapshot.object(),
                reference.remote_machine_id.as_str(),
            )
        };
        let Some(config) = config else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return;
        };

        if message.action == GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal {
            let remote_key = GpuiRemoteAttachSessionKey::from(&reference);
            // CDXC:SessionChat 2026-09-20 WHY:
            // A remote session click now carries the agent's Default Agent View like a local one, so the same rule
            // applies: a session that already has a tab owns its recorded view, and only one without a tab yet
            // takes the launch intent (see `focus_local_workspace_terminal_from_message`).
            let session_has_tab = self
                .remote_attach_sessions
                .get(&remote_key)
                .copied()
                .is_some_and(|shell_session_id| {
                    self.agents_workspace
                        .pane_id_for_session(shell_session_id)
                        .is_some()
                });
            if message.preferred_interface == GpuiPreferredAgentInterface::Chat && !session_has_tab
            {
                self.pending_agents_chat_launch_intents
                    .insert(GpuiWorkspaceTerminalSessionKey::Remote(remote_key.clone()));
            }
            // Every open re-decides the intent, so a stale keep-view left by
            // a failed plan cannot silence a later plain click.
            if message.keep_view {
                self.pending_keep_view_remote_focus.insert(remote_key);
            } else {
                self.pending_keep_view_remote_focus.remove(&remote_key);
            }
            self.begin_gpui_remote_attach_terminal_open(
                reference,
                config,
                target,
                None,
                AgentsWorkspaceNewTerminalPlacement::Tab,
                cx,
            );
        }
    }

    pub(crate) fn handle_gpui_remote_project_native_action(
        &mut self,
        message: GpuiSidebarNativeProjectPathActionMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(reference) =
            gpui_remote_project_reference_from_project_id(message.project_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Remote action unavailable",
                "GPUI could not identify the remote project.",
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using that project.",
                cx,
            );
            return;
        };

        match message.action {
            GpuiSidebarNativeProjectPathAction::CopyRemoteProjectPath => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_remote_gxserver_project_path_by_id(
                                &target,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(path) => {
                            gpui_copy_to_clipboard(ClipboardItem::new_string(path), cx);
                            this.dispatch_gpui_workspace_action_toast(
                                "info",
                                "Remote project path copied",
                                "Remote path copied to the clipboard.",
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote path unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal => {
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote terminal unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let remote_machine_id = reference.remote_machine_id.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_remote_gxserver_rpc_result(
                                &target,
                                "/api/restoreRecentProject",
                                &serde_json::json!({ "projectId": reference.project_id.as_str() }),
                                Duration::from_secs(10),
                            )?;
                            gpui_create_remote_project_workspace_terminal(
                                &config, &target, &reference,
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok((reference, plan)) => {
                            let key = GpuiRemoteAttachSessionKey::from(&reference);
                            this.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
                            this.open_gpui_remote_attach_terminal(
                                reference,
                                plan,
                                None,
                                AgentsWorkspaceNewTerminalPlacement::Tab,
                                GpuiRemoteAttachOpenIntent::CreatedByThisAction,
                                cx,
                            );
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                &remote_machine_id,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote terminal unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInIde
            | GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInVscode
            | GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInZed => {
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote IDE open unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let action = message.action;
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_project_in_ide(
                                &config,
                                &target,
                                action,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote IDE open unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteProjectInFiles => {
                self.open_remote_project_in_files_view(message.project_id.as_str(), cx);
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInTarget => {
                let Some(target_id) = message.target_id else {
                    return;
                };
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote open unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let resolved_command = gpui_installed_editor_open_target(target_id.as_str())
                    .and_then(|target| match target.launch {
                        GpuiOpenTargetLaunch::BuiltIn {
                            resolved_command, ..
                        } => resolved_command,
                        _ => None,
                    });
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_project_in_open_target(
                                &config,
                                &target,
                                reference.project_id.as_str(),
                                target_id.as_str(),
                                resolved_command.as_deref(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote open unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteExistingPullRequestInBrowser => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_existing_project_pull_request_in_browser(
                                &target,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote pull request unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteSidebarGitChangedFileInIde => {
                let Some(file_path) = message.file_path else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote file open unavailable",
                        "Choose a changed file from the current remote Git review.",
                        cx,
                    );
                    return;
                };
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote file open unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_sidebar_git_changed_file_in_ide(
                                &config,
                                &target,
                                reference.project_id.as_str(),
                                file_path.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote file open unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteProjectPortsBrowser => {
                /*
                CDXC:RemoteMachines 2026-07-30:
                A remote project's Browser pane opens on the machine's
                listening-ports page: Rust lists the sockets over the saved
                SSH configuration, renders a local HTML page whose rows link
                to `http://<ssh host>:<port>`, and opens it in the remote
                project's own browser tab model. The renderer contributed only
                the fixed action plus the machine-scoped project id; hosts,
                ports, and process names never cross back through CEF.
                */
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote browser unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let scoped_project_id = gpui_remote_scoped_project_id(
                    reference.remote_machine_id.as_str(),
                    reference.project_id.as_str(),
                );
                let execution_target = target.execution_target.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_prepare_remote_ports_browser_page(&config, &execution_target)
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(page_url) => {
                            this.open_browser_url_from_renderer_command(
                                GpuiSidebarOpenBrowserUrlMessage {
                                    url: page_url,
                                    reuse: GpuiBrowserRendererOpenReuse::Similar,
                                    from_quick_header: false,
                                    project_id: Some(scoped_project_id),
                                },
                                window,
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote ports unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            _ => {}
        }
    }
}
