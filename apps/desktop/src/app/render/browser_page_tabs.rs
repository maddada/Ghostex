//! The Browser's pages as a row of tabs under the side panel's strip (`browserPagesInViewRow`),
//! drawn by the shared view row (`view_instance_tabs.rs`); the strip then holds one Browser tab
//! (`view_strip_entries`).
//!
//! CDXC:Workarea 2026-10-25 WHY:
//! The Browser's pages can show in the shared row too: a setting, off by default (`browserPagesInViewRow`), leaves one Browser tab in the strip and draws its pages as tabs in the row under it, as VS Code's windows and Files' open files are drawn. Opening, closing, sleeping, pinning and memory stay as they are; only where the pages are drawn changes, and they drag within their row (or onto a pane to split it, as before) but not into the strip. Closing the Browser tab hides the Browser and puts its pages to sleep; they come back when it opens again. With the setting off the strip is exactly the 2026-09-21 layout.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement as _;
use gpui::KeyDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;

use crate::app::model::*;
use crate::app::render::view_instance_tabs::{ViewInstance, ViewInstanceRow, ViewInstanceTab};
use crate::app::render::view_tab_strip_browser_tabs::{BrowserPageTabParts, ViewStripBrowserTab};
use crate::*;

impl GhostexGpuiApp {
    /// The pages in the row's order: the strip's one stored order, so turning the setting off puts
    /// every page back where it was, with pinned pages first.
    pub(crate) fn browser_page_row(&self) -> Vec<ViewStripBrowserTab> {
        self.in_view_strip_order(self.view_strip_browser_tabs(), |page| {
            ViewStripTabKey::Browser(page.tab_id)
        })
    }

    /// The row, while the setting is on: every page of every pane, the focused pane's page
    /// selected, and the `+` that opens a new one.
    pub(crate) fn render_browser_page_tab_row(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        if !self.browser_pages_in_view_row {
            return None;
        }
        let showing = self
            .browser_tabs
            .active_tab_id_for_pane(self.browser_tabs.focused_pane);
        let pages = self.browser_page_row();
        let shown = pages.iter().position(|page| Some(page.tab_id) == showing);
        let tabs = pages.into_iter().map(|page| {
            let BrowserPageTabParts {
                title,
                icon,
                asleep,
                drag,
            } = self.browser_page_tab_parts(page);
            ViewInstanceTab {
                instance: ViewInstance::BrowserPage(page),
                label: title.clone(),
                tooltip: title,
                showing: Some(page.tab_id) == showing,
                closable: true,
                unsaved: false,
                icon: Some(icon),
                asleep,
                pinned: self.view_strip_tab_pinned(ViewStripTabKey::Browser(page.tab_id)),
                drag: Some(drag),
            }
        });
        let row = self.render_view_instance_tab_row(
            ViewInstanceRow::BrowserPages,
            tabs,
            &self.browser_page_tab_scroll_handle,
            cx,
        );
        // The keys Files' tab row answers, for the pages (`browser_page_row_key_down`).
        let row = div()
            .id("ghostex-gpui-browser-page-tab-keys")
            .flex_shrink_0()
            .track_focus(&self.browser_page_row_focus_handle)
            .on_key_down(cx.listener(Self::browser_page_row_key_down))
            .child(row)
            .into_any_element();
        // Scroll to the shown page when it changes, its place changes or the project does; the
        // row applies the request when it lays out, and otherwise keeps a scroll made by hand.
        let selected = showing.zip(shown);
        let unchanged = match (&self.browser_page_tab_revealed, selected) {
            (Some((project, tab_id, index)), Some((showing, shown))) => {
                *tab_id == showing && *index == shown && *project == self.browser_tabs_project_id
            }
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            if let Some((_, index)) = selected {
                self.browser_page_tab_scroll_handle.scroll_to_item(index);
            }
            self.browser_page_tab_revealed = selected
                .map(|(tab_id, index)| (self.browser_tabs_project_id.clone(), tab_id, index));
        }
        Some(row)
    }

    /// A click on a page's tab shows the page and gives the row the keyboard, as a click on a
    /// Files tab does, so the arrows step on from there.
    pub(crate) fn select_browser_page_from_row(
        &mut self,
        page: ViewStripBrowserTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_browser_tab_from_view_strip(page.pane_id, page.tab_id, window, cx);
        self.focus_browser_page_row(window, cx);
    }

    /// Takes the keys from the page the way the Browser's address field does: a handoff to the
    /// page that is still waiting would take them straight back.
    fn focus_browser_page_row(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.drop_pending_browser_keyboard_handoff();
        crate::cef::focus_gpui_root_view(self.parent_ns_view);
        self.browser_page_row_focus_handle.focus(window, cx);
    }

    /// The keys the row answers once a tab was clicked, as in Files' tab row: the arrows step
    /// between the pages, Delete closes the shown one (a pinned page closes from its menu only, as
    /// in the strip) and Enter goes back to the page.
    fn browser_page_row_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.alt || modifiers.platform || modifiers.control {
            return;
        }
        let pane_id = self.browser_tabs.focused_pane;
        let Some(shown) = self.browser_tabs.active_tab_id_for_pane(pane_id) else {
            return;
        };
        match event.keystroke.key.as_str() {
            "enter" => {
                self.focus_browser_content_for_pane(pane_id, window, cx);
                cx.stop_propagation();
            }
            "delete" | "backspace" => {
                if !self.view_strip_tab_pinned(ViewStripTabKey::Browser(shown)) {
                    self.close_browser_tab(shown, window, cx);
                    // Closing hands the keys to the next page; the row keeps them while it is
                    // still on screen (closing the last page closes the Browser).
                    if self.active_mode == TitlebarMode::Browser {
                        self.focus_browser_page_row(window, cx);
                    }
                }
                cx.stop_propagation();
            }
            "down" | "up" | "right" | "left" => {
                let pages = self.browser_page_row();
                if let Some(index) = pages.iter().position(|page| page.tab_id == shown) {
                    let next = if matches!(event.keystroke.key.as_str(), "down" | "right") {
                        (index + 1).min(pages.len() - 1)
                    } else {
                        index.saturating_sub(1)
                    };
                    if next != index {
                        self.select_browser_page_from_row(pages[next], window, cx);
                    }
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    pub(crate) fn set_browser_page_row_drop_index(
        &mut self,
        index: Option<usize>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.browser_page_row_drop_index != index {
            self.browser_page_row_drop_index = index;
            cx.notify();
        }
    }

    /// A dragged page is over the page drawn at `tab_index`: the line goes before or after it,
    /// whichever half the pointer is in.
    pub(crate) fn update_browser_page_row_drop_feedback(
        &mut self,
        bounds: gpui::Bounds<Pixels>,
        position: gpui::Point<Pixels>,
        tab_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        if bounds.contains(&position) {
            let index = workspace_tab_insertion_index(bounds, position, tab_index);
            self.set_browser_page_row_drop_index(Some(index), cx);
        }
    }

    pub(crate) fn handle_browser_page_row_gap_drop(
        &mut self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.browser_page_row_drop_index {
            Some(index) => self.handle_browser_page_row_drop(tab_id, index, cx),
            None => self.finish_browser_tab_drag(cx),
        }
    }

    /// A page dropped on the row moves within it (`move_view_strip_tab`).
    pub(crate) fn handle_browser_page_row_drop(
        &mut self,
        tab_id: BrowserTabId,
        default_index: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        let insertion_index = self
            .browser_page_row_drop_index
            .take()
            .unwrap_or(default_index);
        self.move_view_strip_tab(ViewStripTabKey::Browser(tab_id), insertion_index, cx);
    }
}
