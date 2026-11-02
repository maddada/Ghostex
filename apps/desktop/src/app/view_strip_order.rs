//! The view panel's strip as one ordered row: which tab is drawn where, which are pinned, and what
//! a drag along the row does.

use gpui::Window;

use crate::app::model::*;
use crate::app::render::view_tab_strip_browser_tabs::ViewStripBrowserTab;
use crate::*;

/// One drawn tab of the strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewStripEntry {
    View(TitlebarMode),
    Browser(ViewStripBrowserTab),
    SideChat(SideChatTabId),
}

impl ViewStripEntry {
    pub(crate) fn key(self) -> ViewStripTabKey {
        match self {
            Self::View(mode) => ViewStripTabKey::View(mode),
            Self::Browser(tab) => ViewStripTabKey::Browser(tab.tab_id),
            Self::SideChat(tab_id) => ViewStripTabKey::SideChat(tab_id),
        }
    }
}

impl GhostexGpuiApp {
    /// The strip in drawn order: pinned tabs first, then the rest, each group in the order the
    /// user dragged it into. A tab the stored order has not seen yet is drawn at the end. While the
    /// Browser's pages have a row of their own (`render/browser_page_tabs.rs`) the strip holds the
    /// Browser tab instead of them, standing where its first page stood until it is dragged.
    pub(crate) fn view_strip_entries(&self) -> Vec<ViewStripEntry> {
        self.view_strip_entries_keeping(None)
    }

    /// `view_strip_entries` with `leaving`, the view that just left the panel, still drawn at its
    /// stored slot although this project can no longer show it. A Browser that left the strip keeps
    /// only the page that was on screen: its other pages leave with it, so they cannot change where
    /// the remaining tabs stand around that page.
    fn view_strip_entries_keeping(&self, leaving: Option<TitlebarMode>) -> Vec<ViewStripEntry> {
        let pages_in_row = self.browser_pages_in_view_row;
        let views = self
            .open_views
            .iter()
            .copied()
            .filter(|mode| Some(*mode) == leaving || self.titlebar_mode_available(*mode))
            .filter(|mode| pages_in_row || *mode != TitlebarMode::Browser)
            // Side chats are drawn as their own tabs, never as one `SideChat` view tab.
            .filter(|mode| *mode != TitlebarMode::SideChat)
            .map(ViewStripEntry::View);
        let side_chats = self
            .view_strip_layout
            .side_chat_tabs()
            .map(ViewStripEntry::SideChat)
            .collect::<Vec<_>>();
        let pages = if pages_in_row {
            Vec::new()
        } else if leaving == Some(TitlebarMode::Browser)
            && !self.titlebar_mode_available(TitlebarMode::Browser)
        {
            self.leaving_browser_anchor().into_iter().collect()
        } else {
            self.view_strip_browser_tabs()
        };
        self.in_view_strip_order(
            views
                .chain(pages.into_iter().map(ViewStripEntry::Browser))
                .chain(side_chats)
                .collect(),
            ViewStripEntry::key,
        )
    }

    /// The focused pane's page of a Browser that left the panel, when the strip would draw it: a
    /// loaded page, or any page while the Browser was the view on screen
    /// (`view_strip_browser_tabs`).
    fn leaving_browser_anchor(&self) -> Option<ViewStripBrowserTab> {
        let pane_id = self.browser_tabs.focused_pane;
        let tab_id = self.browser_tabs.active_tab_id_for_pane(pane_id)?;
        let tab = self.browser_tabs.tab(tab_id)?;
        (self.active_mode == TitlebarMode::Browser || tab.state == BrowserTabState::Loaded)
            .then_some(ViewStripBrowserTab { pane_id, tab_id })
    }

    /// `available` in the strip's drawn order (`GpuiViewStripLayout::in_drawn_order`), the Browser
    /// tab standing in for its pages while they have a row of their own.
    pub(crate) fn in_view_strip_order<T: Copy>(
        &self,
        available: Vec<T>,
        key: impl Fn(T) -> ViewStripTabKey,
    ) -> Vec<T> {
        self.view_strip_layout
            .in_drawn_order(available, self.browser_pages_in_view_row, |entry| {
                key(*entry)
            })
    }

    /// The strip's tab on screen: the open view, or while the Browser is the open view its page on
    /// screen, or its own tab when the pages have a row of their own. `None` while the panel is
    /// closed or on the picker.
    pub(crate) fn view_strip_showing_key(&self) -> Option<ViewStripTabKey> {
        match self.open_view_mode()? {
            TitlebarMode::Browser if !self.browser_pages_in_view_row => {
                self.browser_page_on_screen().map(ViewStripTabKey::Browser)
            }
            TitlebarMode::SideChat => self
                .side_chat_tab_on_screen()
                .map(ViewStripTabKey::SideChat),
            mode => Some(ViewStripTabKey::View(mode)),
        }
    }

