//! Sessions in popups: windows of their own. Popout (the session's right-click menu, the
//! work area header's button, the split-pane grip's menu, the Popout hotkey, or
//! Option-click on the sidebar row) moves a chat or terminal session out of its pane into a plain
//! top-level window drawn on the same app state; the pane shows a card with Show Window and Bring
//! Back, and closing the window puts its sessions back. A popup holds several sessions as tabs
//! (tabs.rs).
//!
//! CDXC:SessionPopup 2026-11-16 DECISION:
//! User (GitHub #641, then the Ghostex page Q3570-Q3577): a session opens in a window of its own.
//! Entry points: the session menu's Open in New Window (desktop only, never the phone's menu), the
//! work area header button, the split-pane grip's menu, the existing Pop Out Pane hotkey and
//! Option-click on a sidebar row; dragging a tab out comes later. The window opens small and
//! grows to a full window with no sidebar with Expand, and back with Shrink. It is not on top by
//! default; its Pin keeps it above other apps (hidden on Wayland, which has no such state). The
//! pane keeps a placeholder with Show Window and Bring Back. Closing the window (its close button
//! or Cmd+W) puts the session back in its pane without bringing the main window forward. Each
//! session keeps its own size, place, Pin and small or expanded state; with nothing saved it opens
//! centred on the Ghostex window. The pop-ups that were open come back at the next launch. Chats
//! and terminals only; browser tabs not yet.
//!
//! CDXC:SessionPopup 2026-11-20 DECISION:
//! User, picking one name per session action (Q4292, the one session menu in gx-core sidebar_menu/layout.rs): every place that offers a session's own window says "Popout": the session menu's quick row, the work area header button's tooltip and label, the header's ⋯, the split-pane grip's menu, and the hotkey's name in Settings > Keyboard and Quick Access ("Session: Popout"). The hotkey id `popOutPane` and its setting keys stay. Supersedes "Open in a Popup" (2026-11-17) and Open in New Window above; the Project board's own Open in New Window is not a session's and keeps its name.
//!
//! CDXC:SessionPopup 2026-11-16 WHY:
//! It follows the Kanban board's pop-out (native_kanban/window.rs): the session's chat view and
//! terminal view are app state, and each is drawn in one window at a time (the chat's own windows
//! and focus follow the window that last drew it, native_chat/child_window.rs), so the pop-up
//! draws the pane's own views and the pane draws a card instead. A pop-up belongs to the workspace
//! window it came from and closes with it. It is keyed by the session (`SessionKey`), not the
//! pane's shell id, because a project switch parks the shell ids and their views: while the main
//! window shows another project the pop-up says so and offers to show the session there.
//! The popped-out session counts as on screen for the chat and terminal reconciles
//! (`session_popup_shows`), so its chat keeps streaming and its terminal viewer stays attached
//! even after its pane shows another session.
//!
//! SEE-ALSO: apps/desktop/src/app/native_kanban/window.rs, render/terminal_body_slot.rs (the
//! placeholder), terminal_sync/gpui_engine_terminal_viewers.rs and session_chat_surfaces.rs (the
//! on-screen checks), ghostex_capture/platform.rs (`set_window_stays_on_top`).

mod panes;
mod render;
mod saved;
mod tabs;

use std::collections::HashMap;

use ghostex_gx_core::SessionKey;
use gpui::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Pixels, Styled as _, TitlebarOptions,
    WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;

use crate::GhostexGpuiApp;
use crate::app::ghostex_capture::platform;
use crate::app::model::*;

pub(crate) use render::SessionPopupRoot;
pub(crate) use tabs::SessionPopupTabDrag;

/// The small window's default size, and the smallest either size can be.
const SMALL_WIDTH: f32 = 420.0;
const SMALL_HEIGHT: f32 = 600.0;
const MIN_WIDTH: f32 = 340.0;
const MIN_HEIGHT: f32 = 320.0;

/// One popup window of a workspace window, for as long as it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SessionPopupId(u64);

/// The popups of this workspace window.
#[derive(Default)]
pub(crate) struct SessionPopups {
    windows: Vec<SessionPopupWindow>,
    next_id: u64,
    /// The launch's reopen ran (`reopen_saved_session_popups`).
    reopened: bool,
    /// The popup last brought forward, which a newly popped-out session joins.
    last_active: Option<SessionPopupId>,
    /// The tab being dragged out of a popup's tab bar (tabs.rs), so a release outside every window
    /// knows what it carried.
    pub(crate) drag: Option<SessionPopupTabDrag>,
}

