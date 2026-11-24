use super::state::{NativeChatConfig, NativeChatEvent, NativeChatView};
use crate::*;

impl GhostexGpuiApp {
    /// Runs a chat's host action on the app in `chat_window`, the window that chat is drawn in,
    /// or, before it has drawn or once that window is gone, in the app's current window.
    ///
    /// CDXC:SessionChat 2026-11-11 WHY:
    /// User: "i click here and the menu appears off screen pls fix", of the "30 suggested" links pill. A host action used to run through `update_in` from a spawned task, which takes the window GPUI last saw the app drawn in: the floating sidebar, a small panel at the screen's edge or any other window that reads the app. The links list measured the pill's anchor, which is in the chat's window, from that window's frame and opened far from the pill, mostly off screen. Every host action that places something (the links list, a visual page, a file in Browser) means the chat's own window.
    pub(crate) fn update_in_chat_window(
        this: &gpui::WeakEntity<Self>,
        chat_window: Option<gpui::AnyWindowHandle>,
        cx: &mut gpui::AsyncApp,
        update: impl FnOnce(&mut Self, &mut gpui::Window, &mut gpui::Context<Self>),
    ) {
        let mut update = Some(update);
        if let Some(handle) = chat_window {
            let ran = handle.update(cx, |_, window, cx| {
                this.update(cx, |this, cx| {
                    if let Some(update) = update.take() {
                        update(this, window, cx);
                    }
                })
                .is_ok()
            });
            if ran.unwrap_or(false) {
                return;
            }
        }
        if let Some(update) = update.take() {
            let _ = this.update_in(cx, update);
        }
    }

