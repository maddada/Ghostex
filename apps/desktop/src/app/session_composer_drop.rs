//! A session row dragged from the sidebar onto the chat box of a chat pane: its link is typed at
//! the caret instead of the pane splitting (the text comes from `gx_store/session_mentions.rs`,
//! the typing from `native_chat/composer_drop.rs`).

use gpui::{CursorStyle, DragMoveEvent, Window};

use crate::GhostexGpuiApp;
use crate::app::model::{TerminalSessionId, WorkspacePaneId};
use crate::app::native_sidebar::drag::SidebarDrag;

impl GhostexGpuiApp {
    /// Rings the chat box of the pane's chat while a session row is held over it, with the copy
    /// cursor and none of the pane's split zones; returns `true` then, so the pane draws no split
    /// feedback. Any session drops here, whatever its project or computer.
    ///
    /// CDXC:SessionChat 2026-11-12 SEE-ALSO: the user's decision on
    /// `sidebar_drag_session_mentions` (gx_store/session_mentions.rs): over the chat box the drop
    /// shows as a text insertion, no split preview; anywhere else in the pane keeps the split
    /// (session_pane_placement.rs).
    pub(crate) fn update_sidebar_session_composer_drag_feedback(
        &mut self,
        event: &DragMoveEvent<SidebarDrag>,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let drag = event.drag(cx).clone();
        if drag.kind != "session" {
            return false;
        }
        let Some(chat) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        let position = event.event.position;
        let over = event.bounds.contains(&position) && chat.read(cx).composer_drop_target(position);
        let changed = chat.update(cx, |chat, cx| chat.set_composer_drop_hover(over, cx));
        if !over {
            // Leaving the box hands the cursor back to whatever the pane under the pointer says.
            if changed {
                let cursor = if drag.refused.get() {
                    CursorStyle::OperationNotAllowed
                } else {
                    CursorStyle::Arrow
                };
                cx.set_active_drag_cursor_style(cursor, window);
            }
            return false;
        }
        if self
            .workspace_drop_feedback
            .is_some_and(|feedback| feedback.pane_id == pane_id)
        {
            self.clear_workspace_drop_feedback(cx);
        }
        if changed {
            drag.refused.set(false);
            cx.set_active_drag_cursor_style(CursorStyle::DragCopy, window);
        }
        true
    }

    /// Types the dropped sessions' links into the pane's chat box when the drop lands on it, and
    /// returns `true`; `false` leaves the drop to the pane's split.
    pub(crate) fn handle_sidebar_session_composer_drop(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        drag: &SidebarDrag,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if drag.kind != "session" {
            return false;
        }
        let Some(chat) = self.native_chat_views.get(&session_id).cloned() else {
            return false;
        };
        if !chat.read(cx).composer_drop_target(window.mouse_position()) {
            return false;
        }
        self.finish_workspace_tab_drag_state(cx);
        window.prevent_default();
        cx.stop_propagation();
        let text = self.sidebar_drag_session_mentions(drag);
        // The pane takes the keyboard the way a click on its chat box gives it.
        self.focus_agents_pane(pane_id, cx);
        chat.update(cx, |chat, cx| match text {
            Some(text) => chat.insert_composer_drop_text(&text, window, cx),
            None => {
                chat.set_composer_drop_hover(false, cx);
            }
        });
        cx.notify();
        true
    }

    /// Takes the ring off every chat box when a drag ends, wherever it was released.
    pub(crate) fn clear_composer_drop_hover(&mut self, cx: &mut gpui::Context<Self>) {
        for chat in self.native_chat_views.values() {
            chat.update(cx, |chat, cx| {
                chat.set_composer_drop_hover(false, cx);
            });
        }
    }
}