/// A popup window and the sessions it shows.
///
/// CDXC:SessionPopup 2026-11-17 DECISION:
/// User: a second session popped out joins the existing popup window as a tab instead of opening another window. A tab bar at the top of the popup shows one tab per session (agent icon and title, and a close button that brings that session back to the main window, as closing the window does); a popup with a single session shows no tab bar. Dragging a tab out of the bar, or the tab menu's Move to New Popup, puts it in a popup of its own; dragging it onto another popup, or Move to Popup ›, moves it there. Pin (always on top), Expand and the window's size and place belong to the whole popup, which takes its first session's saved ones; this supersedes the 2026-11-16 rule that each session keeps its own. The tab bar looks like Ghostex's view tab strip. Supersedes the 2026-11-16 note that dragging a tab out comes later.
///
/// CDXC:SessionPopup 2026-11-17 WHY:
/// The tabs (and the splits, panes.rs) are the main window's own pane model (`WorkspaceModel`), so a popup moves, groups and splits tabs with the same code the work area does. Its tab ids are the popup's own: each maps to the session (`keys`), because the sessions' shell ids change with the main window's project.
pub(crate) struct SessionPopupWindow {
    pub(crate) id: SessionPopupId,
    pub(crate) window: AnyWindowHandle,
    /// The window's NSView, HWND or X11 id, for Pin and for Expand's move.
    native: Option<platform::NativeWindow>,
    pub(crate) expanded: bool,
    pub(crate) pinned: bool,
    /// The popup's tabs and splits.
    pub(crate) layout: WorkspaceModel,
    /// The session each of the layout's tabs shows.
    pub(crate) keys: HashMap<TerminalSessionId, SessionKey>,
    /// The session whose saved frame, Pin and size the window takes and keeps (its first session,
    /// or the first one left once that one moved out).
    lead: String,
    /// Bumped on each change to the tabs or the drag feedback, so the window redraws.
    pub(crate) revision: u64,
    /// The pane a dragged tab would split or join, and how (panes.rs).
    pub(crate) drop: Option<WorkspaceDropFeedback>,
    /// The tab bar and the place in it a dragged tab would land.
    pub(crate) strip_drop: Option<(WorkspacePaneId, usize)>,
    /// Each split's span as last laid out, for its divider (panes.rs).
    pub(crate) split_metrics: HashMap<WorkspaceSplitId, SplitResizeMetrics>,
    /// The divider being dragged.
    pub(crate) split_drag: Option<WorkspaceSplitResizeDragState>,
}

impl SessionPopupWindow {
    pub(crate) fn contains(&self, key: &SessionKey) -> bool {
        self.keys.values().any(|known| known == key)
    }

    pub(crate) fn tab_for(&self, key: &SessionKey) -> Option<TerminalSessionId> {
        self.keys
            .iter()
            .find(|(_, known)| *known == key)
            .map(|(tab, _)| *tab)
    }

    /// The sessions in tab order (pane by pane).
    pub(crate) fn keys_in_order(&self) -> Vec<SessionKey> {
        let mut tabs = Vec::new();
        collect_workspace_tabs_in_tree_order(&self.layout.root, &mut tabs);
        tabs.iter()
            .filter_map(|tab| self.keys.get(&tab.session_id).cloned())
            .collect()
    }

    /// The session of the focused pane's selected tab: the one the window is titled after.
    pub(crate) fn active_key(&self) -> Option<SessionKey> {
        self.layout
            .active_session_in_pane(self.layout.focused_pane)
            .and_then(|tab| self.keys.get(&tab).cloned())
            .or_else(|| self.keys_in_order().into_iter().next())
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.keys.len()
    }

    /// Adds `key` as the focused pane's last tab and selects it.
    fn add(&mut self, key: SessionKey) {
        if let Some(tab) = self.tab_for(&key) {
            if let Some(pane) = self.layout.pane_id_for_session(tab) {
                self.layout.select_tab(pane, tab);
            }
            return;
        }
        let pane = self.layout.focused_pane;
        if let Some((_, tab)) =
            self.layout
                .add_running_session_to_pane(pane, key.session_id.clone(), None)
        {
            self.keys.insert(tab, key);
        }
        self.revision += 1;
    }

