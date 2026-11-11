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
    /// Claude takes the sessions column's place while it is on (app/claude_web/takeover.rs).
    pub(crate) fn agents_workspace_visible(&self) -> bool {
        (!self.view_panel_maximized() && !self.claude_on())
            || self.floating_reveal_hosts_agents_column()
    }

    /// CDXC:Workarea 2026-09-24 DECISION:
    /// User: on the "Open a view" picker, "still allow me to use" Toggle Agents Panel (a button the
    /// user removed on 2026-11-10; Expand side panel does the same), Expand side
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
            self.close_view_panel(cx);
            return;
        }
        match self.view_panel_toggle_target() {
            Some(target) => {
                self.open_view_tab(target, window, cx);
            }
            None => self.open_view_picker(cx),
        }
    }

    /// Open the panel onto the picker. The picker takes shell focus so its single-letter shortcuts
    /// reach it rather than the terminal the user was typing in a moment ago.
    pub(crate) fn open_view_picker(&mut self, cx: &mut gpui::Context<Self>) {
        self.pending_project_view_restore = None;
        // Marked open before the mode change, so a panel that was expanded stays expanded on the
        // picker instead of being closed and reopened (`set_active_mode`).
        self.view_panel_picker_open = true;
        if self.active_mode != TitlebarMode::Agents {
            self.activate_mode(TitlebarMode::Agents, cx);
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

    /// CDXC:Workarea 2026-10-01 WHY:
    /// Each view focused itself from a bubbling mouse-down on its host, so a press on anything inside that stops propagation (a Kanban card, a Docs editor, a button) left the Agents pane focused, and session hotkeys such as Cmd+W then acted on the session instead of the view. The panel claims focus in the capture phase for every press. Browser and the Terminal view keep their own per-pane focus.
    pub(crate) fn focus_view_panel_on_press(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(mode, TitlebarMode::Browser | TitlebarMode::Terminal)
            || mode != self.active_mode
            || self.shell_focus == ShellFocusTarget::ProjectEditorSurface(mode)
        {
            return;
        }
        self.focus_project_editor_surface(mode, window, cx);
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
    /// same thing. Browser stays in the open-views list (the toggle target and whether its page tabs
    /// are listed read that list); only the drawn view tabs, their drag indices and the Option/Alt
    /// number hotkeys walk this narrower list.
    /// With `browserPagesInViewRow` on (off by default) the Browser is a view tab here too and its
    /// pages are in their own row (`render/browser_page_tabs.rs`).
    pub(crate) fn strip_view_tabs(&self) -> Vec<TitlebarMode> {
        self.view_strip_entries()
            .into_iter()
            .filter_map(|entry| match entry {
                ViewStripEntry::View(mode) => Some(mode),
                ViewStripEntry::Browser(_) | ViewStripEntry::SideChat(_) => None,
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
        // Browser has a tab of its own in the strip only while its pages have their own row;
        // otherwise its pages are placed as they open. Side chats are always their own tabs.
        if mode == TitlebarMode::SideChat {
            return;
        }
        if mode != TitlebarMode::Browser || self.browser_pages_in_view_row {
            self.append_view_strip_tab(ViewStripTabKey::View(mode));
        }
    }

    /// The view that takes over when `mode` leaves the panel: the nearest one to its right that this
    /// project can show, else the nearest to its left, else nothing. `mode` itself may be a view
    /// this project can no longer show.
    ///
    /// CDXC:Workarea 2026-10-26 WHY:
    /// Hiding a view and restoring an unavailable saved view use `leaving_view_successor`; this mode-only fallback handles a leaving view with no drawn anchor. It reads drawn view order because only a drag rewrites `open_views`, so pinning and unpinning can leave that list in a different order.
    pub(crate) fn view_tab_successor(&self, mode: TitlebarMode) -> Option<TitlebarMode> {
        let tabs = self.open_views_in_view_strip_order();
        let index = tabs.iter().position(|tab| *tab == mode)?;
        let (left, right) = tabs.split_at(index);
        right[1..]
            .iter()
            .chain(left.iter().rev())
            .copied()
            .find(|tab| self.titlebar_mode_available(*tab))
    }

    /// Shows `successor` in place of the view leaving the panel or, with none, follows the last-tab
    /// rule of `close_view_tab`. A Browser successor's address field is left to the caller
    /// (`sync_browser_address_input_if_shown`), the one step that needs a `Window`.
    pub(crate) fn show_view_tab_successor(
        &mut self,
        successor: Option<TitlebarMode>,
        cx: &mut gpui::Context<Self>,
    ) {
        match successor {
            Some(next) => {
                self.activate_mode(next, cx);
            }
            None if crate::shared_settings::shared_sidebar_settings_snapshot()
                .close_side_panel_with_last_tab() =>
            {
                self.close_view_panel(cx)
            }
            None => self.open_view_picker(cx),
        }
    }

    /// Shows `successor` (`leaving_view_successor`) in place of a view that left the panel without a
    /// `Window`, or follows the last-tab rule with none. A page is selected in its pane before the
    /// Browser is shown, so the Browser comes back on it instead of first waking the page it was last
    /// on, and its pane takes the focus as a click on the page gives it. `asleep` shows the successor
    /// the way a launch restores a view (`show_view_tab_asleep`). A Browser successor's address field
    /// is left to the caller, as in `show_view_tab_successor`.
    pub(crate) fn show_leaving_view_successor(
        &mut self,
        successor: Option<ViewStripEntry>,
        asleep: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let (mode, pane_id) = match successor {
            Some(ViewStripEntry::View(mode)) => (mode, None),
            Some(ViewStripEntry::SideChat(tab_id)) => {
                self.view_strip_layout.active_side_chat = Some(tab_id);
                (TitlebarMode::SideChat, None)
            }
            Some(ViewStripEntry::Browser(tab)) => {
                // The page's pane is looked up again, as `show_view_strip_entry` does.
                let Some(pane_id) =
                    find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab.tab_id)
                else {
                    return self.show_view_tab_successor(None, cx);
                };
                self.browser_tabs.select_tab_in_pane(pane_id, tab.tab_id);
                (TitlebarMode::Browser, Some(pane_id))
            }
            None => return self.show_view_tab_successor(None, cx),
        };
        if asleep {
            self.show_view_tab_asleep(mode, cx);
        } else if self.activate_mode(mode, cx)
            && let Some(pane_id) = pane_id
        {
            self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
            self.persist_shell_layout_state();
            cx.notify();
        }
    }

    /// Shows `mode` the way a launch restores the view on screen: its tab and pane state move, and
    /// its page keeps sleeping until clicked (CDXC:Browser 2026-09-19), where `activate_mode` would
    /// wake it. The built-in views are asleep since `sleep_all_modes_for_launch`; a custom view has
    /// no lifecycle until it is opened and reads as awake without one, so it is recorded asleep, as
    /// that function does for the view a launch restores.
    pub(crate) fn show_view_tab_asleep(
        &mut self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(mode, TitlebarMode::Extension(_)) {
            self.project_editor_shell.mark_mode_sleeping(mode);
        }
        self.change_active_mode_with_pane_state(mode, cx);
        self.seed_terminal_view_for_open(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
    }

    /// CDXC:Workarea 2026-09-22 DECISION:
    /// User: closing the last tab in the side panel goes back to the "Open a view" picker instead of
    /// closing the panel, so the panel stays where it was with the next thing to open in front of
    /// you. This supersedes the 2026-09-20 rule that a close with no successor closed the panel.
    ///
    /// CDXC:Workarea 2026-09-27 DECISION:
    /// User: closing the last tab in the side panel closes the side panel fully, as an advanced
    /// setting "enabled by default" (supersedes the 2026-09-26 off-by-default setting). With
    /// `closeSidePanelWithLastTab` on, the close with no successor closes the panel instead of
    /// showing the picker; off keeps the 2026-09-22 rule.
    pub(crate) fn close_view_tab(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if mode == TitlebarMode::Agents || !self.open_views.contains(&mode) {
            return;
        }
        if mode == TitlebarMode::SideChat {
            return self.close_current_side_chat_tab(window, cx);
        }
        let shown = mode == self.active_mode;
        // The Browser view takes all of its pages with it; the one on screen is where the search
        // for the next tab starts.
        let successor = self
            .view_strip_showing_key()
            .filter(|_| shown)
            .and_then(|key| {
                self.view_strip_successor(&self.view_strip_entries(), key, |other| {
                    mode == TitlebarMode::Browser && matches!(other, ViewStripTabKey::Browser(_))
                })
            });
        self.open_views.retain(|tab| *tab != mode);
        // Closing the Code tab closes its windows too; the Sleep below releases their pages.
        if mode == TitlebarMode::Source {
            self.code_windows = CodeWindows::default();
        }
        // A closed tab is not a sleeping tab: the page it owned has no way back on screen, so it
        // releases its CEF surface here instead of waiting for the idle timer.
        self.sleep_titlebar_view(mode, cx);
        if shown {
            match successor {
                Some(next) => self.show_view_strip_entry(next, window, cx),
                None => self.show_view_tab_successor(None, cx),
            }
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// Closing the panel leaves the tab strip alone: reopening it comes back to the same tabs.
    pub(crate) fn close_view_panel(&mut self, cx: &mut gpui::Context<Self>) {
        self.pending_project_view_restore = None;
        self.view_panel_maximized = false;
        self.view_panel_picker_open = false;
        self.activate_mode(TitlebarMode::Agents, cx);
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
