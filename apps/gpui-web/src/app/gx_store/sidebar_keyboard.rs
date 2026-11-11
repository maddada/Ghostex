//! The web build's answers to the sidebar keyboard's host questions (native_sidebar/keyboard.rs).
//! The page runs no hotkeys, so the keyboard reaches its sidebar by Tab or a click only, and
//! leaves it for the session the work area shows.

use gpui::Window;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The open session's surface: its terminal while the page shows it, else its chat composer.
    pub(crate) fn sidebar_keyboard_active_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = self.open_session.clone() else {
            return;
        };
        if self.web_shows_terminal(&session) {
            if let Some(terminal) = self.terminals.get(&session) {
                gpui::Focusable::focus_handle(terminal.read(cx), cx).focus(window, cx);
            }
            return;
        }
        if let Some((_, chat)) = self.native_chats.get(&session).cloned() {
            chat.update(cx, |chat, cx| chat.focus_composer_from_hotkey(window, cx));
        }
    }

    /// The page records no terminal to go back to.
    pub(crate) fn sidebar_keyboard_restore_terminal(
        &mut self,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> bool {
        false
    }

    /// The page has one window, which draws the sidebar unless it is hidden.
    pub(crate) fn sidebar_keyboard_draws_sidebar_in(&self, _window: gpui::AnyWindowHandle) -> bool {
        !self.sidebar_collapsed
    }

    /// Whether `session_id` is the session the work area shows.
    pub(crate) fn sidebar_keyboard_row_is_active(&self, session_id: &str) -> bool {
        self.open_session
            .as_ref()
            .is_some_and(|session| session.session_id == session_id)
    }
}
