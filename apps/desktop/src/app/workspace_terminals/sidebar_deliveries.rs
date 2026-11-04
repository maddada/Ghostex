//! What the sidebar and store hand to workspace terminals: rename commands, the completion sound and flash, and the presentation focus state.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_workspace_terminal_rename_command_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:SessionTitles 2026-06-27-02:27:
        The sidebar rename bridge may request only the fixed agent rename command for the exact local gxserver project/session pair it already owns. Rust parses the fixed payload, resolves the process-local mapped Agents shell tab, surfaces that exact tab through the shared focus/attach pipeline, then stages the command text and presses a real Return key with no fallback attach, wake, creation, focused-terminal typing, logging, persistence of the title, or raw renderer JSON.
        */
        let Ok(message) = gpui_sidebar_workspace_terminal_rename_command_from_json(payload) else {
            return;
        };
        let _ = self.send_workspace_terminal_rename_command_to_local_agents_session(&message, cx);
    }

    pub(crate) fn send_workspace_terminal_rename_command_to_local_agents_session(
        &mut self,
        message: &GpuiSidebarWorkspaceTerminalRenameCommandMessage,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            let key = GpuiLocalWorkspaceSessionKey::from(message);
            if self.local_workspace_rename_command_target(&key).is_none() {
                return false;
            }
            /*
            CDXC:SessionTitles 2026-07-29:
            Renaming a session that is not the visible mounted tab must first
            surface it the same way a sidebar click does: raw tab selection
            never mounts the slot's Ghostty surface (attach payloads are
            one-shot per mount slot), which silently dropped every rename of a
            background session. Route through the shared focus/attach pipeline,
            then deliver immediately when the surface is already mounted or
            re-validate the same exact target on a short bounded timer until
            its surface mount completes; an invalidated target stops the wait
            instead of retargeting.
            */
            self.focus_local_workspace_terminal_from_message(
                &GpuiSidebarWorkspaceTerminalFocusMessage {
                    force_remount: false,
                    placement_target_session_id: None,
                    preferred_interface: GpuiPreferredAgentInterface::Terminal,
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                    startup_restore: false,
                    keep_view: false,
                    wake_sleeping: false,
                    keep_sleeping: false,
                },
                cx,
            );
            match self.deliver_workspace_terminal_rename_command_to_selected_tab(&key, message, cx)
            {
                GpuiWorkspaceRenameCommandDelivery::Delivered => true,
                GpuiWorkspaceRenameCommandDelivery::TargetInvalid => false,
                GpuiWorkspaceRenameCommandDelivery::SurfaceNotMounted => {
                    let key = key.clone();
                    let message = message.clone();
                    cx.spawn(async move |this, cx| {
                        for _ in 0..WORKSPACE_RENAME_COMMAND_MOUNT_RETRY_LIMIT {
                            cx.background_executor()
                                .timer(WORKSPACE_RENAME_COMMAND_MOUNT_RETRY_INTERVAL)
                                .await;
                            let delivery = this.update(cx, |this, cx| {
                                this.deliver_workspace_terminal_rename_command_to_selected_tab(
                                    &key, &message, cx,
                                )
                            });
                            match delivery {
                                Ok(GpuiWorkspaceRenameCommandDelivery::SurfaceNotMounted) => {}
                                Ok(GpuiWorkspaceRenameCommandDelivery::Delivered)
                                | Ok(GpuiWorkspaceRenameCommandDelivery::TargetInvalid)
                                | Err(_) => return,
                            }
                        }
                    })
                    .detach();
                    true
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (message, cx);
            false
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn deliver_workspace_terminal_rename_command_to_selected_tab(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        message: &GpuiSidebarWorkspaceTerminalRenameCommandMessage,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiWorkspaceRenameCommandDelivery {
        let Some(target) = self.local_workspace_rename_command_target(key) else {
            return GpuiWorkspaceRenameCommandDelivery::TargetInvalid;
        };
        let engine_terminal_view = self
            .agents_workspace
            .is_current_terminal_body_mount_slot(target.slot_id)
            .then(|| {
                self.agents_gpui_engine_terminals
                    .get(&target.slot_id.session_id)
            })
            .flatten()
            .map(|record| record.view.clone());
        let Some(view) = engine_terminal_view else {
            return GpuiWorkspaceRenameCommandDelivery::SurfaceNotMounted;
        };
        let terminal_input =
            gpui_workspace_terminal_rename_command_input(message.command, &message.title);
        /*
        CDXC:SessionTitles 2026-08-26:
        The composer may already hold user-typed draft text, and the rename is
        written onto that same line, so the command opens with the measured
        clear burst (WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS) as its own pty
        write before the command text, never concatenated with it. A single
        Ctrl+U kills only one logical line: a two-line draft kept its first line
        and the rename was submitted glued to it.

        There is no Ctrl+Y restore any more, matching gxserver's title jobs: a
        yank returns only the LAST kill, so after a 2N-1 burst it can restore at
        most a fragment of a multi-line draft, and after the trailing Ctrl+K
        kills it restores nothing. The draft is discarded, the same way a chat
        send owns and clears this line; terminal -> chat view switching remains
        the loss-safe transfer path.
        */
        view.update(cx, |view, cx| {
            for _ in 0..WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS {
                view.send_ctrl_letter_key(ghostty_vt::VtKey::U, 'u', cx);
            }
            for _ in 0..WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS {
                view.send_ctrl_letter_key(ghostty_vt::VtKey::K, 'k', cx);
            }
            view.send_text_input(&terminal_input, cx);
        });
        /*
        CDXC:SessionTitles 2026-07-29:
        macOS parity (AUTO_SUBMIT_STAGED_RENAME_DELAY_MS): agent CLIs treat
        command text and Enter arriving in one stdin chunk as a paste and
        insert a newline instead of submitting. Stage the command now, then
        press the real Return on the re-validated exact target after the same
        one-second delay native uses.
        */
        let submit_key = key.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(WORKSPACE_RENAME_COMMAND_SUBMIT_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(target) = this.local_workspace_rename_command_target(&submit_key) else {
                    return;
                };
                let _ = this.send_return_key_to_mounted_agents_terminal_surface(target.slot_id, cx);
            });
        })
        .detach();
        self.persist_shell_layout_state();
        cx.notify();
        GpuiWorkspaceRenameCommandDelivery::Delivered
    }

    /// The completion sound and the sidebar card's completion flash, which are one event. Called by
    /// the store's attention host (gx_store/attention/).
    pub(crate) fn play_session_completion(
        &mut self,
        sound: &str,
        session_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = gpui_play_completion_sound(sound);
        /*
        CDXC:Sessions 2026-09-21 WHY:
        The card's completion flash rides the same message as the sound, because they are the same
        event: the runtime posted both from one place and the flash only took the long way round,
        through the old projection's snapshot bridge (the deleted sidebar page), which M4d
        part 2 deletes.
        */
        let Some(session_id) = session_id else {
            return;
        };
        self.native_sidebar
            .completion_flashes
            .retain(|_, started| started.elapsed().as_secs_f32() < 3.0);
        self.native_sidebar
            .completion_flashes
            .insert(session_id, std::time::Instant::now());
        cx.notify();
    }

    pub(crate) fn set_sidebar_gxserver_presentation_focus_state(
        &mut self,
        next_state: GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) {
        let _profile = crate::profiling::span(crate::profiling::Metric::ProjectFocus);
        /*
        CDXC:Navigation 2026-07-29:
        This snapshot is the authoritative project switch for sidebar clicks
        and remote attach focus, so it is the leading edge of the coalescer.
        A snapshot for the already-active project is an intra-project session
        focus change and is never coalesced.
        */
        if self.project_switch_request_is_coalesced(
            next_state.active_project_id.as_deref(),
            GpuiProjectSwitchRequestKind::GxserverPresentationFocusState,
        ) {
            self.enqueue_coalesced_project_switch_request(
                next_state.active_project_id.clone(),
                GpuiPendingProjectSwitchPayload::GxserverPresentationFocusState(next_state),
                cx,
            );
            return;
        }
        self.swap_agents_workspace_to_project_id(next_state.active_project_id.clone(), cx);
        self.apply_sidebar_gxserver_presentation_focus_state(next_state, cx);
    }

    /// Everything a focus state does once the workspace project is settled. Called on its own for a payload that lost to a newer local selection, which must neither enter the project switch coalescer nor swap the workspace (gx_store/local_focus.rs).
    pub(crate) fn apply_sidebar_gxserver_presentation_focus_state(
        &mut self,
        next_state: GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.sidebar_gxserver_presentation_focus_state == next_state {
            self.attach_surfaced_local_workspace_terminals(&next_state, cx);
            return;
        }
        let session_selection_changed = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_id
            != next_state.active_project_id
            || self
                .sidebar_gxserver_presentation_focus_state
                .focused_session_id
                != next_state.focused_session_id;
        let workspace_changed = self.reconcile_local_workspace_tabs_with_sidebar(&next_state, cx);
        self.sidebar_gxserver_presentation_focus_state = next_state;
        self.sync_gpui_engine_first_prompt_input_suppression(cx);
        self.gx_store_persist_focus_state_file();
        let _ = session_selection_changed;
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        if workspace_changed {
            // A sidebar projection that adds or removes tabs can change which chat-mode session a pane shows; its page has to exist before the next render, not after the next click (see the matching reconcile in sync_agents_gpui_engine_terminals).
            self.reconcile_agents_chat_surfaces(cx);
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.broadcast_extension_context_changes(cx);
        if self
            .sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .is_none()
        {
            self.gx_store_follow_project_only_focus(cx);
        }
        cx.notify();
    }
}