    /// The focused pane's page while the Browser is the open view.
    pub(crate) fn browser_page_on_screen(&self) -> Option<BrowserTabId> {
        (self.active_mode == TitlebarMode::Browser)
            .then(|| {
                self.browser_tabs
                    .active_tab_id_for_pane(self.browser_tabs.focused_pane)
            })
            .flatten()
    }

    /// The row the Browser's pages are drawn in, in drawn order: the strip, with the views, or the
    /// pages' own row while they have one (`browserPagesInViewRow`).
    pub(crate) fn browser_pages_drawn_row(&self) -> Vec<ViewStripEntry> {
        if self.browser_pages_in_view_row {
            self.browser_page_row()
                .into_iter()
                .map(ViewStripEntry::Browser)
                .collect()
        } else {
            self.view_strip_entries()
        }
    }

    /// CDXC:Workarea 2026-10-26 WHY:
    /// The strip draws views and pages in an order of its own (`view_strip_layout`), while the open-views list keeps one slot for the Browser and each Browser pane keeps its own tab order. Closing the tab on screen picked its successor from those lists, so once they had parted from the strip (a drag reorders only the strip), × selected a tab nowhere near the closed one (an image page opened from a pull request page jumped to the first tab). The successor is read from `entries`, the row `shown` is drawn in (the strip, or the Browser's page row while its pages have one): the page that opened `shown` while that still holds (`browser_tab_opener`), else the nearest tab to its right that stays, else the nearest to its left. `leaving` names the tabs that close together with `shown`.
    pub(crate) fn view_strip_successor(
        &self,
        entries: &[ViewStripEntry],
        shown: ViewStripTabKey,
        leaving: impl Fn(ViewStripTabKey) -> bool,
    ) -> Option<ViewStripEntry> {
        let stays = |entry: &&ViewStripEntry| entry.key() != shown && !leaving(entry.key());
        let opener = self
            .browser_tab_opener
            .filter(|opener| ViewStripTabKey::Browser(opener.tab) == shown)
            .and_then(|opener| {
                entries
                    .iter()
                    .find(|entry| entry.key() == ViewStripTabKey::Browser(opener.opener))
            })
            .filter(stays);
        if let Some(opener) = opener {
            return Some(*opener);
        }
        let index = entries.iter().position(|entry| entry.key() == shown)?;
        let (left, right) = entries.split_at(index);
        right.iter().chain(left.iter().rev()).find(stays).copied()
    }

    /// CDXC:Workarea 2026-10-26 WHY:
    /// A view that leaves the panel without its tab being closed (hidden here, turned off, or a restored project's saved view it can no longer show) is gone from the strip by the time the panel moves on, so the mode-only `view_tab_successor` answered "Browser" for a page drawn beside it, and the Browser showed the page it was last on: with Issue, Files and Pull request in the strip, × on Files showed Pull request but hiding Files did not. The strip is read again with `mode` at its stored slot and its neighbour chosen as × chooses it (`view_strip_successor`), a page included. A Browser that left is anchored at its focused pane's page, and all its pages leave with it. Only a leaving view with no tab of its own in the strip (a Browser with none of its pages drawn) falls back to `view_tab_successor`.
    pub(crate) fn leaving_view_successor(&self, mode: TitlebarMode) -> Option<ViewStripEntry> {
        let entries = self.view_strip_entries_keeping(Some(mode));
        let anchor = if mode == TitlebarMode::Browser && !self.browser_pages_in_view_row {
            self.browser_tabs
                .active_tab_id_for_pane(self.browser_tabs.focused_pane)
                .map(ViewStripTabKey::Browser)
        } else {
            Some(ViewStripTabKey::View(mode))
        };
        match anchor.filter(|anchor| entries.iter().any(|entry| entry.key() == *anchor)) {
            Some(anchor) => self.view_strip_successor(&entries, anchor, |other| {
                mode == TitlebarMode::Browser && matches!(other, ViewStripTabKey::Browser(_))
            }),
            None => self.view_tab_successor(mode).map(ViewStripEntry::View),
        }
    }

    /// Puts a tab of the strip on screen the way a click on it does.
    pub(crate) fn show_view_strip_entry(
        &mut self,
        entry: ViewStripEntry,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match entry {
            ViewStripEntry::View(mode) => {
                self.open_view_tab(mode, window, cx);
            }
            // The page's pane is looked up again: closing a pane's last tab folds that pane away.
            ViewStripEntry::Browser(tab) => {
                if let Some(pane_id) =
                    find_browser_leaf_id_for_tab(&self.browser_tabs.root, tab.tab_id)
                {
                    self.open_browser_tab_from_view_strip(pane_id, tab.tab_id, window, cx);
                }
            }
            ViewStripEntry::SideChat(tab_id) => self.show_side_chat_tab(tab_id, window, cx),
        }
    }

