//! Agents tab actions: select, close and sleep from actions and scopes, focus mode, and closing running surfaces.

use crate::app::actions::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn select_agents_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .agents_workspace
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
        {
            self.select_agents_tab(pane_id, session_id, cx);
            cx.notify();
        }
    }

    pub(crate) fn close_agents_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_agents_tab(pane_id, session_id, cx);
    }

    pub(crate) fn forget_local_workspace_mappings_for_shell_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:FocusRouting 2026-06-26-06:57:
        Removing a GPUI shell tab must also drop only the process-local gxserver/session mapping for that shell id. Close provider cleanup and acknowledged Sleep transitions remain sidebar-owned through the lifecycle bridge; this cleanup prevents stale GPUI mappings from selecting a deleted tab without fabricating daemon success, deleting gxserver rows, logging ids, or touching persisted private data.
        */
        let scoped_remote_key = self
            .workspace_terminal_key_for_shell_session(shell_session_id)
            .and_then(|key| match key {
                GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                GpuiWorkspaceTerminalSessionKey::Local(_) => None,
            });
        self.remove_agents_chat_surface_for_session(shell_session_id, cx);
        if let Some(remote_key) = scoped_remote_key.as_ref() {
            self.source_code_server_runtime
                .cancel_remote_prompt_editor_request_for_remote_session(remote_key);
            self.remote_attach_sessions.remove(remote_key);
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            self.remote_attach_askpass_scripts.remove(remote_key);
        }
        let removed_keys = self
            .local_workspace_session_mappings
            .iter()
            .filter_map(|(key, mapped_session_id)| {
                (*mapped_session_id == shell_session_id).then_some(key.clone())
            })
            .collect::<Vec<_>>();
        if removed_keys.is_empty() {
            return;
        }
        self.local_workspace_session_mappings
            .retain(|_, mapped_session_id| *mapped_session_id != shell_session_id);
        self.local_workspace_attach_pending
            .retain(|key| !removed_keys.contains(key));
        self.local_workspace_lifecycle_requests
            .retain(|_, request| request.shell_session_id != shell_session_id);
        if self
            .local_workspace_latest_focus_key
            .as_ref()
            .is_some_and(|key| removed_keys.contains(key))
        {
            self.local_workspace_latest_focus_key = None;
        }
    }

    pub(crate) fn close_agents_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.agents_workspace.can_close_tab(pane_id, session_id) {
            return false;
        }
        // Closing a coordinator or a busy agent asks first; Close runs this again
        // (gx_store/sidebar_close_confirm.rs). On 2026-11-02 a stray tab close here closed the
        // Main Coordinator without a word.
        let sessions: Vec<_> = self
            .workspace_terminal_session_key(session_id)
            .into_iter()
            .collect();
        if self.gx_store_ask_before_close(
            &sessions,
            Box::new(move |app, cx| {
                app.close_agents_tab(pane_id, session_id, cx);
            }),
            cx,
        ) {
            return false;
        }
        let target_key = self.workspace_terminal_key_for_shell_session(session_id);
        let replacement_key = self
            .agents_workspace
            .selected_session_after_direct_tab_close(pane_id, session_id)
            .and_then(|replacement_session_id| {
                self.workspace_terminal_key_for_shell_session(replacement_session_id)
            });
        /*
        CDXC:Workarea 2026-06-26-05:23:
        Direct mapped Close mirrors macOS pane tabs, including final-root close. When there is no pane-local replacement, tell the store (gx_store/terminal_lifecycle/lifecycle_requests.rs) not to focus a fallback session; Rust removes the shell tab immediately and leaves the workspace empty if this was the final terminal.

        CDXC:Workarea 2026-06-26-23:59:
        Mapped GPUI workspace close bypasses Ghostty close-confirm, commits the Rust tab mutation locally, and routes only provider cleanup through the store (formerly SidebarApp). Mounted surface close-confirm remains for unmapped/local-only running terminals only.
        */
        let skip_replacement_fallback = replacement_key.is_none();
        match target_key {
            Some(GpuiWorkspaceTerminalSessionKey::Local(_)) => {
                return self.request_local_workspace_terminal_lifecycle(
                    pane_id,
                    session_id,
                    GpuiLocalWorkspaceLifecycleAction::Close,
                    GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                    replacement_key.and_then(|key| key.as_local().cloned()),
                    skip_replacement_fallback,
                    cx,
                );
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(_)) => {
                return self.request_remote_workspace_terminal_lifecycle(
                    pane_id,
                    session_id,
                    GpuiLocalWorkspaceLifecycleAction::Close,
                    GpuiLocalWorkspaceLifecycleMutationKind::DirectClose,
                    replacement_key.and_then(|key| match key {
                        GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                        GpuiWorkspaceTerminalSessionKey::Local(_) => None,
                    }),
                    skip_replacement_fallback,
                    cx,
                );
            }
            None => {}
        }

        if self.request_close_agents_gpui_engine_terminal(
            AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            },
            cx,
        ) {
            cx.notify();
            return true;
        }
        if !self
            .agents_workspace
            .close_tab_from_direct_tab_close(pane_id, session_id)
        {
            return false;
        }
        self.forget_local_workspace_mappings_for_shell_session(session_id, cx);
        self.focus_shell_target(
            ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
            cx,
        );
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
        true
    }

    pub(crate) fn close_agents_tabs_for_scope(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabCloseScope,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-26-06:57:
        Scoped Agents context-menu Close rows are local tab mutations plus asynchronous lifecycle cleanup, not tab-selection actions. Resolve the clicked pane group before mutation, preserve current shell focus for Close Right/Left/Others, and let direct inline close keep using the clicked-tab focus path.
        */
        if scope == AgentsWorkspaceTabCloseScope::Close {
            return self.close_agents_tab(pane_id, session_id, cx);
        }

        let session_ids = self
            .agents_workspace
            .tab_session_ids_for_close_scope(pane_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }
        let sessions: Vec<_> = session_ids
            .iter()
            .filter_map(|id| self.workspace_terminal_session_key(*id))
            .collect();
        if self.gx_store_ask_before_close(
            &sessions,
            Box::new(move |app, cx| {
                app.close_agents_tabs_for_scope(pane_id, session_id, scope, cx);
            }),
            cx,
        ) {
            return false;
        }

        let mut close_requested = false;
        let mut model_changed = false;
        for close_session_id in session_ids {
            if let Some(workspace_key) =
                self.workspace_terminal_key_for_shell_session(close_session_id)
            {
                /*
                CDXC:Workarea 2026-06-26-23:59:
                Scoped mapped close follows macOS by removing the Rust tab immediately and asking the store (formerly SidebarApp) to clean up the provider asynchronously, before considering any mounted Ghostty close-confirm path. This prevents either a retryable terminal prompt or a delayed external bridge from blocking local tab removal.
                */
                let requested = match workspace_key {
                    GpuiWorkspaceTerminalSessionKey::Local(_) => self
                        .request_local_workspace_terminal_lifecycle(
                            pane_id,
                            close_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Close,
                            GpuiLocalWorkspaceLifecycleMutationKind::ScopedClose,
                            None,
                            false,
                            cx,
                        ),
                    GpuiWorkspaceTerminalSessionKey::Remote(_) => self
                        .request_remote_workspace_terminal_lifecycle(
                            pane_id,
                            close_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Close,
                            GpuiLocalWorkspaceLifecycleMutationKind::ScopedClose,
                            None,
                            false,
                            cx,
                        ),
                };
                if requested {
                    close_requested = true;
                }
                continue;
            }
            if self.request_close_agents_gpui_engine_terminal(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: close_session_id,
                },
                cx,
            ) {
                close_requested = true;
                continue;
            }
            if self.agents_workspace.close_tab(pane_id, close_session_id) {
                self.forget_local_workspace_mappings_for_shell_session(close_session_id, cx);
                model_changed = true;
            }
        }

        if model_changed {
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        if model_changed || close_requested {
            cx.notify();
        }
        model_changed || close_requested
    }

    pub(crate) fn close_agents_tabs_for_scope_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = AgentsWorkspaceTabCloseScope::from_action_value(scope_value) else {
            return;
        };
        self.close_agents_tabs_for_scope(pane_id, session_id, scope, cx);
    }

    pub(crate) fn sleep_agents_tabs_for_scope(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabSleepScope,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:SessionSleep 2026-06-26-06:57:
        Agents context-menu Sleep keeps tabs in the main workspace instead of closing them. Direct Sleep may retarget active selection to an awake sibling from the clicked pane group; Sleep Right/Left/Others preserve shell focus and only mark resolved sessions sleeping so parked-owner detach happens through normal surface reconciliation.
        */
        let session_ids = self
            .agents_workspace
            .tab_session_ids_for_sleep_scope(pane_id, session_id, scope);
        if session_ids.is_empty() {
            return false;
        }

        let mut lifecycle_requested = false;
        let mut model_changed = false;
        for sleep_session_id in session_ids {
            let replacement_key = if scope == AgentsWorkspaceTabSleepScope::Sleep {
                self.agents_workspace
                    .replacement_session_after_direct_tab_sleep(pane_id, session_id)
                    .and_then(|replacement_session_id| {
                        self.workspace_terminal_key_for_shell_session(replacement_session_id)
                    })
            } else {
                None
            };
            let mutation_kind = if scope == AgentsWorkspaceTabSleepScope::Sleep {
                GpuiLocalWorkspaceLifecycleMutationKind::DirectSleep
            } else {
                GpuiLocalWorkspaceLifecycleMutationKind::ScopedSleep
            };
            if let Some(workspace_key) =
                self.workspace_terminal_key_for_shell_session(sleep_session_id)
            {
                let skip_replacement_fallback =
                    scope == AgentsWorkspaceTabSleepScope::Sleep && replacement_key.is_none();
                let requested = match workspace_key {
                    GpuiWorkspaceTerminalSessionKey::Local(_) => self
                        .request_local_workspace_terminal_lifecycle(
                            pane_id,
                            sleep_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Sleep,
                            mutation_kind,
                            replacement_key
                                .clone()
                                .and_then(|key| key.as_local().cloned()),
                            skip_replacement_fallback,
                            cx,
                        ),
                    GpuiWorkspaceTerminalSessionKey::Remote(_) => self
                        .request_remote_workspace_terminal_lifecycle(
                            pane_id,
                            sleep_session_id,
                            GpuiLocalWorkspaceLifecycleAction::Sleep,
                            mutation_kind,
                            replacement_key.clone().and_then(|key| match key {
                                GpuiWorkspaceTerminalSessionKey::Remote(key) => Some(key),
                                GpuiWorkspaceTerminalSessionKey::Local(_) => None,
                            }),
                            skip_replacement_fallback,
                            cx,
                        ),
                };
                if requested {
                    lifecycle_requested = true;
                }
                continue;
            }
            if self
                .agents_workspace
                .set_session_sleeping(sleep_session_id, true)
            {
                model_changed = true;
            }
        }
        if scope == AgentsWorkspaceTabSleepScope::Sleep
            && !lifecycle_requested
            && self
                .agents_workspace
                .select_replacement_after_direct_tab_sleep(pane_id, session_id)
        {
            model_changed = true;
        }

        if model_changed {
            self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
            self.persist_shell_layout_state();
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            cx.notify();
        }
        if lifecycle_requested {
            cx.notify();
        }
        model_changed || lifecycle_requested
    }

    pub(crate) fn sleep_agents_tabs_for_scope_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope_value: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = AgentsWorkspaceTabSleepScope::from_action_value(scope_value) else {
            return;
        };
        self.sleep_agents_tabs_for_scope(pane_id, session_id, scope, cx);
    }

    pub(crate) fn toggle_agents_focus_mode_for_tab_from_action(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .agents_workspace
            .toggle_focus_mode_from_tab_double_click(pane_id, session_id)
        {
            return;
        }
        self.focus_shell_target(
            ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
            cx,
        );
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /*
    CDXC:Terminal 2026-07-04:
    GPUI-engine close requests decide confirmation at request time from live
    process/prompt state (mirroring ghostty `needsConfirmQuit`: exited never
    confirms; `confirm-close-surface = true` skips confirmation at a
    shell-integration prompt). A confirmation-needed request becomes a
    pending entry for the close-confirm dialog; a no-confirm request returns false so the caller's existing direct
    model-close path runs and the engine record is pruned by sync.
    */
    pub(crate) fn request_close_agents_gpui_engine_terminal(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id)
            || !self
                .agents_workspace
                .can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            return false;
        }
        let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) else {
            return false;
        };
        let behavior = record.confirm_close_behavior;
        let view = record.view.clone();
        if view.update(cx, |view, _cx| view.needs_confirm_close(behavior)) {
            self.agents_gpui_engine_close_confirms.insert(slot_id);
            true
        } else {
            false
        }
    }
}
