// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: tab/pane drag-and-drop feedback, split handles, and divider resize drags

use std::collections::HashSet;
use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::Bounds;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::Pixels;
use gpui::Window;

use crate::app::consts::*;
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

    pub(crate) fn set_browser_tab_hovered(
        &mut self,
        tab: BrowserHoverTab,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIBrowserTabs 2026-07-08:
        Browser tab close controls are hover-only chrome derived from runtime pointer state. Track the hovered Browser tab separately from BrowserTabModel so hover can reveal the real inline close button without changing tab selection, focus, persistence, drag/drop, or favicon status rendering.
        */
        if hovered {
            if self.hovered_browser_tab != Some(tab) {
                self.hovered_browser_tab = Some(tab);
                cx.notify();
            }
            return;
        }

        if self.hovered_browser_tab == Some(tab) {
            self.hovered_browser_tab = None;
            cx.notify();
        }
    }

    pub(crate) fn update_browser_tab_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedBrowserTab>,
        pane_id: BrowserPaneId,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_browser_tab_drag(cx);

        let insertion_index =
            workspace_tab_insertion_index(event.bounds, event.event.position, tab_index);
        let feedback = BrowserDropFeedback {
            pane_id,
            target: BrowserTabDropTarget::TabStrip(insertion_index),
        };

        if event.drag(cx).source_pane_id != pane_id || !event.bounds.contains(&event.event.position)
        {
            if self.browser_tab_drop_feedback == Some(feedback) {
                self.clear_browser_drop_feedback(cx);
            }
            return;
        }

        self.set_browser_tab_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn update_browser_tab_end_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedBrowserTab>,
        pane_id: BrowserPaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_browser_tab_drag(cx);

        let feedback = BrowserDropFeedback {
            pane_id,
            target: BrowserTabDropTarget::TabStrip(insertion_index),
        };

        if event.drag(cx).source_pane_id != pane_id || !event.bounds.contains(&event.event.position)
        {
            if self.browser_tab_drop_feedback == Some(feedback) {
                self.clear_browser_drop_feedback(cx);
            }
            return;
        }

        self.set_browser_tab_drop_feedback(Some(feedback), cx);
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

    pub(crate) fn handle_browser_tab_strip_drop(
        &mut self,
        pane_id: BrowserPaneId,
        default_insertion_index: usize,
        dragged: &DraggedBrowserTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let insertion_index = match self.browser_tab_drop_feedback {
            Some(BrowserDropFeedback {
                pane_id: feedback_pane_id,
                target: BrowserTabDropTarget::TabStrip(insertion_index),
            }) if feedback_pane_id == pane_id => insertion_index,
            _ => default_insertion_index,
        };
        self.browser_tab_drag_active = false;
        self.browser_tab_drop_feedback = None;

        if dragged.source_pane_id == pane_id
            && self
                .browser_tabs
                .reorder_tab_within_pane(pane_id, dragged.tab_id, insertion_index)
        {
            self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
            self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_browser_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
        }
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
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
            self.set_shell_focus(ShellFocusTarget::BrowserPane(
                self.browser_tabs.focused_pane,
            ));
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

    pub(crate) fn begin_pending_workspace_tab_click(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) {
        self.pending_workspace_tab_click = Some(WorkspacePendingTabClick {
            pane_id,
            session_id,
        });
    }

    pub(crate) fn cancel_pending_workspace_tab_click(&mut self) {
        self.pending_workspace_tab_click = None;
    }

    pub(crate) fn cancel_pending_workspace_tab_click_for_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) {
        let target = WorkspacePendingTabClick {
            pane_id,
            session_id,
        };
        self.pending_workspace_tab_click = workspace_tab_pending_click_after_mouse_up_out(
            self.pending_workspace_tab_click,
            target,
        );
    }

    pub(crate) fn handle_workspace_tab_left_mouse_up(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        click_count: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = WorkspacePendingTabClick {
            pane_id,
            session_id,
        };
        let pending_click = self.pending_workspace_tab_click.take();
        if workspace_tab_left_mouse_up_focuses(
            click_count,
            pending_click,
            target,
            self.workspace_tab_drag_active,
        ) {
            self.double_click_agents_workspace_tab(pane_id, session_id, cx);
            return;
        }
        if workspace_tab_left_mouse_up_selects(
            pending_click,
            target,
            self.workspace_tab_drag_active,
        ) {
            self.select_agents_tab(pane_id, session_id, cx);
            cx.notify();
        }
    }

    pub(crate) fn begin_workspace_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUIWorkspaceTabDragVisibility 2026-07-03:
        Workspace tab drag begin/finish only flips the runtime drag flag and lets the existing gated sync passes do the hiding: CEF child views re-evaluate through the shared allows-cef-child-views gate, and mounted Agents/command terminals hide-and-park on the next render-driven host sync. No native view is created, destroyed, or overlaid here; drop and cancel restore through the same parked-owner reattach machinery.
        */
        self.pending_workspace_tab_click = None;
        if self.workspace_tab_drag_active {
            return;
        }

        self.workspace_tab_drag_active = true;
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn finish_workspace_tab_drag_state(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.pending_workspace_tab_click = None;
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

    pub(crate) fn update_workspace_tab_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        pane_id: WorkspacePaneId,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index =
            workspace_tab_insertion_index(event.bounds, event.event.position, tab_index);
        let feedback = WorkspaceDropFeedback {
            pane_id,
            target: WorkspaceDropTarget::TabStrip(insertion_index),
        };

        if event.drag(cx).source_pane_id != pane_id || !event.bounds.contains(&event.event.position)
        {
            if self.workspace_drop_feedback == Some(feedback) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        self.set_workspace_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn update_workspace_tab_end_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        pane_id: WorkspacePaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let feedback = WorkspaceDropFeedback {
            pane_id,
            target: WorkspaceDropTarget::TabStrip(insertion_index),
        };

        if event.drag(cx).source_pane_id != pane_id || !event.bounds.contains(&event.event.position)
        {
            if self.workspace_drop_feedback == Some(feedback) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        self.set_workspace_drop_feedback(Some(feedback), cx);
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
        if self
            .agents_workspace
            .workspace_tab_edge_drop_is_single_tab_own_pane_noop(
                dragged.source_pane_id,
                pane_id,
                zone,
            )
        {
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

    pub(crate) fn update_command_tab_over_workspace_tab_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        pane_id: WorkspacePaneId,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index =
            workspace_tab_insertion_index(event.bounds, event.event.position, tab_index);
        self.update_command_tab_over_workspace_tab_strip_feedback(
            event,
            pane_id,
            insertion_index,
            cx,
        );
    }

    pub(crate) fn update_command_tab_over_workspace_tab_end_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        pane_id: WorkspacePaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_command_tab_over_workspace_tab_strip_feedback(
            event,
            pane_id,
            insertion_index,
            cx,
        );
    }

    pub(crate) fn update_command_tab_over_workspace_tab_strip_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        pane_id: WorkspacePaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_command_tab_drag(cx);

        let feedback = WorkspaceDropFeedback {
            pane_id,
            target: WorkspaceDropTarget::TabStrip(insertion_index),
        };
        let dragged = event.drag(cx);
        let source_has_session = self
            .command_pane
            .find_leaf(dragged.source_group_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(dragged.session_id));

        if !source_has_session || !event.bounds.contains(&event.event.position) {
            if self.workspace_drop_feedback == Some(feedback) {
                self.clear_workspace_drop_feedback(cx);
            }
            return;
        }

        self.set_workspace_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn handle_workspace_tab_strip_drop(
        &mut self,
        pane_id: WorkspacePaneId,
        default_insertion_index: usize,
        dragged: &DraggedWorkspaceTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let insertion_index = match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id: feedback_pane_id,
                target: WorkspaceDropTarget::TabStrip(insertion_index),
            }) if feedback_pane_id == pane_id => insertion_index,
            _ => default_insertion_index,
        };
        self.finish_workspace_tab_drag_state(cx);

        if dragged.source_pane_id == pane_id
            && self.agents_workspace.reorder_tab_within_pane(
                pane_id,
                dragged.session_id,
                insertion_index,
            )
        {
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
            self.scroll_workspace_pane_active_tab(pane_id);
            self.persist_shell_layout_state();
            cx.notify();
        } else {
            cx.notify();
        }
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
        let zone = if let Some(zone) = feedback_zone {
            zone
        } else {
            if self
                .agents_workspace
                .workspace_tab_body_drop_is_single_tab_own_pane_noop(
                    dragged.source_pane_id,
                    target_pane_id,
                )
            {
                cx.notify();
                return;
            }
            WorkspaceDropZone::Center
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
        CDXC:GPUICommandWorkspaceTransfer 2026-08-01:
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
            CommandToAgentsDropPlacement::TabStrip(insertion_index) => {
                transfer_command_placeholder_to_workspace_tab_strip(
                    &mut self.agents_workspace,
                    &mut self.command_pane,
                    source_group_id,
                    source_session_id,
                    target_pane_id,
                    insertion_index,
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
            } = record;
            drop(_subscription);
            if let Some(osc_state) = self
                .command_terminal_runtime_osc_states
                .remove(&previous_runtime_session_id)
            {
                self.agents_terminal_runtime_osc_states
                    .insert(agents_runtime_session_id, osc_state);
            }
            let subscription = cx.subscribe(
                &view,
                move |this: &mut Self, _view, event: &terminal_element::TerminalViewEvent, cx| {
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
                },
            );
        }

        if let Some(key) = gxserver_key {
            self.local_workspace_session_mappings
                .insert(key.clone(), inserted_session_id);
            self.local_app_shot_session_mappings
                .insert(key.session_id.clone(), inserted_session_id);
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
        CDXC:GPUICommandWorkspaceTransfer 2026-06-22-15:55:
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

        self.active_mode = TitlebarMode::Agents;
        self.set_shell_focus(ShellFocusTarget::AgentsPane(
            self.agents_workspace.focused_pane,
        ));
        self.update_active_mode_cef_child_visibility(cx);
        self.scroll_workspace_pane_active_tab(inserted_pane_id);
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
    }

    pub(crate) fn handle_command_tab_workspace_tab_strip_drop(
        &mut self,
        target_pane_id: WorkspacePaneId,
        default_insertion_index: usize,
        dragged: &DraggedCommandTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandWorkspaceTransfer 2026-06-22-16:04:
        Dropping a command tab on an Agents tab strip uses the visible tab boundary/end marker as the exact insertion point, switches to Agents, and focuses the target Agents pane so the selected placeholder is visible. This path must not reorder existing Agents tabs for command drags and must not transfer command text, stdout/stderr, terminal content, real process state, libghostty state, Source/Kanban/Automate/Manage surfaces, overlays, hidden hit regions, or native/root hit-test routing.
        */
        window.prevent_default();
        cx.stop_propagation();

        let insertion_index = match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::TabStrip(insertion_index),
            }) if pane_id == target_pane_id => insertion_index,
            _ => default_insertion_index,
        };
        self.workspace_drop_feedback = None;
        self.finish_command_tab_drag_state(cx);

        let Some((inserted_pane_id, _inserted_session_id)) = self
            .transfer_live_command_tab_to_agents(
                dragged.source_group_id,
                dragged.session_id,
                target_pane_id,
                CommandToAgentsDropPlacement::TabStrip(insertion_index),
                cx,
            )
        else {
            cx.notify();
            return;
        };

        self.active_mode = TitlebarMode::Agents;
        self.set_shell_focus(ShellFocusTarget::AgentsPane(inserted_pane_id));
        self.update_active_mode_cef_child_visibility(cx);
        self.scroll_workspace_pane_active_tab(inserted_pane_id);
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
    }

    pub(crate) fn set_command_drop_feedback(
        &mut self,
        feedback: Option<CommandPaneDropFeedback>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_drop_feedback != feedback {
            self.command_drop_feedback = feedback;
            cx.notify();
        }
    }

    pub(crate) fn clear_command_drop_feedback(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_command_drop_feedback(None, cx);
    }

    pub(crate) fn clear_command_tab_strip_feedback_for_group(
        &mut self,
        group_id: CommandPaneGroupId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_drop_feedback.is_some_and(|feedback| {
            feedback.group_id == group_id
                && matches!(feedback.target, CommandPaneDropTarget::TabStrip(_))
        }) {
            self.clear_command_drop_feedback(cx);
        }
    }

    pub(crate) fn begin_pending_command_tab_click(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        expand_on_click: bool,
    ) {
        self.pending_command_tab_click = Some(CommandPanePendingTabClick {
            group_id,
            session_id,
            expand_on_click,
        });
    }

    pub(crate) fn cancel_pending_command_tab_click(&mut self) {
        self.pending_command_tab_click = None;
    }

    pub(crate) fn cancel_pending_command_tab_click_for_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        expand_on_click: bool,
    ) {
        let target = CommandPanePendingTabClick {
            group_id,
            session_id,
            expand_on_click,
        };
        self.pending_command_tab_click = command_pane_tab_pending_click_after_mouse_up_out(
            self.pending_command_tab_click,
            target,
        );
    }

    pub(crate) fn handle_command_pane_tab_left_mouse_up(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        expand_on_click: bool,
        click_count: usize,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = CommandPanePendingTabClick {
            group_id,
            session_id,
            expand_on_click,
        };
        let pending_click = self.pending_command_tab_click.take();
        if command_pane_tab_left_mouse_up_focuses(
            click_count,
            pending_click,
            target,
            self.command_tab_drag_active,
            &self.command_pane,
        ) {
            self.toggle_command_pane_focus_mode_for_tab(group_id, session_id, cx);
            return;
        }
        if command_pane_tab_left_mouse_up_selects(
            pending_click,
            target,
            self.command_tab_drag_active,
        ) {
            self.select_command_pane_tab(group_id, session_id, expand_on_click, window, cx);
        }
    }

    pub(crate) fn begin_command_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        self.pending_command_tab_click = None;
        if self.command_tab_drag_active {
            return;
        }

        self.command_tab_drag_active = true;
        self.update_active_mode_cef_child_visibility(cx);
        cx.notify();
    }

    pub(crate) fn finish_command_tab_drag_state(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.pending_command_tab_click = None;
        let drag_was_active = self.command_tab_drag_active;
        let changed = drag_was_active || self.command_drop_feedback.is_some();
        if !changed {
            return false;
        }

        self.command_tab_drag_active = false;
        self.command_drop_feedback = None;
        if drag_was_active {
            self.update_active_mode_cef_child_visibility(cx);
        }
        true
    }

    pub(crate) fn finish_command_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.finish_command_tab_drag_state(cx) {
            cx.notify();
        }
    }

    pub(crate) fn update_command_tab_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        group_id: CommandPaneGroupId,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_command_tab_drag(cx);

        let insertion_index =
            workspace_tab_insertion_index(event.bounds, event.event.position, tab_index);
        let feedback = CommandPaneDropFeedback {
            group_id,
            target: CommandPaneDropTarget::TabStrip(insertion_index),
        };
        let dragged = event.drag(cx);

        if dragged.source_group_id != group_id || !event.bounds.contains(&event.event.position) {
            if self.command_drop_feedback == Some(feedback) {
                self.clear_command_drop_feedback(cx);
            }
            return;
        }

        if !self.command_pane.tab_strip_reorder_changes_order(
            group_id,
            dragged.session_id,
            insertion_index,
        ) {
            self.clear_command_tab_strip_feedback_for_group(group_id, cx);
            return;
        }

        self.set_command_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn update_command_tab_end_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        group_id: CommandPaneGroupId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_command_tab_drag(cx);

        let feedback = CommandPaneDropFeedback {
            group_id,
            target: CommandPaneDropTarget::TabStrip(insertion_index),
        };
        let dragged = event.drag(cx);

        if dragged.source_group_id != group_id || !event.bounds.contains(&event.event.position) {
            if self.command_drop_feedback == Some(feedback) {
                self.clear_command_drop_feedback(cx);
            }
            return;
        }

        if !self.command_pane.tab_strip_reorder_changes_order(
            group_id,
            dragged.session_id,
            insertion_index,
        ) {
            self.clear_command_tab_strip_feedback_for_group(group_id, cx);
            return;
        }

        self.set_command_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn update_command_pane_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedCommandTab>,
        group_id: CommandPaneGroupId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.begin_command_tab_drag(cx);

        if !event.bounds.contains(&event.event.position) {
            if self.command_drop_feedback.is_some_and(|feedback| {
                feedback.group_id == group_id
                    && matches!(feedback.target, CommandPaneDropTarget::PaneBody(_))
            }) {
                self.clear_command_drop_feedback(cx);
            }
            return;
        }

        let dragged = event.drag(cx);
        let mut zone = command_pane_body_drop_zone(event.bounds, event.event.position);
        if dragged.source_group_id == group_id
            && !matches!(zone, WorkspaceDropZone::Center)
            && self
                .command_pane
                .pane_tab_count(group_id)
                .unwrap_or_default()
                <= 1
        {
            zone = WorkspaceDropZone::Center;
        }

        self.set_command_drop_feedback(
            Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::PaneBody(zone),
            }),
            cx,
        );
    }

    pub(crate) fn update_workspace_tab_over_command_pane_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        group_id: CommandPaneGroupId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            if self.command_drop_feedback.is_some_and(|feedback| {
                feedback.group_id == group_id
                    && matches!(feedback.target, CommandPaneDropTarget::PaneBody(_))
            }) {
                self.clear_command_drop_feedback(cx);
            }
            return;
        }

        let zone = command_pane_body_drop_zone(event.bounds, event.event.position);
        self.set_command_drop_feedback(
            Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::PaneBody(zone),
            }),
            cx,
        );
    }

    pub(crate) fn update_workspace_tab_over_command_tab_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        group_id: CommandPaneGroupId,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index =
            workspace_tab_insertion_index(event.bounds, event.event.position, tab_index);
        self.update_workspace_tab_over_command_tab_strip_feedback(
            event,
            group_id,
            insertion_index,
            cx,
        );
    }

    pub(crate) fn update_workspace_tab_over_command_tab_end_drag_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        group_id: CommandPaneGroupId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_workspace_tab_over_command_tab_strip_feedback(
            event,
            group_id,
            insertion_index,
            cx,
        );
    }

    pub(crate) fn update_workspace_tab_over_command_tab_strip_feedback(
        &mut self,
        event: &gpui::DragMoveEvent<DraggedWorkspaceTab>,
        group_id: CommandPaneGroupId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let feedback = CommandPaneDropFeedback {
            group_id,
            target: CommandPaneDropTarget::TabStrip(insertion_index),
        };
        let dragged = event.drag(cx);
        let source_can_close = self
            .agents_workspace
            .can_transfer_tab_to_command_pane(dragged.source_pane_id, dragged.session_id);
        let target_exists = self.command_pane.find_leaf(group_id).is_some();

        if !source_can_close || !target_exists || !event.bounds.contains(&event.event.position) {
            if self.command_drop_feedback == Some(feedback) {
                self.clear_command_drop_feedback(cx);
            }
            return;
        }

        self.set_command_drop_feedback(Some(feedback), cx);
    }

    pub(crate) fn handle_command_tab_strip_drop(
        &mut self,
        group_id: CommandPaneGroupId,
        default_insertion_index: usize,
        dragged: &DraggedCommandTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let insertion_index = match self.command_drop_feedback {
            Some(CommandPaneDropFeedback {
                group_id: feedback_group_id,
                target: CommandPaneDropTarget::TabStrip(insertion_index),
            }) if feedback_group_id == group_id => insertion_index,
            _ => default_insertion_index,
        };
        let mut changed = self.finish_command_tab_drag_state(cx);

        if dragged.source_group_id == group_id
            && self.command_pane.reorder_tab_within_group(
                group_id,
                dragged.session_id,
                insertion_index,
            )
        {
            self.scroll_command_group_active_tab(group_id);
            self.persist_shell_layout_state();
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn handle_workspace_tab_command_tab_strip_drop(
        &mut self,
        target_group_id: CommandPaneGroupId,
        default_insertion_index: usize,
        dragged: &DraggedWorkspaceTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-22-16:18:
        Dropping an Agents tab on a command-pane tab strip uses the active command insertion marker as the exact grouping index, expands/focuses the command pane, and scrolls both the source Agents strip and inserted command strip. It is not a split/reorder path and transfers only the visible Agents title into a command placeholder.
        */
        window.prevent_default();
        cx.stop_propagation();

        let insertion_index = match self.command_drop_feedback {
            Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::TabStrip(insertion_index),
            }) if group_id == target_group_id => insertion_index,
            _ => default_insertion_index,
        };
        self.command_drop_feedback = None;
        self.finish_workspace_tab_drag_state(cx);

        let source_pane_id = dragged.source_pane_id;
        let Some((inserted_group_id, _inserted_session_id)) =
            transfer_workspace_placeholder_to_command_tab_strip(
                &mut self.agents_workspace,
                &mut self.command_pane,
                dragged.source_pane_id,
                dragged.session_id,
                target_group_id,
                insertion_index,
            )
        else {
            cx.notify();
            return;
        };

        self.focus_command_pane();
        self.scroll_workspace_pane_active_tab(source_pane_id);
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.scroll_command_group_active_tab(inserted_group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
    }

    pub(crate) fn handle_command_pane_body_drop(
        &mut self,
        target_group_id: CommandPaneGroupId,
        dragged: &DraggedCommandTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        let zone = match self.command_drop_feedback {
            Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::PaneBody(zone),
            }) if group_id == target_group_id => zone,
            _ => WorkspaceDropZone::Center,
        };
        self.finish_command_tab_drag_state(cx);

        let changed = match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => {
                self.command_pane.split_tab_to_group(
                    dragged.source_group_id,
                    target_group_id,
                    dragged.session_id,
                    zone,
                )
            }
            WorkspaceDropZone::Center | WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => self
                .command_pane
                .group_tab_into_group(dragged.source_group_id, target_group_id, dragged.session_id),
        };

        if changed {
            self.focus_command_pane();
            self.scroll_focused_command_active_tab();
            self.persist_shell_layout_state();
        }
        cx.notify();
    }

    pub(crate) fn handle_workspace_tab_command_pane_body_drop(
        &mut self,
        target_group_id: CommandPaneGroupId,
        dragged: &DraggedWorkspaceTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-22-13:05:
        Dropping an Agents tab into the command pane must preserve the placeholder boundary: preflight the Agents final-root transfer guard, create only a command-pane placeholder with the visible title, then remove the Agents shell tab/session and persist layout state. Real process transfer, libghostty remounting, terminal content, command text, stdout/stderr, overlays, and hit-test routing remain deferred.

        CDXC:GPUICommandPaneDragDrop 2026-06-25-19:45:
        Body drops share the rollback-capable Agents-to-command transfer helper with tab-strip drops so failed Agents source close restores prior command-pane mode, focus group, and active tabs after removing only the inserted command placeholder.
        */
        window.prevent_default();
        cx.stop_propagation();

        let zone = match self.command_drop_feedback {
            Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::PaneBody(zone),
            }) if group_id == target_group_id => zone,
            _ => WorkspaceDropZone::Center,
        };
        self.command_drop_feedback = None;
        self.finish_workspace_tab_drag_state(cx);

        let Some((inserted_group_id, _inserted_session_id)) =
            transfer_workspace_placeholder_to_command_pane(
                &mut self.agents_workspace,
                &mut self.command_pane,
                dragged.source_pane_id,
                dragged.session_id,
                target_group_id,
                zone,
            )
        else {
            cx.notify();
            return;
        };

        self.focus_command_pane();
        self.scroll_workspace_pane_active_tab(self.agents_workspace.focused_pane);
        self.scroll_command_group_active_tab(inserted_group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
    }

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
        CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
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
        if payload.to_ghostty_launch_payload().is_err() {
            gpui_close_command_terminal_gxserver_session(&plan.key);
            self.close_command_terminal_after_gxserver_attach_failure(
                current_slot_id,
                "GPUI could not prepare the command terminal attach command.",
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
        CDXC:GPUICommandPaneGxserverRestore 2026-08-13:
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
        CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
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

    pub(crate) fn close_command_terminal_after_gxserver_attach_failure(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let current_slot_id =
            command_pane_group_for_session(&self.command_pane, slot_id.session_id)
                .map(|group_id| CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id: slot_id.session_id,
                })
                .unwrap_or(slot_id);
        let project_board_action = self
            .command_pane
            .session(current_slot_id.session_id)
            .and_then(|session| session.action_command_id.as_deref())
            .and_then(gpui_project_board_action_for_command_id);
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
        let changed = self
            .command_pane
            .close_session(current_slot_id.group_id, current_slot_id.session_id);
        if changed {
            self.clear_gpui_command_delayed_send_timer(current_slot_id.session_id);
            self.clear_gpui_command_close_after_done_timer(current_slot_id.session_id);
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if self.command_pane.has_sessions() {
                self.focus_command_pane();
            } else {
                self.restore_previous_non_command_focus_or_default();
            }
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        }
        self.dispatch_gpui_app_modal_toast("warning", "Command terminal unavailable", message, cx);
        if let Some(action) = project_board_action {
            self.dispatch_gpui_project_board_command_completed(action, 1, cx);
        }
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
        CDXC:GPUICommandWorkspaceTransfer 2026-08-01:
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
            CDXC:RemoteProjectActions 2026-08-29:
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
        #[cfg(target_os = "windows")]
        if matches!(
            windows_terminal_backend::resolve_current(),
            Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
        ) {
            self.start_command_terminal_powershell_for_slot(slot_id, startup_text, cx);
            return;
        }
        /*
        CDXC:RemoteProjectActions 2026-08-29:
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
        CDXC:RemoteProjectActions 2026-08-29:
        An Action tab in a remote project's command pane that has neither a
        remote nor a local identity is unmountable by construction: its Action
        runs on the remote machine, and the local daemon has no session and no
        project path for it. That state exists only when the app was quit (or
        the pane was swapped away) between creating the tab and the remote
        session's identity landing on it, so retire the leftover tab instead of
        asking the local daemon for a session it can never create and reporting
        that as a command-terminal error on every launch. Plain command tabs are
        untouched: they still take the local path and its honest failure.
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
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_prepare_command_terminal_attach_plan(input) })
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
        CDXC:GPUIWindowsPowerShellCommandPane 2026-07-15:
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
        if payload.to_ghostty_launch_payload().is_err() {
            self.close_command_terminal_after_gxserver_attach_failure(
                current_slot_id,
                "GPUI could not prepare the PowerShell command terminal.",
                cx,
            );
            return;
        }
        self.command_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(current_slot_id, payload);
        cx.notify();
    }

    pub(crate) fn open_command_pane_from_shared_settings(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let result = self.command_pane.open_with_default_height_px(
            command_pane_content_height(window),
            command_pane_default_height_px_from_shared_settings(&settings_snapshot),
        );
        if let Some((group_id, session_id, true)) = result {
            self.start_command_terminal_gxserver_attach_for_slot(
                CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                },
                COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                None,
                None,
                None,
                cx,
            );
        }
        result
    }

    pub(crate) fn open_command_pane_from_command_palette(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        Open Commands Panel and F12 expand a hidden pane, focus a visible pane
        that is not already active, and minimize when the command pane already
        owns shell focus. Minimize reuses the titlebar chevron collapse path so
        sessions, height, and previous non-command focus stay intact.
        */
        match command_pane_palette_open_decision(self.command_pane.is_expanded(), self.shell_focus)
        {
            CommandPanePaletteOpenDecision::Minimize => {
                self.handle_command_pane_control_action(
                    CommandPaneControlAction::ToggleExpanded,
                    None,
                    window,
                    cx,
                );
            }
            CommandPanePaletteOpenDecision::OpenAndFocus
            | CommandPanePaletteOpenDecision::FocusVisible => {
                let Some((_group_id, _session_id, _created_session)) =
                    self.open_command_pane_from_shared_settings(window, cx)
                else {
                    return;
                };
                self.command_pane
                    .acknowledge_attention_for_live_focused_group_activation();
                self.focus_command_pane();
                self.request_focused_command_terminal_text_focus_handoff();
                self.scroll_focused_command_active_tab();
                self.persist_shell_layout_state();
                self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn handle_command_pane_control_action(
        &mut self,
        action: CommandPaneControlAction,
        target_group_id: Option<CommandPaneGroupId>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !command_pane_focus_clicked_control_group(
            &mut self.command_pane,
            action,
            target_group_id,
        ) {
            return;
        }

        match action {
            CommandPaneControlAction::NewCommandPlaceholder => {
                self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                let Some((group_id, session_id)) =
                    self.command_pane.add_new_session(target_group_id)
                else {
                    return;
                };
                self.start_command_terminal_gxserver_attach_for_slot(
                    CommandTerminalBodyMountSlotId {
                        group_id,
                        session_id,
                    },
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                    None,
                    None,
                    None,
                    cx,
                );
                self.focus_command_pane();
                self.request_command_terminal_text_focus_handoff(CommandTerminalBodyMountSlotId {
                    group_id,
                    session_id,
                });
                self.scroll_focused_command_active_tab();
            }
            CommandPaneControlAction::TogglePinned => {
                let was_expanded = self.command_pane.is_expanded();
                if !was_expanded {
                    self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                }
                self.command_pane.toggle_pinned();
                let is_expanded_after = self.command_pane.is_expanded();
                if !was_expanded && is_expanded_after {
                    if let Some((group_id, session_id, true)) =
                        self.command_pane.ensure_session_for_open()
                    {
                        self.start_command_terminal_gxserver_attach_for_slot(
                            CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            },
                            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                            None,
                            None,
                            None,
                            cx,
                        );
                    }
                }
                if command_pane_control_action_focuses_command_pane(
                    CommandPaneControlAction::TogglePinned,
                    was_expanded,
                    is_expanded_after,
                ) {
                    self.command_pane
                        .acknowledge_attention_for_focused_session_activation();
                    self.focus_command_pane();
                    self.request_focused_command_terminal_text_focus_handoff();
                    self.scroll_focused_command_active_tab();
                }
            }
            CommandPaneControlAction::ToggleExpanded => {
                let was_expanded = self.command_pane.is_expanded();
                if !was_expanded {
                    self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
                }
                self.command_pane.toggle_expanded();
                let is_expanded_after = self.command_pane.is_expanded();
                if was_expanded && !is_expanded_after {
                    self.clear_command_resize_hover_state();
                }
                if command_pane_control_action_focuses_command_pane(
                    CommandPaneControlAction::ToggleExpanded,
                    was_expanded,
                    is_expanded_after,
                ) {
                    if let Some((group_id, session_id, true)) =
                        self.command_pane.ensure_session_for_open()
                    {
                        self.start_command_terminal_gxserver_attach_for_slot(
                            CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            },
                            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                            None,
                            None,
                            None,
                            cx,
                        );
                    }
                    self.command_pane
                        .acknowledge_attention_for_focused_session_activation();
                    self.focus_command_pane();
                    self.request_focused_command_terminal_text_focus_handoff();
                    self.scroll_focused_command_active_tab();
                } else if was_expanded && !is_expanded_after {
                    self.restore_previous_non_command_focus_or_default();
                }
            }
        }
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    pub(crate) fn handle_command_pane_empty_titlebar_mouse_down(
        &mut self,
        group_id: Option<CommandPaneGroupId>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandTabDoubleClick 2026-06-25-13:58:
        Native accepts double-click on empty pane-titlebar chrome after real tabs and controls decline the hit. GPUI command tab-strip backgrounds use this shared handler so expanded and collapsed command chrome create a New Terminal without affecting child tab/control clicks.
        */
        if !command_pane_empty_titlebar_double_click_creates_new_terminal(event.click_count) {
            return;
        }

        window.prevent_default();
        cx.stop_propagation();
        self.handle_command_pane_control_action(
            CommandPaneControlAction::NewCommandPlaceholder,
            group_id,
            window,
            cx,
        );
    }

    pub(crate) fn open_command_pane_from_keyboard(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        F12 and the configured Open Commands Panel hotkey share the
        command-palette route: expand when hidden, focus when visible but
        inactive, minimize when the command pane is already the active surface.
        */
        self.open_command_pane_from_command_palette(window, cx);
    }

    pub(crate) fn record_workspace_split_layout_metrics(
        &mut self,
        split_id: WorkspaceSplitId,
        axis: WorkspaceSplitAxis,
        child_bounds: &[Bounds<Pixels>],
    ) {
        let Some(content_span) = split_resize_content_span(child_bounds, axis) else {
            return;
        };

        self.workspace_split_layout_metrics
            .insert(split_id, SplitResizeMetrics { content_span });
    }

    pub(crate) fn record_command_split_layout_metrics(
        &mut self,
        split_id: CommandPaneSplitId,
        axis: WorkspaceSplitAxis,
        child_bounds: &[Bounds<Pixels>],
    ) {
        let Some(content_span) = split_resize_content_span(child_bounds, axis) else {
            return;
        };

        self.command_split_layout_metrics
            .insert(split_id, SplitResizeMetrics { content_span });
    }

    pub(crate) fn record_browser_split_layout_metrics(
        &mut self,
        split_id: BrowserSplitId,
        axis: WorkspaceSplitAxis,
        child_bounds: &[Bounds<Pixels>],
    ) {
        let Some(content_span) = split_resize_content_span(child_bounds, axis) else {
            return;
        };

        self.browser_split_layout_metrics
            .insert(split_id, SplitResizeMetrics { content_span });
    }

    pub(crate) fn record_project_editor_companion_layout_metrics(
        &mut self,
        child_bounds: &[Bounds<Pixels>],
    ) {
        let Some(content_span) =
            split_resize_content_span(child_bounds, WorkspaceSplitAxis::Horizontal)
        else {
            return;
        };

        self.project_editor_companion_layout_metrics = Some(SplitResizeMetrics { content_span });
    }

    pub(crate) fn record_project_editor_companion_split_layout_metrics(
        &mut self,
        child_bounds: &[Bounds<Pixels>],
    ) {
        let Some(content_span) =
            split_resize_content_span(child_bounds, WorkspaceSplitAxis::Vertical)
        else {
            return;
        };

        self.project_editor_companion_split_layout_metrics =
            Some(SplitResizeMetrics { content_span });
    }

    pub(crate) fn handle_workspace_split_handle_mouse_down(
        &mut self,
        split_id: WorkspaceSplitId,
        axis: WorkspaceSplitAxis,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIWorkspaceResize 2026-06-22-06:45:
        Agents workspace split handles are real five-pixel layout siblings with one-pixel visual separators. Dragging a horizontal handle updates the left/right split ratio, dragging a vertical handle updates the top/bottom ratio, and double-click resets that split to the shell default 0.5 ratio while persisting only placeholder shell layout state.
        */
        window.prevent_default();
        cx.stop_propagation();

        self.workspace_split_drag = None;

        if event.click_count >= 2 {
            let reset_ratio = self.agents_workspace.reset_split_ratio(split_id);
            let cleared_hover = self.clear_workspace_split_hover_state();
            if reset_ratio {
                self.persist_shell_layout_state();
            }
            if reset_ratio || cleared_hover {
                cx.notify();
            }
            return;
        }

        let Some(start_ratio) = self.agents_workspace.split_ratio(split_id) else {
            return;
        };
        let Some(metrics) = self.workspace_split_layout_metrics.get(&split_id).copied() else {
            return;
        };
        let content_span = metrics.content_span.max(1.0);

        self.workspace_split_drag = Some(WorkspaceSplitResizeDragState {
            split_id,
            axis,
            start_position: split_resize_event_position(axis, event.position),
            start_ratio,
            content_span,
        });
        self.workspace_split_hover_epoch = self.workspace_split_hover_epoch.wrapping_add(1);
        self.workspace_split_hovering = Some(split_id);
        self.workspace_split_hover_visible = Some(split_id);
        cx.notify();
    }

    pub(crate) fn handle_workspace_split_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.workspace_split_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_workspace_split_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let raw_ratio = drag.start_ratio
            + (split_resize_event_position(drag.axis, event.position) - drag.start_position)
                / drag.content_span.max(1.0);
        let Some((min_ratio, max_ratio)) = self
            .agents_workspace
            .split_drag_ratio_bounds(drag.split_id, drag.content_span)
        else {
            return;
        };
        if self
            .agents_workspace
            .set_split_ratio(drag.split_id, raw_ratio.clamp(min_ratio, max_ratio))
        {
            cx.notify();
        }
    }

    pub(crate) fn handle_workspace_split_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.workspace_split_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_workspace_split_resize_drag(cx);
    }

    pub(crate) fn finish_workspace_split_resize_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.workspace_split_drag.take().is_some() {
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn workspace_split_hover_line_visible(&self, split_id: WorkspaceSplitId) -> bool {
        self.workspace_split_hover_visible == Some(split_id)
            || self
                .workspace_split_drag
                .is_some_and(|drag| drag.split_id == split_id)
    }

    pub(crate) fn set_workspace_split_hovering(
        &mut self,
        split_id: WorkspaceSplitId,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if hovered {
            if self.workspace_split_hovering == Some(split_id) {
                return;
            }

            self.workspace_split_hover_epoch = self.workspace_split_hover_epoch.wrapping_add(1);
            self.workspace_split_hovering = Some(split_id);
            self.workspace_split_hover_visible = None;
            let epoch = self.workspace_split_hover_epoch;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                    .await;

                let _ = this.update(cx, |this, cx| {
                    if this.workspace_split_hover_epoch == epoch
                        && this.workspace_split_hovering == Some(split_id)
                    {
                        this.workspace_split_hover_visible = Some(split_id);
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }

        if self.workspace_split_hovering == Some(split_id)
            || self.workspace_split_hover_visible == Some(split_id)
        {
            self.workspace_split_hover_epoch = self.workspace_split_hover_epoch.wrapping_add(1);
            self.workspace_split_hovering = None;
            self.workspace_split_hover_visible = None;
            cx.notify();
        }
    }

    pub(crate) fn clear_workspace_split_hover_state(&mut self) -> bool {
        if self.workspace_split_hovering.is_some() || self.workspace_split_hover_visible.is_some() {
            self.workspace_split_hover_epoch = self.workspace_split_hover_epoch.wrapping_add(1);
            self.workspace_split_hovering = None;
            self.workspace_split_hover_visible = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn handle_command_split_handle_mouse_down(
        &mut self,
        split_id: CommandPaneSplitId,
        axis: WorkspaceSplitAxis,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPaneResize 2026-06-22-06:45:
        Command-pane split handles use the same real-layout resize contract as Agents splits. Dragging updates horizontal split ratios from x movement and vertical split ratios from y movement, double-click resets to 0.5, and finished mutations persist with the placeholder shell state without starting real command processes.

        CDXC:GPUICommandPaneResize 2026-06-27-03:38:
        Command split resize hover chrome is gesture-owned runtime state. Double-click reset must clear the split rail cursor/hover affordance and invalidate delayed hover timers even when the split ratio was already at the default, while layout persistence remains tied to an actual ratio reset.
        */
        window.prevent_default();
        cx.stop_propagation();

        self.command_split_drag = None;

        if event.click_count >= 2 {
            let reset_ratio = self.command_pane.reset_split_ratio(split_id);
            let cleared_hover = self.clear_command_resize_hover_state();
            if reset_ratio {
                self.persist_shell_layout_state();
            }
            if reset_ratio || cleared_hover {
                cx.notify();
            }
            return;
        }

        let Some(start_ratio) = self.command_pane.split_ratio(split_id) else {
            return;
        };
        let Some(metrics) = self.command_split_layout_metrics.get(&split_id).copied() else {
            return;
        };

        self.command_split_drag = Some(CommandPaneSplitResizeDragState {
            split_id,
            axis,
            start_position: split_resize_event_position(axis, event.position),
            start_ratio,
            content_span: metrics.content_span.max(1.0),
        });
        cx.notify();
    }

    pub(crate) fn handle_command_split_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.command_split_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_command_split_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let raw_ratio = drag.start_ratio
            + (split_resize_event_position(drag.axis, event.position) - drag.start_position)
                / drag.content_span.max(1.0);
        let Some((min_ratio, max_ratio)) = self
            .command_pane
            .split_drag_ratio_bounds(drag.split_id, drag.content_span)
        else {
            return;
        };
        if self
            .command_pane
            .set_split_ratio(drag.split_id, raw_ratio.clamp(min_ratio, max_ratio))
        {
            cx.notify();
        }
    }

    pub(crate) fn handle_command_split_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_split_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_command_split_resize_drag(cx);
    }

    pub(crate) fn finish_command_split_resize_drag(&mut self, cx: &mut gpui::Context<Self>) {
        let consumed_drag = self.command_split_drag.take().is_some();
        let cleared_hover = self.clear_command_resize_hover_state();
        if consumed_drag {
            self.persist_shell_layout_state();
            cx.notify();
        } else if cleared_hover {
            cx.notify();
        }
    }

    pub(crate) fn handle_browser_split_handle_mouse_down(
        &mut self,
        split_id: BrowserSplitId,
        axis: WorkspaceSplitAxis,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIBrowserSplitResize 2026-06-22-09:05:
        Browser split handles use the same normal-layout resize contract as Agents workspace and command-pane splits. The visible five-pixel divider is the only grab target; dragging updates the targeted Browser split ratio from that branch's rendered first/handle/second child metrics, double-click resets to 0.5, and finished mutations persist through sanitized GPUI shell state while Browser leaf bodies keep existing tab-owned CEF surfaces keyed by BrowserTabId.
        */
        window.prevent_default();
        cx.stop_propagation();

        self.browser_split_drag = None;

        if event.click_count >= 2 {
            if self.browser_tabs.reset_split_ratio(split_id) {
                self.persist_shell_layout_state();
                cx.notify();
            }
            return;
        }

        let Some(start_ratio) = self.browser_tabs.split_ratio(split_id) else {
            return;
        };
        let Some(metrics) = self.browser_split_layout_metrics.get(&split_id).copied() else {
            return;
        };
        let content_span = metrics.content_span.max(1.0);

        self.browser_split_drag = Some(BrowserSplitResizeDragState {
            split_id,
            axis,
            start_position: split_resize_event_position(axis, event.position),
            start_ratio,
            content_span,
        });
        cx.notify();
    }

    pub(crate) fn handle_browser_split_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.browser_split_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_browser_split_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let raw_ratio = drag.start_ratio
            + (split_resize_event_position(drag.axis, event.position) - drag.start_position)
                / drag.content_span.max(1.0);
        let Some((min_ratio, max_ratio)) = self
            .browser_tabs
            .split_drag_ratio_bounds(drag.split_id, drag.content_span)
        else {
            return;
        };
        if self
            .browser_tabs
            .set_split_ratio(drag.split_id, raw_ratio.clamp(min_ratio, max_ratio))
        {
            cx.notify();
        }
    }

    pub(crate) fn handle_browser_split_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_split_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_browser_split_resize_drag(cx);
    }

    pub(crate) fn finish_browser_split_resize_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.browser_split_drag.take().is_some() {
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn handle_project_editor_companion_divider_mouse_down(
        &mut self,
        mode: TitlebarMode,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIProjectEditorResize 2026-06-22-06:53:
        Source, Browser, Kanban, and Manage companion panes use the visible five-pixel divider as the real resize control. Dragging adjusts the stored companion width ratio, double-click resets toward the 0.32 default within practical width clamps, and shell-state persistence happens after reset or finished drag without hidden overlays or root hit-test routing.
        */
        window.prevent_default();
        cx.stop_propagation();

        self.project_editor_companion_drag = None;

        if self.active_mode != mode || !self.project_editor_shell.left_companion_visible {
            return;
        }

        self.set_project_editor_companion_divider_hovering(mode, true, cx);

        if event.click_count >= 2 {
            let content_span = self
                .project_editor_companion_layout_metrics
                .map(|metrics| metrics.content_span);
            if self
                .project_editor_shell
                .reset_left_companion_width_ratio(content_span)
            {
                self.persist_shell_layout_state();
                cx.notify();
            }
            return;
        }

        let Some(metrics) = self.project_editor_companion_layout_metrics else {
            return;
        };

        self.project_editor_companion_drag = Some(ProjectEditorCompanionResizeDragState {
            start_x: event.position.x.as_f32(),
            start_ratio: self.project_editor_shell.left_companion_width_ratio,
            content_span: metrics.content_span.max(1.0),
        });
        cx.notify();
    }

    pub(crate) fn handle_project_editor_companion_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.project_editor_companion_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_project_editor_companion_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let next_ratio =
            drag.start_ratio + (event.position.x.as_f32() - drag.start_x) / drag.content_span;
        if self
            .project_editor_shell
            .set_left_companion_width_ratio(next_ratio, drag.content_span)
        {
            cx.notify();
        }
    }

    pub(crate) fn handle_project_editor_companion_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_companion_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_project_editor_companion_resize_drag(cx);
    }

    pub(crate) fn finish_project_editor_companion_resize_drag(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_companion_drag.take().is_some() {
            self.clear_project_editor_companion_divider_hover_state();
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn handle_project_editor_companion_split_divider_mouse_down(
        &mut self,
        mode: TitlebarMode,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();
        self.project_editor_companion_split_drag = None;

        if self.active_mode != mode
            || !self.project_editor_shell.left_companion_visible
            || self
                .project_editor_companion_secondary_terminal_session_id
                .is_none()
        {
            return;
        }

        self.set_project_editor_companion_split_divider_hovering(mode, true, cx);
        if event.click_count >= 2 {
            let content_span = self
                .project_editor_companion_split_layout_metrics
                .map(|metrics| metrics.content_span);
            if self
                .project_editor_shell
                .reset_left_companion_split_ratio(content_span)
            {
                self.persist_shell_layout_state();
                cx.notify();
            }
            return;
        }

        let Some(metrics) = self.project_editor_companion_split_layout_metrics else {
            return;
        };
        self.project_editor_companion_split_drag =
            Some(ProjectEditorCompanionSplitResizeDragState {
                start_y: event.position.y.as_f32(),
                start_ratio: self.project_editor_shell.left_companion_split_ratio,
                content_span: metrics.content_span.max(1.0),
            });
        cx.notify();
    }

    pub(crate) fn handle_project_editor_companion_split_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.project_editor_companion_split_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_project_editor_companion_split_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();
        let next_ratio =
            drag.start_ratio + (event.position.y.as_f32() - drag.start_y) / drag.content_span;
        if self
            .project_editor_shell
            .set_left_companion_split_ratio(next_ratio, drag.content_span)
        {
            cx.notify();
        }
    }

    pub(crate) fn handle_project_editor_companion_split_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_companion_split_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_project_editor_companion_split_resize_drag(cx);
    }

    pub(crate) fn finish_project_editor_companion_split_resize_drag(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_companion_split_drag.take().is_some() {
            self.clear_project_editor_companion_split_divider_hover_state();
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    pub(crate) fn set_project_editor_companion_split_divider_hovering(
        &mut self,
        mode: TitlebarMode,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if hovered {
            if self.project_editor_companion_split_divider_hovering == Some(mode) {
                return;
            }
            self.project_editor_companion_split_divider_hover_epoch = self
                .project_editor_companion_split_divider_hover_epoch
                .wrapping_add(1);
            self.project_editor_companion_split_divider_hovering = Some(mode);
            self.project_editor_companion_split_divider_hover_visible = None;
            let epoch = self.project_editor_companion_split_divider_hover_epoch;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.project_editor_companion_split_divider_hover_epoch == epoch
                        && this.project_editor_companion_split_divider_hovering == Some(mode)
                    {
                        this.project_editor_companion_split_divider_hover_visible = Some(mode);
                        cx.notify();
                    }
                });
            })
            .detach();
            return;
        }

        if self.project_editor_companion_split_divider_hovering == Some(mode)
            || self.project_editor_companion_split_divider_hover_visible == Some(mode)
        {
            self.clear_project_editor_companion_split_divider_hover_state();
            cx.notify();
        }
    }

    pub(crate) fn clear_project_editor_companion_split_divider_hover_state(&mut self) -> bool {
        if self
            .project_editor_companion_split_divider_hovering
            .is_none()
            && self
                .project_editor_companion_split_divider_hover_visible
                .is_none()
        {
            return false;
        }
        self.project_editor_companion_split_divider_hover_epoch = self
            .project_editor_companion_split_divider_hover_epoch
            .wrapping_add(1);
        self.project_editor_companion_split_divider_hovering = None;
        self.project_editor_companion_split_divider_hover_visible = None;
        true
    }

    pub(crate) fn set_project_editor_companion_divider_hovering(
        &mut self,
        mode: TitlebarMode,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if hovered {
            if self.project_editor_companion_divider_hovering == Some(mode) {
                return;
            }

            self.project_editor_companion_divider_hover_epoch = self
                .project_editor_companion_divider_hover_epoch
                .wrapping_add(1);
            self.project_editor_companion_divider_hovering = Some(mode);
            self.project_editor_companion_divider_hover_visible = None;
            let epoch = self.project_editor_companion_divider_hover_epoch;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                    .await;

                let _ = this.update(cx, |this, cx| {
                    if this.project_editor_companion_divider_hover_epoch == epoch
                        && this.project_editor_companion_divider_hovering == Some(mode)
                    {
                        this.project_editor_companion_divider_hover_visible = Some(mode);
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }

        if self.project_editor_companion_divider_hovering == Some(mode)
            || self.project_editor_companion_divider_hover_visible == Some(mode)
        {
            self.clear_project_editor_companion_divider_hover_state();
            cx.notify();
        }
    }

    pub(crate) fn clear_project_editor_companion_divider_hover_state(&mut self) -> bool {
        if self.project_editor_companion_divider_hovering.is_none()
            && self
                .project_editor_companion_divider_hover_visible
                .is_none()
        {
            return false;
        }

        self.project_editor_companion_divider_hover_epoch = self
            .project_editor_companion_divider_hover_epoch
            .wrapping_add(1);
        self.project_editor_companion_divider_hovering = None;
        self.project_editor_companion_divider_hover_visible = None;
        true
    }

    pub(crate) fn handle_command_pane_resize_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cpraildbg(&format!(
            "rail_mouse_down pos=({:?},{:?}) clicks={}",
            event.position.x, event.position.y, event.click_count
        ));
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.clear_command_resize_hover_state();
            let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
            self.command_pane.reset_height_from_shared_settings(
                command_pane_content_height(window),
                &settings_snapshot,
            );
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }

        let content_height = command_pane_content_height(window);
        self.command_pane.resize_drag = Some(CommandPaneResizeDragState {
            side: GpuiCommandPaneSide::Bottom,
            start_position: event.position.y.as_f32(),
            start_extent: command_pane_height_for_ratio(
                self.command_pane.height_ratio,
                content_height,
            ),
        });
        cx.notify();
    }

    pub(crate) fn handle_command_pane_side_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPaneSide 2026-08-16:
        The right-dock divider mirrors the bottom rail contract on the X axis:
        double-click resets the width ratio to the constant default, a single
        press stores the absolute start width and pointer X, and the root
        mouse-move/up handlers finish the drag through the shared resize state.
        */
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.clear_command_resize_hover_state();
            self.command_pane.reset_width_to_default();
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }

        let content_width =
            command_pane_workspace_width(window, self.sidebar_width, self.sidebar_collapsed);
        self.command_pane.resize_drag = Some(CommandPaneResizeDragState {
            side: GpuiCommandPaneSide::Right,
            start_position: event.position.x.as_f32(),
            start_extent: command_pane_width_for_ratio(
                self.command_pane.width_ratio,
                content_width,
            ),
        });
        cx.notify();
    }

    pub(crate) fn handle_command_pane_resize_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.command_pane.resize_drag else {
            return;
        };
        cpraildbg(&format!(
            "drag_move y={:?} dragging={}",
            event.position.y,
            event.dragging()
        ));

        if !event.dragging() {
            self.finish_command_pane_resize_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        match drag.side {
            GpuiCommandPaneSide::Bottom => {
                let next_ratio = command_pane_resize_drag_height_ratio(
                    drag,
                    event.position.y.as_f32(),
                    command_pane_content_height(window),
                );
                if (next_ratio - self.command_pane.height_ratio).abs() >= 0.001 {
                    self.command_pane.height_ratio = next_ratio;
                    cx.notify();
                }
            }
            GpuiCommandPaneSide::Right => {
                let next_ratio = command_pane_resize_drag_width_ratio(
                    drag,
                    event.position.x.as_f32(),
                    command_pane_workspace_width(
                        window,
                        self.sidebar_width,
                        self.sidebar_collapsed,
                    ),
                );
                if (next_ratio - self.command_pane.width_ratio).abs() >= 0.001 {
                    self.command_pane.width_ratio = next_ratio;
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn handle_command_pane_resize_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.command_pane.resize_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_command_pane_resize_drag(cx);
    }

    pub(crate) fn finish_command_pane_resize_drag(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUICommandPaneResize 2026-06-25-19:13:
        Native command-panel drag continuations update layout during pointer movement, but the durable height-ratio notification is emitted once from `endCommandsPanelResize`.
        GPUI mirrors that by mutating the ratio during drag and persisting only when the stored resize state is consumed here.

        CDXC:GPUICommandPaneResize 2026-06-27-03:13:
        Ending top-rail resize ownership must also remove runtime hover/cursor chrome and invalidate delayed hover timers. Persist layout only for a consumed drag, but still repaint when clearing hover state is the only visible change.
        */
        let consumed_drag = self.command_pane.resize_drag.take().is_some();
        let cleared_resize_hover = self.clear_command_resize_hover_state();
        if consumed_drag {
            self.persist_shell_layout_state();
        }
        if consumed_drag || cleared_resize_hover {
            cx.notify();
        }
    }
}
