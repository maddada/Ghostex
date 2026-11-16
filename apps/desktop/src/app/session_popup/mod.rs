//! A session in a window of its own. Open in New Window (the session's right-click menu, the work
//! area header's button, the split-pane grip's menu, the Pop Out Pane hotkey, or Option-click on
//! the sidebar row) moves a chat or terminal session out of its pane into a plain top-level window
//! drawn on the same app state; the pane shows a card with Show Window and Bring Back, and closing
//! the window puts the session back.
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

mod render;
mod saved;

use std::collections::HashMap;

use ghostex_gx_core::SessionKey;
use gpui::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Pixels, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use gpui_component::Root;

use crate::GhostexGpuiApp;
use crate::app::ghostex_capture::platform;
use crate::app::model::*;

pub(crate) use render::SessionPopupRoot;

/// The small window's default size, and the smallest either size can be.
const SMALL_WIDTH: f32 = 420.0;
const SMALL_HEIGHT: f32 = 600.0;
const MIN_WIDTH: f32 = 340.0;
const MIN_HEIGHT: f32 = 320.0;

/// The pop-ups of this workspace window, by the session's sidebar row id.
#[derive(Default)]
pub(crate) struct SessionPopups {
    windows: HashMap<String, SessionPopupWindow>,
    /// The launch's reopen ran (`reopen_saved_session_popups`).
    reopened: bool,
}

pub(crate) struct SessionPopupWindow {
    pub(crate) key: SessionKey,
    pub(crate) window: AnyWindowHandle,
    /// The window's NSView, HWND or X11 id, for Pin and for Expand's move.
    native: Option<platform::NativeWindow>,
    pub(crate) expanded: bool,
    pub(crate) pinned: bool,
}

impl SessionPopups {
    pub(crate) fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub(crate) fn get(&self, row_id: &str) -> Option<&SessionPopupWindow> {
        self.windows.get(row_id)
    }
}