    /// Takes `key` out; true when it was here.
    fn remove(&mut self, key: &SessionKey) -> bool {
        let Some(tab) = self.tab_for(key) else {
            return false;
        };
        if let Some(pane) = self.layout.pane_id_for_session(tab) {
            self.layout.close_tab(pane, tab);
        }
        self.keys.remove(&tab);
        if self.lead == key.to_sidebar_session_id()
            && let Some(next) = self.keys_in_order().into_iter().next()
        {
            self.lead = next.to_sidebar_session_id();
        }
        self.revision += 1;
        true
    }
}

impl SessionPopups {
    pub(crate) fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub(crate) fn get(&self, id: SessionPopupId) -> Option<&SessionPopupWindow> {
        self.windows.iter().find(|popup| popup.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: SessionPopupId) -> Option<&mut SessionPopupWindow> {
        self.windows.iter_mut().find(|popup| popup.id == id)
    }

    /// The popup that shows `key`, if one does.
    pub(crate) fn containing(&self, key: &SessionKey) -> Option<&SessionPopupWindow> {
        self.windows.iter().find(|popup| popup.contains(key))
    }

    /// The popup a newly popped-out session joins: the one last brought forward.
    fn join_target(&self) -> Option<SessionPopupId> {
        self.last_active
            .filter(|id| self.get(*id).is_some())
            .or_else(|| self.windows.last().map(|popup| popup.id))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &SessionPopupWindow> {
        self.windows.iter()
    }
}

/// The row's right-click menu with Popout added (above More; `popup_row_into_quick_row` then moves
/// it into the quick row). Added here, where the desktop answers the menu, so the phone, which has
/// no popups, never gets it.
pub(crate) fn with_open_in_new_window_row(
    mut items: Vec<serde_json::Value>,
    row_id: &str,
) -> Vec<serde_json::Value> {
    if items.is_empty() || SessionKey::parse_sidebar_session_id(row_id).is_none() {
        return items;
    }
    let entry = ghostex_gx_core::session_menu_layout::entry("popout").expect("popout entry");
    let row = serde_json::json!({
        "label": entry.label,
        "icon": entry.icon,
        "hotkey": entry.hotkey,
        "checked": false,
        "disabled": false,
        "command": {
            "type": "sessionAction",
            "sessionId": row_id,
            "action": OPEN_IN_NEW_WINDOW_ACTION,
        },
    });
    let more = ghostex_gx_core::session_menu_layout::MORE.label;
    let at = items
        .iter()
        .position(|item| item["label"] == more)
        .unwrap_or(items.len());
    items.insert(at, row);
    items
}

/// The `sessionAction` the menu row sends (`native_sidebar/actions.rs`).
pub(crate) const OPEN_IN_NEW_WINDOW_ACTION: &str = "openInNewWindow";

/// Whether this desktop can keep a window above other apps (not Wayland).
pub(crate) fn pin_supported() -> bool {
    platform::supported()
}

/// `window`'s frame in GPUI's global space: `Window::bounds` is read against the window's own
/// display on macOS, so that display's origin is added back there.
pub(crate) fn window_global_bounds(window: &gpui::Window, cx: &gpui::App) -> Bounds<Pixels> {
    let mut bounds = window.bounds();
    if cfg!(target_os = "macos")
        && let Some(display) = window.display(cx)
    {
        bounds.origin += display.bounds().origin;
    }
    bounds
}

/// A frame in GPUI's global space as `WindowOptions` takes it: on the display it sits on, read
/// against that display on macOS.
fn window_options_bounds(
    global: Bounds<Pixels>,
    cx: &gpui::App,
) -> (Bounds<Pixels>, Option<gpui::DisplayId>) {
    let center = global.center();
    let Some(display) = cx
        .displays()
        .into_iter()
        .find(|display| display.bounds().contains(&center))
    else {
        return (global, None);
    };
    let display_bounds = display.bounds();
    let fitted = clamp_into(global, display.visible_bounds());
    let bounds = if cfg!(target_os = "macos") {
        Bounds::new(
            gpui::point(
                fitted.origin.x - display_bounds.origin.x,
                fitted.origin.y - display_bounds.origin.y,
            ),
            fitted.size,
        )
    } else {
        fitted
    };
    (bounds, Some(display.id()))
}

impl GhostexGpuiApp {
    /// The shell id `key` has in this window right now: `None` while its project is not the one
    /// the window shows (its views are parked) or before its tab exists.
    pub(crate) fn session_popup_shell_session(
        &self,
        key: &SessionKey,
    ) -> Option<TerminalSessionId> {
        match key.machine.remote_id() {
            None => self
                .local_workspace_session_mappings
                .get(&GpuiLocalWorkspaceSessionKey {
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                })
                .copied(),
            Some(machine_id) => self
                .remote_attach_sessions
                .get(&crate::app::helpers::GpuiRemoteAttachSessionKey {
                    remote_machine_id: machine_id.to_string(),
                    project_id: key.project_id.clone(),
                    session_id: key.session_id.clone(),
                })
                .copied(),
        }
    }

    /// The popup that shows the Agents session `session_id`, if one does.
    pub(crate) fn session_popup_for_shell(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&SessionPopupWindow> {
        if self.session_popups.is_empty() {
            return None;
        }
        let key = self.workspace_terminal_session_key(session_id)?;
        self.session_popups.containing(&key)
    }

    /// Whether the Agents session `session_id` is drawn in a popup: its pane shows the
    /// placeholder, and the chat and terminal reconciles count it as on screen.
    pub(crate) fn session_popup_shows(&self, session_id: TerminalSessionId) -> bool {
        self.session_popup_for_shell(session_id).is_some()
    }

    /// The popped-out sessions this window shows, by shell id, for the chat reconcile.
    pub(crate) fn session_popup_shell_sessions(&self) -> Vec<TerminalSessionId> {
        self.session_popups
            .windows
            .iter()
            .flat_map(|popup| popup.keys.values())
            .filter_map(|key| self.session_popup_shell_session(key))
            .collect()
    }

    /// The session the header's button and the hotkey act on: the focused pane's.
    pub(crate) fn session_popup_focused_target(&self) -> Option<TerminalSessionId> {
        if !self.agents_workspace_visible() {
            return None;
        }
        self.agents_workspace
            .find_leaf(self.agents_workspace.focused_pane)
            .and_then(|leaf| leaf.tab_group.active_session_id())
    }

    /// The Open in a Popup hotkey (`popOutPane`): the focused pane's session goes to a popup, or
    /// comes back when it is already in one.
    pub(crate) fn toggle_session_popup_for_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_popup_focused_target() else {
            return;
        };
        if self.session_popup_shows(session_id) {
            if let Some(key) = self.workspace_terminal_session_key(session_id) {
                self.bring_back_session_popup(&key.to_sidebar_session_id(), cx);
            }
        } else {
            self.open_session_popup_for_shell(session_id, cx);
        }
    }

    /// The header's button and the grip's menu row: `session_id` in a popup.
    pub(crate) fn open_session_popup_for_shell(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.workspace_terminal_session_key(session_id) else {
            return;
        };
        self.open_session_popup(key, cx);
    }

    /// The session menu's row: the session is shown in this window first (so it is a tab here and
    /// attached), then moves to a popup.
    pub(crate) fn open_session_popup_for_row(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(key) = SessionKey::parse_sidebar_session_id(row_id.trim()) else {
            return;
        };
        if self.session_popups.containing(&key).is_none() {
            self.gx_store_focus_session_row(
                row_id,
                crate::app::gx_store::RowFocusOptions::default(),
                cx,
            );
        }
        self.open_session_popup(key, cx);
    }

    /// A focus of `row_id` (a row click, a notification, Quick Access) brings its popup forward
    /// with its tab selected: the session is there, not in the pane.
    pub(crate) fn session_popup_bring_forward_for_row(
        &mut self,
        row_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = SessionKey::parse_sidebar_session_id(row_id.trim()) else {
            return;
        };
        if let Some(id) = self.session_popups.containing(&key).map(|popup| popup.id) {
            self.session_popup_select(id, &key, cx);
            self.session_popup_activate(id, cx);
        }
    }

    /// Opens `key` in a popup: its tab when it is in one already, else a tab of the popup last
    /// brought forward, else a new popup window.
    pub(crate) fn open_session_popup(&mut self, key: SessionKey, cx: &mut Context<Self>) {
        if let Some(id) = self.session_popups.containing(&key).map(|popup| popup.id) {
            self.session_popup_select(id, &key, cx);
            self.session_popup_activate(id, cx);
            return;
        }
        if let Some(session_id) = self.session_popup_shell_session(&key)
            && self
                .agents_workspace
                .session(session_id)
                .is_some_and(|session| session.agent_icon == Some("browser"))
        {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Browser tabs stay in the main window",
                "Only chats and terminals can open in a popup for now.",
                cx,
            );
            return;
        }
        if let Some(id) = self.session_popups.join_target() {
            self.session_popup_join(id, key, cx);
            return;
        }
        self.open_session_popup_window(vec![key], None, false, cx);
    }

    /// `key` becomes the last tab of popup `id`, selected, and that popup comes forward.
    fn session_popup_join(&mut self, id: SessionPopupId, key: SessionKey, cx: &mut Context<Self>) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.add(key);
        self.session_popup_save_open_list();
        self.session_popup_settle(cx);
        self.session_popup_activate(id, cx);
    }

