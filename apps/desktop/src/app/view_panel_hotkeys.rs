//! Session hotkeys that act on the focused side-panel view instead.

use gpui::Window;

use crate::app::model::*;
use crate::app::view_strip_order::ViewStripEntry;
use crate::*;

impl GhostexGpuiApp {
    /// The view the side panel shows, when the keyboard is in it. Browser pages and the Terminal
    /// view keep their own pane focus, so they never answer here.
    ///
    /// CDXC:Hotkeys 2026-10-01 DECISION:
    /// User: with a view in the side panel focused, session hotkeys act on that view. Close Focused Session closes it like Cmd+W, Previous/Next Tab in Pane walks the side panel's tabs, Sleep Focused Session and Reload Session sleep or reload the view, Find opens Docs' find or Kanban's search, and Rename renames the open Docs file.
    pub(crate) fn focused_view_mode(&self) -> Option<TitlebarMode> {
        let ShellFocusTarget::ProjectEditorSurface(mode) = self.shell_focus else {
            return None;
        };
        (mode == self.active_mode && self.open_view_mode().is_some()).then_some(mode)
    }

    /// Previous/Next Tab in Pane with a view focused: one step along the drawn tab strip, views and
    /// browser pages alike, wrapping at the ends.
    pub(crate) fn cycle_focused_view_strip_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(mode) = self.focused_view_mode() else {
            return false;
        };
        let entries = self.view_strip_entries();
        // Side chats are one mode drawn as many tabs: the one on screen is where the step starts.
        let current = match mode {
            TitlebarMode::SideChat => self.side_chat_tab_on_screen().map(ViewStripEntry::SideChat),
            mode => Some(ViewStripEntry::View(mode)),
        };
        let Some(current) = entries.iter().position(|entry| Some(*entry) == current) else {
            return false;
        };
        if entries.len() < 2 {
            return false;
        }
        let next = if reverse {
            (current + entries.len() - 1) % entries.len()
        } else {
            (current + 1) % entries.len()
        };
        self.show_view_strip_entry(entries[next], window, cx);
        true
    }

    pub(crate) fn sleep_focused_view(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(mode) = self.focused_view_mode() else {
            return false;
        };
        self.sleep_titlebar_view(mode, cx);
        cx.notify();
        true
    }

    pub(crate) fn reload_focused_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(mode) = self.focused_view_mode() else {
            return false;
        };
        self.reload_titlebar_view(mode, window, cx);
        true
    }

    /// Find with Docs or Kanban focused, for presses that reach the window rather than the view's
    /// own key handler (a click on the view's chrome leaves no GPUI focus inside it).
    pub(crate) fn find_in_focused_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match self.focused_view_mode() {
            Some(TitlebarMode::Manage) if self.native_docs.focus.is_some() => {
                self.native_docs_find_shortcut(window, cx);
                true
            }
            // A board out in its own window keeps its search box there (native_kanban/window.rs).
            Some(TitlebarMode::Kanban) if !self.native_kanban_detached() => {
                let Some(search) = self.native_kanban.search.clone() else {
                    return false;
                };
                search.update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn rename_in_focused_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.focused_view_mode() != Some(TitlebarMode::Manage) {
            return false;
        }
        let Some(path) = self.native_docs.active.clone() else {
            return false;
        };
        self.native_docs_open_rename(&path, window, cx);
        true
    }
}
