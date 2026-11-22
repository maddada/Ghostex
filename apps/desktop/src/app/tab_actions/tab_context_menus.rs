//! Browser and command tab context menus.

use crate::app::context_menu::GpuiContextMenu;
use gpui::Pixels;
use gpui::Window;

use crate::app::actions::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn open_browser_pane_in_external_browser(
        &mut self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) {
        if pane_id == crate::app::custom_links::CUSTOM_LINK_PAGE_PANE {
            self.open_custom_link_page_externally();
            return;
        }
        let Some(tab) = self.browser_tabs.active_tab_for_pane(pane_id) else {
            return;
        };
        match self.browser_tab_system_browser_url(&tab.url, tab.remote_machine_id.as_deref()) {
            Ok(url) => {
                let _ = gpui_open_external_http_url(&url);
            }
            Err(reason) => {
                self.dispatch_gpui_app_modal_toast(
                    "info",
                    "Can't open this page in your browser",
                    &reason,
                    cx,
                );
            }
        }
    }

    pub(crate) fn show_browser_tab_context_menu(
        &self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ContextMenus 2026-06-22-11:27:
        Individual Browser tabs need an owned GPUI popup window at the right-click position. The menu is scoped to the clicked Browser pane and tab ids, contains only tab-level Select Tab and Close Tab commands, and relies on the existing Browser selection/close helpers so address sync, CEF visibility, split/reorder state, favicon runtime state, history, and last-tab address-only placeholder behavior stay unchanged.
        */
        let tab_exists = self
            .browser_tabs
            .find_leaf(pane_id)
            .is_some_and(|leaf| leaf.tab_group.has_tab(tab_id));
        if !tab_exists {
            return;
        }

        /*
        CDXC:Browser 2026-09-21 DECISION:
        User: a browser tab's right-click menu has Sleep, which sleeps that tab, and Sleep Browser,
        which sleeps every browser tab. Sleep Browser is the Browser view's own Sleep, which the
        view's tab used to carry before Browser stopped having a view tab. Each row is offered only
        while there is a page for it to drop. Supersedes the 2026-09-20 note that offered Sleep Tab
        alone.
        */
        let mut menu = GpuiContextMenu::new().menu(
            "Select Tab",
            Box::new(SelectBrowserTabInPane {
                pane_id: pane_id.0,
                tab_id: tab_id.0,
            }),
        );
        menu = menu.menu(
            if self.view_strip_tab_pinned(ViewStripTabKey::Browser(tab_id)) {
                "Unpin Tab"
            } else {
                "Pin Tab"
            },
            Box::new(ToggleGpuiViewStripTabPinned {
                mode_index: TitlebarMode::Browser.switcher_index(),
                browser_tab_id: Some(tab_id.0),
            }),
        );
        if self.browser_surfaces.contains_key(&tab_id) {
            menu = menu.menu(
                "Sleep",
                Box::new(SleepBrowserTabInPane {
                    pane_id: pane_id.0,
                    tab_id: tab_id.0,
                }),
            );
        }
        if !self.browser_surfaces.is_empty() {
            menu = menu.menu(
                "Sleep Browser",
                Box::new(SleepGpuiTitlebarView {
                    mode_index: TitlebarMode::Browser.switcher_index(),
                }),
            );
        }
        menu.menu(
            "Close Tab",
            Box::new(CloseBrowserTabInPane {
                pane_id: pane_id.0,
                tab_id: tab_id.0,
            }),
        )
        .show(position, window, cx);
    }

    pub(crate) fn show_command_tab_context_menu(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        _expand_pane: bool,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ContextMenus 2026-06-22-11:31:
        Individual command-pane tabs, including collapsed-strip tabs, need an owned GPUI popup window at the right-click position. The menu is scoped to the clicked command group id and session id and contains macOS-style Close Left/Right/Others commands. Tab selection and collapsed-strip expansion stay on left-click activation, not right-click menu rows.

            CDXC:CommandPane 2026-06-25-11:20:
            Scoped command tab rows carry only group id, session id, and a fixed scope enum; they do not carry command text, paths, terminal output, or cross-pane identifiers.

            CDXC:ContextMenus 2026-06-25-14:13:
            Native command tab right-click menus filter out command-panel controls such as Pin/Unpin, Minimize, and Expand, and retain only per-session tab actions that are actually present in the native action payload before scoped Sleep/Close rows.

            CDXC:ContextMenus 2026-06-27-01:49:
            Native command-panel tabs receive only fixed panel action payloads, and Swift keeps primary tab context rows only when per-session actions are present in that payload. GPUI command-tab right-click menus therefore omit Rename Session, Delayed Send, and Close After Done rows here while preserving focused Rename/Close After Done dispatch and explicit Delayed Send modal/session-id routes.

            CDXC:ContextMenus 2026-06-25-14:19:
            Native tab context menus do not add a direct Close Tab row; direct close is hover/middle-click chrome, while right-click close commands start at Close Right, Close Left, and Close Other Tabs.

            CDXC:ContextMenus 2026-06-25-14:22:
            Native tab context menus do not add Select Tab or Expand Commands Panel rows. Opening a context menu must not select the clicked tab or expand a hidden command panel; those remain left-click tab activation behavior.

            CDXC:SessionSleep 2026-06-25-14:27:
            Native command-tab context menus offer Sleep scopes before Close scopes. GPUI resolves those rows against the clicked command group and marks command sessions sleeping without removing their tabs, content-derived titles, or group layout.

            CDXC:ContextMenus 2026-06-25-14:42:
            Native AppKit command-tab menus leave Sleep Right/Left/Others and Close Right/Left/Others enabled even when the clicked tab has no targets in that scope. Keep GPUI rows action-backed and let the scope resolver no-op on empty target lists instead of disabling rows.

            CDXC:ContextMenus 2026-06-27-01:49:
            Command-tab context menus have no primary per-session action block under native command-panel payloads. Do not add placeholder Fork, Reload, Pop Out, Rename, Delayed Send, or Close After Done rows to fill that gap.

            CDXC:FocusMode 2026-06-25-21:40:
            Command-tab Focus is now action-backed only for split command-pane groups with more than one visible awake owner. Place it before Sleep when eligible, matching native's Focus placement without adding fake Fork, Reload, or Pop Out behavior.

            CDXC:ContextMenus 2026-06-27-01:55:
            Native inserts the separator before Sleep only when `primaryTabContextMenuActions()` is non-empty. Command-panel payloads produce no primary actions, so GPUI must not insert an extra separator between eligible Focus and Sleep.
            */
        let tab_exists = self
            .command_pane
            .find_leaf(group_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id));
        if !tab_exists {
            return;
        }
        let clicked_tab_is_sleeping = self
            .command_pane
            .session(session_id)
            .is_some_and(|session| session.is_sleeping);
        let mut menu = GpuiContextMenu::new();

        if self
            .command_pane
            .tab_context_focus_row_index(group_id, session_id)
            .is_some()
        {
            menu = menu.menu(
                command_pane_tab_context_focus_label(),
                Box::new(FocusCommandPaneTab {
                    group_id: group_id.0,
                    session_id: session_id.0,
                }),
            );
        }

        for scope in command_pane_tab_context_sleep_order(clicked_tab_is_sleeping) {
            menu = menu.menu(
                command_pane_tab_context_sleep_scope_label(scope),
                Box::new(SleepCommandPaneTabsByScope {
                    group_id: group_id.0,
                    session_id: session_id.0,
                    scope: scope.action_value(),
                }),
            );
        }
        menu = menu.separator();

        for scope in command_pane_tab_context_scoped_close_order() {
            menu = menu.menu(
                command_pane_tab_context_close_scope_label(scope),
                Box::new(CloseCommandPaneTabsByScope {
                    group_id: group_id.0,
                    session_id: session_id.0,
                    scope: scope.action_value(),
                }),
            );
        }

        menu.show(position, window, cx);
    }
}