    /// CDXC:SessionChat 2026-09-21 DECISION:
    /// User: "remove the ability to switch between GPUI chat view and the React chat view in the settings, and take out the React chat view completely from the GPUI app". Desktop chat is GPUI only, superseding the 2026-09-17 GPUI/React toggle.
    pub(crate) fn ensure_native_chat(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<NativeChatView>> {
        let existing = self.native_chat_views.get(&session_id).cloned();
        if let Some(view) = &existing
            && (!view.read(cx).config.session_id.is_empty()
                || self
                    .workspace_terminal_key_for_shell_session(session_id)
                    .is_none())
        {
            return Some(view.clone());
        }
        let key = self.workspace_terminal_key_for_shell_session(session_id)?;
        let sidebar_session_id = match &key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => {
                gpui_combined_presentation_session_id(&key.project_id, &key.session_id)
            }
            GpuiWorkspaceTerminalSessionKey::Remote(key) => gpui_remote_scoped_session_id(
                &key.remote_machine_id,
                &key.project_id,
                &key.session_id,
            ),
        };
        // The machine id is the same one `build_session_chat_runtime_request` puts on the broker's
        // `identity`, because the Rust brain's per-session storage keys are built from it.
        let (machine_id, project_id, server_session_id, remote) = match &key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => (
                crate::app::gx_chat::LOCAL_MACHINE_ID.to_string(),
                key.project_id.clone(),
                key.session_id.clone(),
                None,
            ),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => (
                key.remote_machine_id.clone(),
                key.project_id.clone(),
                key.session_id.clone(),
                Some(self.gpui_remote_gxserver_request_target(&key.remote_machine_id)?),
            ),
        };
        let config = NativeChatConfig {
            machine_id,
            project_id,
            session_id: server_session_id,
            sidebar_session_id,
            shell_session_id: session_id,
            app: Some(cx.weak_entity()),
            parent_native_view: self.parent_ns_view,
            client_id: format!("native-desktop-{}", std::process::id()),
            remote,
            initial_snapshot: None,
            initial_presentation: self.initial_session_chat_presentation(Some(&key)),
        };
        if let Some(view) = existing {
            self.agents_chat_page_states
                .get_mut(&session_id)?
                .account_key = Some(key);
            view.update(cx, |view, cx| view.bind_launched_session(config, cx));
            return Some(view);
        }
        Some(self.insert_native_chat(config, Some(key), cx))
    }

    pub(crate) fn insert_native_chat(
        &mut self,
        config: NativeChatConfig,
        key: Option<GpuiWorkspaceTerminalSessionKey>,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<NativeChatView> {
        let session_id = config.shell_session_id;
        let armed_actions = self.session_chat_armed_actions(session_id);
        let draft_coordinator_mode = self
            .gx_store
            .unified_sidebar
            .carry_coordinator_draft
            .replace(false);
        let view = cx.new(|cx| {
            let mut view = NativeChatView::new(config, cx);
            view.armed_actions = armed_actions;
            view.draft_coordinator_mode = draft_coordinator_mode;
            view.draft_setup_changed = draft_coordinator_mode;
            view
        });
        self.register_native_chat_view(session_id, view, key, cx)
    }

    fn register_native_chat_view(
        &mut self,
        session_id: TerminalSessionId,
        view: Entity<NativeChatView>,
        key: Option<GpuiWorkspaceTerminalSessionKey>,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<NativeChatView> {
        let mut state = SessionChatPageState::new();
        state.account_key = key;
        let generation = state.generation;
        let subscription =
            cx.subscribe(
                &view,
                move |this, view, event: &NativeChatEvent, cx| match event {
                    NativeChatEvent::Broker(message) => {
                        this.relay_session_chat_runtime_request(generation, message);
                    }
                    NativeChatEvent::SessionDetailsRing => {
                        if this.session_details_target_chat().as_ref() == Some(&view) {
                            cx.notify();
                        }
                    }
                    // The main header's or a popup's Work button (the popup redraws from the app).
                    NativeChatEvent::WorkButton => cx.notify(),
                    // The header leaves the project out while the pill names it.
                    NativeChatEvent::WherePillProject => cx.notify(),
                    NativeChatEvent::ComposerFocused => {
                        this.reclaim_gpui_root_for_chrome_input_focus();
                        if this.record_shell_focus_for_session_chat(session_id) == Some(true) {
                            this.persist_shell_layout_state();
                            cx.notify();
                        }
                    }
                    NativeChatEvent::DraftState(empty) => {
                        if this
                            .agents_chat_page_states
                            .get(&session_id)
                            .is_some_and(|state| state.generation == generation)
                        {
                            this.session_chat_composer_empty_reports
                                .insert(session_id, *empty);
                        }
                    }
                    NativeChatEvent::Host(message) => {
                        let message = message.clone();
                        let chat_window = view.read(cx).main_window;
                        cx.spawn(async move |this, cx| {
                            Self::update_in_chat_window(
                                &this,
                                chat_window,
                                cx,
                                move |this, window, cx| {
                                    if this
                                        .agents_chat_page_states
                                        .get(&session_id)
                                        .is_none_or(|state| state.generation != generation)
                                    {
                                        return;
                                    }
                                    this.receive_session_chat_host_action(
                                        session_id,
                                        &message.to_string(),
                                        window,
                                        cx,
                                    );
                                },
                            );
                        })
                        .detach();
                    }
                },
            );
        view.update(cx, |view, _| {
            view.subscriptions.clear();
            view.subscriptions.push(subscription);
        });
        self.agents_chat_page_states.insert(session_id, state);
        if let Some(previous) = self.native_chat_views.insert(session_id, view.clone())
            && previous != view
        {
            previous.update(cx, |previous, cx| {
                previous.dismiss_windows_for_hidden_pane(cx)
            });
        }
        view
    }

    /// Single-frame follow into a pending target project after a draft commits on Send (Q4849
    /// `"follow"` + Q4850 `"keep-space"`): removes the draft's old tab from the outgoing project
    /// before parking it, switches to the target project once, transfers the exact live
    /// `Entity<NativeChatView>` into the target tab so the 0ms optimistic sent message and focus
    /// stay visible with zero remount flash, and dispatches the send into the rebound runtime.
    pub(crate) fn follow_moved_draft_chat(
        &mut self,
        view: Entity<NativeChatView>,
        send: super::state::PendingDraftSend,
        target_machine_id: String,
        target_project_id: String,
        target_session_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let old_shell_id = send.from_shell_session_id;
        let old_title = self
            .agents_workspace
            .session(old_shell_id)
            .map(|session| session.title.clone());
        let old_icon = self
            .agents_workspace
            .session(old_shell_id)
            .and_then(|session| session.agent_icon);
        // Remove the old draft's tab and surface cleanly from the outgoing workspace BEFORE any
        // project swap so the outgoing project never keeps a stale ghost tab for the moved draft.
        let detached_view = self.native_chat_views.remove(&old_shell_id);
        self.agents_chat_page_states.remove(&old_shell_id);
        self.agents_chat_mode_sessions.remove(&old_shell_id);
        self.agents_chat_notice_admitted_sessions
            .remove(&old_shell_id);
        self.agents_chat_notice_probe_in_flight
            .remove(&old_shell_id);
        self.session_chat_composer_ready_sessions
            .remove(&old_shell_id);
        self.session_chat_composer_empty_reports
            .remove(&old_shell_id);
        if send.from_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            let old_key = GpuiLocalWorkspaceSessionKey {
                project_id: send.from_project_id.clone(),
                session_id: send.from_session_id.clone(),
            };
            self.local_workspace_session_mappings.remove(&old_key);
        } else {
            let old_key = GpuiRemoteAttachSessionKey {
                remote_machine_id: send.from_machine_id.clone(),
                project_id: send.from_project_id.clone(),
                session_id: send.from_session_id.clone(),
            };
            self.remote_attach_sessions.remove(&old_key);
        }
        if let Some(old_pane_id) = self.agents_workspace.pane_id_for_session(old_shell_id) {
            self.agents_workspace.close_tab(old_pane_id, old_shell_id);
        }
        let view = detached_view.unwrap_or(view);

        let is_local = target_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID;
        let sidebar_session_id = if is_local {
            gpui_combined_presentation_session_id(&target_project_id, &target_session_id)
        } else {
            gpui_remote_scoped_session_id(
                &target_machine_id,
                &target_project_id,
                &target_session_id,
            )
        };
        self.gx_store
            .sidebar_ui
            .suppress_next_space_follow_for(&sidebar_session_id);

        if is_local {
            let switched = match self.route_local_project_to_its_window(&target_project_id, cx) {
                crate::app::workspace_windows::ProjectWindowRoute::Here => false,
                crate::app::workspace_windows::ProjectWindowRoute::Switched => true,
                crate::app::workspace_windows::ProjectWindowRoute::Other(other_app) => {
                    let worktree_action = send.pending_worktree_action.clone();
                    let submission = send.submission.clone();
                    let send_request_id = send.send_request_id.clone();
                    let target_project = target_project_id.clone();
                    let target_session = target_session_id.clone();
                    cx.defer(move |cx| {
                        let _ = other_app.update(cx, |app, cx| {
                            app.gx_store
                                .sidebar_ui
                                .suppress_next_space_follow_for(&sidebar_session_id);
                            app.gx_store_focus_created_session(
                                &target_project,
                                &target_session,
                                true,
                                Some("chat"),
                                cx,
                            );
                            let key = GpuiLocalWorkspaceSessionKey {
                                project_id: target_project.clone(),
                                session_id: target_session.clone(),
                            };
                            if let Some(shell_id) =
                                app.local_workspace_session_mappings.get(&key).copied()
                                && let Some(other_view) = app.ensure_native_chat(shell_id, cx)
                            {
                                other_view.update(cx, |chat, cx| {
                                    chat.pending_send = true;
                                    chat.watch_send(
                                        send_request_id.clone(),
                                        submission.clone(),
                                        cx,
                                    );
                                    if let Some(worktree) = worktree_action.clone() {
                                        chat.invoke(worktree, cx);
                                    }
                                    chat.invoke(submission.clone(), cx);
                                });
                            }
                        });
                    });
                    cx.notify();
                    return;
                }
            };
            self.note_opened_session(crate::app::claude_web::OpenedSession::Created, cx);
            let local_key = GpuiLocalWorkspaceSessionKey {
                project_id: target_project_id.clone(),
                session_id: target_session_id.clone(),
            };
            self.gx_store_take_created_session(&local_key, cx);
            let focused_pane = self.agents_workspace.focused_pane;
            let new_shell_id = self
                .local_workspace_session_mappings
                .get(&local_key)
                .copied()
                .filter(|id| self.agents_workspace.pane_id_for_session(*id).is_some())
                .or_else(|| {
                    self.agents_workspace
                        .add_mounting_session_to_pane(focused_pane)
                });
            if let Some(new_shell_id) = new_shell_id {
                if let Some(session) = self
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == new_shell_id)
                {
                    if let Some(title) = old_title {
                        session.title = title;
                    }
                    if let Some(icon) = old_icon {
                        session.agent_icon = Some(icon);
                    }
                    session.set_presentation_state_with_startup_eligibility(
                        TerminalSessionPresentationState::Mounting,
                        false,
                    );
                }
                self.local_workspace_session_mappings
                    .insert(local_key.clone(), new_shell_id);
                self.agents_chat_mode_sessions.insert(new_shell_id);
                let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(local_key.clone());
                let config = NativeChatConfig {
                    machine_id: target_machine_id,
                    project_id: target_project_id.clone(),
                    session_id: target_session_id.clone(),
                    sidebar_session_id,
                    shell_session_id: new_shell_id,
                    app: Some(cx.weak_entity()),
                    parent_native_view: self.parent_ns_view,
                    client_id: format!("native-desktop-{}", std::process::id()),
                    remote: None,
                    initial_snapshot: None,
                    initial_presentation: self
                        .initial_session_chat_presentation(Some(&workspace_key)),
                };
                view.update(cx, |chat, cx| {
                    chat.rebind_moved_session(
                        config,
                        send.pending_worktree_action,
                        Some(send.submission),
                        cx,
                    );
                });
                self.register_native_chat_view(new_shell_id, view, Some(workspace_key), cx);
                if let Some(pane_id) = self.agents_workspace.pane_id_for_session(new_shell_id) {
                    self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
                    self.scroll_workspace_pane_active_tab(pane_id);
                }
                self.reconcile_agents_pane_surfaces(cx);
                self.update_active_mode_cef_child_visibility(cx);
                let focus_msg = GpuiSidebarWorkspaceTerminalFocusMessage {
                    force_remount: false,
                    placement_target_session_id: None,
                    preferred_interface: GpuiPreferredAgentInterface::Chat,
                    project_id: target_project_id,
                    session_id: target_session_id,
                    startup_restore: false,
                    keep_view: true,
                    wake_sleeping: false,
                    keep_sleeping: false,
                };
                self.focus_local_workspace_terminal_from_message(&focus_msg, cx);
                if switched {
                    self.settle_routed_window_workspace(cx);
                }
                self.persist_shell_layout_state();
                cx.notify();
                return;
            }
            self.gx_store_focus_created_session(
                &target_project_id,
                &target_session_id,
                true,
                Some("chat"),
                cx,
            );
            if switched {
                self.settle_routed_window_workspace(cx);
            }
            return;
        }

        // Remote target computer: rebind the live view to the remote session and open it in place.
        let remote_target = self.gpui_remote_gxserver_request_target(&target_machine_id);
        let remote_key = GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey {
            remote_machine_id: target_machine_id.clone(),
            project_id: target_project_id.clone(),
            session_id: target_session_id.clone(),
        });
        let focused_pane = self.agents_workspace.focused_pane;
        let new_shell_id = self
            .agents_workspace
            .add_mounting_session_to_pane(focused_pane);
        if let Some(new_shell_id) = new_shell_id {
            if let Some(session) = self
                .agents_workspace
                .terminal_sessions
                .iter_mut()
                .find(|session| session.id == new_shell_id)
            {
                if let Some(title) = old_title {
                    session.title = title;
                }
                if let Some(icon) = old_icon {
                    session.agent_icon = Some(icon);
                }
            }
            self.agents_chat_mode_sessions.insert(new_shell_id);
            let config = NativeChatConfig {
                machine_id: target_machine_id.clone(),
                project_id: target_project_id.clone(),
                session_id: target_session_id.clone(),
                sidebar_session_id: sidebar_session_id.clone(),
                shell_session_id: new_shell_id,
                app: Some(cx.weak_entity()),
                parent_native_view: self.parent_ns_view,
                client_id: format!("native-desktop-{}", std::process::id()),
                remote: remote_target,
                initial_snapshot: None,
                initial_presentation: self.initial_session_chat_presentation(Some(&remote_key)),
            };
            view.update(cx, |chat, cx| {
                chat.rebind_moved_session(
                    config,
                    send.pending_worktree_action,
                    Some(send.submission),
                    cx,
                );
            });
            self.register_native_chat_view(new_shell_id, view, Some(remote_key), cx);
            if let Some(pane_id) = self.agents_workspace.pane_id_for_session(new_shell_id) {
                self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
                self.scroll_workspace_pane_active_tab(pane_id);
            }
            self.reconcile_agents_pane_surfaces(cx);
        }
        let session = ghostex_gx_core::SessionKey::remote(
            target_machine_id.as_str(),
            target_project_id.as_str(),
            target_session_id.as_str(),
        );
        self.gx_store_select_opened_session(&session, cx);
        let payload = ghostex_gx_core::open_remote_session_terminal(
            &session.to_sidebar_session_id(),
            true,
            Some("chat"),
        );
        self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
        cx.notify();
    }

    pub(crate) fn native_chat_for_generation(
        &self,
        generation: u64,
    ) -> Option<Entity<NativeChatView>> {
        self.agents_chat_page_states
            .iter()
            .find_map(|(id, state)| {
                (state.generation == generation)
                    .then(|| self.native_chat_views.get(id).cloned())
                    .flatten()
            })
            .or_else(|| {
                self.parked_agents_chat_runtimes_by_project
                    .values()
                    .find_map(|parked| {
                        parked.page_states.iter().find_map(|(id, state)| {
                            (state.generation == generation)
                                .then(|| parked.native_views.get(id).cloned())
                                .flatten()
                        })
                    })
            })
    }
}
