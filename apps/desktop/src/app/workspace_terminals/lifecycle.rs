//! Local workspace session mappings, local and remote lifecycle requests and results, wakes, and attach-state queries.

use std::collections::HashSet;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn prune_local_workspace_session_mappings(&mut self) {
        let current_shell_session_ids = self
            .agents_workspace
            .terminal_session_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        self.local_workspace_session_mappings
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

        // The door this close came through, for gxserver's close log (session_close_log.rs).
        if action == GpuiLocalWorkspaceLifecycleAction::Close
            && let Some(door) = self.gx_store.close_source.take()
        {
            message.insert("closeSource".to_string(), serde_json::json!(door));
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
        // The door this close came through, for gxserver's close log (session_close_log.rs).
        if action == GpuiLocalWorkspaceLifecycleAction::Close
            && let Some(door) = self.gx_store.close_source.take()
        {
            message.insert("closeSource".to_string(), serde_json::json!(door));
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

    /// The daemon's answer to a pending Sleep or Wake of a tab: apply it when it succeeded, drop
    /// the request either way. Close is never pending, so its answer finds nothing here.
    pub(crate) fn finish_local_workspace_lifecycle_request(
        &mut self,
        request_id: u64,
        ok: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = self.local_workspace_lifecycle_requests.remove(&request_id) else {
            return;
        };
        if !ok {
            return;
        }
        self.apply_local_workspace_terminal_lifecycle_result(request, cx);
    }

    pub(crate) fn apply_local_workspace_terminal_lifecycle_result(
        &mut self,
        request: GpuiLocalWorkspaceLifecycleRequest,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .agents_workspace
            .session_belongs_to_pane(request.pane_id, request.shell_session_id)
        {
            self.forget_local_workspace_mappings_for_shell_session(request.shell_session_id, cx);
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
        if !changed {
            return false;
        }
        if request.action == GpuiLocalWorkspaceLifecycleAction::Close {
            self.forget_local_workspace_mappings_for_shell_session(request.shell_session_id, cx);
        }
        if focus_agents_pane_after_mutation {
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
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
        Mapped sleeping Agents sessions must wake through the store/gxserver (formerly SidebarApp) before local placeholder materialization. The request carries only pane/session ids plus the fixed Wake action, reuses the existing mapped native tab, and deliberately has no replacement fallback because wake keeps the selected tab.
        */
        self.request_local_workspace_terminal_lifecycle(
            pane_id,
            session_id,
            GpuiLocalWorkspaceLifecycleAction::Wake,
            mutation_kind,
            None,
            false,
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
        if self.local_workspace_terminal_has_attach_state(slot_id) {
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
            && self
                .agents_gpui_engine_terminals
                .get(&slot_id.session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id)
        {
            return true;
        }

        false
    }

    /// CDXC:FocusRouting 2026-09-13 WHY:
    /// Chat intentionally releases a zmx terminal viewer while retaining the runtime's attach recipe.
    /// Treating that state as a missing runtime repeatedly fetched attach plans, promoted restore completions to sidebar focus, and stole the keyboard from menus and transcript selections.
    /// The recipe must match the current runtime, just as a live viewer must; a stale recipe still requires the normal attach path.
    pub(crate) fn local_workspace_terminal_has_attach_state(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.local_workspace_terminal_has_live_terminal_owner(slot_id)
            || self.agents_terminal_has_detachable_viewer(slot_id.session_id)
            || self.local_workspace_terminal_has_pending_attach_payload(slot_id)
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
        self.local_workspace_terminal_has_attach_state(slot_id)
    }

    pub(crate) fn agents_tab_selected_local_runtime_missing(
        &self,
        pane_id: WorkspacePaneId,
        shell_session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:FocusRouting 2026-07-11:
        A restored-after-restart mapped tab keeps Running presentation while nothing local can render it: no live terminal owner, no reusable viewer recipe, no pending mount-slot attach payload, and no parked owner waiting for same-slot reattach. Only that fully-empty Running combination reports `localRuntimeMissing`; sleeping, mounting, popped-out, parked-inactive, and attach-in-flight tabs keep the ordinary one-way selection path.
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
        if self.local_workspace_terminal_has_attach_state(slot_id) {
            return false;
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

    /// CDXC:Workarea 2026-09-20 WHY:
    /// Selecting a session never closes the view panel any more: the Agents column is beside it, so
    /// the session appears without the view going away. The active-project check stays, because a
    /// click on another project's session still swaps the whole workspace.
    pub(crate) fn should_keep_project_editor_open_for_workspace_terminal_focus(
        &self,
        key: &GpuiWorkspaceTerminalSessionKey,
    ) -> bool {
        if !self.view_panel_open() {
            return false;
        }
        self.active_sidebar_project_id()
            .is_some_and(|active_project_id| key.scoped_project_id() == active_project_id)
    }
}
