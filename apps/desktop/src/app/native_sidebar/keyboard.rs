//! The sidebar's keys while one of its controls has focus (focus_order.rs): Up and Down move
//! between session rows, Home and End to the ends, Escape gives the keyboard back. Also where the
//! keyboard goes back to, and the Focus Sidebar hotkey's landing, shared by the desktop (its
//! hotkey entry is gx_store/sidebar_keyboard.rs) and the web build. The host files answer
//! `sidebar_keyboard_active_surface`, `sidebar_keyboard_restore_terminal` and
//! `sidebar_keyboard_row_is_active`.

use std::cell::Cell;

use gpui::{AnyWindowHandle, App, Context, FocusHandle, KeyDownEvent, SharedString, Window};
use web_time::{Duration, Instant};

use super::focus_order::{
    ENTRY_FOCUS_FRAMES, PENDING_FOCUS_FRAMES, PendingFocus, PendingTarget, session_row_id,
    stop_order,
};
use super::scroll::reveal_delta;
use crate::GhostexGpuiApp;

/// Where the keyboard goes back to when it leaves the sidebar.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) enum SidebarKeyboardReturn {
    /// The terminal the Focus Sidebar hotkey took it from.
    Terminal,
    /// The control it took it from, in that window.
    Focus {
        handle: FocusHandle,
        window: AnyWindowHandle,
    },
}

gpui::actions!(native_sidebar_keyboard, [DrawnProbe]);

