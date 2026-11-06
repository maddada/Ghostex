//! Command terminal creation and gxserver session attach, promotion, titles and cleanup.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn prepare_hidden_command_pane_open_height_from_shared_settings(
        &mut self,
        window: &Window,
    ) {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        self.command_pane
            .prepare_hidden_open_with_default_height_px(
                command_pane_content_height(window),
                command_pane_default_height_px_from_shared_settings(&settings_snapshot),
            );
    }

    pub(crate) fn command_terminal_create_input_for_active_project(
        &self,
        title: String,
        startup_text: Option<String>,
        command_id: Option<String>,
        command_title: Option<String>,
    ) -> Result<GpuiCommandTerminalCreateInputResolution, String> {
        /*
        CDXC:Workarea 2026-07-04:
        Restored legacy command tabs can render before the first sidebar project
        snapshot hydrates. Treat only that missing snapshot as not-ready so the
        sync pass retries later; invalid hydrated project metadata and daemon/RPC
        failures remain honest terminal-close failures.
        */
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return Ok(GpuiCommandTerminalCreateInputResolution::NotReady);
        };
        let project_id = snapshot
            .active_project_id
            .as_ref()
            .map(|project_id| project_id.0.clone())
            .ok_or_else(|| "Command terminals need an active gxserver project.".to_string())?;
        if !gpui_remote_sidebar_project_id_allowed(project_id.as_str()) {
            return Err("The active gxserver project id is invalid.".to_string());
        }
        let cwd = snapshot
            .in_memory_project_path
            .as_ref()
            .and_then(|path| path.to_str())
            .map(str::to_string)
            .ok_or_else(|| "Command terminals need an active project path.".to_string())?;

        Ok(GpuiCommandTerminalCreateInputResolution::Ready(
            GpuiCommandTerminalCreateInput {
                command_id,
                command_title,
                cwd,
                project_id,
                startup_text,
                title,
            },
        ))
    }

    pub(crate) fn insert_command_terminal_gxserver_attach_payload(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        plan: GpuiCommandTerminalAttachPlan,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.command_pane.has_session(slot_id.session_id) {
            gpui_close_command_terminal_gxserver_session(&plan.key);
            return false;
        }
        let Some(group_id) = command_pane_group_for_session(&self.command_pane, slot_id.session_id)
        else {
            gpui_close_command_terminal_gxserver_session(&plan.key);
            return false;
        };
        let current_slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id: slot_id.session_id,
        };
        let payload = CommandTerminalExplicitLaunchPayload {
            working_directory: plan.working_directory,
            command: Some(plan.attach_command),
            env_vars: Vec::new(),
            initial_input: plan.initial_input,
            wait_after_command: false,
        };
        if payload.validate_launch_strings().is_err() {
            gpui_close_command_terminal_gxserver_session(&plan.key);
            self.close_command_terminal_after_gxserver_attach_failure(
                current_slot_id,
                "Ghostex couldn't open the command terminal.",
                cx,
            );
            return false;
        }
        self.remember_command_gxserver_session_for_command_tab(
            slot_id.session_id,
            plan.key.clone(),
            Some(plan.title),
        );
        if let Some(session) = self.command_pane.session_mut(slot_id.session_id) {
            session.zmx_session_name = plan.zmx_name;
            if let Some(command_id) = plan.command_id {
                session.action_command_id = Some(command_id);
            }
        }
        /*
        CDXC:Workarea 2026-08-13:
        Action tabs are first persisted before their asynchronous gxserver
        create/attach finishes. Persist again at the successful attach boundary
        after installing the canonical daemon key, otherwise a rebuild can
        replace the process while shell state still contains only an
        unidentifiable placeholder and the next Action click allocates a new
        pane instead of reattaching the previous one.
        */
        self.persist_shell_layout_state();
        self.command_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(current_slot_id, payload);
        cx.notify();
        true
    }

    pub(crate) fn command_gxserver_session_key_for_command_tab(
        &self,
        session_id: CommandSessionId,
    ) -> Option<GpuiLocalWorkspaceSessionKey> {
        self.command_gxserver_session_mappings
            .get(&session_id)
            .cloned()
            .or_else(|| {
                self.command_pane
                    .session(session_id)
                    .and_then(|session| session.gxserver_session_key.clone())
            })
    }

    pub(crate) fn remember_command_gxserver_session_for_command_tab(
        &mut self,
        session_id: CommandSessionId,
        key: GpuiLocalWorkspaceSessionKey,
        title: Option<String>,
    ) {
        /*
        CDXC:Workarea 2026-07-04:
        Keep the runtime attach map and persisted command-session metadata synchronized at the successful daemon attach boundary. This stores only the gxserver key, bounded display title, and validated bounded Action selector; attach commands and process details stay one-shot launch payload data.
        */
        self.command_gxserver_session_mappings
            .insert(session_id, key.clone());
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.gxserver_session_key = Some(key);
            if let Some(title) = title.map(|title| title.trim().to_string()).filter(|title| {
                !title.is_empty()
                    && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                    && !title.contains('\0')
                    && !title.chars().any(char::is_control)
            }) {
                session.title = title;
            }
        }
    }

    /// CDXC:CommandPane 2026-09-25 WHY:
    /// Closing or restarting a tab removes its server session while an asynchronous attach can still be running. Its later 404 belongs to the closed tab, so only a tab that still exists may be closed and report an attach error.
    pub(crate) fn close_command_terminal_after_gxserver_attach_failure(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.command_pane.has_session(slot_id.session_id) {
            return false;
        }
        let current_slot_id =
            command_pane_group_for_session(&self.command_pane, slot_id.session_id)
                .map(|group_id| CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id: slot_id.session_id,
                })
                .unwrap_or(slot_id);
        self.command_gxserver_attach_pending
            .remove(&current_slot_id.session_id);
        let session_key = self
            .command_pane
            .session(current_slot_id.session_id)
            .and_then(|session| session.gxserver_session_key.clone());
        if let Some(session) = self.command_pane.session_mut(current_slot_id.session_id) {
            session.gxserver_session_key = None;
        }
        if let Some(key) = self
            .command_gxserver_session_mappings
            .remove(&current_slot_id.session_id)
            .or(session_key)
        {
            self.close_command_gxserver_session_in_background(key, cx);
        }
        self.close_remote_command_action_session_for_closed_tab(current_slot_id.session_id, cx);
        self.command_terminal_launch_payload_source
            .remove_payloads_for_command_session(current_slot_id.session_id);
        let keyboard_owner_before = self.keyboard_owner_session();
        let changed = self
            .command_pane
            .close_session(current_slot_id.group_id, current_slot_id.session_id);
        if changed {
            self.clear_gpui_command_delayed_send_timer(current_slot_id.session_id);
            self.clear_gpui_command_close_after_done_timer(current_slot_id.session_id);
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if self.command_pane.has_sessions() {
                self.follow_shell_focus_after_surface_removed(
                    ShellFocusTarget::CommandPane,
                    keyboard_owner_before,
                    cx,
                );
            } else {
                self.restore_non_command_focus_after_surface_removed(keyboard_owner_before, cx);
            }
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        self.dispatch_gpui_app_modal_toast("warning", "Command terminal unavailable", message, cx);
        cx.notify();
        changed
    }

    pub(crate) fn promote_transferred_gxserver_session_surface_in_background(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        shell_session_id: TerminalSessionId,
        attempt: u32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-08-01:
        The moved tab is already live locally, so this update runs off the UI
        thread. Until it lands the session is held out of sidebar-driven
        reconciliation by `agents_sessions_pending_surface_transfer`, because a
        `commands`-surface row is absent from the sidebar tab projection and
        reconciliation would otherwise delete the tab and kill the terminal the
        user just dragged. Retry a bounded number of times, then release the
        hold so reconciliation can resume rather than pinning it forever.
        */
        const MAX_ATTEMPTS: u32 = 5;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let update_key = key.clone();
            let result = background
                .spawn(async move {
                    gpui_update_command_terminal_gxserver_session_surface(&update_key, "workspace")
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if result.is_ok() {
                    this.agents_sessions_pending_surface_transfer
                        .remove(&shell_session_id);
                    return;
                }
                if attempt + 1 >= MAX_ATTEMPTS {
                    this.agents_sessions_pending_surface_transfer
                        .remove(&shell_session_id);
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.commandTransfer.surfacePromotionFailed",
                        serde_json::json!({ "sessionId": shell_session_id.0 }),
                    );
                    return;
                }
                let retry_key = key.clone();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this
                            .agents_sessions_pending_surface_transfer
                            .contains(&shell_session_id)
                        {
                            this.promote_transferred_gxserver_session_surface_in_background(
                                retry_key,
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

    pub(crate) fn close_command_gxserver_session_in_background(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        self.pending_command_gxserver_cleanup.insert(key.clone());
        self.persist_shell_layout_state();
        if !self.command_gxserver_cleanup_in_flight.insert(key.clone()) {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let close_key = key.clone();
            let removed = background
                .spawn(async move { gpui_close_command_terminal_gxserver_session(&close_key) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.command_gxserver_cleanup_in_flight.remove(&key);
                if removed {
                    if this.pending_command_gxserver_cleanup.remove(&key) {
                        this.persist_shell_layout_state();
                    }
                    return;
                }
                let retry_key = key.clone();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(5)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.pending_command_gxserver_cleanup.contains(&retry_key) {
                            this.close_command_gxserver_session_in_background(retry_key, cx);
                        }
                    });
                })
                .detach();
            });
        })
        .detach();
    }

    pub(crate) fn retry_pending_command_gxserver_cleanup(&mut self, cx: &mut gpui::Context<Self>) {
        let pending = self
            .pending_command_gxserver_cleanup
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in pending {
            self.close_command_gxserver_session_in_background(key, cx);
        }
    }

    pub(crate) fn update_command_gxserver_session_title_in_background(
        &self,
        key: GpuiLocalWorkspaceSessionKey,
        title: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result =
                background
                    .spawn(async move {
                        gpui_update_command_terminal_gxserver_session_title(&key, &title)
                    })
                    .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Rename not synced",
                        message.as_str(),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub(crate) fn forget_command_gxserver_session_for_closed_tab(
        &mut self,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.command_gxserver_attach_pending.remove(&session_id);
        self.command_terminal_launch_payload_source
            .remove_payloads_for_command_session(session_id);
        self.command_gpui_engine_close_confirms
            .retain(|slot_id| slot_id.session_id != session_id);
        if let Some(session) = self.command_pane.session_mut(session_id) {
            session.gxserver_session_key = None;
        }
        if let Some(key) = self.command_gxserver_session_mappings.remove(&session_id) {
            self.close_command_gxserver_session_in_background(key, cx);
        }
        self.close_remote_command_action_session_for_closed_tab(session_id, cx);
    }

    pub(crate) fn prune_command_gxserver_sessions_for_command_model(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let live_session_ids = self
            .command_pane
            .terminal_sessions
            .iter()
            .map(|session| session.id)
            .collect::<HashSet<_>>();
        self.command_gxserver_attach_pending
            .retain(|session_id| live_session_ids.contains(session_id));
        let stale = self
            .command_gxserver_session_mappings
            .keys()
            .copied()
            /*
            CDXC:RemoteMachines 2026-08-29:
            Remote Action tabs are pruned by the same sweep as local ones. A tab
            removed straight from the command model (Action pruning of stale
            same-command tabs, layout repair) never reaches the tab-close path,
            so this is the only place that closes the remote session it owned.
            */
            .chain(self.command_remote_action_sessions.keys().copied())
            .filter(|session_id| !live_session_ids.contains(session_id))
            .collect::<HashSet<_>>();
        for session_id in stale {
            self.forget_command_gxserver_session_for_closed_tab(session_id, cx);
        }
    }

    pub(crate) fn start_existing_command_terminal_gxserver_attach_for_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        key: GpuiLocalWorkspaceSessionKey,
        initial_input: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .command_gxserver_attach_pending
            .insert(slot_id.session_id)
        {
            if let Some(input) = initial_input {
                self.command_terminal_launch_payload_source
                    .queue_input_for_pending_attach(slot_id.session_id, input);
            }
            return;
        }
        let command_pane_project_epoch = self.command_pane_project_epoch;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let attach_key = key.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_existing_command_terminal_attach_plan(attach_key, initial_input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.command_pane_project_epoch != command_pane_project_epoch {
                    // The command pane was swapped to another project while the
                    // attach plan was prepared. The target tab is parked, not
                    // closed, so neither mutate the live pane nor kill the
                    // parked project's daemon session; reattach happens when
                    // its project becomes active again.
                    return;
                }
                this.command_gxserver_attach_pending
                    .remove(&slot_id.session_id);
                match result {
                    Ok(plan) => {
                        this.insert_command_terminal_gxserver_attach_payload(slot_id, plan, cx);
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

    pub(crate) fn start_command_terminal_gxserver_attach_for_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        title: String,
        startup_text: Option<String>,
        command_id: Option<String>,
        command_title: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-29:
        A remote Action's command tab owns a session on another machine, so it
        reattaches over SSH instead of creating or reclaiming a local one. This
        branch is what keeps a restored remote Action tab from asking the local
        daemon for a command session in a project that only exists remotely,
        which fails on the missing local project path.
        */
        if let Some(reference) =
            self.command_remote_action_session_for_command_tab(slot_id.session_id)
        {
            self.start_remote_command_terminal_attach_for_slot(slot_id, reference, cx);
            return;
        }
        /*
        CDXC:RemoteMachines 2026-08-29:
        An Action tab in a remote project's command pane that has neither a
        remote nor a local identity is unmountable by construction: its Action
        runs on the remote machine, and the local daemon has no session and no
        project path for it. That state exists only when the app was quit (or
        the pane was swapped away) between creating the tab and the remote
        session's identity landing on it, so retire the leftover tab instead of
        asking the local daemon for a session it can never create and reporting
        that as a command-terminal error on every launch. Plain command tabs use
        remote creation below because they have no Action command to recover.
        */
        if self.active_gpui_remote_project_reference().is_some()
            && self
                .command_pane
                .session(slot_id.session_id)
                .is_some_and(|session| session.action_command_id.is_some())
            && self
                .command_gxserver_session_key_for_command_tab(slot_id.session_id)
                .is_none()
        {
            self.close_command_pane_tab(slot_id.group_id, slot_id.session_id, cx);
            return;
        }
        if let Some(key) = self.command_gxserver_session_key_for_command_tab(slot_id.session_id) {
            self.start_existing_command_terminal_gxserver_attach_for_slot(
                slot_id,
                key,
                startup_text,
                cx,
            );
            return;
        }
        if self
            .command_gxserver_attach_pending
            .contains(&slot_id.session_id)
        {
            if let Some(input) = startup_text {
                self.command_terminal_launch_payload_source
                    .queue_input_for_pending_attach(slot_id.session_id, input);
            }
            return;
        }
        if let Some(reference) = self.active_gpui_remote_project_reference() {
            self.start_new_remote_command_terminal_for_slot(
                slot_id,
                reference,
                title,
                startup_text,
                cx,
            );
            return;
        }
        let input = match self.command_terminal_create_input_for_active_project(
            title,
            startup_text,
            command_id,
            command_title,
        ) {
            Ok(GpuiCommandTerminalCreateInputResolution::Ready(input)) => input,
            Ok(GpuiCommandTerminalCreateInputResolution::NotReady) => return,
            Err(message) => {
                self.close_command_terminal_after_gxserver_attach_failure(
                    slot_id,
                    message.as_str(),
                    cx,
                );
                return;
            }
        };
        self.command_gxserver_attach_pending
            .insert(slot_id.session_id);
        let command_pane_project_epoch = self.command_pane_project_epoch;
        // CDXC:CommandPane 2026-09-13 WHY:
        // Restart closes the old Action asynchronously. Exclude pending cleanup identities before yielding, or recovery can reattach the daemon that the close worker is about to kill.
        let closing_sessions = self.pending_command_gxserver_cleanup.clone();
        let live_snapshot = self.gx_store_local_snapshot_value();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_prepare_command_terminal_attach_plan(
                        input,
                        closing_sessions,
                        live_snapshot,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.command_pane_project_epoch != command_pane_project_epoch {
                    // The command pane was swapped to another project while the
                    // create/attach plan was prepared. The freshly created
                    // daemon session is not referenced by any tab (its tab was
                    // parked before the key existed), so close it instead of
                    // leaking it; do not touch the live pane.
                    if let Ok(plan) = result {
                        gpui_close_command_terminal_gxserver_session(&plan.key);
                    }
                    return;
                }
                this.command_gxserver_attach_pending
                    .remove(&slot_id.session_id);
                match result {
                    Ok(plan) => {
                        this.insert_command_terminal_gxserver_attach_payload(slot_id, plan, cx);
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

    #[cfg(target_os = "windows")]
    pub(crate) fn start_command_terminal_powershell_for_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        startup_text: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:PlatformSupport 2026-07-15:
        PowerShell mode intentionally has no gxserver/zmx persistence, so a
        command tab must launch its own ConPTY process from an exact-slot,
        one-shot payload. This path covers new, restored, and woken tabs and
        preserves Action startup text without inventing a daemon identity.
        Existing WSL keys are detached from the local tab metadata; no token,
        command text, cwd, or process identity enters persisted shell state.
        */
        if !self.command_pane.has_session(slot_id.session_id) {
            return;
        }
        let Some(group_id) = command_pane_group_for_session(&self.command_pane, slot_id.session_id)
        else {
            return;
        };
        let current_slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id: slot_id.session_id,
        };
        self.command_gxserver_attach_pending
            .remove(&slot_id.session_id);
        let mut removed_stale_gxserver_mapping = self
            .command_gxserver_session_mappings
            .remove(&slot_id.session_id)
            .is_some();
        if let Some(session) = self.command_pane.session_mut(slot_id.session_id) {
            removed_stale_gxserver_mapping |= session.gxserver_session_key.is_some();
            session.gxserver_session_key = None;
            session.zmx_session_name = None;
        }
        if removed_stale_gxserver_mapping {
            self.persist_shell_layout_state();
        }
        let working_directory = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.as_ref())
            .and_then(|path| path.to_str())
            .map(str::to_string);
        let payload = CommandTerminalExplicitLaunchPayload {
            working_directory,
            command: None,
            env_vars: Vec::new(),
            initial_input: startup_text,
            wait_after_command: false,
        };
        if payload.validate_launch_strings().is_err() {
            self.close_command_terminal_after_gxserver_attach_failure(
                current_slot_id,
                "Ghostex couldn't prepare the PowerShell command terminal.",
                cx,
            );
            return;
        }
        self.command_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(current_slot_id, payload);
        cx.notify();
    }
}
