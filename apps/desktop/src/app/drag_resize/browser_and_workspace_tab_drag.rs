//! Browser tab and workspace (agents) tab drag feedback and drops, including moving a live command tab into the agents workspace.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn begin_browser_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.browser_tab_drag_active {
            return;
        }

        self.browser_tab_drag_active = true;
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn set_browser_tab_drop_feedback(
        &mut self,
        feedback: Option<BrowserDropFeedback>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_tab_drop_feedback != feedback {
            self.browser_tab_drop_feedback = feedback;
            cx.notify();
        }
    }

    pub(crate) fn clear_browser_drop_feedback(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_browser_tab_drop_feedback(None, cx);
    }

    pub(crate) fn finish_browser_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        let changed = self.browser_tab_drag_active || self.browser_tab_drop_feedback.is_some();
        if !changed {
            return;
        }

        self.browser_tab_drag_active = false;
        self.browser_tab_drop_feedback = None;
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn update_browser_pane_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedBrowserTab>,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_browser_tab_drag(cx);

        if !event.bounds.contains(&event.event.position) {
            if self.browser_tab_drop_feedback.is_some_and(|feedback| {
                feedback.pane_id == pane_id
                    && matches!(feedback.target, BrowserTabDropTarget::PaneBody(_))
            }) {
                self.clear_browser_drop_feedback(cx);
            }
            return;
        }

        let dragged = event.drag(cx);
        let mut zone = workspace_pane_body_drop_zone(event.bounds, event.event.position);
        if dragged.source_pane_id == pane_id
            && !matches!(zone, WorkspaceDropZone::Center)
            && self
                .browser_tabs
                .pane_tab_count(pane_id)
                .unwrap_or_default()
                <= 1
        {
            zone = WorkspaceDropZone::Center;
        }

        self.set_browser_tab_drop_feedback(
            Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::PaneBody(zone),
            }),
            cx,
        );
    }

    pub(crate) fn handle_browser_pane_body_drop(
        &mut self,
        target_pane_id: BrowserPaneId,
        dragged: &DraggedBrowserTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let zone = match self.browser_tab_drop_feedback {
            Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::PaneBody(zone),
            }) if pane_id == target_pane_id => zone,
            _ => WorkspaceDropZone::Center,
        };
        self.browser_tab_drag_active = false;
        self.browser_tab_drop_feedback = None;

        let changed = match zone {
            WorkspaceDropZone::Center => self.browser_tabs.group_tab_into_pane(
                dragged.source_pane_id,
                target_pane_id,
                dragged.tab_id,
            ),
            WorkspaceDropZone::Left
            | WorkspaceDropZone::Right
            | WorkspaceDropZone::Top
            | WorkspaceDropZone::Bottom => self.browser_tabs.split_tab_to_pane(
                dragged.source_pane_id,
                target_pane_id,
                dragged.tab_id,
                zone,
            ),
        };

        if changed {
            self.reconcile_browser_address_inputs();
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.focus_shell_target(
                ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                cx,
            );
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_focused_browser_pane_active_tab();
            self.persist_shell_layout_state();
        }
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn set_workspace_drop_feedback(
        &mut self,
        feedback: Option<WorkspaceDropFeedback>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.workspace_drop_feedback != feedback {
            self.workspace_drop_feedback = feedback;
            cx.notify();
        }
    }

    pub(crate) fn clear_workspace_drop_feedback(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_workspace_drop_feedback(None, cx);
    }

    pub(crate) fn begin_workspace_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:Workarea 2026-07-03:
        Workspace tab drag begin/finish only flips the runtime drag flag and lets the existing gated sync passes do the hiding: CEF child views re-evaluate through the shared allows-cef-child-views gate, and mounted Agents/command terminals hide-and-park on the next render-driven host sync. No native view is created, destroyed, or overlaid here; drop and cancel restore through the same parked-owner reattach machinery.
        */
        if self.workspace_tab_drag_active {
            return;
        }

        self.workspace_tab_drag_active = true;
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn finish_workspace_tab_drag_state(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.clear_composer_drop_hover(cx);
        let drag_was_active = self.workspace_tab_drag_active;
        let changed = drag_was_active || self.workspace_drop_feedback.is_some();
        if !changed {
            return false;
        }

        self.workspace_tab_drag_active = false;
        self.workspace_drop_feedback = None;
        if drag_was_active {
            self.update_active_mode_cef_child_visibility(cx);
        }
        true
    }

    pub(crate) fn finish_workspace_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.finish_workspace_tab_drag_state(cx) {
            cx.notify();
        }
    }

    pub(crate) fn update_workspace_pane_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            if self.workspace_drop_feedback.is_some_and(|feedback| {
                feedback.pane_id == pane_id
                    && matches!(feedback.target, WorkspaceDropTarget::PaneBody(_))
            }) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        let dragged = event.drag(cx);
        let zone = workspace_pane_body_drop_zone(event.bounds, event.event.position);
        // A pane dragged by its grip lands on another pane only (session_pane_placement.rs).
        if dragged.source_pane_id == pane_id {
            if self.workspace_drop_feedback.is_some_and(|feedback| {
                feedback.pane_id == pane_id
                    && matches!(feedback.target, WorkspaceDropTarget::PaneBody(_))
            }) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        self.set_workspace_drop_feedback(
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }),
            cx,
        );
    }

    pub(crate) fn update_command_tab_over_workspace_pane_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_command_tab_drag(cx);

        if !event.bounds.contains(&event.event.position) {
            if self.workspace_drop_feedback.is_some_and(|feedback| {
                feedback.pane_id == pane_id
                    && matches!(feedback.target, WorkspaceDropTarget::PaneBody(_))
            }) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        let zone = workspace_pane_body_drop_zone(event.bounds, event.event.position);
        self.set_workspace_drop_feedback(
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }),
            cx,
        );
    }

    pub(crate) fn handle_workspace_pane_body_drop(
        &mut self,
        target_pane_id: WorkspacePaneId,
        dragged: &DraggedWorkspaceTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let feedback_zone = match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }) if pane_id == target_pane_id => Some(zone),
            _ => None,
        };
        self.finish_workspace_tab_drag_state(cx);
        // No zone is shown over the dragged pane itself, so a release there moves nothing.
        let Some(zone) = feedback_zone else {
            cx.notify();
            return;
        };

        let changed = match zone {
            WorkspaceDropZone::Center => self.agents_workspace.group_tab_into_pane(
                dragged.source_pane_id,
                target_pane_id,
                dragged.session_id,
            ),
            WorkspaceDropZone::Left
            | WorkspaceDropZone::Right
            | WorkspaceDropZone::Top
            | WorkspaceDropZone::Bottom => self.agents_workspace.split_tab_to_pane(
                dragged.source_pane_id,
                target_pane_id,
                dragged.session_id,
                zone,
            ),
        };

        if changed {
            self.close_pane_left_by_dragged_session(dragged.source_pane_id, target_pane_id);
            /*
            A pane-body drop makes the dragged tab active in the destination,
            so complete the interaction through the same activation path as a
            tab click. This is what reports sleeping or runtime-missing mapped
            sessions to the sidebar for gxserver wake/reattach reconciliation.
            */
            self.select_agents_tab(self.agents_workspace.focused_pane, dragged.session_id, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn transfer_live_command_tab_to_agents(
        &mut self,
        source_group_id: CommandPaneGroupId,
        source_session_id: CommandSessionId,
        target_pane_id: WorkspacePaneId,
        placement: CommandToAgentsDropPlacement,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        /*
        CDXC:Workarea 2026-08-01:
        Dragging a command tab into the Agents workspace moves the running
        terminal instead of re-creating it. The previous placeholder boundary
        carried only the title, so the dragged tab became a fresh shell while
        the real process was abandoned and its daemon session reaped moments
        later by the command-model pruner.

        The move is ordered deliberately:
        1. read the daemon identity while the command row still exists;
        2. take the engine record AND the gxserver mapping out of command
           ownership BEFORE the model close, so the pruner never observes a
           mapping whose session has disappeared and closes the very session
           being moved;
        3. insert the Agents tab as Running and startup-ineligible, so the
           startup pipeline cannot spawn a second shell into it;
        4. re-subscribe the view against the Agents event target and adopt an
           Agents runtime id, because the engine record is retained only while
           its runtime id matches the Agents registry;
        5. map the session so the sidebar shows it and promote the daemon row
           to the workspace surface.

        Every step is synchronous: the frame that follows this call already
        reconciles both surfaces.
        */
        let gxserver_key = self.command_gxserver_session_key_for_command_tab(source_session_id);
        let zmx_session_name = self
            .command_pane
            .session(source_session_id)
            .and_then(|session| session.zmx_session_name.clone());
        let record = self
            .command_gpui_engine_terminals
            .remove(&source_session_id);
        let mapping = self
            .command_gxserver_session_mappings
            .remove(&source_session_id);
        let remote_reference = self
            .command_remote_action_sessions
            .remove(&source_session_id);

        let transferred = match placement {
            CommandToAgentsDropPlacement::PaneBody(zone) => {
                transfer_command_placeholder_to_workspace(
                    &mut self.agents_workspace,
                    &mut self.command_pane,
                    source_group_id,
                    source_session_id,
                    target_pane_id,
                    zone,
                )
            }
        };

        let Some((inserted_pane_id, inserted_session_id)) = transferred else {
            // The model transfer already rolled itself back, so restore the
            // ownership we removed and leave the command tab exactly as it was.
            if let Some(record) = record {
                self.command_gpui_engine_terminals
                    .insert(source_session_id, record);
            }
            if let Some(mapping) = mapping {
                self.command_gxserver_session_mappings
                    .insert(source_session_id, mapping);
            }
            if let Some(reference) = remote_reference {
                self.command_remote_action_sessions
                    .insert(source_session_id, reference);
            }
            return None;
        };

        let agents_runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(inserted_session_id);
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == inserted_session_id)
        {
            session.zmx_session_name = zmx_session_name;
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Running,
                false,
            );
        }

        if let Some(mut recipe) = self
            .command_gpui_terminal_viewer_recipes
            .remove(&source_session_id)
        {
            recipe.runtime_session_id = agents_runtime_session_id;
            self.agents_gpui_terminal_viewer_recipes
                .insert(inserted_session_id, recipe);
        }
        if let Some(record) = record {
            // Destructure rather than drop: `view` moves out intact, so the
            // child process is never released, while the old subscription
            // (bound to the command event target) dies with the binding.
            let terminal_gpui_engine::GpuiEngineTerminalRecord {
                view,
                runtime_session_id: previous_runtime_session_id,
                wait_after_command,
                confirm_close_behavior,
                _subscription,
                viewer_leases,
            } = record;
            drop(_subscription);
            if let Some(osc_state) = self
                .command_terminal_runtime_osc_states
                .remove(&previous_runtime_session_id)
            {
                self.agents_terminal_runtime_osc_states
                    .insert(agents_runtime_session_id, osc_state);
            }
            let view_id = view.entity_id();
            let subscription = cx.subscribe(
                &view,
                move |this: &mut Self, _view, event: &terminal_element::TerminalViewEvent, cx| {
                    if this
                        .agents_gpui_engine_terminals
                        .get(&inserted_session_id)
                        .is_none_or(|record| record.view.entity_id() != view_id)
                    {
                        return;
                    }
                    this.handle_gpui_engine_terminal_view_event(
                        GpuiEngineTerminalEventTarget::Agents(inserted_session_id),
                        event,
                        cx,
                    );
                },
            );
            self.agents_gpui_engine_terminals.insert(
                inserted_session_id,
                terminal_gpui_engine::GpuiEngineTerminalRecord {
                    view,
                    runtime_session_id: agents_runtime_session_id,
                    wait_after_command,
                    confirm_close_behavior,
                    _subscription: subscription,
                    viewer_leases,
                },
            );
        }

        if let Some(key) = gxserver_key {
            self.local_workspace_session_mappings
                .insert(key.clone(), inserted_session_id);
            self.agents_sessions_pending_surface_transfer
                .insert(inserted_session_id);
            self.promote_transferred_gxserver_session_surface_in_background(
                key,
                inserted_session_id,
                0,
                cx,
            );
        }
        if let Some(reference) = remote_reference {
            self.adopt_transferred_remote_command_action_session(
                source_session_id,
                reference,
                inserted_session_id,
                cx,
            );
        }

        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        Some((inserted_pane_id, inserted_session_id))
    }

    pub(crate) fn handle_command_tab_workspace_pane_body_drop(
        &mut self,
        target_pane_id: WorkspacePaneId,
        dragged: &DraggedCommandTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-06-22-15:55:
        Dropping a command tab on an Agents pane body switches to Agents and focuses the inserted placeholder pane so the result is immediately visible. The source command session is removed only after the Agents placeholder insert succeeds; the transfer carries only the visible title and never captures command text, stdout/stderr, terminal content, process state, libghostty state, CEF/code bridges, overlays, or hidden hit routing.
        */
        window.prevent_default();
        cx.stop_propagation();

        let zone = match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }) if pane_id == target_pane_id => zone,
            _ => WorkspaceDropZone::Center,
        };
        self.workspace_drop_feedback = None;
        self.finish_command_tab_drag_state(cx);

        let Some((inserted_pane_id, _inserted_session_id)) = self
            .transfer_live_command_tab_to_agents(
                dragged.source_group_id,
                dragged.session_id,
                target_pane_id,
                CommandToAgentsDropPlacement::PaneBody(zone),
                cx,
            )
        else {
            cx.notify();
            return;
        };

        self.change_active_mode_with_pane_state(TitlebarMode::Agents, cx);
        self.focus_shell_target(
            ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
            cx,
        );
        self.update_active_mode_cef_child_visibility(cx);
        self.scroll_workspace_pane_active_tab(inserted_pane_id);
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
    }
}
