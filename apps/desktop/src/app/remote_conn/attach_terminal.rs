use std::env;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn request_gpui_remote_attach_terminal_open(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self
            .remote_machine_connect_states
            .get(&reference.remote_machine_id)
            .map(String::as_str)
            != Some(GpuiRemoteGxserverConnectState::Connected.wire_status_state())
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return false;
        }
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return false;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            reference.remote_machine_id.as_str(),
        ) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return false;
        };
        self.begin_gpui_remote_attach_terminal_open(
            reference,
            config,
            target,
            requested_pane_id,
            placement,
            cx,
        );
        true
    }

    pub(crate) fn begin_gpui_remote_attach_terminal_open(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        config: GpuiRemoteMachineConfig,
        target: GpuiRemoteGxserverRequestTarget,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        /*
        A remote sidebar click or restored native placeholder activates the
        owning machine-scoped workspace before lookup. The same path then
        focuses a live SSH attachment or re-arms the existing canonical tab.
        */
        self.swap_agents_workspace_to_project_id(
            Some(gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            )),
            cx,
        );
        self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.openRequested",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        if self.focus_existing_gpui_remote_attach_terminal(&key, cx) {
            return;
        }
        let prepare_reference = reference.clone();
        let update_reference = reference;
        let remote_machine_id = key.remote_machine_id;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
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
            let _ = this.update(cx, |this, cx| match result {
                Ok(plan) => {
                    this.open_gpui_remote_attach_terminal(
                        update_reference,
                        plan,
                        requested_pane_id,
                        placement,
                        GpuiRemoteAttachOpenIntent::AttachExistingSession,
                        cx,
                    );
                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                        remote_machine_id,
                        false,
                        cx,
                    );
                }
                Err(message) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.remoteAttach.planFailed",
                        serde_json::json!({ "machineId": remote_machine_id }),
                    );
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Remote attach unavailable",
                        message.as_str(),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn focus_existing_gpui_remote_attach_terminal(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.remote_attach_sessions.get(key).copied() else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
            /*
            CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
            The mapped tab lives in the remote project's own workspace model.
            While another project's model is live its absence proves nothing;
            forgetting the mapping here would strand the parked tab and stack a
            duplicate attach tab on every click.
            */
            if self.agents_workspace_project_id.as_deref()
                == Some(
                    gpui_remote_scoped_project_id(
                        key.remote_machine_id.as_str(),
                        key.project_id.as_str(),
                    )
                    .as_str(),
                )
            {
                self.remote_attach_sessions.remove(key);
            }
            return false;
        };
        if self.agents_tab_selected_local_runtime_missing(pane_id, session_id) {
            /*
            Parking a workspace drops local attach clients, so a restored
            remote tab keeps Running presentation with no live SSH client.
            Report "not existing" so the open path re-arms this same tab with a
            freshly prepared SSH attach payload instead of focusing dead
            content.
            */
            return false;
        }
        let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
        let keep_editor_mode =
            self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key);
        let project_editor_mode = self.active_mode;
        self.agents_workspace.select_tab(pane_id, session_id);
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        if keep_editor_mode {
            self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                project_editor_mode,
                pane_id,
                session_id,
                &workspace_key,
            );
            self.retarget_project_editor_companion_to_workspace_terminal(
                project_editor_mode,
                session_id,
                &workspace_key,
                true,
                cx,
            );
        } else {
            self.active_mode = TitlebarMode::Agents;
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.request_agents_session_text_focus_handoff(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                },
                cx,
            );
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        self.set_sidebar_gxserver_remote_attach_focus_state(key, cx);
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        cx.notify();
        true
    }

    pub(crate) fn open_gpui_remote_attach_terminal(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        plan: GpuiRemoteAttachTerminalPlan,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        intent: GpuiRemoteAttachOpenIntent,
        cx: &mut gpui::Context<Self>,
    ) {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.remoteNewTerminal.tabMaterializeStarted",
            serde_json::json!({}),
        );
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        /*
        SSH plan preparation runs in the background, so by the time it
        completes the user may have focused something else. Mirror the local
        attach completion guard: only materialize the terminal while this
        remote session still owns the published presentation focus, instead
        of yanking the workspace back to the remote project.
        */
        let scoped_session_id = gpui_remote_scoped_session_id(
            key.remote_machine_id.as_str(),
            key.project_id.as_str(),
            key.session_id.as_str(),
        );
        if intent == GpuiRemoteAttachOpenIntent::AttachExistingSession
            && self
                .sidebar_gxserver_presentation_focus_state
                .focused_session_id
                .as_deref()
                != Some(scoped_session_id.as_str())
        {
            return;
        }
        /*
        CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
        Activate the remote project's own Agents workspace model before any
        tab lookup or creation. The per-project layout swap (2026-07-23)
        parks the outgoing model and clears the one-shot startup launch
        payload source, so mounting the SSH attach tab into whichever project
        happened to be live let a trailing project switch destroy the tab
        before it mounted. Swapping first makes the mount deterministic and
        turns the follow-up focus-state swap into a same-project no-op.
        */
        self.swap_agents_workspace_to_project_id(
            Some(gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            )),
            cx,
        );
        /*
        The sidebar projection is the display-title authority for both local
        and remote workspaces. Attach metadata is still needed before the
        sidebar has published a row, but it must not overwrite a projected
        display/primary/terminal/alias title when re-arming a restored tab.
        */
        let projected_tab_session = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_deref()
            .and_then(|sessions| {
                sessions.iter().find(|session| {
                    session.key == GpuiWorkspaceTerminalSessionKey::Remote(key.clone())
                })
            });
        let tab_title = projected_tab_session
            .map(|session| session.title.clone())
            .unwrap_or_else(|| plan.title.clone());
        let tab_agent_icon = projected_tab_session
            .and_then(|session| session.agent_icon)
            .or(plan.agent_icon);
        if self.focus_existing_gpui_remote_attach_terminal(&key, cx) {
            return;
        }
        #[cfg(target_os = "macos")]
        let env_vars = plan
            .askpass
            .as_ref()
            .map(|askpass| {
                vec![
                    (
                        "DISPLAY".to_string(),
                        env::var("DISPLAY").unwrap_or_else(|_| "localhost:0".to_string()),
                    ),
                    (
                        "SSH_ASKPASS".to_string(),
                        gpui_path_string(askpass.script.as_path()),
                    ),
                    ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
                ]
            })
            .unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let env_vars = Vec::new();
        let payload = AgentsTerminalExplicitLaunchPayload {
            working_directory: None,
            command: Some(plan.terminal_command),
            env_vars,
            initial_input: None,
            wait_after_command: false,
        };
        if payload.to_ghostty_launch_payload().is_err() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote attach unavailable",
                "GPUI could not prepare the remote attach terminal command.",
                cx,
            );
            return;
        }
        let resolved_requested_pane_id =
            requested_pane_id.unwrap_or(self.agents_workspace.focused_pane);
        if let Some(existing_session_id) = self.remote_attach_sessions.get(&key).copied() {
            if let Some(existing_pane_id) = self
                .agents_workspace
                .pane_id_for_session(existing_session_id)
            {
                /*
                The live-tab case already returned through
                focus_existing_gpui_remote_attach_terminal above, so this
                mapped tab is a restored one whose SSH attach client was
                dropped when its workspace was parked. Re-arm the same tab
                with the freshly prepared attach payload — the identical
                reuse path local sessions take after a restart — instead of
                stacking a duplicate attach tab.
                */
                let Some(placed_pane_id) = self
                    .agents_workspace
                    .place_existing_session_for_new_terminal(
                        existing_pane_id,
                        requested_pane_id.unwrap_or(existing_pane_id),
                        existing_session_id,
                        placement,
                    )
                else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Remote attach unavailable",
                        "GPUI could not place the remote terminal in the requested pane.",
                        cx,
                    );
                    return;
                };
                let runtime_session_id = self
                    .agents_terminal_runtime_sessions
                    .ensure_runtime_session_id(existing_session_id);
                let mount_slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id: placed_pane_id,
                    session_id: existing_session_id,
                };
                self.agents_terminal_launch_payload_source
                    .insert_explicit_payload_for_mount_slot(
                        runtime_session_id,
                        mount_slot_id,
                        payload.clone(),
                    );
                if let Some(session) = self
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == existing_session_id)
                {
                    session.title = tab_title.clone();
                    session.agent_icon = tab_agent_icon;
                }
                #[cfg(target_os = "macos")]
                if let Some(askpass) = plan.askpass {
                    self.remote_attach_askpass_scripts
                        .insert(key.clone(), askpass);
                }
                self.agents_workspace
                    .select_tab(placed_pane_id, existing_session_id);
                let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
                self.activate_preferred_agents_chat_launch_intent(existing_session_id, cx);
                if self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key)
                {
                    let project_editor_mode = self.active_mode;
                    self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                        project_editor_mode,
                        placed_pane_id,
                        existing_session_id,
                        &workspace_key,
                    );
                    self.retarget_project_editor_companion_to_workspace_terminal(
                        project_editor_mode,
                        existing_session_id,
                        &workspace_key,
                        true,
                        cx,
                    );
                } else {
                    self.active_mode = TitlebarMode::Agents;
                    self.set_shell_focus_with_terminal_handoff(
                        ShellFocusTarget::AgentsPane(placed_pane_id),
                        true,
                    );
                    self.request_agents_session_text_focus_handoff(mount_slot_id, cx);
                }
                self.scroll_workspace_pane_active_tab(placed_pane_id);
                self.persist_shell_layout_state();
                self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.remoteAttach.terminalOpened",
                    serde_json::json!({
                        "machineId": key.remote_machine_id,
                        "mode": "rearmedRestoredTab",
                        "sessionId": key.session_id,
                    }),
                );
                cx.notify();
                return;
            }
            self.remote_attach_sessions.remove(&key);
        }
        let created = match placement {
            AgentsWorkspaceNewTerminalPlacement::Tab => {
                self.agents_workspace.add_running_session_to_pane(
                    resolved_requested_pane_id,
                    tab_title.clone(),
                    tab_agent_icon,
                )
            }
            AgentsWorkspaceNewTerminalPlacement::SplitRight => self
                .agents_workspace
                .split_mounting_session_to_right_of_pane(resolved_requested_pane_id),
            AgentsWorkspaceNewTerminalPlacement::SplitBelow => self
                .agents_workspace
                .split_mounting_session_below_pane(resolved_requested_pane_id),
            AgentsWorkspaceNewTerminalPlacement::BottomRow => self
                .agents_workspace
                .resolve_action_pane_id(resolved_requested_pane_id)
                .map(|resolved_pane_id| {
                    self.agents_workspace.focus_pane(resolved_pane_id);
                    self.agents_workspace.append_mounting_session_bottom_row()
                }),
        };
        let Some((pane_id, session_id)) = created else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote attach unavailable",
                "GPUI could not create a terminal pane for the remote session.",
                cx,
            );
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = tab_title;
            session.agent_icon = tab_agent_icon;
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Running,
                false,
            );
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        let mount_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        };
        self.agents_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(runtime_session_id, mount_slot_id, payload);
        #[cfg(target_os = "macos")]
        if let Some(askpass) = plan.askpass {
            self.remote_attach_askpass_scripts
                .insert(key.clone(), askpass);
        }
        self.remote_attach_sessions.insert(key.clone(), session_id);
        let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        if self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key) {
            let project_editor_mode = self.active_mode;
            self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                project_editor_mode,
                pane_id,
                session_id,
                &workspace_key,
            );
            self.retarget_project_editor_companion_to_workspace_terminal(
                project_editor_mode,
                session_id,
                &workspace_key,
                true,
                cx,
            );
        } else {
            self.active_mode = TitlebarMode::Agents;
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.request_agents_session_text_focus_handoff(mount_slot_id, cx);
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.terminalOpened",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "mode": "createdRunningAttachTab",
                "sessionId": key.session_id,
            }),
        );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.remoteNewTerminal.tabMaterialized",
            serde_json::json!({}),
        );
        cx.notify();
    }
}