    /// Selects `key`'s tab in popup `id`.
    pub(crate) fn session_popup_select(
        &mut self,
        id: SessionPopupId,
        key: &SessionKey,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        let Some(tab) = popup.tab_for(key) else {
            return;
        };
        if let Some(pane) = popup.layout.pane_id_for_session(tab) {
            popup.layout.select_tab(pane, tab);
            popup.revision += 1;
        }
        self.session_popup_settle(cx);
    }

    /// Brings popup `id`'s window forward.
    pub(crate) fn session_popup_activate(&mut self, id: SessionPopupId, cx: &mut Context<Self>) {
        let Some(handle) = self.session_popups.get(id).map(|popup| popup.window) else {
            return;
        };
        self.session_popups.last_active = Some(id);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        });
    }

    /// The popup window `id` was brought forward by the user, so the next popped-out session
    /// joins it.
    pub(crate) fn session_popup_note_active(&mut self, id: SessionPopupId) {
        self.session_popups.last_active = Some(id);
    }

    /// Opens a popup window with `keys` as its tabs, the first selected. `frame` places it (in
    /// GPUI's global space, a tab dragged out); otherwise it opens where its first session's popup
    /// was last, or centred on this Ghostex window. Unless `own_window`, sessions that find a
    /// popup opened meanwhile join it instead.
    pub(crate) fn open_session_popup_window(
        &mut self,
        keys: Vec<SessionKey>,
        frame: Option<Bounds<Pixels>>,
        own_window: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(lead) = keys.first().map(SessionKey::to_sidebar_session_id) else {
            return;
        };
        let saved = saved::session(&lead);
        let expanded = saved.as_ref().is_some_and(|saved| saved.expanded);
        let pinned = pin_supported() && saved.as_ref().is_some_and(|saved| saved.pinned);
        // A saved frame opens on its own display while that display is still there; otherwise the
        // window opens centred on this Ghostex window.
        let placed = frame.map(|frame| window_options_bounds(frame, cx));
        let (bounds, display_id) = placed
            .or_else(|| {
                saved
                    .as_ref()
                    .and_then(|saved| if expanded { saved.full } else { saved.small })
                    .and_then(|frame| match frame.display.map(gpui::DisplayId::new) {
                        Some(id) => cx.find_display(id).map(|_| (frame.bounds, Some(id))),
                        None => Some((frame.bounds, self.main_window_display_id)),
                    })
            })
            .unwrap_or_else(|| {
                (
                    self.session_popup_default_bounds(expanded),
                    self.main_window_display_id,
                )
            });
        let title = self.session_popup_title(&keys[0]);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            // The popup's header is its title band on every platform (render.rs): on macOS the
            // traffic lights sit in it, Windows hides its system caption, and Linux asks for no
            // window-manager frame, as the main window does (workspace_windows/open.rs); where the
            // window manager draws a titlebar anyway, the header sits under it without buttons of
            // its own.
            titlebar: Some(TitlebarOptions {
                title: Some(title.clone().into()),
                appears_transparent: true,
                traffic_light_position: cfg!(target_os = "macos")
                    .then(|| gpui::point(px(11.0), px(11.5))),
            }),
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            // The header holds draggable tabs, so the app, not AppKit, decides which drags there
            // move the window (`window_drag_region`).
            #[cfg(target_os = "macos")]
            app_owns_titlebar_drag: true,
            // The popup takes part in window glass as the main window does (window_glass.rs).
            window_background: crate::app::helpers::live_glass_window_creation_background(),
            window_min_size: Some(size(px(MIN_WIDTH), px(MIN_HEIGHT))),
            app_id: crate::gpui_platform_window_app_id(),
            icon: crate::gpui_platform_window_icon(),
            focus: true,
            show: true,
            ..Default::default()
        };
        let app = cx.entity();
        // Deferred: the click arrives inside a window update, where opening a window is refused.
        cx.defer(move |cx| {
            // A session that reached a popup before this ran (a second click) stays there; one
            // popped out while another popup opened joins it instead of opening a third window.
            let keys = {
                let popups = &app.read(cx).session_popups;
                keys.into_iter()
                    .filter(|key| popups.containing(key).is_none())
                    .collect::<Vec<_>>()
            };
            if keys.is_empty() {
                return;
            }
            if !own_window && let Some(id) = app.read(cx).session_popups.join_target() {
                app.update(cx, |app, cx| {
                    for key in keys {
                        app.session_popup_join(id, key, cx);
                    }
                });
                return;
            }
            let id = app.update(cx, |app, _| {
                app.session_popups.next_id += 1;
                SessionPopupId(app.session_popups.next_id)
            });
            let observed = app.clone();
            let on_close = app.downgrade();
            let opened = cx.open_window(options, move |window, cx| {
                window.set_window_title(&title);
                let root = cx.new(|cx| SessionPopupRoot::new(observed, id, window, cx));
                // The window's own close button puts its sessions back in their panes.
                let window_id = window.window_handle().window_id();
                window.on_window_should_close(cx, move |_, cx| {
                    crate::app::helpers::set_own_glass_window(window_id, false);
                    let on_close = on_close.clone();
                    // Deferred so the app is never updated from inside the platform's close.
                    cx.defer(move |cx| {
                        let _ = on_close.update(cx, |app, cx| {
                            app.session_popup_window_closed(id, window_id, cx)
                        });
                    });
                    true
                });
                cx.new(|cx| Root::new(root, window, cx).bg(gpui::transparent_black()))
            });
            let Ok(handle) = opened else {
                return;
            };
            let handle: AnyWindowHandle = handle.into();
            crate::app::helpers::set_own_glass_window(handle.window_id(), true);
            let native = handle
                .update(cx, |_, window, _| {
                    crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| view as usize)
                })
                .ok()
                .flatten();
            if pinned && let Some(native) = native {
                platform::set_window_stays_on_top(native, true);
            }
            app.update(cx, |app, cx| {
                let mut popup = SessionPopupWindow {
                    id,
                    window: handle,
                    native,
                    expanded,
                    pinned,
                    layout: WorkspaceModel::empty_default(),
                    keys: HashMap::new(),
                    lead,
                    revision: 0,
                    drop: None,
                    strip_drop: None,
                    split_metrics: HashMap::new(),
                    split_drag: None,
                };
                for key in &keys {
                    popup.add(key.clone());
                }
                if let Some(first) = keys.first()
                    && let Some(tab) = popup.tab_for(first)
                    && let Some(pane) = popup.layout.pane_id_for_session(tab)
                {
                    popup.layout.select_tab(pane, tab);
                }
                app.session_popups.windows.push(popup);
                app.session_popups.last_active = Some(id);
                app.session_popup_save_open_list();
                app.session_popup_settle(cx);
            });
        });
    }

    /// After a popup changed what it shows: the chat and terminal reconciles see what is on
    /// screen now, and the panes and popups redraw.
    pub(crate) fn session_popup_settle(&mut self, cx: &mut Context<Self>) {
        for session_id in self.session_popup_shell_sessions() {
            if !self.agents_chat_mode_sessions.contains(&session_id) {
                self.ensure_agents_gpui_engine_terminal_view(session_id, cx);
            }
        }
        self.reconcile_agents_pane_surfaces(cx);
        // The terminal viewer reconcile runs on the redraw this asks for.
        cx.notify();
    }

    /// Popup `id`'s window `closed` went away (its close button): its sessions are back in their
    /// panes, and the main window stays where it is. A window already closed or replaced leaves
    /// the current state alone.
    fn session_popup_window_closed(
        &mut self,
        id: SessionPopupId,
        closed: gpui::WindowId,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session_popups
            .get(id)
            .is_some_and(|popup| popup.window.window_id() == closed);
        if current {
            self.session_popups.windows.retain(|popup| popup.id != id);
            self.session_popup_save_open_list();
            self.session_popup_settle(cx);
        }
    }

    /// The popup shown in window `window`, if that window is a popup's.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn session_popup_in_window(&self, window: gpui::WindowId) -> Option<SessionPopupId> {
        self.session_popups
            .windows
            .iter()
            .find(|popup| popup.window.window_id() == window)
            .map(|popup| popup.id)
    }

    /// Closes popup `id`'s window quietly; its sessions go back to their panes.
    pub(crate) fn close_session_popup_window(
        &mut self,
        id: SessionPopupId,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .session_popups
            .windows
            .iter()
            .position(|popup| popup.id == id)
        else {
            return;
        };
        let popup = self.session_popups.windows.remove(index);
        let handle = popup.window;
        crate::app::helpers::set_own_glass_window(handle.window_id(), false);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
        self.session_popup_save_open_list();
        self.session_popup_settle(cx);
    }

    /// Takes `row_id`'s session out of its popup quietly (its tab's close button, Cmd+W): it is
    /// back in its pane, and a popup left with nothing closes. False when no popup had it.
    pub(crate) fn close_session_popup(&mut self, row_id: &str, cx: &mut Context<Self>) -> bool {
        let Some(key) = SessionKey::parse_sidebar_session_id(row_id) else {
            return false;
        };
        let Some(popup) = self
            .session_popups
            .windows
            .iter_mut()
            .find(|popup| popup.contains(&key))
        else {
            return false;
        };
        popup.remove(&key);
        let (id, empty) = (popup.id, popup.keys.is_empty());
        if empty {
            self.close_session_popup_window(id, cx);
        } else {
            self.session_popup_save_open_list();
            self.session_popup_settle(cx);
        }
        true
    }

    /// Bring Back, from the placeholder, the popup's header, its tab menu or the hotkey: the
    /// session leaves its popup, shows in this window's focused pane and this window comes
    /// forward.
    pub(crate) fn bring_back_session_popup(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if !self.close_session_popup(row_id, cx) {
            return;
        }
        self.gx_store_focus_session_row(
            row_id,
            crate::app::gx_store::RowFocusOptions::default(),
            cx,
        );
        if let Some(main) = self.main_window_handle {
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            });
        }
    }

    /// Pin: the popup stays above other apps' windows, or stops.
    pub(crate) fn toggle_session_popup_pin(&mut self, id: SessionPopupId, cx: &mut Context<Self>) {
        if !pin_supported() {
            return;
        }
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.pinned = !popup.pinned;
        if let Some(native) = popup.native {
            platform::set_window_stays_on_top(native, popup.pinned);
        }
        let pinned = popup.pinned;
        saved::update_session(&popup.lead, |saved| saved.pinned = pinned);
        cx.notify();
    }

    /// Expand and Shrink: the window takes its other size where it stands (the size each was last
    /// given, or a default one), keeping its centre on the same screen.
    ///
    /// CDXC:SessionPopup 2026-11-17 WHY:
    /// User: after Expand or Shrink the window changed size but its content kept the old size until the border was dragged by hand. The frame was set from inside the button's own mouse handler, while GPUI held the popup's window; the platform reports the new size synchronously (`setFrame:display:animate:` on macOS, `SetWindowPos` on Windows), and GPUI's resize callback cannot update a window that is held, so the report was dropped. The frame is now set after the handler returns, and GPUI then re-reads the window's size (`Window::bounds_changed`) for platforms that report it late or not at all.
    pub(crate) fn toggle_session_popup_expanded(
        &mut self,
        id: SessionPopupId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let default_full = self.session_popup_default_bounds(true);
        let default_small = self.session_popup_default_bounds(false);
        let Some(popup) = self.session_popups.get_mut(id) else {
            return;
        };
        popup.expanded = !popup.expanded;
        let expanded = popup.expanded;
        let native = popup.native;
        let handle = popup.window;
        let lead = popup.lead.clone();
        let saved = saved::session(&lead).unwrap_or_default();
        let target_size = if expanded {
            saved
                .full
                .map_or(default_full.size, |frame| frame.bounds.size)
        } else {
            saved
                .small
                .map_or(default_small.size, |frame| frame.bounds.size)
        };
        let current = window_global_bounds(window, cx);
        let mut target = Bounds::centered_at(current.center(), target_size);
        if let Some(display) = window.display(cx) {
            target = clamp_into(target, display.visible_bounds());
        }
        let scale = window.scale_factor();
        cx.defer(move |cx| {
            match native {
                // Moves and sizes the window in one step where the OS lets a client place it.
                Some(native) if platform::supported() => {
                    platform::set_window_frame(native, target, scale)
                }
                _ => {
                    let _ = handle.update(cx, |_, window, _| window.resize(target.size));
                }
            }
            let _ = handle.update(cx, |_, window, cx| window.bounds_changed(cx));
        });
        saved::update_session(&lead, |saved| saved.expanded = expanded);
        cx.notify();
    }

    /// The popup moved or changed size: the size it has now is the one its current state (small
    /// or expanded) opens with next time.
    pub(crate) fn session_popup_note_bounds(
        &mut self,
        id: SessionPopupId,
        bounds: Bounds<Pixels>,
        display: Option<gpui::DisplayId>,
    ) {
        let Some(popup) = self.session_popups.get(id) else {
            return;
        };
        let expanded = popup.expanded;
        let frame = saved::SavedFrame {
            bounds,
            display: display.map(u64::from),
        };
        saved::update_session(&popup.lead, |saved| {
            if expanded {
                saved.full = Some(frame);
            } else {
                saved.small = Some(frame);
            }
        });
    }

    /// Where a popup with nothing saved opens: centred on this Ghostex window, small, or most of
    /// that window's size when expanded.
    fn session_popup_default_bounds(&self, expanded: bool) -> Bounds<Pixels> {
        let main = self.main_window_bounds;
        let center = main.center();
        let size = if expanded {
            size(
                px((f32::from(main.size.width) * 0.8).max(SMALL_WIDTH)),
                px((f32::from(main.size.height) * 0.86).max(SMALL_HEIGHT)),
            )
        } else {
            size(px(SMALL_WIDTH), px(SMALL_HEIGHT))
        };
        Bounds::centered_at(center, size)
    }

    /// A session's title: its tab's, falling back to "Session".
    pub(crate) fn session_popup_title(&self, key: &SessionKey) -> String {
        self.session_popup_shell_session(key)
            .map(|session_id| self.agents_workspace_tab_display_title(session_id))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Session".to_string())
    }

    /// Forgets this window's popups and returns them for the caller to close: a workspace window
    /// closing takes its popups with it (workspace_windows/close.rs), and they do not come back.
    pub(crate) fn take_session_popup_windows(&mut self) -> Vec<AnyWindowHandle> {
        let handles = self
            .session_popups
            .windows
            .drain(..)
            .map(|popup| {
                crate::app::helpers::set_own_glass_window(popup.window.window_id(), false);
                popup.window
            })
            .collect();
        saved::set_open(self.workspace_window_slot, Vec::new());
        handles
    }

    pub(crate) fn session_popup_save_open_list(&self) {
        let open = self
            .session_popups
            .windows
            .iter()
            .map(|popup| {
                popup
                    .keys_in_order()
                    .iter()
                    .map(SessionKey::to_sidebar_session_id)
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect::<Vec<_>>();
        saved::set_open(self.workspace_window_slot, open);
    }

    /// Launch: the popups this window had open when the app quit open again, each where it was
    /// with the same tabs. They show their sessions once the window has restored them.
    pub(crate) fn reopen_saved_session_popups(&mut self, cx: &mut Context<Self>) {
        if self.session_popups.reopened {
            return;
        }
        self.session_popups.reopened = true;
        for group in saved::open(self.workspace_window_slot) {
            let keys = group
                .iter()
                .filter_map(|row_id| SessionKey::parse_sidebar_session_id(row_id))
                .collect::<Vec<_>>();
            if !keys.is_empty() {
                // Each saved popup opens as its own window, not as tabs of the first.
                self.open_session_popup_window(keys, None, true, cx);
            }
        }
    }
}

/// `bounds` moved (not resized) so it sits inside `area` where it fits.
fn clamp_into(bounds: Bounds<Pixels>, area: Bounds<Pixels>) -> Bounds<Pixels> {
    let clamp = |origin: f32, length: f32, start: f32, span: f32| {
        if length >= span {
            start
        } else {
            origin.clamp(start, start + span - length)
        }
    };
    let x = clamp(
        f32::from(bounds.origin.x),
        f32::from(bounds.size.width),
        f32::from(area.origin.x),
        f32::from(area.size.width),
    );
    let y = clamp(
        f32::from(bounds.origin.y),
        f32::from(bounds.size.height),
        f32::from(area.origin.y),
        f32::from(area.size.height),
    );
    Bounds::new(gpui::point(px(x), px(y)), bounds.size)
}