/// The row's right-click menu with Open in New Window added under Move to Group (or above
/// Advanced). Added here, where the desktop answers the menu, so the menu the phone builds never
/// has it.
pub(crate) fn with_open_in_new_window_row(
    mut items: Vec<serde_json::Value>,
    row_id: &str,
) -> Vec<serde_json::Value> {
    if items.is_empty() || SessionKey::parse_sidebar_session_id(row_id).is_none() {
        return items;
    }
    let row = serde_json::json!({
        "label": "Open in New Window",
        "icon": "external-link",
        "checked": false,
        "disabled": false,
        "command": {
            "type": "sessionAction",
            "sessionId": row_id,
            "action": OPEN_IN_NEW_WINDOW_ACTION,
        },
    });
    let at = items
        .iter()
        .position(|item| item["label"] == "Move to Group")
        .map(|index| index + 1)
        .or_else(|| items.iter().position(|item| item["label"] == "Advanced"))
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

    /// The pop-up that shows the Agents session `session_id`, if it is in one.
    pub(crate) fn session_popup_for_shell(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&SessionPopupWindow> {
        if self.session_popups.is_empty() {
            return None;
        }
        let key = self.workspace_terminal_session_key(session_id)?;
        self.session_popups.get(&key.to_sidebar_session_id())
    }

    /// Whether the Agents session `session_id` is drawn in its own window: its pane shows the
    /// placeholder, and the chat and terminal reconciles count it as on screen.
    pub(crate) fn session_popup_shows(&self, session_id: TerminalSessionId) -> bool {
        self.session_popup_for_shell(session_id).is_some()
    }

    /// The popped-out sessions this window shows, by shell id, for the chat reconcile.
    pub(crate) fn session_popup_shell_sessions(&self) -> Vec<TerminalSessionId> {
        self.session_popups
            .windows
            .values()
            .filter_map(|popup| self.session_popup_shell_session(&popup.key))
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

    /// The Pop Out Pane hotkey: the focused pane's session goes to its own window, or comes back
    /// when it is already in one.
    pub(crate) fn toggle_session_popup_for_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_popup_focused_target() else {
            return;
        };
        if let Some(row_id) = self
            .session_popup_for_shell(session_id)
            .map(|popup| popup.key.to_sidebar_session_id())
        {
            self.bring_back_session_popup(&row_id, cx);
        } else {
            self.open_session_popup_for_shell(session_id, cx);
        }
    }

    /// The header's button and the grip's menu row: `session_id`'s own window.
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
    /// attached), then moves to its own window.
    pub(crate) fn open_session_popup_for_row(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(key) = SessionKey::parse_sidebar_session_id(row_id.trim()) else {
            return;
        };
        if self
            .session_popups
            .get(&key.to_sidebar_session_id())
            .is_none()
        {
            self.gx_store_focus_session_row(
                row_id,
                crate::app::gx_store::RowFocusOptions::default(),
                cx,
            );
        }
        self.open_session_popup(key, cx);
    }

    /// A focus of `row_id` (a row click, a notification, Quick Access) brings its pop-up forward:
    /// the session is there, not in the pane.
    pub(crate) fn session_popup_bring_forward_for_row(&self, row_id: &str, cx: &mut Context<Self>) {
        let Some(key) = SessionKey::parse_sidebar_session_id(row_id.trim()) else {
            return;
        };
        if let Some(popup) = self.session_popups.get(&key.to_sidebar_session_id()) {
            let handle = popup.window;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
        }
    }

    /// Opens `key`'s own window, or brings it forward when it is open.
    pub(crate) fn open_session_popup(&mut self, key: SessionKey, cx: &mut Context<Self>) {
        let row_id = key.to_sidebar_session_id();
        if let Some(popup) = self.session_popups.get(&row_id) {
            let handle = popup.window;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
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
                "Only chats and terminals can open in a window of their own for now.",
                cx,
            );
            return;
        }
        let saved = saved::session(&row_id);
        let expanded = saved.as_ref().is_some_and(|saved| saved.expanded);
        let pinned = pin_supported() && saved.as_ref().is_some_and(|saved| saved.pinned);
        // A saved frame opens on its own display while that display is still there; otherwise the
        // window opens centred on this Ghostex window.
        let (bounds, display_id) = saved
            .as_ref()
            .and_then(|saved| if expanded { saved.full } else { saved.small })
            .and_then(|frame| match frame.display.map(gpui::DisplayId::new) {
                Some(id) => cx.find_display(id).map(|_| (frame.bounds, Some(id))),
                None => Some((frame.bounds, self.main_window_display_id)),
            })
            .unwrap_or_else(|| {
                (
                    self.session_popup_default_bounds(expanded),
                    self.main_window_display_id,
                )
            });
        let title = self.session_popup_title(&key);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            // The Ghostex titlebar on macOS, as the board's window has (native_kanban/window.rs).
            titlebar: Some(TitlebarOptions {
                title: Some(title.clone().into()),
                appears_transparent: cfg!(target_os = "macos"),
                traffic_light_position: cfg!(target_os = "macos")
                    .then(|| gpui::point(px(11.0), px(11.5))),
            }),
            #[cfg(target_os = "macos")]
            app_owns_titlebar_drag: true,
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
            // A second open before this ran brings the first window forward instead.
            if let Some(popup) = app.read(cx).session_popups.get(&row_id) {
                let handle = popup.window;
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                return;
            }
            let observed = app.clone();
            let on_close = app.downgrade();
            let close_row = row_id.clone();
            let root_row = row_id.clone();
            let opened = cx.open_window(options, move |window, cx| {
                window.set_window_title(&title);
                let root = cx.new(|cx| SessionPopupRoot::new(observed, root_row, window, cx));
                // The window's own close button and Cmd+W put the session back in its pane.
                let window_id = window.window_handle().window_id();
                window.on_window_should_close(cx, move |_, cx| {
                    let on_close = on_close.clone();
                    let row_id = close_row.clone();
                    // Deferred so the app is never updated from inside the platform's close.
                    cx.defer(move |cx| {
                        let _ = on_close.update(cx, |app, cx| {
                            app.session_popup_window_closed(&row_id, window_id, cx)
                        });
                    });
                    true
                });
                cx.new(|cx| Root::new(root, window, cx))
            });
            let Ok(handle) = opened else {
                return;
            };
            let handle: AnyWindowHandle = handle.into();
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
                app.session_popups.windows.insert(
                    row_id.clone(),
                    SessionPopupWindow {
                        key,
                        window: handle,
                        native,
                        expanded,
                        pinned,
                    },
                );
                app.session_popup_save_open_list();
                app.session_popup_settle(cx);
            });
        });
    }

    /// After a pop-up opened or closed: the chat and terminal reconciles see what is on screen
    /// now, and the panes redraw (a placeholder, or the session again).
    fn session_popup_settle(&mut self, cx: &mut Context<Self>) {
        for session_id in self.session_popup_shell_sessions() {
            if !self.agents_chat_mode_sessions.contains(&session_id) {
                self.ensure_agents_gpui_engine_terminal_view(session_id, cx);
            }
        }
        self.reconcile_agents_pane_surfaces(cx);
        // The terminal viewer reconcile runs on the redraw this asks for.
        cx.notify();
    }

    /// The window `closed` of `row_id` went away (its close button or Cmd+W): the session is back
    /// in its pane, and the main window stays where it is. A window already brought back or
    /// replaced leaves the current state alone.
    fn session_popup_window_closed(
        &mut self,
        row_id: &str,
        closed: gpui::WindowId,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session_popups
            .get(row_id)
            .is_some_and(|popup| popup.window.window_id() == closed);
        if current {
            self.session_popups.windows.remove(row_id);
            self.session_popup_save_open_list();
            self.session_popup_settle(cx);
        }
    }

    /// Closes `row_id`'s window quietly (Cmd+W inside it): the same as its close button.
    pub(crate) fn close_session_popup(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(popup) = self.session_popups.windows.remove(row_id) else {
            return;
        };
        let handle = popup.window;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
        self.session_popup_save_open_list();
        self.session_popup_settle(cx);
    }

    /// Bring Back, from the placeholder, the pop-up's header or the hotkey: the window closes, the
    /// session shows in this window's focused pane and this window comes forward.
    pub(crate) fn bring_back_session_popup(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if self.session_popups.get(row_id).is_none() {
            return;
        }
        self.close_session_popup(row_id, cx);
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

    /// Pin: the window stays above other apps' windows, or stops.
    pub(crate) fn toggle_session_popup_pin(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if !pin_supported() {
            return;
        }
        let Some(popup) = self.session_popups.windows.get_mut(row_id) else {
            return;
        };
        popup.pinned = !popup.pinned;
        if let Some(native) = popup.native {
            platform::set_window_stays_on_top(native, popup.pinned);
        }
        let pinned = popup.pinned;
        saved::update_session(row_id, |saved| saved.pinned = pinned);
        cx.notify();
    }

    /// Expand and Shrink: the window takes its other size where it stands (the size each was last
    /// given, or a default one), keeping its centre on the same screen.
    pub(crate) fn toggle_session_popup_expanded(
        &mut self,
        row_id: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let default_full = self.session_popup_default_bounds(true);
        let default_small = self.session_popup_default_bounds(false);
        let Some(popup) = self.session_popups.windows.get_mut(row_id) else {
            return;
        };
        popup.expanded = !popup.expanded;
        let expanded = popup.expanded;
        let native = popup.native;
        let saved = saved::session(row_id).unwrap_or_default();
        let target_size = if expanded {
            saved
                .full
                .map_or(default_full.size, |frame| frame.bounds.size)
        } else {
            saved
                .small
                .map_or(default_small.size, |frame| frame.bounds.size)
        };
        // `Window::bounds` is read against the window's own display on macOS, and the move below
        // takes GPUI's global space, so the display's origin is added back there.
        let mut current = window.bounds();
        let display = window.display(cx);
        if cfg!(target_os = "macos")
            && let Some(display) = &display
        {
            current.origin += display.bounds().origin;
        }
        let mut target = Bounds::centered_at(current.center(), target_size);
        if let Some(display) = &display {
            target = clamp_into(target, display.visible_bounds());
        }
        match native {
            // Moves and sizes the window in one step where the OS lets a client place it.
            Some(native) if platform::supported() => {
                platform::set_window_frame(native, target, window.scale_factor())
            }
            _ => window.resize(target.size),
        }
        saved::update_session(row_id, |saved| saved.expanded = expanded);
        cx.notify();
    }

    /// The window moved or changed size: the size it has now is the one its current state (small
    /// or expanded) opens with next time.
    pub(crate) fn session_popup_note_bounds(
        &mut self,
        row_id: &str,
        bounds: Bounds<Pixels>,
        display: Option<gpui::DisplayId>,
    ) {
        let Some(popup) = self.session_popups.get(row_id) else {
            return;
        };
        let expanded = popup.expanded;
        let frame = saved::SavedFrame {
            bounds,
            display: display.map(u64::from),
        };
        saved::update_session(row_id, |saved| {
            if expanded {
                saved.full = Some(frame);
            } else {
                saved.small = Some(frame);
            }
        });
    }

    /// Where a pop-up with nothing saved opens: centred on this Ghostex window, small, or most of
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

    /// The window's title: the session's, falling back to its id.
    pub(crate) fn session_popup_title(&self, key: &SessionKey) -> String {
        self.session_popup_shell_session(key)
            .map(|session_id| self.agents_workspace_tab_display_title(session_id))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Session".to_string())
    }

    /// Forgets this window's pop-ups and returns them for the caller to close: a workspace window
    /// closing takes its pop-ups with it (workspace_windows/close.rs), and they do not come back.
    pub(crate) fn take_session_popup_windows(&mut self) -> Vec<AnyWindowHandle> {
        let handles = self
            .session_popups
            .windows
            .drain()
            .map(|(_, popup)| popup.window)
            .collect();
        saved::set_open(self.workspace_window_slot, Vec::new());
        handles
    }

    fn session_popup_save_open_list(&self) {
        let mut open = self
            .session_popups
            .windows
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        open.sort();
        saved::set_open(self.workspace_window_slot, open);
    }

    /// Launch: the pop-ups this window had open when the app quit open again, each where it was.
    /// They show their session once the window has restored it.
    pub(crate) fn reopen_saved_session_popups(&mut self, cx: &mut Context<Self>) {
        if self.session_popups.reopened {
            return;
        }
        self.session_popups.reopened = true;
        for row_id in saved::open(self.workspace_window_slot) {
            if let Some(key) = SessionKey::parse_sidebar_session_id(&row_id) {
                self.open_session_popup(key, cx);
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
