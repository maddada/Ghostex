// Remote project Actions in the command pane.
//
// A remote project's terminal Action has to run its command on the machine
// that owns the project, but the user's mental model is the one the local
// Action already establishes: the Action opens the hidden bottom command pane
// and takes over its own tab there. This module owns that tab's whole
// lifecycle — selecting/replacing the Action-owned tab, mounting the SSH
// attach that reaches the freshly started remote session, reattaching it after
// an app restart, and closing the remote session the tab owns.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn command_remote_action_session_for_command_tab(
        &self,
        session_id: CommandSessionId,
    ) -> Option<GpuiRemoteAttachSessionReference> {
        self.command_remote_action_sessions
            .get(&session_id)
            .cloned()
            .or_else(|| {
                self.command_pane
                    .session(session_id)
                    .and_then(|session| session.remote_action_session.clone())
            })
    }

    pub(crate) fn remember_remote_command_action_session_for_command_tab(
        &mut self,
        session_id: CommandSessionId,
        reference: GpuiRemoteAttachSessionReference,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        A command tab has exactly one daemon identity. This one lives on the
        remote machine, so the local key and zmx name are cleared with the same
        gesture that installs it — a tab that carried both would be attached
        over SSH while local close, rename, and restore paths acted on a local
        session that is not the one on screen.
        */
        self.command_remote_action_sessions
            .insert(session_id, reference.clone());
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.gxserver_session_key = None;
            session.zmx_session_name = None;
            session.remote_action_session = Some(reference);
        }
    }

    pub(crate) fn close_remote_command_action_session_for_closed_tab(
        &mut self,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        The remote Action tab owns the `commands`-surface session it created on
        the remote daemon, so closing the tab closes that session, exactly like
        a local Action tab kills its local zmx session. Rerunning an Action
        always replaces the tab, so without this every rerun would leave a live
        session and its command process behind on the remote machine.

        When the owning machine is not connected there is no tunnel to close it
        through. That is reported honestly by doing nothing rather than by
        queueing a retry against a machine the user may never reconnect: the
        session stays visible in that machine's Running Sessions, which is
        where a disconnected remote's leftovers belong.
        */
        #[cfg(target_os = "macos")]
        self.command_remote_attach_askpass_scripts
            .remove(&session_id);
        let Some(reference) = self.command_remote_action_sessions.remove(&session_id) else {
            return;
        };
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.remote_action_session = None;
        }
        let Some(target) =
            self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str())
        else {
            return;
        };
        cx.background_executor()
            .spawn(async move { gpui_close_remote_command_action_session(&target, &reference) })
            .detach();
    }

    pub(crate) fn run_gpui_remote_command_action_terminal(
        &mut self,
        reference: GpuiRemoteProjectReference,
        command_id: String,
        title: String,
        command: String,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        Running a remote project's terminal Action reuses the remote attach path
        end to end: the owning daemon creates the session row and starts its zmx
        provider with the Action command, and this app attaches to it over the
        same saved SSH machine as any other remote terminal. The renderer never
        supplies the machine, token, SSH details, or the attach command; only the
        trusted HUD Action title and command reach the tunnel.

        CDXC:RemoteMachines 2026-08-29:
        The terminal itself belongs in the command pane, not the Agents
        workspace: a terminal Action is a command-pane run on every other
        project, and where the command happens to execute is not a reason for
        the user to find it in a different place. So the pane, the Action-owned
        tab, and the tab's reuse rules are decided here synchronously — exactly
        as `open_gpui_command_action_terminal` decides them — and only the
        create/start/attach round trip is deferred to the background.
        */
        let Some(target) =
            self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Remote Action unavailable",
                "Reconnect the remote machine before running its Actions.",
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
                "Remote Action unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return;
        };

        /*
        CDXC:RemoteMachines 2026-08-29:
        A remote Action never reruns inside an existing tab. The local idle-reuse
        path writes the next command into the mounted shell, but here that shell
        is an SSH attach to the *previous* remote session: the text would be
        typed into whatever that command left behind on the remote machine
        instead of starting the Action, and the new run's own remote session
        would have nowhere to mount. So every reusable owner tab is replaced —
        which also closes the remote session that tab owned — until the
        selection is a fresh tab. That covers the still-working owner too, the
        case the local path restarts the same way. Each pass removes one
        candidate, so the walk is bounded by the pane's own tabs; a tab that
        refuses to close keeps its Action and is revealed instead.
        */
        self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
        let mut selection = self
            .command_pane
            .select_or_create_action_session(command_id.clone(), title.clone());
        let mut remaining_replacements = self.command_pane.terminal_sessions.len();
        while !matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::Created
        ) {
            if remaining_replacements == 0
                || !self.close_command_pane_tab(selection.group_id, selection.session_id, cx)
            {
                self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                self.scroll_command_group_active_tab(selection.group_id);
                self.scroll_focused_command_active_tab();
                self.persist_shell_layout_state();
                cx.notify();
                return;
            }
            remaining_replacements -= 1;
            self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
            selection = self
                .command_pane
                .select_or_create_action_session(command_id.clone(), title.clone());
        }
        let group_id = selection.group_id;
        let session_id = selection.session_id;
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        };
        /*
        CDXC:RemoteMachines 2026-08-29:
        Claim the attach before yielding. The command-terminal sync pass mounts
        any rendered slot that has no launch payload by starting a *local*
        gxserver attach for it, so an unclaimed remote Action tab would race a
        local command session into the slot during the remote round trip.
        */
        self.command_gxserver_attach_pending.insert(session_id);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        if gpui_command_pane_default_action_should_focus_command_pane() {
            self.focus_command_pane();
            self.request_command_terminal_text_focus_handoff(slot_id);
        }
        self.begin_titlebar_quick_action_button_cooldown(cx);
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        cx.notify();

        let remote_machine_id = reference.remote_machine_id.clone();
        let command_pane_project_epoch = self.command_pane_project_epoch;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_run_remote_project_action_terminal(
                        &config,
                        &target,
                        &reference,
                        title.as_str(),
                        command.as_str(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.command_pane_project_epoch != command_pane_project_epoch {
                    /*
                    The command pane was swapped to another project while the
                    remote session was created. Its tab was parked before it
                    ever had an identity, so nothing references the new remote
                    session; close it instead of leaking it, and leave the live
                    pane alone.
                    */
                    if let Ok((reference, _plan)) = result {
                        let target = this.gpui_remote_gxserver_request_target(
                            reference.remote_machine_id.as_str(),
                        );
                        if let Some(target) = target {
                            cx.background_executor()
                                .spawn(async move {
                                    gpui_close_remote_command_action_session(&target, &reference)
                                })
                                .detach();
                        }
                    }
                    return;
                }
                this.command_gxserver_attach_pending.remove(&session_id);
                match result {
                    Ok((reference, plan)) => {
                        if this.insert_remote_command_terminal_attach_payload(
                            slot_id, reference, plan, cx,
                        ) {
                            this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                remote_machine_id,
                                false,
                                cx,
                            );
                        }
                    }
                    Err(message) => {
                        this.close_command_terminal_after_gxserver_attach_failure(
                            slot_id,
                            message.as_str(),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn start_remote_command_terminal_attach_for_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        reference: GpuiRemoteAttachSessionReference,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        A restored remote Action tab reattaches to the session it already owns
        on the remote machine, so its history survives a restart the same way a
        local Action tab's zmx session does. Its machine is connected
        asynchronously after launch, so an unresolvable machine is a not-ready
        state, not an error: return without claiming the slot and the command
        terminal sync pass asks again once the machine connects — the same
        contract `command_terminal_create_input_for_active_project` uses for an
        unhydrated project snapshot. This attach wakes the session, because
        unlike the launch path nothing has just started its provider.
        */
        if self
            .command_gxserver_attach_pending
            .contains(&slot_id.session_id)
        {
            return;
        }
        let Some(target) =
            self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str())
        else {
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            reference.remote_machine_id.as_str(),
        ) else {
            return;
        };
        self.command_gxserver_attach_pending
            .insert(slot_id.session_id);
        let command_pane_project_epoch = self.command_pane_project_epoch;
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
                        true,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.command_pane_project_epoch != command_pane_project_epoch {
                    // The command pane was swapped to another project while the
                    // attach plan was prepared. The target tab is parked, not
                    // closed, so neither mutate the live pane nor kill the
                    // parked project's remote session; reattach happens when
                    // its project becomes active again.
                    return;
                }
                this.command_gxserver_attach_pending
                    .remove(&slot_id.session_id);
                match result {
                    Ok(plan) => {
                        this.insert_remote_command_terminal_attach_payload(
                            slot_id, reference, plan, cx,
                        );
                    }
                    Err(message) => {
                        this.close_command_terminal_after_gxserver_attach_failure(
                            slot_id,
                            message.as_str(),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn insert_remote_command_terminal_attach_payload(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        reference: GpuiRemoteAttachSessionReference,
        plan: GpuiRemoteAttachTerminalPlan,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:RemoteMachines 2026-08-29:
        The command tab mounts the authoritative SSH attach command as its own
        process, so it also owns the saved-password askpass helper for as long
        as that process runs — dropping the handle deletes the temp script and
        stops its password server, and ssh would fall back to prompting on the
        pane's TTY.
        */
        if !self.command_pane.has_session(slot_id.session_id) {
            self.close_orphaned_remote_command_action_session(reference, cx);
            return false;
        }
        let Some(group_id) = command_pane_group_for_session(&self.command_pane, slot_id.session_id)
        else {
            self.close_orphaned_remote_command_action_session(reference, cx);
            return false;
        };
        let current_slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id: slot_id.session_id,
        };
        #[cfg(target_os = "macos")]
        let env_vars = plan
            .askpass
            .as_ref()
            .map(|askpass| {
                vec![
                    (
                        "DISPLAY".to_string(),
                        std::env::var("DISPLAY").unwrap_or_else(|_| "localhost:0".to_string()),
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
        let payload = CommandTerminalExplicitLaunchPayload {
            working_directory: None,
            command: Some(plan.terminal_command),
            env_vars,
            initial_input: None,
            wait_after_command: false,
        };
        if payload.to_ghostty_launch_payload().is_err() {
            self.close_orphaned_remote_command_action_session(reference, cx);
            self.close_command_terminal_after_gxserver_attach_failure(
                current_slot_id,
                "GPUI could not prepare the remote Action attach command.",
                cx,
            );
            return false;
        }
        self.remember_remote_command_action_session_for_command_tab(slot_id.session_id, reference);
        #[cfg(target_os = "macos")]
        if let Some(askpass) = plan.askpass {
            self.command_remote_attach_askpass_scripts
                .insert(slot_id.session_id, askpass);
        }
        /*
        Persist at the successful attach boundary, once the tab carries the
        remote identity, so a rebuild between now and the next layout write
        cannot restore a remote Action tab that has forgotten which session it
        owns and would create a second one.
        */
        self.persist_shell_layout_state();
        self.command_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(current_slot_id, payload);
        cx.notify();
        true
    }

    pub(crate) fn adopt_transferred_remote_command_action_session(
        &mut self,
        source_session_id: CommandSessionId,
        reference: GpuiRemoteAttachSessionReference,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        A remote Action tab dragged into the Agents workspace keeps the same
        live SSH attach, so its remote identity and askpass helper move to the
        workspace-side owners rather than being dropped — otherwise the command
        pane's own pruning would close the remote session out from under the
        tab the user just moved, and the SSH password helper would be deleted
        while ssh still needs it.
        */
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        self.remote_attach_sessions
            .insert(key.clone(), shell_session_id);
        #[cfg(target_os = "macos")]
        if let Some(askpass) = self
            .command_remote_attach_askpass_scripts
            .remove(&source_session_id)
        {
            self.remote_attach_askpass_scripts.insert(key, askpass);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = source_session_id;
        self.agents_sessions_pending_surface_transfer
            .insert(shell_session_id);
        self.promote_transferred_remote_command_action_surface_in_background(
            reference,
            shell_session_id,
            0,
            cx,
        );
    }

    pub(crate) fn promote_transferred_remote_command_action_surface_in_background(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        shell_session_id: TerminalSessionId,
        attempt: u32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        The remote mirror of `promote_transferred_gxserver_session_surface_in_background`:
        the moved tab is already live, so the surface update runs off the UI
        thread while the session is held out of sidebar-driven reconciliation.
        Retry a bounded number of times, then release the hold so reconciliation
        can resume rather than pinning the whole pass forever.
        */
        const MAX_ATTEMPTS: u32 = 5;
        let Some(target) =
            self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str())
        else {
            self.agents_sessions_pending_surface_transfer
                .remove(&shell_session_id);
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let update_reference = reference.clone();
            let result = background
                .spawn(async move {
                    gpui_update_remote_command_action_session_surface(
                        &target,
                        &update_reference,
                        "workspace",
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if result.is_ok() || attempt + 1 >= MAX_ATTEMPTS {
                    this.agents_sessions_pending_surface_transfer
                        .remove(&shell_session_id);
                    return;
                }
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this
                            .agents_sessions_pending_surface_transfer
                            .contains(&shell_session_id)
                        {
                            this.promote_transferred_remote_command_action_surface_in_background(
                                reference,
                                shell_session_id,
                                attempt + 1,
                                cx,
                            );
                        }
                    });
                })
                .detach();
            });
        })
        .detach();
    }

    pub(crate) fn close_orphaned_remote_command_action_session(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) =
            self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str())
        else {
            return;
        };
        cx.background_executor()
            .spawn(async move { gpui_close_remote_command_action_session(&target, &reference) })
            .detach();
    }
}