    pub(crate) fn view_strip_tab_pinned(&self, key: ViewStripTabKey) -> bool {
        self.view_strip_layout.pinned.contains(&key)
    }

    pub(crate) fn view_strip_tab_position(&self, key: ViewStripTabKey) -> Option<usize> {
        self.view_strip_entries()
            .iter()
            .position(|entry| entry.key() == key)
    }

    /// Pinning sends the tab to the end of the pinned tabs, those not drawn right now included;
    /// unpinning puts it at the head of the unpinned tabs. A page in the Browser's own row lands
    /// there within the row, which draws the pages in the same stored order. Moving the tab shifts
    /// the slots of the undrawn tabs between its old and new place by one.
    pub(crate) fn toggle_view_strip_tab_pinned(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        // Store the drawn order first, so the tab moves among its neighbours as they are drawn
        // rather than as an older stored order had them.
        let drawn = self.drawn_view_strip_keys(key);
        self.write_view_strip_projection(&drawn);
        let layout = &mut self.view_strip_layout;
        layout.order.retain(|existing| *existing != key);
        let place = if layout.pinned.contains(&key) {
            layout.pinned.retain(|pinned| *pinned != key);
            layout
                .order
                .iter()
                .position(|existing| !layout.pinned.contains(existing))
                .unwrap_or(layout.order.len())
        } else {
            layout.pinned.push(key);
            layout
                .order
                .iter()
                .rposition(|existing| layout.pinned.contains(existing))
                .map_or(0, |last_pinned| last_pinned + 1)
        };
        layout.order.insert(place, key);
        self.persist_shell_layout_state();
        cx.notify();
    }

    /// A tab that just opened goes last. The drawn order is stored first, so every tab already on
    /// screen, including one the stored order had not seen yet, keeps its place ahead of it.
    pub(crate) fn append_view_strip_tab(&mut self, key: ViewStripTabKey) {
        let drawn = self.drawn_view_strip_keys(key);
        self.write_view_strip_projection(&drawn);
        let order = &mut self.view_strip_layout.order;
        order.retain(|existing| *existing != key);
        order.push(key);
    }

    /// The keys of the row `key` is drawn in, in drawn order: the Browser's page row for a page
    /// while the pages have a row of their own, the strip otherwise. Before reading the strip, the
    /// Browser tab gets a stored place of its own where it is drawn.
    fn drawn_view_strip_keys(&mut self, key: ViewStripTabKey) -> Vec<ViewStripTabKey> {
        if self.browser_pages_in_view_row && matches!(key, ViewStripTabKey::Browser(_)) {
            return self
                .browser_page_row()
                .into_iter()
                .map(|page| ViewStripTabKey::Browser(page.tab_id))
                .collect();
        }
        let browser_tab = ViewStripTabKey::View(TitlebarMode::Browser);
        let order = &mut self.view_strip_layout.order;
        if self.browser_pages_in_view_row && !order.contains(&browser_tab) {
            let first_page = order
                .iter()
                .position(|stored| matches!(stored, ViewStripTabKey::Browser(_)))
                .unwrap_or(order.len());
            order.insert(first_page, browser_tab);
        }
        self.view_strip_entries()
            .into_iter()
            .map(ViewStripEntry::key)
            .collect()
    }

    fn pinned_view_strip_tab_count(&self, keys: &[ViewStripTabKey]) -> usize {
        keys.iter()
            .filter(|key| self.view_strip_tab_pinned(**key))
            .count()
    }

    /// `GpuiViewStripLayout::write_projection` for the project on screen.
    fn write_view_strip_projection(&mut self, drawn: &[ViewStripTabKey]) {
        let open_views = &self.open_views;
        let browser_tabs = &self.browser_tabs;
        self.view_strip_layout
            .write_projection(drawn, |key| match key {
                ViewStripTabKey::View(mode) => open_views.contains(mode),
                ViewStripTabKey::Browser(tab_id) => browser_tabs.tab(*tab_id).is_some(),
                // The order is the list of open side chats, so its keys stay until closed.
                ViewStripTabKey::SideChat(_) => true,
            });
    }

    pub(crate) fn set_view_strip_drop_index(
        &mut self,
        index: Option<usize>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.view_strip_drop_index != index {
            self.view_strip_drop_index = index;
            cx.notify();
        }
    }

    /// A dragged tab is over the tab drawn at `tab_index`: the marker goes before or after it,
    /// whichever half the pointer is in.
    pub(crate) fn update_view_strip_drop_feedback(
        &mut self,
        bounds: gpui::Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        if !bounds.contains(&position) {
            return;
        }
        let insertion_index = workspace_tab_insertion_index(bounds, position, tab_index);
        self.set_view_strip_drop_index(Some(insertion_index), cx);
    }

