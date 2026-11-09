//! Where a sidebar session lands in the Agents panes: the selection rule that puts it in the
//! focused pane, and the drag from a sidebar row onto a pane body.

use gpui::{CursorStyle, DragMoveEvent, Window};

use crate::GhostexGpuiApp;
use crate::app::helpers::{GpuiRemoteAttachSessionKey, gpui_remote_scoped_project_id};
use crate::app::model::{
    GpuiLocalWorkspaceSessionKey, TerminalSessionId, WorkspaceDropFeedback, WorkspaceDropTarget,
    WorkspaceDropZone, WorkspacePaneId, workspace_pane_body_drop_zone,
};
use crate::app::native_sidebar::drag::{SidebarDrag, SidebarDragPreview};
use crate::app::native_sidebar::row_drag::RowDragIdentity;

impl GhostexGpuiApp {
    /// CDXC:FocusRouting 2026-09-22 DECISION:
    /// User: selecting a session (a sidebar click, the session hotkeys, or any other way) shows it
    /// in the focused pane, replacing the session that pane was showing, so the sidebar is the
    /// list of what can be shown and a split pane is a viewport onto it. A session that is already
    /// on screen in another split pane is focused there instead of being pulled across. Before
    /// this, a selection switched to whichever pane held the session's tab and left the focused
    /// pane alone.
    ///
    /// Returns the pane the session is now the active tab of.
    pub(crate) fn pull_workspace_session_into_focused_pane(
        &mut self,
        source_pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> WorkspacePaneId {
        let focused_pane_id = self.agents_workspace.focused_pane;
        if source_pane_id == focused_pane_id
            || self.agents_workspace.find_leaf(focused_pane_id).is_none()
        {
            return source_pane_id;
        }
        let on_screen = self
            .agents_workspace
            .rendered_leaf_order()
            .contains(&source_pane_id)
            && self.agents_workspace.active_session_in_pane(source_pane_id) == Some(session_id);
        if on_screen {
            return source_pane_id;
        }
        if self
            .agents_workspace
            .group_tab_into_pane(source_pane_id, focused_pane_id, session_id)
        {
            focused_pane_id
        } else {
            source_pane_id
        }
    }

    /// What a sidebar row drag can do on an Agents pane; the hover feedback and the drop both ask
    /// this, so a pane never shows a zone the release then ignores. `None` for a row that is not a
    /// session (no feedback at all); `Some(Err(reason))` for a session no pane here can take (one of
    /// another project than the panes show), which
    /// the pane says while the pointer is over it; `Some(Ok(None))` for a session of the shown
    /// project (local, or on another computer) that has no tab in any pane yet, which only the
    /// middle of a pane takes.
    fn sidebar_drag_pane_session(
        &self,
        drag: &SidebarDrag,
    ) -> Option<Result<Option<(TerminalSessionId, WorkspacePaneId)>, &'static str>> {
        if drag.kind != "session" {
            return None;
        }
        let SidebarDragPreview::Row(row) = &drag.preview else {
            return None;
        };
        let RowDragIdentity::Session { session } = &row.identity else {
            return None;
        };
        if session.is_browser() {
            return None;
        }
        let key = ghostex_gx_core::SessionKey::parse_sidebar_session_id(&drag.id)?;
        // CDXC:RemoteMachines 2026-11-09 DECISION:
        // User: "i can't drag sessions from the sidebar onto the main work area to split it if they are remote machine projects pls fix". A session on another computer drops like a local one: the panes show one project at a time, and a remote project's panes are its machine-scoped workspace (`gpui_remote_scoped_project_id`), whose tabs are mapped in `remote_attach_sessions`. A session of that project splits or replaces a pane exactly as a local session of the active local project does; one of any other project (local or on another computer) is refused as before. Supersedes the 2026-10-08 refusal of every session on another computer.
        let shell_session_id = match &key.machine {
            ghostex_gx_core::MachineId::Local => {
                if self.agents_workspace_project_id.as_deref() != Some(&key.project_id) {
                    return Some(Err("Can't split here: this session is in another project"));
                }
                self.local_workspace_session_mappings
                    .get(&GpuiLocalWorkspaceSessionKey {
                        project_id: key.project_id,
                        session_id: key.session_id,
                    })
                    .copied()
            }
            ghostex_gx_core::MachineId::Remote(machine_id) => {
                if self.agents_workspace_project_id.as_deref()
                    != Some(gpui_remote_scoped_project_id(machine_id, &key.project_id).as_str())
                {
                    return Some(Err("Can't split here: this session is in another project"));
                }
                self.remote_attach_sessions
                    .get(&GpuiRemoteAttachSessionKey {
                        remote_machine_id: machine_id.clone(),
                        project_id: key.project_id,
                        session_id: key.session_id,
                    })
                    .copied()
            }
        };
        Some(Ok(shell_session_id.and_then(|shell_session_id| {
            self.agents_workspace
                .pane_id_for_session(shell_session_id)
                .map(|pane_id| (shell_session_id, pane_id))
        })))
    }

