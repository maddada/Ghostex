//! The Focus Sidebar hotkey: the keyboard moves to the sidebar, onto the active session's row, and
//! Escape or the same hotkey gives it back to where it was. Also the desktop's answers to the
//! sidebar keyboard's host questions (native_sidebar/keyboard.rs).
//!
//! CDXC:Hotkeys 2026-11-11 WHY:
//! A terminal consumes Tab, so a keyboard user in one could not reach the sidebar (GitHub #558; the user's decision is on `focusSidebar` in packages/settings-catalog/src/hotkey_definitions.rs). The hotkey lands on a real focus stop of the sidebar (native_sidebar/focus_order.rs), so the arrow keys, Tab, Enter and Escape that follow are the sidebar's one keyboard model and the ring shows where it is; the rows are the ones drawn, in drawn order, in either sidebar. The sidebar is drawn in one window at a time (the main window, or the floating panel while the sidebar is collapsed), and the hotkey works in that one. The keyboard leaves a terminal the way Unfocus Terminal takes it (`unfocus_terminal`) and comes back the way Focus Last Terminal returns it, and anything else gets its own focus handle back.

use gpui::{AnyWindowHandle, Window};

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::keyboard::SidebarKeyboardReturn;

impl GhostexGpuiApp {
    /// The window the sidebar is drawn in: the floating panel while it shows the sidebar, else
    /// the main window unless the sidebar is collapsed.
    fn sidebar_keyboard_window(&self) -> Option<AnyWindowHandle> {
        self.floating_reveal
            .panel
            .as_ref()
            .filter(|panel| panel.content.sidebar)
            .map(|panel| panel.window.into())
            .or(self.main_window_handle.filter(|_| !self.sidebar_collapsed))
    }

    /// Whether `window` draws the sidebar now (native_sidebar/keyboard.rs ends an entry waiting in
    /// a window that no longer does).
    pub(crate) fn sidebar_keyboard_draws_sidebar_in(&self, window: AnyWindowHandle) -> bool {
        self.sidebar_keyboard_window() == Some(window)
    }

    /// The Focus Sidebar hotkey, pressed in `window`: in, or back out when the sidebar already
    /// has the keyboard.
    pub(crate) fn toggle_sidebar_keyboard_focus(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let here = window.window_handle();
        // The sidebar has the keyboard: one of its controls has focus here, the entry still waits
        // for its row, or the sidebar is in the other window and its last frame ended focused.
        let active = self.native_sidebar.focus_stops.entry_pending()
            || self.native_sidebar_stop_focused(window)
            || self.native_sidebar_focused_window().is_some_and(|focused| {
                focused != here && Some(focused) == self.sidebar_keyboard_window()
            });
        if active {
            self.sidebar_keyboard_give_back(window, cx);
            return;
        }
        let panel_shows_sidebar = self
            .floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| panel.content.sidebar);
        if self.sidebar_collapsed && !panel_shows_sidebar {
            self.toggle_sidebar_from_user(cx);
        }
        let Some(owner) = self.sidebar_keyboard_window() else {
            return;
        };
        let from_terminal =
            Some(here) == self.main_window_handle && self.unfocus_terminal(window, cx);
        let record = if from_terminal {
            Some(SidebarKeyboardReturn::Terminal)
        } else {
            window
                .focused(cx)
                .filter(|focused| {
                    *focused != self.root_action_focus_handle
                        && !self.native_sidebar.focus_stops.is_stop(focused)
                })
                .map(|handle| SidebarKeyboardReturn::Focus {
                    handle,
                    window: here,
                })
        };
        self.native_sidebar.keyboard.set_return(record);
        // The entry holds the keyboard on the sidebar's own handle until its row takes it, so
        // nothing the hotkey took it from (a composer, whose capture listener runs first, or a
        // terminal) sees a key meanwhile, and Escape reaches the sidebar to cancel it.
        let entry = self.native_sidebar.focus_stops.entry_handle(cx);
        self.start_native_sidebar_keyboard_entry(owner, here, Some(Some(entry.clone())), cx);
        cx.notify();
        if owner == here {
            window.focus(&entry, cx);
            window.refresh();
        } else {
            cx.defer(move |cx| {
                let _ = owner.update(cx, |_, window, cx| {
                    window.activate_window();
                    window.focus(&entry, cx);
                    window.refresh();
                });
            });
        }
    }

    /// The surface of the focused pane: where the keyboard goes when nothing else was recorded,
    /// and where Enter on a session row sends it.
    pub(crate) fn sidebar_keyboard_active_surface(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if Some(window.window_handle()) == self.main_window_handle {
            self.focus_default_surface_for_active_mode(cx);
            return;
        }
        // The sidebar was in the floating panel: the panel's sessions column when it shows one,
        // else the main window's surface.
        if self
            .floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| panel.content.agents_column)
        {
            self.schedule_floating_reveal_focus(cx);
            return;
        }
        self.focus_default_surface_for_active_mode(cx);
        if let Some(main) = self.main_window_handle {
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            });
        }
    }

    /// The terminal the hotkey took the keyboard from, as Focus Last Terminal returns it. False
    /// when it has closed since.
    pub(crate) fn sidebar_keyboard_restore_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if Some(window.window_handle()) == self.main_window_handle {
            return self.focus_last_terminal(window, cx);
        }
        let Some(main) = self.main_window_handle else {
            return false;
        };
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                window.activate_window();
                app.update(cx, |app, cx| {
                    if !app.focus_last_terminal(window, cx) {
                        app.focus_default_surface_for_active_mode(cx);
                    }
                });
            });
        });
        true
    }

    /// Whether `session_id`'s row is the one the sidebar draws as focused.
    pub(crate) fn sidebar_keyboard_row_is_active(&self, session_id: &str) -> bool {
        self.gx_store_sidebar_row_focus(session_id, false, false, false)
            .0
    }
}
