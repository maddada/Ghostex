//! The Focus Sidebar hotkey: the keyboard moves to the sidebar's session list, the arrow keys
//! move a cursor over its rows and Enter opens one, and Escape or the same hotkey gives the
//! keyboard back to where it was.
//!
//! CDXC:Hotkeys 2026-11-11 WHY:
//! A terminal consumes Tab, so a keyboard user in one could not reach the sidebar (GitHub #558; the user's decision is on `focusSidebar` in packages/settings-catalog/src/hotkey_definitions.rs). The rows are the ones the list draws, in the order the session slot hotkeys count them (gx-core `rendered_session_ids`), the cursor is drawn as the row's hover (`hovered_session`, which every sidebar layout fills), and Enter is a row click with its reveal (`gx_store_focus_and_reveal_slot_row`), so it opens a session exactly as cmd+1 to cmd+9 do. The keyboard leaves a terminal the way Unfocus Terminal takes it (`unfocus_terminal`) and comes back the way Focus Last Terminal returns it, and anything else gets its own focus handle back.

use gpui::Window;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The handle the session list takes the keyboard on, made on first use.
    pub(crate) fn sidebar_keyboard_focus_handle(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::FocusHandle {
        self.native_sidebar
            .keyboard_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Whether the session list has the keyboard now.
    fn sidebar_keyboard_active(&self, window: &Window) -> bool {
        self.native_sidebar.keyboard_cursor.is_some()
            && self
                .native_sidebar
                .keyboard_focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
    }

    /// The Focus Sidebar hotkey: in, or back out when the list already has the keyboard.
    pub(crate) fn toggle_sidebar_keyboard_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.sidebar_keyboard_active(window) {
            self.leave_sidebar_keyboard(true, window, cx);
            return;
        }
        if self.sidebar_collapsed {
            self.toggle_sidebar_from_user(cx);
        }
        let rows = self.sidebar_keyboard_rows();
        if rows.is_empty() {
            return;
        }
        let handle = self.sidebar_keyboard_focus_handle(cx);
        let from_terminal = self.unfocus_terminal(window, cx);
        self.native_sidebar.keyboard_return_terminal = from_terminal;
        self.native_sidebar.keyboard_return_focus = if from_terminal {
            None
        } else {
            window.focused(cx).filter(|focused| *focused != handle)
        };
        let cursor = rows
            .iter()
            .find(|row| self.gx_store_sidebar_row_focus(row, false, false, false).0)
            .unwrap_or(&rows[0])
            .clone();
        window.focus(&handle, cx);
        self.move_sidebar_keyboard_cursor(cursor);
        cx.notify();
    }

    /// The keys the session list takes while it has the keyboard.
    pub(crate) fn sidebar_keyboard_key_down(
        app: &mut Self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(cursor) = app.native_sidebar.keyboard_cursor.clone() else {
            return;
        };
        let modifiers = &event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let rows = app.sidebar_keyboard_rows();
        let at = rows.iter().position(|row| *row == cursor);
        let next = match event.keystroke.key.as_str() {
            "escape" => {
                app.leave_sidebar_keyboard(true, window, cx);
                None
            }
            "enter" => {
                if at.is_some() {
                    app.leave_sidebar_keyboard(false, window, cx);
                    app.gx_store_focus_and_reveal_slot_row(&cursor, true, cx);
                    app.focus_default_surface_for_active_mode(cx);
                }
                None
            }
            "down" => Some(at.map_or(0, |at| (at + 1).min(rows.len().saturating_sub(1)))),
            "up" => Some(at.map_or(0, |at| at.saturating_sub(1))),
            "home" => Some(0),
            "end" => Some(rows.len().saturating_sub(1)),
            _ => return,
        };
        if let Some(row) = next.and_then(|index| rows.get(index)).cloned() {
            app.move_sidebar_keyboard_cursor(row);
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    /// The rows the arrows walk: every row the list draws, in its order.
    fn sidebar_keyboard_rows(&self) -> Vec<String> {
        if !self.gx_store_sidebar_list_ready() {
            return Vec::new();
        }
        ghostex_gx_core::rendered_session_ids(self.gx_store.sidebar_list.view())
            .cloned()
            .collect()
    }

    fn move_sidebar_keyboard_cursor(&mut self, row: String) {
        self.native_sidebar.hovered_session = Some(row.clone());
        self.gx_store_reveal_walk_row(&row);
        self.native_sidebar.keyboard_cursor = Some(row);
    }

    /// Ends the keyboard mode; `give_back` returns the keyboard to where it was.
    fn leave_sidebar_keyboard(
        &mut self,
        give_back: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(cursor) = self.native_sidebar.keyboard_cursor.take()
            && self.native_sidebar.hovered_session.as_deref() == Some(cursor.as_str())
        {
            self.native_sidebar.hovered_session = None;
        }
        let terminal = std::mem::take(&mut self.native_sidebar.keyboard_return_terminal);
        let focus = self.native_sidebar.keyboard_return_focus.take();
        if !give_back {
            return;
        }
        if terminal && self.focus_last_terminal(window, cx) {
            return;
        }
        match focus {
            Some(focus) => window.focus(&focus, cx),
            None => window.focus(&self.root_action_focus_handle, cx),
        }
    }
}
