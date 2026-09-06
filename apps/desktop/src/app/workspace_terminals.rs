// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: local/remote workspace terminal lifecycle, attach, and rename plumbing

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// The measured clear burst as raw bytes, for the Ghostty-surface pipeline
/// (the engine pipeline sends the same law as VT key events). Kills toward the
/// start first, then toward the end, exactly like gxserver's
/// `build_agent_tui_clear_input`.
#[cfg(target_os = "macos")]
fn workspace_terminal_clear_input_burst() -> String {
    format!(
        "{}{}",
        AGENT_TUI_CLEAR_INPUT_LINE.repeat(WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS),
        AGENT_TUI_CLEAR_INPUT_FORWARD.repeat(WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS)
    )
}

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
                    placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
                    placement_target_session_id: None,
                    preferred_interface: GpuiPreferredAgentInterface::Terminal,
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                    startup_restore: false,
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
        /*
        CDXC:SessionTitles 2026-07-29:
        Agents terminals run one of two pipelines. The default GPUI engine
        pipeline never registers a Ghostty surface, so a Ghostty-only owner
        check classified every engine tab as unmounted and silently dropped
        the rename. Accept the exact current slot's engine terminal view (the
        same owner `send_return_key_to_mounted_agents_terminal_surface`
        already uses) or the exact mounted Ghostty surface, and nothing else.
        */
        let engine_terminal_view = self
            .agents_workspace
            .is_current_terminal_body_mount_slot(target.slot_id)
            .then(|| {
                self.agents_gpui_engine_terminals
                    .get(&target.slot_id.session_id)
            })
            .flatten()
            .map(|record| record.view.clone());
        if engine_terminal_view.is_none()
            && !self.agents_terminal_ghostty_surface_matches(target.slot_id)
        {
            return GpuiWorkspaceRenameCommandDelivery::SurfaceNotMounted;
        }
        let terminal_input =
            gpui_workspace_terminal_rename_command_input(message.command, &message.title);
        /*
        CDXC:SessionTitles 2026-08-26:
        The composer may already hold user-typed draft text, and the rename is
        written onto that same line, so BOTH pipelines open with the measured
        clear burst (WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS) as its own pty
        write before the command text.

        This replaces two different wrong behaviours. The engine pipeline sent a
        single Ctrl+U, which kills one logical line: a two-line draft kept its
        first line and the rename was submitted glued to it. The Ghostty-surface
        pipeline sent no clear at all, so any draft simply got `/rename …`
        appended. The burst is a separate write from the command text in both,
        never concatenated with it.

        There is no Ctrl+Y restore any more, matching gxserver's title jobs: a
        yank returns only the LAST kill, so after a 2N-1 burst it can restore at
        most a fragment of a multi-line draft, and after the trailing Ctrl+K
        kills it restores nothing. The draft is discarded, the same way a chat
        send owns and clears this line; terminal -> chat view switching remains
        the loss-safe transfer path.
        */
        let text_sent = if let Some(view) = engine_terminal_view {
            view.update(cx, |view, cx| {
                for _ in 0..WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS {
                    view.send_ctrl_letter_key(ghostty_vt::VtKey::U, 'u', cx);
                }
                for _ in 0..WORKSPACE_RENAME_COMMAND_CLEAR_REPETITIONS {
                    view.send_ctrl_letter_key(ghostty_vt::VtKey::K, 'k', cx);
                }
                view.send_text_input(&terminal_input, cx);
            });
            true
        } else {
            self.send_text_bytes_to_mounted_agents_terminal_surface(
                target.slot_id,
                workspace_terminal_clear_input_burst().as_bytes(),
            ) && self.send_text_bytes_to_mounted_agents_terminal_surface(
                target.slot_id,
                terminal_input.as_bytes(),
            )
        };
        if !text_sent {
            return GpuiWorkspaceRenameCommandDelivery::TargetInvalid;
        }
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

    pub(crate) fn receive_sidebar_workspace_terminal_enter_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = gpui_sidebar_workspace_terminal_enter_from_json(payload) else {
            return;
        };
        let _ = self.send_enter_key_to_local_agents_workspace_session(&message, cx);
    }

    pub(crate) fn receive_sidebar_session_completion_sound_payload(&mut self, payload: &str) {
        /*
        Session-attention completion sound (macOS parity): the sidebar runtime
        owns the attention transition edge, the attention-event dedupe, and the
        completionBellEnabled gate. Rust only validates the fixed message shape
        and plays a bundled sound asset; the sound id goes through the existing
        whitelist normalization so no renderer-provided path or file name can
        reach the player.
        */
        let Ok(sound) = gpui_sidebar_session_completion_sound_from_json(payload) else {
            return;
        };
        let _ = gpui_play_completion_sound(&sound);
    }

    pub(crate) fn send_enter_key_to_local_agents_workspace_session(
        &mut self,
        message: &GpuiSidebarWorkspaceTerminalEnterMessage,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            let key = GpuiLocalWorkspaceSessionKey::from(message);
            let Some(target) = self.local_workspace_rename_command_target(&key) else {
                return false;
            };
            // macOS sendTerminalEnter preserves focus: press Return on the mapped
            // surface without selecting its tab or moving focus. A session whose
            // tab is not the active mounted tab has no surface to receive the key
            // and is skipped rather than yanking the visible tab.
            self.send_return_key_to_mounted_agents_terminal_surface(target.slot_id, cx)
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (message, cx);
            false
        }
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
        self.reconcile_local_app_shot_session_mappings(&next_state);
        if self.sidebar_gxserver_presentation_focus_state == next_state {
            self.attach_surfaced_local_workspace_terminals(&next_state, cx);
            return;
        }
        let workspace_changed = self.reconcile_local_workspace_tabs_with_sidebar(&next_state, cx);
        self.sidebar_gxserver_presentation_focus_state = next_state;
        self.sync_gpui_engine_first_prompt_input_suppression(cx);
        persist_gpui_gxserver_presentation_focus_state(
            &self.sidebar_gxserver_presentation_focus_state,
        );
        if self.active_mode.is_project_editor_mode()
            && self.project_editor_shell.left_companion_visible
        {
            let mode = self.active_mode;
            let focus_companion =
                self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(mode);
            if let Some(key) = self.project_editor_companion_active_terminal_key()
                && let Some(session_id) = self.shell_session_for_workspace_terminal_key(&key)
                && self.project_editor_companion_terminal_session_is_active_project_eligible(
                    session_id,
                )
            {
                self.retarget_project_editor_companion_to_workspace_terminal(
                    mode,
                    session_id,
                    &key,
                    focus_companion,
                    cx,
                );
            } else {
                self.sync_project_editor_companion_terminal_selection();
            }
        }
        self.prune_project_editor_companion_remote_attach_states();
        #[cfg(target_os = "windows")]
        {
            let focus_companion =
                self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(self.active_mode);
            self.sync_windows_project_editor_companion_to_presentation_focus(focus_companion, cx);
        }
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        if workspace_changed {
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.broadcast_extension_context_changes(cx);
        cx.notify();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn sync_windows_project_editor_companion_to_presentation_focus(
        &mut self,
        focus_companion: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:PlatformSupport 2026-07-26:
        Windows CEF sidebar selection publishes both an imperative terminal
        focus request and the authoritative presentation focus snapshot. The
        companion used to follow only the imperative request, so a focus-state
        change could update the sidebar's selection owner while leaving a visible
        project-editor companion on its previous session. Bind the Windows
        companion to the changed presentation owner after workspace
        reconciliation, reusing the existing terminal companion paths instead
        of creating a second session model or fallback selection. Both the focus
        snapshot and active-project update drive this helper, so whichever
        authoritative state arrives second completes synchronization. The
        caller captures whether the companion still owns shell focus; delayed
        reconciliation updates content without stealing focus from the editor.
        */
        let mode = self.active_mode;
        if !mode.is_project_editor_mode() || !self.project_editor_shell.left_companion_visible {
            return false;
        }
        let Some(project_id) = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_id
            .clone()
        else {
            return false;
        };
        if self.project_editor_companion_active_project_id().as_deref() != Some(project_id.as_str())
        {
            return false;
        }
        let Some(session_id) = self
            .sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .clone()
        else {
            return false;
        };
        let key = GpuiLocalWorkspaceSessionKey {
            project_id,
            session_id,
        };
        let Some(shell_session_id) = self.local_workspace_session_mappings.get(&key).copied()
        else {
            return false;
        };
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        let current_runtime_session_id = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(shell_session_id);
        let current_terminal_matches = self.project_editor_companion_focused_terminal_session_id()
            == Some(shell_session_id)
            && self.project_editor_companion_terminal_session_is_eligible(shell_session_id)
            && self
                .project_editor_companion_terminal_slot_for_mode(mode)
                .is_some_and(|slot_id| slot_id.session_id == shell_session_id)
            && current_runtime_session_id.is_some_and(|runtime_session_id| {
                self.agents_gpui_engine_terminals
                    .get(&shell_session_id)
                    .is_some_and(|record| record.runtime_session_id == runtime_session_id)
            });
        if current_terminal_matches {
            return false;
        }
        if self.project_editor_companion_terminal_session_is_eligible(shell_session_id) {
            let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key);
            self.retarget_project_editor_companion_to_workspace_terminal(
                mode,
                shell_session_id,
                &workspace_key,
                focus_companion,
                cx,
            );
            return true;
        }
        false
    }

    pub(crate) fn reconcile_local_workspace_tabs_with_sidebar(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(tab_sessions) = focus_state.active_project_tab_sessions.as_deref() else {
            return false;
        };
        if self.agents_workspace_project_id.as_deref() != focus_state.active_project_id.as_deref() {
            return false;
        }
        /*
        CDXC:Workarea 2026-08-01:
        A tab just dragged out of the command pane is live locally but its
        gxserver row is still on the `commands` surface, so it is absent from
        this projection by definition. Reconciling against that projection
        would delete the session and take the running terminal with it, so
        skip the pass entirely while any transfer is mid-flight. The hold is
        released on daemon confirmation or after a bounded retry budget, and
        the next sidebar patch reconciles normally.
        */
        if !self.agents_sessions_pending_surface_transfer.is_empty() {
            return false;
        }
        let changed = self.agents_workspace.reconcile_with_sidebar_tab_sessions(
            focus_state.active_project_id.as_deref(),
            tab_sessions,
            &mut self.local_workspace_session_mappings,
            &mut self.remote_attach_sessions,
        );
        if changed {
            self.local_app_shot_session_mappings
                .retain(|_, shell_session_id| {
                    self.agents_workspace.session(*shell_session_id).is_some()
                });
            self.agents_terminal_startup_body_slot_geometries
                .retain(|slot_id, _| {
                    self.agents_workspace
                        .is_current_terminal_startup_body_slot(*slot_id)
                });
            self.agents_terminal_parked_owner_body_slot_geometries
                .retain(|slot_id, _| {
                    self.agents_workspace
                        .is_current_terminal_parked_owner_body_slot(*slot_id)
                });
        }
        self.attach_surfaced_local_workspace_terminals(focus_state, cx);
        changed
    }

    pub(crate) fn attach_surfaced_local_workspace_terminals(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-07-24 (revised 2026-08-07):
        Every terminal the restored split layout currently surfaces — not only
        the focused pane — needs its own process-local gxserver attach payload
        after restart. "Surfaced" is read from the live workspace model (a
        rendered pane's active tab), which is the only current statement of
        what is on screen. It used to be gated on the presentation snapshot's
        visible session ids as well, but that set is a snapshot of what *was*
        surfaced when it was published: after a session closed while the app
        was shut down, reconcile promotes a background tab to active and that
        promoted tab is absent from the stale set, so the pane the user is
        looking at stayed empty until clicked. Prepare those exact active
        pane/session slots without selecting a tab, moving pane focus, or
        publishing a new focused session.
        */
        /*
        The remote and wake passes read the live workspace model rather than
        the local projection, so they run before its guard: a remote workspace
        or a fully-sleeping project must resume even on a snapshot that carries
        no local tab rows.
        */
        if let Some(remote_machine_id) = self
            .agents_workspace_project_id
            .as_deref()
            .and_then(gpui_remote_project_reference_from_project_id)
            .map(|remote_project| remote_project.remote_machine_id)
        {
            self.attach_surfaced_remote_workspace_terminals(&remote_machine_id, cx);
        }
        self.resume_restored_workspace_surfaced_terminals(cx);
        let Some(tab_sessions) = focus_state.active_project_tab_sessions.as_deref() else {
            return;
        };
        let rendered_pane_ids = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .collect::<HashSet<_>>();
        let candidates = tab_sessions
            .iter()
            .filter_map(|session| {
                let key = session.key.as_local()?;
                if session.presentation_state != TerminalSessionPresentationState::Running {
                    return None;
                }
                let shell_session_id = self.local_workspace_session_mappings.get(key).copied()?;
                let pane_id = self
                    .agents_workspace
                    .pane_id_for_session(shell_session_id)?;
                (rendered_pane_ids.contains(&pane_id)
                    && self.agents_workspace.active_session_in_pane(pane_id)
                        == Some(shell_session_id)
                    && self.agents_tab_selected_local_runtime_missing(pane_id, shell_session_id))
                .then(|| (key.clone(), pane_id))
            })
            .collect::<Vec<_>>();

        for (key, pane_id) in candidates {
            self.spawn_local_workspace_attach_plan(
                key,
                GpuiLocalWorkspaceAttachIntent::Attach,
                pane_id,
                true,
                GpuiWorkspaceTerminalFocusPlacement::Tab,
                GpuiLocalWorkspaceAttachOrigin::SurfacedRestore,
                cx,
            );
        }
    }

    pub(crate) fn resume_restored_workspace_surfaced_terminals(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-08-07:
        A restored workspace keeps the sessions its panes surfaced at quit, but
        their daemon providers may have gone to sleep (auto-sleep, or the
        machine rebooted) while the app was closed. Attach alone cannot show
        those: a sleeping session has no zmx provider to attach to. Wake each
        pane's surfaced-but-sleeping session exactly once per restored project,
        which respawns the agent session server-side and then attaches through
        the ordinary lifecycle path. Later sleeps are user decisions, so the
        project key is consumed on the first authoritative pass and never
        re-armed.
        */
        let Some(project_id) = self.agents_workspace_project_id.clone() else {
            return;
        };
        if !self.startup_restore_wake_pending.remove(&project_id) {
            return;
        }
        let surfaced = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                self.agents_workspace
                    .active_session_in_pane(pane_id)
                    .map(|session_id| (pane_id, session_id))
            })
            .collect::<Vec<_>>();
        let focused_pane_id = self.agents_workspace.focused_pane;
        for (pane_id, session_id) in surfaced {
            if self
                .agents_workspace
                .session(session_id)
                .is_none_or(|session| {
                    session.presentation_state != TerminalSessionPresentationState::Sleeping
                })
            {
                continue;
            }
            let mutation_kind = if pane_id == focused_pane_id {
                GpuiLocalWorkspaceLifecycleMutationKind::DirectWake
            } else {
                GpuiLocalWorkspaceLifecycleMutationKind::RestoreWake
            };
            self.request_mapped_sleeping_agents_terminal_wake(
                pane_id,
                session_id,
                mutation_kind,
                cx,
            );
        }
    }

    pub(crate) fn current_project_view_state(&self) -> GpuiProjectViewState {
        GpuiProjectViewState {
            active_mode: self.available_titlebar_mode_or_agents(self.active_mode),
            companion_visible: self.project_editor_shell.left_companion_visible,
            companion_split_enabled: self.project_editor_shell.left_companion_split_enabled,
            companion_width_ratio: self.project_editor_shell.left_companion_width_ratio,
            companion_split_ratio: self.project_editor_shell.left_companion_split_ratio,
            companion_top_session_id: self.project_editor_companion_terminal_session_id,
            companion_bottom_session_id: self
                .project_editor_companion_secondary_terminal_session_id,
            companion_focused_slot: self.project_editor_companion_focused_terminal_slot,
        }
    }

    pub(crate) fn project_view_states_for_shell_state(
        &self,
    ) -> HashMap<String, GpuiProjectViewState> {
        /*
        CDXC:Navigation 2026-08-07:
        The live project's view is only in the app fields, never in the map, so
        the writer folds it in at serialization time. Without this, quitting
        while on a project would persist that project's view as of the last
        time the user switched away from it.
        */
        let mut states = self.project_view_states_by_project.clone();
        if let Some(project_id) = self.agents_workspace_project_id.clone() {
            states.insert(project_id, self.current_project_view_state());
        }
        states
    }

    pub(crate) fn capture_outgoing_project_view_state(&mut self) {
        if let Some(project_id) = self.agents_workspace_project_id.clone() {
            let state = self.current_project_view_state();
            self.project_view_states_by_project
                .insert(project_id, state);
        }
    }

    pub(crate) fn apply_project_view_state_for_active_project(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Navigation 2026-08-07:
        Restore the incoming project's own workarea and companion arrangement.
        Companion occupants are validated against the workspace model that was
        just swapped in — a session the reconcile has since removed simply
        leaves that slot to the ordinary selection sync.
        */
        let Some(state) = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| self.project_view_states_by_project.get(project_id))
            .copied()
        else {
            return;
        };
        self.project_editor_shell.left_companion_visible = state.companion_visible;
        self.project_editor_shell.left_companion_split_enabled = state.companion_split_enabled;
        self.project_editor_shell.left_companion_width_ratio = state.companion_width_ratio;
        self.project_editor_shell.left_companion_split_ratio = state.companion_split_ratio;
        self.project_editor_companion_terminal_session_id = state
            .companion_top_session_id
            .filter(|session_id| self.agents_workspace.has_session(*session_id));
        self.project_editor_companion_secondary_terminal_session_id = state
            .companion_bottom_session_id
            .filter(|session_id| self.agents_workspace.has_session(*session_id));
        self.project_editor_companion_focused_terminal_slot = state.companion_focused_slot;
        let target_mode = self.available_titlebar_mode_or_agents(state.active_mode);
        if self.active_mode != target_mode {
            self.active_mode = target_mode;
            self.set_shell_focus(default_shell_focus_for_mode(
                target_mode,
                &self.agents_workspace,
                &self.project_editor_shell,
            ));
            self.update_active_mode_cef_child_visibility(cx);
        }
    }

    pub(crate) fn agents_remote_connect_status_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<(&'static str, Option<&'static str>)> {
        /*
        CDXC:RemoteMachines 2026-08-07:
        A remote tab can only show content once its machine's tunnel is up, so
        while that machine has no live connection the body states why instead
        of rendering an empty rectangle. Local sessions never qualify, and a
        connected machine returns nothing so the overlay disappears the moment
        the attach can proceed.

        CDXC:RemoteMachines 2026-08-14:
        The launch wrapper now keeps a remote-attach terminal's process alive
        and reconnects its own SSH session after drops, so a Running tab can
        have live, typeable content while the machine-level tunnel is still
        re-establishing. The overlay explains an empty body; it must never
        cover a Running terminal that the user can already interact with.
        */
        let key = match self.workspace_terminal_key_for_shell_session(session_id)? {
            GpuiWorkspaceTerminalSessionKey::Remote(key) => key,
            GpuiWorkspaceTerminalSessionKey::Local(_) => return None,
        };
        if self
            .agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Running
            })
        {
            return None;
        }
        if self
            .remote_gxserver_connections
            .contains_key(&key.remote_machine_id)
            && self
                .remote_machine_connect_states
                .get(&key.remote_machine_id)
                .map(String::as_str)
                == Some(GpuiRemoteGxserverConnectState::Connected.wire_status_state())
        {
            return None;
        }
        Some(gpui_remote_connect_overlay_labels(
            self.remote_machine_connect_states
                .get(&key.remote_machine_id)
                .map(String::as_str),
        ))
    }

    pub(crate) fn attach_surfaced_remote_workspace_terminals(
        &mut self,
        remote_machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-07:
        The remote counterpart of `attach_surfaced_local_workspace_terminals`.
        Parking a workspace always kills its SSH clients, so every restored
        remote tab is dead by definition and used to need a tab-strip or
        sidebar click to come back. Re-arm each surfaced remote tab of the live
        remote workspace as soon as its machine reports connected — the same
        "prepare a plan, insert the payload into that exact mount slot" shape
        the local path uses. This deliberately does not select tabs, move pane
        focus, change the titlebar mode, or publish presentation focus: the
        restored layout already says what is surfaced, and a split's other
        panes must re-arm without fighting each other for focus.
        */
        let Some(active_project_id) = self.agents_workspace_project_id.clone() else {
            return;
        };
        let Some(remote_project) =
            gpui_remote_project_reference_from_project_id(&active_project_id)
        else {
            return;
        };
        if remote_project.remote_machine_id != remote_machine_id {
            return;
        }
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings_snapshot.object(), remote_machine_id)
        else {
            return;
        };
        let candidates = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                self.agents_workspace
                    .active_session_in_pane(pane_id)
                    .map(|session_id| (pane_id, session_id))
            })
            .filter(|(pane_id, session_id)| {
                self.agents_tab_selected_local_runtime_missing(*pane_id, *session_id)
            })
            .filter_map(|(pane_id, session_id)| {
                self.remote_attach_sessions
                    .iter()
                    .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))
                    .filter(|key| key.remote_machine_id == remote_machine_id)
                    .map(|key| (pane_id, session_id, key))
            })
            .collect::<Vec<_>>();

        for (pane_id, session_id, key) in candidates {
            if !self.remote_workspace_attach_pending.insert(key.clone()) {
                continue;
            }
            let reference = GpuiRemoteAttachSessionReference {
                remote_machine_id: key.remote_machine_id.clone(),
                project_id: key.project_id.clone(),
                session_id: key.session_id.clone(),
            };
            let prepare_config = config.clone();
            let prepare_target = target.clone();
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let prepare_reference = reference.clone();
                let result = background
                    .spawn(async move {
                        /*
                        Wake is requested here for the same reason the click
                        path requests it: a restored session whose zmx provider
                        died while the app was closed must be resumed remotely
                        before there is anything to attach to.
                        */
                        gpui_prepare_remote_attach_terminal_plan(
                            &prepare_config,
                            &prepare_target,
                            &prepare_reference,
                            true,
                            true,
                        )
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.remote_workspace_attach_pending.remove(&key);
                    let Ok(plan) = result else {
                        support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.remoteAttach.surfacedRestorePlanFailed",
                            serde_json::json!({ "machineId": key.remote_machine_id }),
                        );
                        return;
                    };
                    this.arm_surfaced_remote_workspace_terminal(
                        &key, pane_id, session_id, plan, cx,
                    );
                });
            })
            .detach();
        }
    }

    pub(crate) fn arm_surfaced_remote_workspace_terminal(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        plan: GpuiRemoteAttachTerminalPlan,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The SSH round trip runs in the background, so the layout may have moved
        on. Mirror the local `SurfacedRestore` completion guard exactly: the
        mapping, the pane placement, and the empty-runtime condition must all
        still hold, otherwise this payload belongs to a slot that no longer
        exists.
        */
        if self.remote_attach_sessions.get(key).copied() != Some(session_id)
            || self.agents_workspace.pane_id_for_session(session_id) != Some(pane_id)
            || self.agents_workspace.active_session_in_pane(pane_id) != Some(session_id)
            || !self.agents_tab_selected_local_runtime_missing(pane_id, session_id)
        {
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
            return;
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        self.agents_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                },
                payload,
            );
        #[cfg(target_os = "macos")]
        if let Some(askpass) = plan.askpass {
            self.remote_attach_askpass_scripts
                .insert(key.clone(), askpass);
        }
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.terminalOpened",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "mode": "surfacedRestore",
                "sessionId": key.session_id,
            }),
        );
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn prune_local_workspace_session_mappings(&mut self) {
        let current_shell_session_ids = self
            .agents_workspace
            .terminal_session_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        self.local_workspace_session_mappings
            .retain(|_, shell_session_id| current_shell_session_ids.contains(shell_session_id));
        self.local_app_shot_session_mappings
            .retain(|_, shell_session_id| current_shell_session_ids.contains(shell_session_id));
    }

    pub(crate) fn local_workspace_key_for_shell_session(
        &mut self,
        shell_session_id: TerminalSessionId,
    ) -> Option<GpuiLocalWorkspaceSessionKey> {
        self.prune_local_workspace_session_mappings();
        self.local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped_session_id)| {
                (*mapped_session_id == shell_session_id).then_some(key.clone())
            })
    }

    pub(crate) fn local_workspace_rename_command_target(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) -> Option<GpuiWorkspaceTerminalRenameCommandTarget> {
        self.prune_local_workspace_session_mappings();
        let shell_session_id = self.local_workspace_session_mappings.get(key).copied()?;
        let target = gpui_workspace_terminal_rename_command_target_from_model(
            &self.agents_workspace,
            &self.local_workspace_session_mappings,
            key,
        );
        if target.is_none()
            && (!self.agents_workspace.has_session(shell_session_id)
                || self
                    .agents_workspace
                    .pane_id_for_session(shell_session_id)
                    .is_none())
        {
            self.local_workspace_session_mappings.remove(key);
            self.local_app_shot_session_mappings
                .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
        }
        target
    }

    pub(crate) fn request_local_workspace_terminal_lifecycle(
        &mut self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
        action: GpuiLocalWorkspaceLifecycleAction,
        mutation_kind: GpuiLocalWorkspaceLifecycleMutationKind,
        replacement_key: Option<GpuiLocalWorkspaceSessionKey>,
        skip_replacement_fallback: bool,
        confirmed_close_slot_id: Option<AgentsTerminalBodyMountSlotId>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self.local_workspace_key_for_shell_session(shell_session_id) else {
            return false;
        };

        let replacement_shell_session_id = replacement_key
            .as_ref()
            .and_then(|key| self.local_workspace_session_mappings.get(key).copied());
        /*
        CDXC:Workarea 2026-06-26-08:01:
        Direct pane-tab close/sleep must carry Rust's pane-local replacement decision to the sidebar. If there is no mapped pane-local replacement, send an explicit no-fallback flag so the sidebar does not substitute project-list focus and diverge from the GPUI tab group.
        */
        let request = GpuiLocalWorkspaceLifecycleRequest {
            action,
            confirmed_close_slot_id,
            mutation_kind,
            pane_id,
            replacement_shell_session_id,
            shell_session_id,
        };
        if action != GpuiLocalWorkspaceLifecycleAction::Close
            && gpui_local_workspace_lifecycle_request_is_pending(
                &self.local_workspace_lifecycle_requests,
                &request,
            )
        {
            return true;
        }
        let Some(request_id) = self.next_local_workspace_lifecycle_request_id() else {
            return false;
        };
        let mut message = serde_json::Map::new();
        message.insert(
            "action".to_string(),
            serde_json::Value::String(action.as_str().to_string()),
        );
        message.insert(
            "projectId".to_string(),
            serde_json::Value::String(key.project_id),
        );
        message.insert("requestId".to_string(), serde_json::json!(request_id));
        message.insert(
            "sessionId".to_string(),
            serde_json::Value::String(key.session_id),
        );
        message.insert(
            "type".to_string(),
            serde_json::Value::String(
                GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE.to_string(),
            ),
        );
        message.insert(
            "version".to_string(),
            serde_json::json!(GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION),
        );
        if let Some(replacement_key) = replacement_key {
            message.insert(
                "replacementProjectId".to_string(),
                serde_json::Value::String(replacement_key.project_id),
            );
            message.insert(
                "replacementSessionId".to_string(),
                serde_json::Value::String(replacement_key.session_id),
            );
        } else if skip_replacement_fallback {
            message.insert(
                "skipReplacementFallback".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        if mutation_kind == GpuiLocalWorkspaceLifecycleMutationKind::RestoreWake {
            message.insert(
                "keepSidebarFocus".to_string(),
                serde_json::Value::Bool(true),
            );
        }

        if action == GpuiLocalWorkspaceLifecycleAction::Close {
            /*
            CDXC:Workarea 2026-07-10:
            A tab-bar Close is a local shell mutation first, matching the
            native sidebar and workspace. Do not leave a visible GPUI tab
            waiting for the sidebar CEF bridge or a gxserver RPC: either can
            be unavailable or delayed even though the user already closed the
            tab. Apply the exact direct/scoped close now, then send the
            bounded lifecycle message only as asynchronous provider cleanup.
            Close results are intentionally not registered as pending because
            the local mutation has already committed; Sleep and Wake continue
            to wait for their backend acknowledgement below.
            */
            let changed = self.apply_local_workspace_terminal_lifecycle_result(request, cx);
            let _ = self.dispatch_gpui_workspace_terminal_lifecycle_request(
                serde_json::Value::Object(message),
                cx,
            );
            return changed;
        }

        self.local_workspace_lifecycle_requests
            .insert(request_id, request);
        if self.dispatch_gpui_workspace_terminal_lifecycle_request(
            serde_json::Value::Object(message),
            cx,
        ) {
            return true;
        }
        self.local_workspace_lifecycle_requests.remove(&request_id);
        false
    }

    pub(crate) fn request_remote_workspace_terminal_lifecycle(
        &mut self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
        action: GpuiLocalWorkspaceLifecycleAction,
        mutation_kind: GpuiLocalWorkspaceLifecycleMutationKind,
        replacement_key: Option<GpuiRemoteAttachSessionKey>,
        skip_replacement_fallback: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.workspace_terminal_key_for_shell_session(shell_session_id)
        else {
            return false;
        };
        let replacement_shell_session_id = replacement_key
            .as_ref()
            .and_then(|key| self.remote_attach_sessions.get(key).copied());
        let request = GpuiLocalWorkspaceLifecycleRequest {
            action,
            confirmed_close_slot_id: None,
            mutation_kind,
            pane_id,
            replacement_shell_session_id,
            shell_session_id,
        };
        if action != GpuiLocalWorkspaceLifecycleAction::Close
            && gpui_local_workspace_lifecycle_request_is_pending(
                &self.local_workspace_lifecycle_requests,
                &request,
            )
        {
            return true;
        }
        let Some(request_id) = self.next_local_workspace_lifecycle_request_id() else {
            return false;
        };
        let mut message = serde_json::Map::new();
        message.insert(
            "action".to_string(),
            serde_json::Value::String(action.as_str().to_string()),
        );
        message.insert(
            "projectId".to_string(),
            serde_json::Value::String(gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            )),
        );
        message.insert("requestId".to_string(), serde_json::json!(request_id));
        message.insert(
            "sessionId".to_string(),
            serde_json::Value::String(key.session_id),
        );
        message.insert(
            "type".to_string(),
            serde_json::Value::String(
                GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_TYPE.to_string(),
            ),
        );
        message.insert(
            "version".to_string(),
            serde_json::json!(GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_MESSAGE_VERSION),
        );
        if let Some(replacement_key) = replacement_key {
            message.insert(
                "replacementProjectId".to_string(),
                serde_json::Value::String(gpui_remote_scoped_project_id(
                    replacement_key.remote_machine_id.as_str(),
                    replacement_key.project_id.as_str(),
                )),
            );
            message.insert(
                "replacementSessionId".to_string(),
                serde_json::Value::String(replacement_key.session_id),
            );
        } else if skip_replacement_fallback {
            message.insert(
                "skipReplacementFallback".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        if action == GpuiLocalWorkspaceLifecycleAction::Close {
            let changed = self.apply_local_workspace_terminal_lifecycle_result(request, cx);
            let _ = self.dispatch_gpui_workspace_terminal_lifecycle_request(
                serde_json::Value::Object(message),
                cx,
            );
            return changed;
        }
        self.local_workspace_lifecycle_requests
            .insert(request_id, request);
        if self.dispatch_gpui_workspace_terminal_lifecycle_request(
            serde_json::Value::Object(message),
            cx,
        ) {
            return true;
        }
        self.local_workspace_lifecycle_requests.remove(&request_id);
        false
    }

    pub(crate) fn receive_sidebar_workspace_terminal_lifecycle_result_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-06-26-07:25:
        The sidebar may acknowledge only a pending native Sleep/Wake request by request id and success boolean. Apply the matching local shell transition after a successful result, drop failed or stale results without mutation, and never trust project/session/title/path/command data from the result payload. Local-first Close notifications are not registered as pending, so their cleanup acknowledgements are intentionally ignored here.

        CDXC:CommandPane 2026-06-26-23:59:
        Mapped close requests no longer wait here: Rust consumes valid close confirmation and commits the shell close before notifying SidebarApp for best-effort gxserver transition.
        */
        let Ok(message) = gpui_sidebar_workspace_terminal_lifecycle_result_from_json(payload)
        else {
            return;
        };
        let Some(request) = self
            .local_workspace_lifecycle_requests
            .remove(&message.request_id)
        else {
            return;
        };
        if !message.ok {
            return;
        }
        self.apply_local_workspace_terminal_lifecycle_result(request, cx);
    }

    pub(crate) fn apply_local_workspace_terminal_lifecycle_result(
        &mut self,
        request: GpuiLocalWorkspaceLifecycleRequest,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-27-00:33:
        Local-first Close invokes this reducer directly, while acknowledged Sleep/Wake invokes it from the result bridge. If a close request carries native confirmation state, clear that exact slot as part of the same committed local mutation.
        */
        // Close-confirm bookkeeping belongs to the macOS-only native Ghostty
        // tab state; the GPUI engine path confirms closes in the terminal
        // model instead, so other OSes have no pending map to clear.
        #[cfg(target_os = "macos")]
        let confirmed_close_slot_id = request.confirmed_close_slot_id;
        if !self
            .agents_workspace
            .session_belongs_to_pane(request.pane_id, request.shell_session_id)
        {
            self.forget_local_workspace_mappings_for_shell_session(request.shell_session_id, cx);
            #[cfg(target_os = "macos")]
            if let Some(slot_id) = confirmed_close_slot_id {
                self.agents_terminal_close_confirms
                    .pending_by_slot
                    .remove(&slot_id);
            }
            return false;
        }

        let mut focus_agents_pane_after_mutation = false;
        let changed = match request.mutation_kind {
            GpuiLocalWorkspaceLifecycleMutationKind::DirectClose => {
                let changed = self
                    .agents_workspace
                    .close_tab_from_direct_tab_close(request.pane_id, request.shell_session_id);
                focus_agents_pane_after_mutation = changed;
                changed
            }
            GpuiLocalWorkspaceLifecycleMutationKind::ScopedClose => self
                .agents_workspace
                .close_tab(request.pane_id, request.shell_session_id),
            GpuiLocalWorkspaceLifecycleMutationKind::DirectSleep => {
                let slept = self
                    .agents_workspace
                    .set_session_sleeping(request.shell_session_id, true);
                let selected = if let Some(replacement_session_id) =
                    request.replacement_shell_session_id
                {
                    let before = self
                        .agents_workspace
                        .find_leaf(request.pane_id)
                        .and_then(|leaf| leaf.tab_group.active_session_id());
                    self.agents_workspace
                        .select_tab(request.pane_id, replacement_session_id);
                    before != Some(replacement_session_id)
                        && self
                            .agents_workspace
                            .find_leaf(request.pane_id)
                            .is_some_and(|leaf| {
                                leaf.tab_group.active_session_id() == Some(replacement_session_id)
                            })
                } else {
                    self.agents_workspace
                        .select_replacement_after_direct_tab_sleep(
                            request.pane_id,
                            request.shell_session_id,
                        )
                };
                focus_agents_pane_after_mutation = selected;
                slept || selected
            }
            GpuiLocalWorkspaceLifecycleMutationKind::DirectWake
            | GpuiLocalWorkspaceLifecycleMutationKind::RestoreWake => {
                /*
                CDXC:FocusRouting 2026-06-26-23:24:
                A mapped sleeping Agents wake result means gxserver has accepted `/api/wakeSession`; only now may Rust move the reused native tab into Mounting. This keeps sidebar session clicks, placeholder body clicks, and click-to-wake-disabled tab selection aligned with macOS and avoids local shell-only wake state.
                */
                let changed = activate_agents_terminal_placeholder_with_runtime_attempt_identity(
                    &mut self.agents_workspace,
                    &mut self.agents_terminal_runtime_sessions,
                    request.pane_id,
                    request.shell_session_id,
                );
                /*
                CDXC:CefRuntime 2026-07-12:
                A zmx sleep kills the daemon and drops the local engine record,
                so a woken placeholder usually has no parked owner, live record,
                or pending payload to finish the Mounting transition — it would
                sit in Mounting forever. Fetch attach metadata from gxserver
                (whose wake already respawned the provider with the restore
                command) and mount the reused tab through the ordinary attach
                pipeline.
                */
                self.request_agents_terminal_wake_attach_if_runtime_missing(
                    request.pane_id,
                    request.shell_session_id,
                    cx,
                );
                focus_agents_pane_after_mutation = changed;
                changed
            }
            GpuiLocalWorkspaceLifecycleMutationKind::ScopedSleep => self
                .agents_workspace
                .set_session_sleeping(request.shell_session_id, true),
        };
        #[cfg(target_os = "macos")]
        if let Some(slot_id) = confirmed_close_slot_id {
            self.agents_terminal_close_confirms
                .pending_by_slot
                .remove(&slot_id);
        }
        if !changed {
            return false;
        }
        if request.action == GpuiLocalWorkspaceLifecycleAction::Close {
            self.forget_local_workspace_mappings_for_shell_session(request.shell_session_id, cx);
        }
        if focus_agents_pane_after_mutation {
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
        }
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
        true
    }

    pub(crate) fn request_mapped_sleeping_agents_terminal_wake(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        mutation_kind: GpuiLocalWorkspaceLifecycleMutationKind,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.agents_terminal_session_is_mapped_sleeping(session_id) {
            return false;
        }
        if let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.workspace_terminal_key_for_shell_session(session_id)
        {
            return self.request_gpui_remote_attach_terminal_open(
                GpuiRemoteAttachSessionReference {
                    remote_machine_id: key.remote_machine_id,
                    project_id: key.project_id,
                    session_id: key.session_id,
                },
                Some(pane_id),
                AgentsWorkspaceNewTerminalPlacement::Tab,
                cx,
            );
        }
        /*
        CDXC:FocusRouting 2026-06-26-23:24:
        Mapped sleeping Agents sessions must wake through SidebarApp/gxserver before local placeholder materialization. The request carries only pane/session ids plus the fixed Wake action, reuses the existing mapped native tab, and deliberately has no replacement fallback because wake keeps the selected tab.
        */
        self.request_local_workspace_terminal_lifecycle(
            pane_id,
            session_id,
            GpuiLocalWorkspaceLifecycleAction::Wake,
            mutation_kind,
            None,
            false,
            None,
            cx,
        )
    }

    pub(crate) fn request_agents_terminal_wake_attach_if_runtime_missing(
        &mut self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        };
        if self.local_workspace_terminal_has_live_terminal_owner(slot_id)
            || self.local_workspace_terminal_has_pending_attach_payload(slot_id)
        {
            return;
        }
        #[cfg(target_os = "macos")]
        if self
            .agents_terminal_parked_runtime_owners
            .values()
            .any(|owner| owner.shell_session_id == shell_session_id)
        {
            return;
        }
        let Some(key) = self.local_workspace_key_for_shell_session(shell_session_id) else {
            return;
        };
        self.local_workspace_latest_focus_key = Some(key.clone());
        self.spawn_local_workspace_attach_plan(
            key,
            GpuiLocalWorkspaceAttachIntent::Attach,
            pane_id,
            true,
            GpuiWorkspaceTerminalFocusPlacement::Tab,
            GpuiLocalWorkspaceAttachOrigin::WakeRecovery,
            cx,
        );
    }

    pub(crate) fn agents_terminal_session_is_mapped_sleeping(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:Workarea 2026-08-07:
        "Mapped" means the tab carries a canonical gxserver identity that wake
        can address, which a remote attach tab does through
        `remote_attach_sessions` just as a local tab does through the local
        mappings. Requiring a local mapping here made the remote wake branch in
        `request_mapped_sleeping_agents_terminal_wake` unreachable, so a
        sleeping remote session could never be resumed from its tab.
        */
        self.agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Sleeping
            })
            && self
                .workspace_terminal_key_for_shell_session(session_id)
                .is_some()
    }

    pub(crate) fn local_workspace_attach_intent_for_key(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) -> GpuiLocalWorkspaceAttachIntent {
        self.prune_local_workspace_session_mappings();
        self.local_workspace_session_mappings
            .get(key)
            .copied()
            .and_then(|session_id| self.agents_workspace.session(session_id))
            .filter(|session| {
                session.presentation_state == TerminalSessionPresentationState::Sleeping
            })
            .map(|_| GpuiLocalWorkspaceAttachIntent::Wake)
            .unwrap_or(GpuiLocalWorkspaceAttachIntent::Attach)
    }

    pub(crate) fn local_workspace_terminal_has_pending_attach_payload(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        else {
            return false;
        };
        self.agents_terminal_launch_payload_source
            .has_payload_for_mount_slot(runtime_session_id, slot_id)
    }

    pub(crate) fn local_workspace_terminal_has_live_terminal_owner(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        if let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(slot_id.session_id)
        {
            if self
                .agents_gpui_engine_terminals
                .get(&slot_id.session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id)
            {
                return true;
            }
        }

        #[cfg(target_os = "macos")]
        {
            if self.agents_terminal_ghostty_surface_matches(slot_id) {
                return true;
            }
        }

        false
    }

    pub(crate) fn local_workspace_terminal_can_focus_existing(
        &self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
    ) -> bool {
        let Some(session) = self.agents_workspace.session(shell_session_id) else {
            return false;
        };
        if session.presentation_state != TerminalSessionPresentationState::Running {
            return false;
        }
        let slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        };
        self.local_workspace_terminal_has_live_terminal_owner(slot_id)
            || self.local_workspace_terminal_has_pending_attach_payload(slot_id)
    }

    pub(crate) fn agents_tab_selected_local_runtime_missing(
        &self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:FocusRouting 2026-07-11:
        A restored-after-restart mapped tab keeps Running presentation while nothing local can render it: no live terminal owner, no pending mount-slot attach payload, and no parked owner waiting for same-slot reattach. Only that fully-empty Running combination reports `localRuntimeMissing`; sleeping, mounting, popped-out, parked-inactive, and attach-in-flight tabs keep the ordinary one-way selection path.
        */
        let Some(session) = self.agents_workspace.session(shell_session_id) else {
            return false;
        };
        if session.presentation_state != TerminalSessionPresentationState::Running {
            return false;
        }
        let slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id: shell_session_id,
        };
        if self.local_workspace_terminal_has_live_terminal_owner(slot_id)
            || self.local_workspace_terminal_has_pending_attach_payload(slot_id)
        {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            if self
                .agents_terminal_parked_runtime_owners
                .values()
                .any(|owner| owner.shell_session_id == shell_session_id)
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn should_keep_project_editor_open_for_local_workspace_terminal_focus(
        &self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) -> bool {
        self.should_keep_project_editor_open_for_workspace_terminal_focus(
            &GpuiWorkspaceTerminalSessionKey::Local(key.clone()),
        )
    }

    pub(crate) fn should_keep_project_editor_open_for_workspace_terminal_focus(
        &self,
        key: &GpuiWorkspaceTerminalSessionKey,
    ) -> bool {
        if !self.active_mode.is_project_editor_mode()
            || !self.project_editor_shell.left_companion_visible
            // CDXC:SessionChat 2026-09-05 WHY: An explicit chat launch must expose the Agents composer; keeping Code or Docs open would focus its terminal companion instead.
            || self.pending_agents_chat_launch_intents.contains(key)
        {
            return false;
        }
        self.project_editor_companion_active_project_id()
            .is_some_and(|active_project_id| key.scoped_project_id() == active_project_id)
    }

    pub(crate) fn retarget_project_editor_companion_to_workspace_terminal(
        &mut self,
        mode: TitlebarMode,
        shell_session_id: TerminalSessionId,
        workspace_key: &GpuiWorkspaceTerminalSessionKey,
        focus_companion: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.shell_session_for_workspace_terminal_key(workspace_key) != Some(shell_session_id)
            || self.project_editor_companion_active_project_id().as_deref()
                != Some(workspace_key.scoped_project_id().as_str())
        {
            return;
        }
        self.mark_project_editor_mode_awake(mode, cx);
        if self.project_editor_companion_terminal_session_id == Some(shell_session_id) {
            self.project_editor_companion_focused_terminal_slot =
                ProjectEditorCompanionTerminalSlot::Top;
        } else if self.project_editor_companion_secondary_terminal_session_id
            == Some(shell_session_id)
        {
            self.project_editor_companion_focused_terminal_slot =
                ProjectEditorCompanionTerminalSlot::Bottom;
        } else {
            match self.project_editor_companion_focused_terminal_slot {
                ProjectEditorCompanionTerminalSlot::Top => {
                    self.project_editor_companion_terminal_session_id = Some(shell_session_id);
                }
                ProjectEditorCompanionTerminalSlot::Bottom
                    if self
                        .project_editor_companion_secondary_terminal_session_id
                        .is_some() =>
                {
                    self.project_editor_companion_secondary_terminal_session_id =
                        Some(shell_session_id);
                }
                ProjectEditorCompanionTerminalSlot::Bottom => {
                    if self.project_editor_companion_terminal_session_id.is_some() {
                        self.project_editor_companion_secondary_terminal_session_id =
                            Some(shell_session_id);
                    } else {
                        self.project_editor_companion_terminal_session_id = Some(shell_session_id);
                        self.project_editor_companion_focused_terminal_slot =
                            ProjectEditorCompanionTerminalSlot::Top;
                    }
                }
            }
        }
        self.sync_project_editor_companion_terminal_selection();
        if focus_companion {
            self.set_shell_focus_with_terminal_handoff(
                ShellFocusTarget::ProjectEditorCompanion(mode),
                true,
            );
            if let Some(slot_id) = self.project_editor_companion_terminal_slot_for_mode(mode) {
                self.request_project_editor_companion_session_text_focus_handoff(slot_id, cx);
            }
            self.set_sidebar_focus_border_handoff_target(shell_session_id);
        }
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
        &mut self,
        mode: TitlebarMode,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        workspace_key: &GpuiWorkspaceTerminalSessionKey,
    ) {
        if self.shell_session_for_workspace_terminal_key(workspace_key) != Some(session_id) {
            return;
        }
        /*
        CDXC:CodeEditor 2026-07-06:
        When a sidebar click keeps a project-editor mode open, the companion
        pane mounts the session first, so it owns the daemon-built attach
        payload including any queued startup text. The agents slot keeps a
        text-free copy of the same attach command so a later Agents-view mount
        attaches the same zmx session without re-sending startup input.
        */
        let Some(runtime_session_id) = self
            .agents_terminal_runtime_sessions
            .runtime_session_id_for_shell_session(session_id)
        else {
            return;
        };
        let agents_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        };
        let Some(payload) = self
            .agents_terminal_launch_payload_source
            .take_explicit_payload_for_mount_slot(runtime_session_id, agents_slot_id)
        else {
            return;
        };
        let mut agents_payload = payload.clone();
        agents_payload.initial_input = None;
        self.agents_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                agents_slot_id,
                agents_payload,
            );
        self.project_editor_companion_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                ProjectEditorCompanionTerminalBodyMountSlotId { mode, session_id },
                payload,
            );
        if let GpuiWorkspaceTerminalSessionKey::Remote(remote_key) = workspace_key {
            self.clear_project_editor_companion_remote_attach_state_for_key(remote_key);
        }
    }

    pub(crate) fn request_project_editor_companion_terminal_attach_payload(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CodeEditor 2026-07-06:
        A current companion slot with no live surface and no stored payload
        asks localhost gxserver for the session's attach metadata, exactly like
        a sidebar click, then stores the attach command for the exact companion
        mount slot. Startup text is never sent from this path; it belongs to
        the first materializing mount only.
        */
        if let Some(GpuiWorkspaceTerminalSessionKey::Remote(remote_key)) =
            self.project_editor_companion_terminal_key_for_slot(slot_id)
        {
            let Some(attempt) =
                self.project_editor_companion_remote_attach_attempt_for_slot(slot_id)
            else {
                return;
            };
            if self
                .project_editor_companion_remote_attach_states
                .get(&slot_id)
                .is_some_and(|state| state.attempt() == &attempt)
            {
                return;
            }
            self.project_editor_companion_remote_attach_states.insert(
                slot_id,
                GpuiProjectEditorCompanionRemoteAttachState::Preparing(attempt.clone()),
            );
            let Some(target) =
                self.gpui_remote_gxserver_request_target(remote_key.remote_machine_id.as_str())
            else {
                self.record_project_editor_companion_remote_attach_unavailable(
                    slot_id,
                    attempt,
                    "Reconnect the remote machine to show this terminal.".to_string(),
                );
                cx.notify();
                return;
            };
            let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
            let Some(config) = gpui_remote_machine_config_from_settings(
                settings_snapshot.object(),
                remote_key.remote_machine_id.as_str(),
            ) else {
                self.record_project_editor_companion_remote_attach_unavailable(
                    slot_id,
                    attempt,
                    "The saved remote machine is missing required SSH settings.".to_string(),
                );
                cx.notify();
                return;
            };
            let reference = GpuiRemoteAttachSessionReference {
                remote_machine_id: remote_key.remote_machine_id.clone(),
                project_id: remote_key.project_id.clone(),
                session_id: remote_key.session_id.clone(),
            };
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move {
                        gpui_prepare_remote_attach_terminal_plan(
                            &config, &target, &reference, true, true,
                        )
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if !this
                        .project_editor_companion_remote_attach_states
                        .get(&slot_id)
                        .is_some_and(|state| state.attempt() == &attempt)
                    {
                        return;
                    }
                    if !this.project_editor_companion_remote_attach_attempt_is_current(
                        slot_id, &attempt,
                    ) {
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
                    let Some(runtime_session_id) = this
                        .agents_terminal_runtime_sessions
                        .runtime_session_id_for_shell_session(slot_id.session_id)
                    else {
                        this.project_editor_companion_remote_attach_states
                            .remove(&slot_id);
                        return;
                    };
                    if this
                        .agents_gpui_engine_terminals
                        .get(&slot_id.session_id)
                        .is_some_and(|record| record.runtime_session_id == runtime_session_id)
                    {
                        this.project_editor_companion_remote_attach_states
                            .remove(&slot_id);
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
                                    env::var("DISPLAY")
                                        .unwrap_or_else(|_| "localhost:0".to_string()),
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
                        this.record_project_editor_companion_remote_attach_unavailable(
                            slot_id,
                            attempt,
                            "GPUI could not prepare the remote attach terminal command."
                                .to_string(),
                        );
                        cx.notify();
                        return;
                    }
                    if let Some(session) = this
                        .agents_workspace
                        .terminal_sessions
                        .iter_mut()
                        .find(|session| session.id == slot_id.session_id)
                    {
                        session.title = plan.title;
                        session.agent_icon = plan.agent_icon;
                    }
                    #[cfg(target_os = "macos")]
                    if let Some(askpass) = plan.askpass {
                        this.remote_attach_askpass_scripts
                            .insert(remote_key.clone(), askpass);
                    }
                    this.project_editor_companion_terminal_launch_payload_source
                        .insert_explicit_payload_for_mount_slot(
                            runtime_session_id,
                            slot_id,
                            payload,
                        );
                    this.project_editor_companion_remote_attach_states
                        .remove(&slot_id);
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if !self
            .project_editor_companion_terminal_attach_plan_pending
            .insert(slot_id)
        {
            return;
        }
        let Some(key) = self.local_workspace_key_for_shell_session(slot_id.session_id) else {
            self.project_editor_companion_terminal_attach_plan_pending
                .remove(&slot_id);
            return;
        };
        let attach_intent = self.local_workspace_attach_intent_for_key(&key);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let prepare_key = key.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_local_workspace_attach_terminal_plan(&prepare_key, attach_intent)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.project_editor_companion_terminal_attach_plan_pending
                    .remove(&slot_id);
                let Ok(plan) = result else {
                    return;
                };
                if !this.is_current_project_editor_companion_terminal_body_mount_slot(slot_id) {
                    return;
                }
                if this.local_workspace_key_for_shell_session(slot_id.session_id) != Some(key) {
                    return;
                }
                let Some(runtime_session_id) = this
                    .agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(slot_id.session_id)
                else {
                    return;
                };
                if this
                    .agents_gpui_engine_terminals
                    .get(&slot_id.session_id)
                    .is_some_and(|record| record.runtime_session_id == runtime_session_id)
                {
                    return;
                }
                let payload = AgentsTerminalExplicitLaunchPayload {
                    working_directory: plan.working_directory,
                    command: Some(plan.attach_command),
                    env_vars: Vec::new(),
                    initial_input: None,
                    wait_after_command: false,
                };
                if payload.to_ghostty_launch_payload().is_err() {
                    return;
                }
                if let Some(session) = this
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == slot_id.session_id)
                {
                    session.zmx_session_name = plan.zmx_name;
                }
                this.project_editor_companion_terminal_launch_payload_source
                    .insert_explicit_payload_for_mount_slot(runtime_session_id, slot_id, payload);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn focus_existing_gpui_local_workspace_terminal(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let focus_started_at = Instant::now();
        self.prune_local_workspace_session_mappings();
        let Some(shell_session_id) = self.local_workspace_session_mappings.get(key).copied() else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            self.local_workspace_session_mappings.remove(key);
            self.local_app_shot_session_mappings
                .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
            return false;
        };
        if !self.local_workspace_terminal_can_focus_existing(pane_id, shell_session_id) {
            return false;
        }

        let keep_editor_mode =
            self.should_keep_project_editor_open_for_local_workspace_terminal_focus(key);
        let project_editor_mode = self.active_mode;
        if !keep_editor_mode {
            self.active_mode = TitlebarMode::Agents;
        }
        /*
        CDXC:FocusRouting 2026-06-26-06:34:
        Focusing an already-mapped local gxserver session reuses the existing GPUI tab only after the session has a live terminal owner or an inserted attach payload for the exact mount slot. Reconciled sidebar placeholders without attach state intentionally fall through to the gxserver attach pipeline so they cannot mount a default shell.
        */
        focus_existing_local_workspace_terminal_tab_model(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            pane_id,
            shell_session_id,
        );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.tabModelSelected",
            serde_json::json!({
                "elapsedMs": focus_started_at.elapsed().as_millis() as u64,
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        self.activate_preferred_agents_chat_launch_intent(shell_session_id, cx);
        if keep_editor_mode {
            let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key.clone());
            self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                project_editor_mode,
                pane_id,
                shell_session_id,
                &workspace_key,
            );
            self.retarget_project_editor_companion_to_workspace_terminal(
                project_editor_mode,
                shell_session_id,
                &workspace_key,
                true,
                cx,
            );
        } else {
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.set_sidebar_focus_border_handoff_target(shell_session_id);
            self.request_agents_session_text_focus_handoff(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                },
                cx,
            );
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.local_app_shot_session_mappings
            .insert(key.session_id.clone(), shell_session_id);
        /*
        Sidebar-originated focus updates React optimistically before this
        native tab selection runs. Publish the post-selection workspace owners
        so the sidebar replaces that provisional click-history set with the
        exact currently surfaced panes: one focused session plus the selected
        session in every other rendered split.
        */
        self.dispatch_gpui_workspace_tab_session_selected(
            key.project_id.as_str(),
            key.session_id.as_str(),
            false,
            false,
            cx,
        );
        self.persist_shell_layout_state();
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
        true
    }

    pub(crate) fn open_gpui_local_workspace_terminal(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        plan: GpuiLocalWorkspaceAttachTerminalPlan,
        requested_pane_id: WorkspacePaneId,
        force_requested_pane_placement: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.focus_existing_gpui_local_workspace_terminal(&key, cx) {
            return true;
        }
        let focused_project_id = key.project_id.clone();
        let focused_session_id = key.session_id.clone();
        /*
        CDXC:FocusRouting 2026-06-26-06:08:
        Local sidebar clicks create or focus Agents workspace tabs like macOS session attach: existing mapped tabs are reused, while new local gxserver attaches start as selected Running mount slots and receive the daemon-built attach command through a one-shot process-local launch payload. Shell persistence keeps layout/lifecycle metadata and must not store commands, paths, daemon bodies, renderer labels, titles from CEF, tokens, stdout/stderr, or terminal content.

        CDXC:FocusRouting 2026-06-26-06:18:
        MacOS decides the workspace tab group at sidebar activation time, then lets async wake/attach complete against that focus intent. GPUI must pass the captured Agents pane through attach completion so focusing another pane while gxserver prepares metadata cannot move the restored session into the wrong tab group.
        */
        let keep_editor_mode =
            self.should_keep_project_editor_open_for_local_workspace_terminal_focus(&key);
        let project_editor_mode = self.active_mode;
        let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key.clone());
        let result = insert_gpui_local_workspace_attach_terminal(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            &mut self.agents_terminal_launch_payload_source,
            &mut self.local_workspace_session_mappings,
            &mut self.local_app_shot_session_mappings,
            requested_pane_id,
            force_requested_pane_placement,
            key,
            plan,
        );
        let (pane_id, session_id) = match result {
            Ok(inserted) => inserted,
            Err(message) => {
                self.cancel_sidebar_focus_border_handoff();
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Session attach unavailable",
                    message,
                    cx,
                );
                return false;
            }
        };
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
            self.set_sidebar_focus_border_handoff_target(session_id);
            self.request_agents_session_text_focus_handoff(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                },
                cx,
            );
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.dispatch_gpui_workspace_tab_session_selected(
            focused_project_id.as_str(),
            focused_session_id.as_str(),
            false,
            false,
            cx,
        );
        self.persist_shell_layout_state();
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
        true
    }

    pub(crate) fn open_gpui_local_workspace_terminal_in_new_leaf(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        plan: GpuiLocalWorkspaceAttachTerminalPlan,
        requested_pane_id: WorkspacePaneId,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-07-24:
        Split-right/split-below/bottom-row quick creation opens the freshly
        created gxserver session in a new workspace leaf instead of a tab in the
        requested pane. The session is otherwise identical to a sidebar attach:
        Running presentation, one-shot mount-slot attach payload, and
        local-workspace mappings so the sidebar lists it and project switches
        keep it.
        */
        if self.focus_existing_gpui_local_workspace_terminal(&key, cx) {
            return true;
        }
        let focused_project_id = key.project_id.clone();
        let focused_session_id = key.session_id.clone();
        let result = insert_gpui_local_workspace_attach_terminal_in_new_leaf(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            &mut self.agents_terminal_launch_payload_source,
            &mut self.local_workspace_session_mappings,
            &mut self.local_app_shot_session_mappings,
            requested_pane_id,
            placement,
            key,
            plan,
        );
        let (pane_id, session_id) = match result {
            Ok(inserted) => inserted,
            Err(message) => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Session attach unavailable",
                    message,
                    cx,
                );
                return false;
            }
        };
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        self.active_mode = TitlebarMode::Agents;
        self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
        self.set_sidebar_focus_border_handoff_target(session_id);
        self.request_agents_session_text_focus_handoff(
            AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            },
            cx,
        );
        self.workspace_drop_feedback = None;
        self.scroll_workspace_pane_active_tab(pane_id);
        self.dispatch_gpui_workspace_tab_session_selected(
            focused_project_id.as_str(),
            focused_session_id.as_str(),
            false,
            false,
            cx,
        );
        self.persist_shell_layout_state();
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
        true
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn compensate_unmaterialized_created_workspace_terminal(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) {
        /*
        This runs inside the serialized GPUI completion update. Re-check the
        canonical mapping immediately before exact close/remove compensation so
        a session already materialized by presentation reconciliation is never
        deleted. The synchronous local cleanup is reserved for the rare case
        where the originally captured pane was deleted while creation ran.
        */
        if self.local_workspace_session_mappings.contains_key(key) {
            return;
        }
        let _ = gpui_close_command_terminal_gxserver_session(key);
    }
}