    /// CDXC:Workarea 2026-09-23 DECISION:
    /// User: a session row dragged from the sidebar onto a terminal or chat pane splits that pane
    /// the way a dragged tab did, so the tab bar is not needed to split, and "allow dragging to the
    /// center": the middle of a pane shows the dragged session in that pane, replacing what it
    /// showed, including with no split and for a session no pane holds yet. This supersedes the
    /// 2026-09-22 edges-only rule. Only local sessions of the active project drop; other rows show
    /// no zone, a session with no tab yet shows only the middle (an edge would split a pane off a
    /// session that has none), and the middle of the pane already showing the session shows
    /// nothing. The pane hides its surfaces for the zones the moment the drag enters a
    /// pane rather than when it starts, so reordering rows in the sidebar leaves the terminals
    /// alone.
    ///
    /// CDXC:Workarea 2026-10-08 DECISION:
    /// User: "if i drag a session from the sidebar and try to drop it on another project session to split you need to indicate it's not possible somehow". A pane that cannot take the dragged session (another project's session, or one on another computer) shows no split zones: it dims with a short label saying why, the pointer shows the not-allowed cursor and the dragged card fades, and releasing there does nothing, with no toast since the label already said so.
    pub(crate) fn update_sidebar_session_pane_drag_feedback(
        &mut self,
        event: &DragMoveEvent<SidebarDrag>,
        pane_id: WorkspacePaneId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let over_pane = event.bounds.contains(&event.event.position);
        let drag = event.drag(cx).clone();
        let Some(tab) = self.sidebar_drag_pane_session(&drag) else {
            return;
        };
        if !over_pane {
            if self
                .workspace_drop_feedback
                .is_some_and(|feedback| feedback.pane_id == pane_id)
            {
                self.clear_workspace_drop_feedback(cx);
                show_sidebar_drag_refused(&drag, false, window, cx);
            }
            return;
        }
        self.begin_workspace_tab_drag(cx);
        let tab = match tab {
            Ok(tab) => tab,
            Err(reason) => {
                self.set_workspace_drop_feedback(
                    Some(WorkspaceDropFeedback {
                        pane_id,
                        target: WorkspaceDropTarget::Refused(reason),
                    }),
                    cx,
                );
                show_sidebar_drag_refused(&drag, true, window, cx);
                return;
            }
        };
        show_sidebar_drag_refused(&drag, false, window, cx);
        let zone = workspace_pane_body_drop_zone(event.bounds, event.event.position);
        let center = matches!(zone, WorkspaceDropZone::Center);
        // A session with no tab yet can only be shown, not split off; the middle of the pane
        // already showing a session has nothing to do.
        let refused = match tab {
            None => !center,
            Some((session_id, source_pane_id)) => {
                (center
                    && source_pane_id == pane_id
                    && self.agents_workspace.active_session_in_pane(pane_id) == Some(session_id))
                    || self
                        .agents_workspace
                        .workspace_tab_edge_drop_is_single_tab_own_pane_noop(
                            source_pane_id,
                            pane_id,
                            zone,
                        )
            }
        };
        if refused {
            self.clear_workspace_drop_feedback(cx);
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

    pub(crate) fn handle_sidebar_session_pane_body_drop(
        &mut self,
        target_pane_id: WorkspacePaneId,
        drag: &SidebarDrag,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let session = self.sidebar_drag_pane_session(drag);
        let zone = match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }) if pane_id == target_pane_id => Some(zone),
            _ => None,
        };
        self.finish_workspace_tab_drag_state(cx);
        // A refused session never gets a zone; the pane's label already said why.
        let (Some(Ok(tab)), Some(zone)) = (session, zone) else {
            cx.notify();
            return;
        };
        window.prevent_default();
        cx.stop_propagation();
        let Some((session_id, source_pane_id)) = tab else {
            // No tab yet: the middle of a pane shows the session there exactly as clicking its row
            // shows it in the focused pane, wake and attach included.
            self.focus_agents_pane(target_pane_id, cx);
            self.dispatch_native_sidebar_ui(
                serde_json::json!({"type": "selectSession", "sessionId": drag.id, "mode": "focus"}),
                cx,
            );
            let _ = self.react_to_native_sidebar_session_click(&drag.id, cx);
            cx.notify();
            return;
        };
        let was_on_screen = self
            .agents_workspace
            .rendered_leaf_order()
            .contains(&source_pane_id)
            && self.agents_workspace.active_session_in_pane(source_pane_id) == Some(session_id);
        if self
            .agents_workspace
            .split_tab_to_pane(source_pane_id, target_pane_id, session_id, zone)
        {
            if was_on_screen {
                self.close_pane_left_by_dragged_session(source_pane_id, target_pane_id);
            }
            // The same activation a tab drop completes with, so a sleeping or runtime-missing
            // session is reported to the sidebar for its wake and reattach.
            self.select_agents_tab(self.agents_workspace.focused_pane, session_id, cx);
        } else {
            cx.notify();
        }
    }

    /// CDXC:Workarea 2026-09-23 DECISION:
    /// User: dragging a session onto the middle of another pane moves it there, and with `a b / c d` dragging `c` onto `b` leaves `a b / d d`: the pane `c` came from goes away and `d` fills the row. `b`'s pane now shows `c`, the way any selection replaces what the focused pane shows. A pane is a viewport onto the session it shows, so when that session is dragged to another pane (the middle or an edge) the viewport it leaves closes instead of showing some other session; the sessions it held behind the scenes join its neighbour and keep running.
    pub(crate) fn close_pane_left_by_dragged_session(
        &mut self,
        source_pane_id: WorkspacePaneId,
        target_pane_id: WorkspacePaneId,
    ) {
        if source_pane_id == target_pane_id
            || self.agents_workspace.find_leaf(source_pane_id).is_none()
        {
            return;
        }
        let focused = self.agents_workspace.focused_pane;
        if self
            .agents_workspace
            .close_pane_keeping_sessions(source_pane_id)
            .is_some()
            && self.agents_workspace.find_leaf(focused).is_some()
        {
            self.agents_workspace.set_focused_pane(focused);
        }
    }
}

/// Fades the dragged row and shows the not-allowed cursor while it is over a pane that refuses it,
/// and puts both back when it leaves. The cursor a sidebar row drag starts with is the arrow its
/// row shows (`cursor_default`).
fn show_sidebar_drag_refused(
    drag: &SidebarDrag,
    refused: bool,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    if drag.refused.replace(refused) == refused {
        return;
    }
    let cursor = if refused {
        CursorStyle::OperationNotAllowed
    } else {
        CursorStyle::Arrow
    };
    cx.set_active_drag_cursor_style(cursor, window);
}