thread_local! {
    static PROBE_LISTENING: Cell<bool> = const { Cell::new(false) };
    static PROBE_REACHED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the control `handle` belongs to is in `window`'s last drawn frame.
///
/// CDXC:FocusRouting 2026-11-11 WHY:
/// `Window::focus` takes a handle whose control is no longer drawn (a composer whose session closed while the sidebar had the keyboard) and leaves typing going nowhere, and GPUI's focus-lost report only fires when a focus that was drawn stops being drawn, so it misses a handle that was already gone. GPUI has no public "is this handle drawn" question, but it dispatches an action on a handle only when the handle's control is in the drawn frame (`FocusHandle::dispatch_action`), and a global listener sees every dispatched action first; a no-op probe action answers it directly, before the focus moves.
fn focus_handle_drawn(handle: &FocusHandle, window: &mut Window, cx: &mut App) -> bool {
    if !PROBE_LISTENING.with(|listening| listening.replace(true)) {
        cx.on_action(|_: &DrawnProbe, _| PROBE_REACHED.with(|reached| reached.set(true)));
    }
    PROBE_REACHED.with(|reached| reached.set(false));
    handle.dispatch_action(&DrawnProbe, window, cx);
    PROBE_REACHED.with(|reached| reached.get())
}

#[derive(Default)]
pub(crate) struct SidebarKeyboardState {
    /// Recorded by the Focus Sidebar hotkey only; Tab or a click into the sidebar records nothing.
    return_to: Option<SidebarKeyboardReturn>,
}

impl SidebarKeyboardState {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn set_return(&mut self, to: Option<SidebarKeyboardReturn>) {
        self.return_to = to;
    }

    /// The keyboard left the sidebar some other way: nothing is handed back later.
    pub(crate) fn forget_return(&mut self) {
        self.return_to = None;
    }
}

impl GhostexGpuiApp {
    /// The keys the sidebar takes while one of its controls has focus. Enter and Space are not
    /// here: GPUI turns them into the focused control's click on key-up.
    pub(crate) fn native_sidebar_keyboard_key_down(
        app: &mut Self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stops = &app.native_sidebar.focus_stops;
        if stops.entry_handle_focused(window) {
            // The hotkey's entry holds the keyboard on the sidebar's own handle while it waits
            // for its row: nothing is typed anywhere, and Escape cancels it.
            if event.keystroke.key == "escape" {
                app.sidebar_keyboard_give_back(window, cx);
            }
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if !app.native_sidebar_stop_focused(window) {
            return;
        }
        let modifiers = &event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => app.sidebar_keyboard_give_back(window, cx),
            "down" | "up" | "home" | "end" => {
                app.move_native_sidebar_row_focus(event.keystroke.key.as_str(), window, cx)
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    /// Up, Down, Home or End: the session row they name, focused, or scrolled to and focused once
    /// built.
    fn move_native_sidebar_row_focus(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stops = &self.native_sidebar.focus_stops;
        let Some(from) = stops.focused.borrow().clone() else {
            return;
        };
        let rows = stops.session_rows();
        if rows.is_empty() {
            return;
        }
        let viewport = self.native_sidebar.scroll.bounds();
        let at = rows.iter().position(|row| row.key == from.key);
        let order = stop_order(&from, viewport);
        let top = |index: usize| f32::from(rows[index].bounds.top());
        let target = match (key, at) {
            ("home", _) => Some(0),
            ("end", _) => Some(rows.len() - 1),
            ("down", Some(at)) => Some((at + 1).min(rows.len() - 1)),
            ("up", Some(at)) => Some(at.saturating_sub(1)),
            // From a control that is not a row: the row drawn after it, or before it.
            ("down", None) => (0..rows.len()).find(|index| top(*index) > order),
            ("up", None) => (0..rows.len()).rev().find(|index| top(*index) < order),
            _ => None,
        };
        let Some(row) = target.map(|index| rows[index].clone()) else {
            return;
        };
        if row.key == from.key {
            return;
        }
        if row.built
            && let Some(handle) = stops.built(&row.key)
        {
            window.focus(&handle, cx);
            return;
        }
        *stops.pending.borrow_mut() = Some(PendingFocus {
            target: PendingTarget::Key(row.key),
            frames: PENDING_FOCUS_FRAMES,
            window: window.window_handle(),
            origin: window.focused(cx),
            origin_known: true,
            entry: None,
        });
        let scale = stops.scale();
        self.set_native_sidebar_list_scroll_by(reveal_delta(viewport, row.bounds, scale), cx);
        cx.notify();
    }

    /// Starts the Focus Sidebar hotkey's entry: the sidebar, once drawn in `window`, focuses its
    /// active session's row (`native_sidebar_landing`). `origin` is the focus the entry waits from,
    /// when the hotkey was pressed in that window.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn start_native_sidebar_keyboard_entry(
        &mut self,
        window: AnyWindowHandle,
        origin: Option<Option<FocusHandle>>,
    ) {
        *self.native_sidebar.focus_stops.pending.borrow_mut() = Some(PendingFocus {
            target: PendingTarget::Landing,
            frames: ENTRY_FOCUS_FRAMES,
            window,
            origin_known: origin.is_some(),
            origin: origin.flatten(),
            entry: Some(Instant::now() + Duration::from_millis(1500)),
        });
    }

    /// Where the hotkey's entry lands, with the bounds of a row still to be scrolled to (`None`
    /// once it is built): the active session's row, else the first row, else New session.
    pub(super) fn native_sidebar_landing(
        &self,
    ) -> Option<(SharedString, Option<gpui::Bounds<gpui::Pixels>>)> {
        let rows = self.native_sidebar.focus_stops.session_rows();
        let row = rows
            .iter()
            .find(|row| {
                session_row_id(&row.key).is_some_and(|id| self.sidebar_keyboard_row_is_active(id))
            })
            .or_else(|| rows.first());
        match row {
            Some(row) => Some((row.key.clone(), (!row.built).then_some(row.bounds))),
            None => Some((SharedString::new_static("new-session:start"), None)),
        }
    }

    /// The hotkey's entry ran out of time: the first row drawn, else New session.
    pub(super) fn native_sidebar_landing_fallback(&self) -> Option<SharedString> {
        let stops = &self.native_sidebar.focus_stops;
        stops
            .session_rows()
            .into_iter()
            .find(|row| row.built)
            .map(|row| row.key)
            .or_else(|| {
                let start = SharedString::new_static("new-session:start");
                stops.built(&start).map(|_| start)
            })
    }

    /// The window whose sidebar control held focus when its last frame ended.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn native_sidebar_focused_window(&self) -> Option<AnyWindowHandle> {
        self.native_sidebar
            .focus_stops
            .focused_at_end
            .borrow()
            .as_ref()
            .map(|(_, _, window)| *window)
    }

    /// Gives the keyboard back: to the terminal or control the hotkey took it from, else the
    /// active surface.
    pub(crate) fn sidebar_keyboard_give_back(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stops = &self.native_sidebar.focus_stops;
        if stops
            .pending
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.entry.is_some())
        {
            stops.pending.borrow_mut().take();
        }
        match self.native_sidebar.keyboard.return_to.take() {
            Some(SidebarKeyboardReturn::Terminal) => {
                if !self.sidebar_keyboard_restore_terminal(window, cx) {
                    self.sidebar_keyboard_active_surface(window, cx);
                }
            }
            Some(SidebarKeyboardReturn::Focus { handle, window: to }) => {
                self.give_sidebar_keyboard_back_to(handle, to, window, cx)
            }
            None => self.sidebar_keyboard_active_surface(window, cx),
        }
    }

    /// Enter on a session row: its click opened the session; the keyboard goes there, not back.
    pub(crate) fn sidebar_keyboard_open_session_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_sidebar.keyboard.forget_return();
        self.native_sidebar.focus_stops.pending.borrow_mut().take();
        self.sidebar_keyboard_active_surface(window, cx);
    }

    fn give_sidebar_keyboard_back_to(
        &mut self,
        handle: FocusHandle,
        to: AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if to == window.window_handle() {
            self.restore_sidebar_keyboard_focus(handle, window, cx);
            return;
        }
        let app = cx.entity();
        let here = window.window_handle();
        cx.defer(move |cx| {
            let landed = to.update(cx, |_, window, cx| {
                window.activate_window();
                app.update(cx, |app, cx| {
                    app.restore_sidebar_keyboard_focus(handle, window, cx)
                });
            });
            if landed.is_err() {
                let _ = here.update(cx, |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.sidebar_keyboard_active_surface(window, cx)
                    });
                });
            }
        });
    }

    /// The recorded control, when it is still drawn in `window`, else the active surface.
    fn restore_sidebar_keyboard_focus(
        &mut self,
        handle: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.native_sidebar.focus_stops.is_stop(&handle)
            && focus_handle_drawn(&handle, window, cx)
        {
            window.focus(&handle, cx);
        } else {
            self.sidebar_keyboard_active_surface(window, cx);
        }
    }
}
