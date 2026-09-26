use gpui::Window;

use crate::app::model::*;
use crate::app::view_strip_order::ViewStripEntry;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 WHY:
    /// `active_mode` no longer says which of several workspaces is on screen; it says which view the
    /// right-hand panel shows, and `Agents` says the panel is closed. Everything that used to ask
    /// "am I in Agents mode?" asks one of the three questions below instead, so the difference
    /// between "no view is open" and "the sessions are on screen" is written down exactly once.
    pub(crate) fn open_view_mode(&self) -> Option<TitlebarMode> {
        (self.active_mode != TitlebarMode::Agents).then_some(self.active_mode)
    }

    /// The same question for a mode that is not (yet) the active one.
    pub(crate) fn open_view_mode_for(&self, mode: TitlebarMode) -> Option<TitlebarMode> {
        (mode != TitlebarMode::Agents).then_some(mode)
    }

    pub(crate) fn view_panel_open(&self) -> bool {
        self.active_mode != TitlebarMode::Agents || self.view_panel_picker_open
    }

    /// Whether the panel's content is a native child view (a CEF page, or the Terminal view's
    /// Ghostty terminals). The picker is GPUI's own drawing, so the rules that exist because a
    /// native child paints over everything do not apply to it.
    pub(crate) fn view_panel_shows_native_surface(&self) -> bool {
        self.open_view_mode().is_some() && !self.website_home_setup_visible(self.active_mode)
    }

    /// The panel is open and showing the picker: no tab is selected, so there is no view to draw.
    pub(crate) fn view_picker_open(&self) -> bool {
        self.view_panel_picker_open && self.active_mode == TitlebarMode::Agents
    }

    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screen 09): Expand gives the view the whole workarea and folds the sessions column away
    /// with nothing left behind, and the same button brings it back. That fold is the one thing that
    /// can take the Agents workspace off screen, which is what this predicate exists for: every
    /// terminal, chat and focus gate reads it instead of comparing the active mode, so hiding the
    /// column hides its native child views in the same breath instead of leaving them painted over a
    /// maximised page. It supersedes the phase 3 note that this always returns true.
    /// CDXC:Sidebar 2026-09-20 WHY:
    /// The left-edge reveal is the second answer to the same question: while the panel carries the
    /// sessions column, the column is on screen in a window of its own and every gate that reads
    /// this must say so, or the terminals and chats it just brought back would be reconciled as
    /// hidden the moment they appeared.
    pub(crate) fn agents_workspace_visible(&self) -> bool {
        !self.view_panel_maximized() || self.floating_reveal_hosts_agents_column()
    }

    /// CDXC:Workarea 2026-09-24 DECISION:
    /// User: on the "Open a view" picker, "still allow me to use" Toggle Agents Panel, Expand side
    /// panel and Expand side panel fully. Maximised counts while the panel is open, whether it holds
    /// a view or the picker; this supersedes the 2026-09-20 rule that the picker put the sessions
    /// column back. Closing the panel still does, rather than leaving the window with nothing in it.
    pub(crate) fn view_panel_maximized(&self) -> bool {
        self.view_panel_maximized && self.view_panel_open()
    }

    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User (screen 02): the toggle comes back to the tab this project was last on, and a project
    /// with no tabs gets the picker instead of a view the app chose for it. This supersedes the
    /// earlier rule that the toggle opened "the first view its context offers" when the project had
    /// none: guessing Code for a project that never opened Code is exactly what the picker replaces.
    pub(crate) fn view_panel_toggle_target(&self) -> Option<TitlebarMode> {
        let tabs = self.open_view_tabs();
        tabs.iter()
            .copied()
            .find(|mode| Some(*mode) == self.last_open_view_mode)
            .or_else(|| tabs.first().copied())
    }

    /// The header's view-panel toggle: close the panel when it is open, otherwise come back to this
    /// project's last tab, or open the picker when it has none.
    pub(crate) fn toggle_view_panel(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.view_panel_open() {
            self.close_view_panel(window, cx);
            return;
        }
        match self.view_panel_toggle_target() {
            Some(target) => {
                self.open_view_tab(target, window, cx);
            }
            None => self.open_view_picker(window, cx),
        }
    }

    /// Open the panel onto the picker. The picker takes shell focus so its single-letter shortcuts
    /// reach it rather than the terminal the user was typing in a moment ago.
    pub(crate) fn open_view_picker(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // Marked open before the mode change, so a panel that was expanded stays expanded on the
        // picker instead of being closed and reopened (`set_active_mode`).
        self.view_panel_picker_open = true;
        if self.active_mode != TitlebarMode::Agents {
            self.set_active_mode(TitlebarMode::Agents, window, cx);
        }
        self.focus_view_picker(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// `ProjectEditorSurface(Agents)` is the shell focus of the view panel with no view in it. The
    /// pairing is impossible any other way round — `Agents` never occupies the panel — so it names
    /// the picker exactly, and the keyboard owner hands the keys to the GPUI root for it instead of
    /// looking for a CEF page that does not exist.
    pub(crate) fn focus_view_picker(&mut self, cx: &mut gpui::Context<Self>) {
        self.focus_shell_target(
            ShellFocusTarget::ProjectEditorSurface(TitlebarMode::Agents),
            cx,
        );
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The tab strip is the open-views list filtered by what this project can actually show, not a
    /// second list: a view whose scope the user narrowed, or whose feature this project does not
    /// have, keeps its place in the stored order and comes back the moment it is available again.
    /// Pruning the stored list instead would forget the tab as soon as the user opened the wrong
    /// project once.
    pub(crate) fn open_view_tabs(&self) -> Vec<TitlebarMode> {
        self.open_views
            .iter()
            .copied()
            .filter(|mode| self.titlebar_mode_available(*mode))
            .collect()
    }

    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: the view tabs bar shows no "Browser" tab. The Browser view is represented by its own
    /// page tabs after the view tabs, so a Browser button among the views would be a second tab for the
    /// same thing. Browser stays in the open-views list (the toggle target, the successor on close,
    /// and whether its page tabs are listed all read that list); only the drawn view tabs, their
    /// drag indices and the Option/Alt number hotkeys walk this narrower list.
    pub(crate) fn strip_view_tabs(&self) -> Vec<TitlebarMode> {
        self.view_strip_entries()
            .into_iter()
            .filter_map(|entry| match entry {
                ViewStripEntry::View(mode) => Some(mode),
                ViewStripEntry::Browser(_) => None,
            })
            .collect()
    }

    /// Open a view as a tab and focus it. An already-open view just gets focused, which is what the
    /// `+` menu's checked rows do.
    pub(crate) fn open_view_tab(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if mode == TitlebarMode::Agents {
            return false;
        }
        if self.set_active_mode(mode, window, cx) {
            cx.notify();
            return true;
        }
        false
    }

    /// Fold the open view's list entry in. Every route that changes `active_mode` passes through
    /// `change_active_mode_with_pane_state`, so this is the one place a tab is born.
    ///
    /// CDXC:Workarea 2026-09-24 DECISION:
    /// User: a newly opened view or browser tab opens at the end of the tabs bar. This supersedes
    /// the 2026-09-20 rule that seeded a new view's place from the Settings view order, which put new
    /// views to the left of the browser tabs while new browser tabs went to the end.
    pub(crate) fn record_open_view_tab(&mut self, mode: TitlebarMode) {
        if mode == TitlebarMode::Agents || self.open_views.contains(&mode) {
            return;
        }
        self.open_views.push(mode);
        // Browser has no tab of its own in the strip; its pages are placed as they open.
        if mode != TitlebarMode::Browser {
            self.append_view_strip_tab(ViewStripTabKey::View(mode));
        }
    }

    /// The tab that takes over when `mode` closes: the one to its right, else the one to its left,
    /// else nothing, which brings the picker back.
    fn view_tab_successor(&self, mode: TitlebarMode) -> Option<TitlebarMode> {
        let tabs = self.open_view_tabs();
        let index = tabs.iter().position(|tab| *tab == mode)?;
        tabs.get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|index| tabs.get(index)))
            .copied()
    }

    /// CDXC:Workarea 2026-09-22 DECISION:
    /// User: closing the last tab in the side panel goes back to the "Open a view" picker instead of
    /// closing the panel, so the panel stays where it was with the next thing to open in front of
    /// you. This supersedes the 2026-09-20 rule that a close with no successor closed the panel.
    ///
    /// CDXC:Workarea 2026-09-26 DECISION:
    /// User: "add a setting where closing the last tab in the side panel closes the side panel
    /// fully (disabled by default)". With `closeSidePanelWithLastTab` on, the close with no
    /// successor closes the panel instead of showing the picker; off keeps the 2026-09-22 rule.
    pub(crate) fn close_view_tab(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if mode == TitlebarMode::Agents || !self.open_views.contains(&mode) {
            return;
        }
        let successor = (mode == self.active_mode)
            .then(|| self.view_tab_successor(mode))
            .flatten();
        self.open_views.retain(|tab| *tab != mode);
        // A closed tab is not a sleeping tab: the page it owned has no way back on screen, so it
        // releases its CEF surface here instead of waiting for the idle timer.
        self.sleep_titlebar_view(mode, cx);
        match successor {
            Some(next) => {
                self.set_active_mode(next, window, cx);
            }
            None if mode == self.active_mode
                && crate::shared_settings::shared_sidebar_settings_snapshot()
                    .close_side_panel_with_last_tab() =>
            {
                self.close_view_panel(window, cx)
            }
            None if mode == self.active_mode => self.open_view_picker(window, cx),
            None => {}
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// Closing the panel leaves the tab strip alone: reopening it comes back to the same tabs.
    pub(crate) fn close_view_panel(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view_panel_maximized = false;
        self.view_panel_picker_open = false;
        self.set_active_mode(TitlebarMode::Agents, window, cx);
    }

    /// Expand and restore, the button in the tab strip and the `⋯` row beside it.
    pub(crate) fn toggle_view_panel_maximized(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.view_panel_open() {
            return;
        }
        self.view_panel_maximized = !self.view_panel_maximized;
        // The sessions column just appeared or vanished, so every Ghostty and chat child view that
        // reads `agents_workspace_visible()` has to be reconciled at this boundary, exactly as a
        // mode switch reconciles them.
        self.reconcile_agents_pane_surfaces(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: Expand side panel fully hides the sidebar too if it is visible, not just the sessions
    /// column. The same control brings the sessions column back, and the sidebar with it.
    pub(crate) fn view_panel_fully_expanded(&self) -> bool {
        self.view_panel_maximized() && self.sidebar_collapsed
    }

    pub(crate) fn toggle_view_panel_fully_expanded(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.view_panel_open() {
            return;
        }
        let expand = !self.view_panel_fully_expanded();
        if self.sidebar_collapsed != expand {
            self.toggle_gpui_sidebar_collapsed(cx);
        }
        if self.view_panel_maximized != expand {
            self.toggle_view_panel_maximized(cx);
        }
    }
}