    /// CDXC:Workarea 2026-09-22 WHY:
    /// The drop line is drawn in the gap between two tabs, and the strip is taller than its tabs, so a release aimed at the line is often over no tab at all and no tab's drop ran while the line was still showing. The strip itself takes those releases and lands the tab where the line is; a tab under the pointer still answers first. The line goes away once the pointer leaves the strip, so a release elsewhere reorders nothing.
    pub(crate) fn handle_view_strip_gap_drop(
        &mut self,
        key: ViewStripTabKey,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.view_strip_drop_index {
            Some(insertion_index) => self.handle_view_strip_drop(key, insertion_index, cx),
            // Taking the drop keeps the release from the window root, which is what ends a browser tab's drag.
            None => self.finish_browser_tab_drag(cx),
        }
    }

    pub(crate) fn clear_view_strip_drop_outside(
        &mut self,
        bounds: gpui::Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !bounds.contains(&position) {
            self.set_view_strip_drop_index(None, cx);
        }
    }

    pub(crate) fn handle_view_strip_drop(
        &mut self,
        key: ViewStripTabKey,
        default_insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index = self
            .view_strip_drop_index
            .take()
            .unwrap_or(default_insertion_index);
        self.move_view_strip_tab(key, insertion_index, cx);
    }

    /// Moves `key` to `insertion_index` of the row it is drawn in (`drawn_view_strip_keys`).
    /// Pinned tabs stay together at the left: a drag moves a tab within its own group only.
    pub(crate) fn move_view_strip_tab(
        &mut self,
        key: ViewStripTabKey,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        // A browser tab's drag hides the pages so the panes can show their drop zones; a drop on
        // a row ends that the same way a release anywhere else does.
        self.finish_browser_tab_drag(cx);
        let mut keys = self.drawn_view_strip_keys(key);
        if let Some(from) = keys.iter().position(|existing| *existing == key) {
            let mut target = insertion_index.min(keys.len());
            if target > from {
                target -= 1;
            }
            keys.remove(from);
            let pinned_count = self.pinned_view_strip_tab_count(&keys);
            let target = if self.view_strip_tab_pinned(key) {
                target.min(pinned_count)
            } else {
                target.max(pinned_count)
            };
            keys.insert(target, key);
            self.write_view_strip_projection(&keys);
            self.sync_open_views_to_view_strip_order();
            self.persist_shell_layout_state();
        }
        cx.notify();
    }

    /// The open-views list keeps the views in the strip's stored order, pinned first, so what reads
    /// it in list order (the panel toggle's first tab, the saved state) sees them as drawn; closing
    /// a tab reads the drawn row itself (`view_strip_successor`), and so does a view that leaves
    /// the panel without its tab being closed (`open_views_in_view_strip_order`). Every open view
    /// takes part, one this project cannot show right now included, so a view that comes back has
    /// the same neighbours in both. Browser takes part while the strip holds its tab (`browserPagesInViewRow`); drawn as
    /// its pages, it keeps its own slot.
    fn sync_open_views_to_view_strip_order(&mut self) {
        let browser_in_strip = self.browser_pages_in_view_row;
        let ranked_slot = |mode: &TitlebarMode| {
            (browser_in_strip || *mode != TitlebarMode::Browser) && *mode != TitlebarMode::SideChat
        };
        let views = self
            .open_views
            .iter()
            .copied()
            .filter(ranked_slot)
            .collect::<Vec<_>>();
        let ranked = self.in_view_strip_order(views, ViewStripTabKey::View);
        let mut next = ranked.into_iter();
        for slot in &mut self.open_views {
            if ranked_slot(slot)
                && let Some(mode) = next.next()
            {
                *slot = mode;
            }
        }
    }

    /// Every open view in the order the strip draws it, those this project cannot show right now
    /// included, which is where `view_tab_successor` looks for the tab beside the one leaving.
    pub(crate) fn open_views_in_view_strip_order(&self) -> Vec<TitlebarMode> {
        let pages_in_row = self.browser_pages_in_view_row;
        let pages = if pages_in_row {
            Vec::new()
        } else {
            self.view_strip_browser_tabs()
                .into_iter()
                .map(|page| page.tab_id)
                .collect()
        };
        self.view_strip_layout
            .open_views_in_drawn_order(&self.open_views, &pages, pages_in_row)
    }

    /// A drag released anywhere but on a tab ends here, so the drop marker cannot outlive the drag
    /// that drew it. The window root calls this on every left mouse up, beside the other tab drags.
    pub(crate) fn cancel_view_tab_drag(&mut self, cx: &mut gpui::Context<Self>) {
        self.set_view_strip_drop_index(None, cx);
        self.set_browser_page_row_drop_index(None, cx);
    }
}
