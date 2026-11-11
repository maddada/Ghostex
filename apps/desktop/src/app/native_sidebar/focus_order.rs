//! The sidebar's keyboard focus order: every always-visible control of the Project and Unified
//! sidebars is a tab stop, Tab and Shift+Tab walk them in the order they are drawn, Enter and Space
//! run the control's own click handler (GPUI fires `on_click` for them on a focused element), and
//! the focused control wears the chat composer's focus ring. The arrow keys, Escape and the Focus
//! Sidebar hotkey move the same focus (keyboard.rs).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::Styled as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, Bounds, BoxShadow, Context, Div, FocusHandle, Hsla,
    InteractiveElement as _, MouseButton, Pixels, Point, SharedString, Stateful,
    StatefulInteractiveElement as _, Window, point, px, size,
};
use gpui_component::ElementExt as _;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::scroll::reveal_delta;
use crate::GhostexGpuiApp;

/// Where a stop is drawn, which decides whether focusing it may scroll the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarStopPlace {
    /// Inside the scrolling list (`native-sidebar-scroll`): focusing it scrolls it into view.
    List,
    /// Outside it (the selectors block, the Unified header and filter bar, the bottom bar):
    /// always in view, so focusing it never moves the list.
    Fixed,
}

/// The focused stop: its key, where it was drawn and where that is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FocusedStop {
    pub(super) key: SharedString,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) place: SidebarStopPlace,
}

/// A stop drawn this frame, in the order GPUI walks them with Tab.
#[derive(Clone, Debug)]
pub(super) struct DrawnStop {
    pub(super) key: SharedString,
    pub(super) bounds: Bounds<Pixels>,
}

/// One Project list's rows this frame: which it built, and enough to say where the others would
/// be drawn and what their stops are called, without naming each of them every frame.
pub(crate) struct UnbuiltSpan {
    pub(crate) origin: Point<Pixels>,
    pub(crate) width: Pixels,
    pub(crate) scale: f32,
    /// Each row's top and height in unscaled pixels.
    pub(crate) tops: Rc<[f32]>,
    pub(crate) heights: Rc<[f32]>,
    pub(crate) sessions: Rc<[Arc<NativeSidebarSession>]>,
    /// The rows in view, which the list built.
    pub(crate) built: Range<usize>,
    /// The focused row, built wherever it is (session_list.rs).
    pub(crate) kept: Option<usize>,
}

impl UnbuiltSpan {
    fn unbuilt(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.sessions.len())
            .filter(|index| !self.built.contains(index) && self.kept != Some(*index))
    }

    fn bounds(&self, index: usize) -> Bounds<Pixels> {
        Bounds::new(
            self.origin + point(px(0.0), px(self.tops[index] * self.scale)),
            size(self.width, px(self.heights[index] * self.scale)),
        )
    }

    /// The row's first stop (the row) and its last (its coordinator fold button, when it has one).
    fn keys(&self, index: usize) -> (SharedString, SharedString) {
        let session = &self.sessions[index];
        let first = session_stop_key(&session.session_id);
        let last = super::decorations::coordinator_fold_button(session)
            .map(|_| coordinator_fold_stop_key(&session.session_id))
            .unwrap_or_else(|| first.clone());
        (first, last)
    }
}

/// A session row of either sidebar, drawn or not, for the arrow keys and the hotkey's landing.
#[derive(Clone, Debug)]
pub(super) struct SessionRowStop {
    pub(super) key: SharedString,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) built: bool,
}

/// What a pending move goes to once it is built.
#[derive(Clone, Debug)]
pub(super) enum PendingTarget {
    Key(SharedString),
    /// The Focus Sidebar hotkey's landing (keyboard.rs), chosen once the sidebar has drawn.
    Landing,
}

/// A move whose stop is not built yet, and the focus it started from: if focus leaves that one
/// (a click, another key), the move is dropped.
pub(super) struct PendingFocus {
    pub(super) target: PendingTarget,
    pub(super) frames: u8,
    /// The window the stop will be drawn in.
    pub(super) window: AnyWindowHandle,
    pub(super) origin: Option<FocusHandle>,
    /// False until the window the stop is drawn in has drawn once (the hotkey pressed in another
    /// window): the origin is read there then.
    pub(super) origin_known: bool,
    /// The Focus Sidebar hotkey's entry (`None` for a Tab or arrow move). The keyboard belongs to
    /// it meanwhile (render/root.rs forwards nothing).
    pub(super) entry: Option<EntryWait>,
}

/// Which entry it is, the window the hotkey was pressed in (where the keyboard goes back when the
/// entry ends without landing), and what had focus there when the entry started, while the sidebar
/// is in another window.
#[derive(Clone, Debug)]
pub(super) struct EntryWait {
    pub(super) id: u64,
    pub(super) from: AnyWindowHandle,
    pub(super) from_focus: Option<FocusHandle>,
}

/// How many frames a stop picked by Tab or an arrow may take to be built before the move is
/// dropped, and how many the hotkey's entry may wait for the sidebar to draw and scroll.
pub(super) const PENDING_FOCUS_FRAMES: u8 = 3;
pub(super) const ENTRY_FOCUS_FRAMES: u8 = 60;

/// One focus handle per logical sidebar control, kept while the control is drawn.
#[derive(Default)]
pub(crate) struct SidebarFocusStops {
    handles: RefCell<HashMap<SharedString, (FocusHandle, u64)>>,
    frame: Cell<u64>,
    /// The focused stop and where it was drawn this frame, written during prepaint.
    pub(super) focused: Rc<RefCell<Option<FocusedStop>>>,
    /// This frame's stops in Tab order, and last frame's.
    drawn: Rc<RefCell<Vec<DrawnStop>>>,
    prev_drawn: RefCell<Vec<DrawnStop>>,
    /// The stop that held focus when the last frame ended, and its window.
    pub(super) focused_at_end: RefCell<Option<(SharedString, FocusHandle, AnyWindowHandle)>>,
    /// The stop the list was last scrolled to, so it is revealed once per visit.
    revealed: RefCell<Option<SharedString>>,
    /// The Project lists' rows this frame (session_list.rs).
    unbuilt: RefCell<Vec<UnbuiltSpan>>,
    /// The stop focused in the last frame, to tell which rows a Tab or Shift+Tab stepped over.
    last_focused: RefCell<Option<FocusedStop>>,
    /// The direction of the last Tab (`true`) or Shift+Tab (`false`), until a frame reads it.
    traversal: Rc<Cell<Option<bool>>>,
    pub(super) pending: RefCell<Option<PendingFocus>>,
    keystrokes: RefCell<Option<gpui::Subscription>>,
    /// The sidebar's scale in the last frame, for scrolls the arrow keys start between frames.
    scale: Cell<f32>,
    /// Open from `begin_frame` until the sidebar's last prepaint: only controls built and
    /// prepainted for the sidebar in that span are its stops.
    recording: Rc<Cell<bool>>,
    /// The Focus Sidebar hotkey's entry holds the keyboard here while it waits for its row
    /// (render.rs draws it on a zero-size child of the sidebar).
    entry: RefCell<Option<FocusHandle>>,
    entry_ids: Cell<u64>,
}

impl SidebarFocusStops {
    /// A sidebar render starts: the handles of controls the last frame did not draw are dropped,
    /// and its bounds are forgotten. The last frame includes its prepaint, where the Project
    /// sidebar builds the session rows it shows (session_list.rs), so this runs here rather than
    /// when the render returns.
    pub(crate) fn begin_frame(&self) {
        let frame = self.frame.get();
        self.handles
            .borrow_mut()
            .retain(|_, (_, drawn)| *drawn == frame);
        self.frame.set(frame.wrapping_add(1));
        self.recording.set(true);
        self.focused.borrow_mut().take();
        let mut drawn = self.drawn.borrow_mut();
        let mut prev = self.prev_drawn.borrow_mut();
        std::mem::swap(&mut *drawn, &mut *prev);
        drawn.clear();
        self.unbuilt.borrow_mut().clear();
    }

    pub(super) fn scale(&self) -> f32 {
        Some(self.scale.get())
            .filter(|scale| *scale > 0.0)
            .unwrap_or(1.0)
    }

    /// A Project list's rows this frame.
    pub(crate) fn record_unbuilt_span(&self, span: UnbuiltSpan) {
        self.unbuilt.borrow_mut().push(span);
    }

    /// The session whose Project row (or its fold button) holds focus, so its list builds that row
    /// even out of view.
    pub(crate) fn focused_session_row(&self) -> Option<SharedString> {
        let focused = self.focused_at_end.borrow();
        let (key, _, _) = focused.as_ref()?;
        key.strip_prefix("session:")
            .or_else(|| key.strip_prefix("coordinator-fold:"))
            .map(SharedString::new)
    }

    /// The handle the hotkey's entry parks the keyboard on: not a tab stop, and never focused by a
    /// click (its element has no size).
    pub(crate) fn entry_handle(&self, cx: &App) -> FocusHandle {
        self.entry
            .borrow_mut()
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    pub(super) fn entry_handle_focused(&self, window: &Window) -> bool {
        self.entry
            .borrow()
            .as_ref()
            .is_some_and(|handle| handle.is_focused(window))
    }

    /// Whether the sidebar is being drawn now (see `recording`).
    pub(super) fn recording(&self) -> bool {
        self.recording.get()
    }

    /// The handle of `key` when the current frame built that control.
    pub(super) fn built(&self, key: &SharedString) -> Option<FocusHandle> {
        let frame = self.frame.get();
        self.handles
            .borrow()
            .get(key)
            .filter(|(_, drawn)| *drawn == frame)
            .map(|(handle, _)| handle.clone())
    }

    /// Notes the direction of each Tab and Shift+Tab that GPUI's root turned into its focus-next or
    /// focus-previous action (no element sees those); a Tab something else took, like the
    /// composer's Tab to queue, is not a step.
    fn observe_traversal(&self, cx: &mut App) {
        if self.keystrokes.borrow().is_some() {
            return;
        }
        let traversal = self.traversal.clone();
        let subscription = cx.observe_keystrokes(move |event, _, _| {
            match event.action.as_ref().map(|action| action.name()) {
                Some("root::Tab") => traversal.set(Some(true)),
                Some("root::TabPrev") => traversal.set(Some(false)),
                _ => {}
            }
        });
        *self.keystrokes.borrow_mut() = Some(subscription);
    }

    /// The handle of the control `key` names, the same one on every frame it is drawn.
    pub(crate) fn handle(&self, key: &SharedString, cx: &App) -> FocusHandle {
        let frame = self.frame.get();
        let mut handles = self.handles.borrow_mut();
        let entry = handles
            .entry(key.clone())
            .or_insert_with(|| (cx.focus_handle().tab_stop(true).tab_index(0), frame));
        entry.1 = frame;
        entry.0.clone()
    }

    /// Whether a sidebar control holds keyboard focus in `window`.
    pub(crate) fn any_focused(&self, window: &Window) -> bool {
        self.handles
            .borrow()
            .values()
            .any(|(handle, _)| handle.is_focused(window))
    }

    /// Whether `handle` is one of the sidebar's stops.
    pub(crate) fn is_stop(&self, handle: &FocusHandle) -> bool {
        self.handles
            .borrow()
            .values()
            .any(|(stop, _)| stop == handle)
    }

    /// Whether the Focus Sidebar hotkey's entry still waits for its row, until it lands, is
    /// cancelled or is ended by its timer (keyboard.rs): the keyboard is the sidebar's meanwhile
    /// (render/root.rs forwards nothing, Escape cancels, the hotkey gives it back).
    pub(crate) fn entry_pending(&self) -> bool {
        self.pending
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.entry.is_some())
    }

    /// The hotkey's entry still waiting, and the window its sidebar is in.
    pub(super) fn pending_entry(&self) -> Option<(EntryWait, AnyWindowHandle)> {
        let pending = self.pending.borrow();
        let pending = pending.as_ref()?;
        Some((pending.entry.clone()?, pending.window))
    }

    /// Whether `handle` is the entry's own parking handle, which is never a place to give the
    /// keyboard back to.
    pub(crate) fn is_entry_handle(&self, handle: &FocusHandle) -> bool {
        self.entry.borrow().as_ref() == Some(handle)
    }

    pub(super) fn next_entry_id(&self) -> u64 {
        let id = self.entry_ids.get().wrapping_add(1);
        self.entry_ids.set(id);
        id
    }

    /// Every session row of this frame, drawn or not, top to bottom.
    pub(super) fn session_rows(&self) -> Vec<SessionRowStop> {
        let mut rows: Vec<SessionRowStop> = self
            .drawn
            .borrow()
            .iter()
            .filter(|stop| session_row_id(&stop.key).is_some())
            .map(|stop| SessionRowStop {
                key: stop.key.clone(),
                bounds: stop.bounds,
                built: true,
            })
            .collect();
        for span in self.unbuilt.borrow().iter() {
            rows.extend(span.unbuilt().map(|index| SessionRowStop {
                key: span.keys(index).0,
                bounds: span.bounds(index),
                built: false,
            }));
        }
        rows.sort_by(|a, b| f32::from(a.bounds.top()).total_cmp(&f32::from(b.bounds.top())));
        rows
    }

    /// The stop that takes focus when `removed` disappears: the first one after it in last
    /// frame's order that this frame drew again, else the nearest one before it.
    fn successor_of(&self, removed: &SharedString) -> Option<FocusHandle> {
        let prev = self.prev_drawn.borrow();
        let at = prev.iter().position(|stop| stop.key == *removed)?;
        let drawn: HashSet<SharedString> = self
            .drawn
            .borrow()
            .iter()
            .map(|stop| stop.key.clone())
            .collect();
        prev[at + 1..]
            .iter()
            .chain(prev[..at].iter().rev())
            .find(|stop| drawn.contains(&stop.key))
            .and_then(|stop| self.built(&stop.key))
    }
}

/// Adds `stop` to this frame's order. gpui-component's `on_prepaint` reports through a canvas
/// appended as the element's last child, so a control inside another (a card's crew toggle, a
/// chip's remove button) reports before the control that contains it, while GPUI puts the
/// container's tab stop first (div.rs registers it before painting its children). The stops
/// already recorded inside `stop`'s bounds at the end of the list are those children (a subtree
/// prepaints right before its trailing canvas, and sidebar controls never overlap otherwise), so
/// `stop` goes in front of them.
fn record_drawn(drawn: &mut Vec<DrawnStop>, stop: DrawnStop) {
    let inside = |inner: &Bounds<Pixels>| {
        let slack = px(0.5);
        inner.left() >= stop.bounds.left() - slack
            && inner.top() >= stop.bounds.top() - slack
            && inner.right() <= stop.bounds.right() + slack
            && inner.bottom() <= stop.bounds.bottom() + slack
    };
    let mut at = drawn.len();
    while at > 0 && inside(&drawn[at - 1].bounds) {
        at -= 1;
    }
    drawn.insert(at, stop);
}

/// The chat composer's focus ring (3px of the ring colour at a fifth, then a 1px line of it), as
/// shadows so focusing a control moves nothing. `inset` draws it inside the control's box.
pub(crate) fn sidebar_focus_ring(ring: Hsla, scale: f32, inset: bool) -> Vec<BoxShadow> {
    let shadow = |color: Hsla, spread: f32| BoxShadow {
        color,
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(spread * scale),
        inset,
    };
    vec![shadow(ring.opacity(0.2), 3.0), shadow(ring, 1.0)]
}

impl GhostexGpuiApp {
    /// Makes `element` the sidebar control `key`: a tab stop with the composer's focus ring that
    /// Enter and Space click. Call it after the element's own mouse-down and click handlers: GPUI
    /// runs one element's mouse-down listeners last-added first, so this one runs before them, and
    /// its click listeners first-added first, so a session row's own click runs before this one.
    ///
    /// CDXC:FocusRouting 2026-11-11 DECISION:
    /// PR #564 (sebastianarena), following CodeRabbit's keyboard focus note on #539: no control in either sidebar could take Tab focus. There is one sidebar-wide focus order (Project and Unified): every always-visible card, row, header, filter and button is a stop, in the order it is drawn; Enter and Space run the control's existing click handler; the focused control wears the chat composer's ring (3px ring/20 and a 1px ring line). Controls that only appear on hover, and popup menus, are not stops. Then, on the user's ask on PR #564 ("it would be good if your Tab stops and that hotkey ended up as one keyboard model"), which extends the 2026-11-10 rule: the focused stop is the only sidebar cursor; Up and Down move it between session rows in drawn order without wrapping (from any other control, to the row drawn after or before it), Home and End to the first and last row; Escape or the hotkey give the keyboard back; Enter on a session row also hands the keyboard to the session it opened, while Space only clicks. The user merged it as is (GitHub page Q2523, 2026-11-11).
    ///
    /// CDXC:FocusRouting 2026-11-10 WHY:
    /// A tab stop is focusable, and GPUI focuses a focusable element on every mouse-down. Before this, no sidebar control could take focus, so a click on a header or chip left typing going to the chat or terminal, and the root's key routing (render/root.rs) only forwards keys while nothing, or its own handle, is focused; a click that focused a sidebar control would have taken typing (and a terminal's IME) away. So a press only moves focus while another sidebar control already has it (keyboard use of the sidebar is under way: the clicked control becomes what Space and Enter act on, moved explicitly before the control's own handlers, some of which stop propagation); every other press cancels the focus `track_focus` would add, leaving focus where it was.
    pub(crate) fn sidebar_focus_stop(
        &self,
        element: Stateful<Div>,
        key: impl Into<SharedString>,
        place: SidebarStopPlace,
        appearance: &SidebarAppearance,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let stops = &self.native_sidebar.focus_stops;
        // CDXC:FocusRouting 2026-11-11 WHY:
        // Sidebar controls are also drawn outside the sidebar (the Claude header draws its top row while the sidebar is hidden, app/claude_web/header.rs). Those copies are not its stops: they would record into a sidebar frame that never starts, so the records piled up, and focusing one counted as the sidebar having the keyboard. Only controls built while the sidebar itself is being drawn become stops.
        if !stops.recording() {
            return element;
        }
        let key = key.into();
        stops.observe_traversal(cx);
        let handle = stops.handle(&key, cx);
        // Inset: GPUI paints an outer shadow as a filled shape under the element, which shows
        // through a control with no fill of its own as a grey slab instead of a ring.
        let ring = sidebar_focus_ring(appearance.ring, appearance.scale, true);
        let press_handle = handle.clone();
        let session_row = session_row_id(&key).is_some();
        let element = element
            .track_focus(&handle)
            .focus_visible(move |mut style| {
                style.box_shadow = Some(ring.clone());
                style
            })
            .on_any_mouse_down(
                cx.listener(move |app, event: &gpui::MouseDownEvent, window, cx| {
                    // A control inside this one (a card's crew toggle) already took the press.
                    if window.default_prevented() {
                        return;
                    }
                    if event.button == MouseButton::Left
                        && !press_handle.is_focused(window)
                        && app.native_sidebar.focus_stops.any_focused(window)
                    {
                        window.focus(&press_handle, cx);
                    }
                    window.prevent_default();
                }),
            )
            // Enter on a session row: after the row's own click, which selected the session once,
            // the keyboard follows the session it opened.
            .when(session_row, |element| {
                element.on_click(cx.listener(|_, event: &gpui::ClickEvent, window, cx| {
                    if let gpui::ClickEvent::Keyboard(click) = event
                        && click.button == gpui::KeyboardButton::Enter
                    {
                        cx.defer_in(window, |app, window, cx| {
                            if app.native_sidebar_stop_focused(window) {
                                app.sidebar_keyboard_open_session_surface(window, cx);
                            }
                        });
                    }
                }))
            });
        let focused = stops.focused.clone();
        let drawn = stops.drawn.clone();
        let recording = stops.recording.clone();
        element.on_prepaint(move |bounds, window, _| {
            if !recording.get() {
                return;
            }
            if handle.is_focused(window) {
                *focused.borrow_mut() = Some(FocusedStop {
                    key: key.clone(),
                    bounds,
                    place,
                });
            }
            record_drawn(&mut drawn.borrow_mut(), DrawnStop { key, bounds });
        })
    }

    /// The pinned copy of a project header (sticky.rs): not a stop of its own, the header in the
    /// list is. It wears that header's ring, inset because the pinned bar clips outside its box,
    /// and a press on it moves focus to that header under the same rule as a stop.
    pub(crate) fn sidebar_focus_mirror(
        &self,
        element: Div,
        key: impl Into<SharedString>,
        appearance: &SidebarAppearance,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let key = key.into();
        let handle = self.native_sidebar.focus_stops.handle(&key, cx);
        let ring = (handle.is_focused(window) && window.last_input_was_keyboard())
            .then(|| sidebar_focus_ring(appearance.ring, appearance.scale, true));
        element
            .when_some(ring, |element, ring| {
                element.rounded(px(5.0 * appearance.scale)).shadow(ring)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _: &gpui::MouseDownEvent, window, cx| {
                    if window.default_prevented()
                        || handle.is_focused(window)
                        || !app.native_sidebar.focus_stops.any_focused(window)
                    {
                        return;
                    }
                    window.focus(&handle, cx);
                    window.prevent_default();
                }),
            )
    }

    /// Whether a sidebar control holds keyboard focus (render/root.rs leaves keys alone then).
    pub(crate) fn native_sidebar_stop_focused(&self, window: &Window) -> bool {
        self.native_sidebar.focus_stops.any_focused(window)
    }

    /// After the whole sidebar prepainted. A pending move (Tab past rows left unbuilt, an arrow to
    /// one, the hotkey's entry) is carried on; a focused control that disappeared hands its focus
    /// on; otherwise the focused `List` stop is scrolled into view, once per visit, unless it is a
    /// project header whose pinned copy is showing.
    ///
    /// CDXC:FocusRouting 2026-11-10 WHY:
    /// The Project list builds only the session rows in view (session_list.rs), and GPUI's Tab
    /// walks only what the last frame built, so Shift+Tab from the bottom bar landed on the last
    /// row in view and Tab from a project header skipped its rows below the fold.
    pub(crate) fn reveal_focused_native_sidebar_stop(
        &mut self,
        scale: f32,
        pinned_header: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_sidebar.focus_stops.scale.set(scale);
        self.native_sidebar.focus_stops.recording.set(false);
        let traversal = self.native_sidebar.focus_stops.traversal.take();
        if self.carry_pending_native_sidebar_focus(scale, window, cx) {
            return;
        }
        let stops = &self.native_sidebar.focus_stops;
        let current = stops.focused.borrow().clone();
        let window_handle = window.window_handle();
        let previous_end = stops.focused_at_end.replace(
            current
                .as_ref()
                .and_then(|stop| Some((stop.key.clone(), stops.built(&stop.key)?, window_handle))),
        );
        if current.is_none() {
            /*
            CDXC:FocusRouting 2026-11-11 WHY:
            A focused control that disappears (its filter removed, its session closed) left nothing focused, so the next Tab started from the window's first stop and the root's key routing took the keys again. Its handle outlives the control by one frame (`begin_frame` drops it a frame later), so it is told apart by its key missing from this frame's stops while its handle still holds focus, not by GPUI dropping it; a row merely scrolled out of view is never missing, because its list keeps the focused row built.
            */
            if let Some((key, removed, in_window)) = previous_end
                && in_window == window_handle
                && removed.is_focused(window)
                && let Some(successor) = stops.successor_of(&key)
            {
                cx.defer_in(window, move |_, window, cx| {
                    if removed.is_focused(window) {
                        window.focus(&successor, cx);
                    }
                });
                return;
            }
            if !stops.entry_pending() {
                self.native_sidebar.keyboard.forget_return();
            }
        }
        if self.native_sidebar.pending_scroll_offset.is_some() {
            return;
        }
        let stops = &self.native_sidebar.focus_stops;
        let viewport = self.native_sidebar.scroll.bounds();
        let last = stops.last_focused.replace(current.clone());
        let stepped_over = match (traversal, last, current.as_ref()) {
            (Some(forward), Some(last), Some(current)) if last.key != current.key => {
                stepped_over_row(
                    &stops.unbuilt.borrow(),
                    stop_order(&last, viewport),
                    stop_order(current, viewport),
                    forward,
                )
            }
            _ => None,
        };
        if let Some((key, bounds)) = stepped_over
            && let Some(origin) = current.as_ref().and_then(|stop| stops.built(&stop.key))
        {
            *stops.pending.borrow_mut() = Some(PendingFocus {
                target: PendingTarget::Key(key),
                frames: PENDING_FOCUS_FRAMES,
                window: window_handle,
                origin: Some(origin),
                origin_known: true,
                entry: None,
            });
            self.scroll_native_sidebar_list_by(reveal_delta(viewport, bounds, scale), window, cx);
            return;
        }
        let Some(current) = current.filter(|stop| stop.place == SidebarStopPlace::List) else {
            stops.revealed.borrow_mut().take();
            return;
        };
        if stops.revealed.borrow().as_ref() == Some(&current.key) {
            return;
        }
        *stops.revealed.borrow_mut() = Some(current.key.clone());
        if pinned_header.is_some_and(|group| current.key == project_header_stop_key(group)) {
            return;
        }
        let delta = reveal_delta(viewport, current.bounds, scale);
        self.scroll_native_sidebar_list_by(delta, window, cx);
    }

    /// Carries a pending move on by one frame. True while it holds the frame (nothing else of the
    /// pass runs then).
    fn carry_pending_native_sidebar_focus(
        &mut self,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let stops = &self.native_sidebar.focus_stops;
        let Some(mut pending) = stops.pending.borrow_mut().take() else {
            return false;
        };
        if pending.window != window.window_handle() {
            *stops.pending.borrow_mut() = Some(pending);
            return false;
        }
        if !pending.origin_known {
            pending.origin = window.focused(cx);
            pending.origin_known = true;
        }
        // Focus moved on since the move started (a click, another key): the move is dropped.
        if window.focused(cx) != pending.origin {
            return false;
        }
        let target = match pending.target.clone() {
            PendingTarget::Key(key) => Some((key, None)),
            PendingTarget::Landing => self.native_sidebar_landing(),
        };
        let stops = &self.native_sidebar.focus_stops;
        let target = match target {
            Some((key, _)) if stops.built(&key).is_some() => Some(key),
            Some((key, Some(bounds))) => {
                // Out of view: scroll to it and focus it once it is built.
                pending.target = PendingTarget::Key(key);
                *stops.pending.borrow_mut() = Some(pending);
                let viewport = self.native_sidebar.scroll.bounds();
                self.scroll_native_sidebar_list_by(
                    reveal_delta(viewport, bounds, scale),
                    window,
                    cx,
                );
                window.request_animation_frame();
                return true;
            }
            Some((key, None)) if pending.frames > 0 => {
                pending.frames -= 1;
                pending.target = PendingTarget::Key(key);
                *stops.pending.borrow_mut() = Some(pending);
                window.request_animation_frame();
                return true;
            }
            // The hotkey's row never came: the first row drawn, else New session.
            _ if pending.entry.is_some() => self.native_sidebar_landing_fallback(),
            _ => None,
        };
        let stops = &self.native_sidebar.focus_stops;
        let Some(handle) = target.and_then(|key| stops.built(&key)) else {
            // The hotkey's entry found nothing to land on: the keyboard it holds goes back.
            if pending.entry.is_some() {
                cx.defer_in(window, |app, window, cx| {
                    app.sidebar_keyboard_give_back(window, cx)
                });
                return true;
            }
            return false;
        };
        let origin = pending.origin;
        cx.defer_in(window, move |_, window, cx| {
            if window.focused(cx) == origin {
                window.focus(&handle, cx);
            }
        });
        true
    }

    /// During the sidebar's prepaint: the scrolled list paints on the next frame even in an idle
    /// window.
    fn scroll_native_sidebar_list_by(
        &mut self,
        delta: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.set_native_sidebar_list_scroll_by(delta, cx) {
            window.request_animation_frame();
        }
    }

    /// Outside a frame (a key handler): `request_animation_frame` needs the view being drawn, so
    /// only the notify asks for the next frame. False when there was nothing to scroll.
    pub(super) fn set_native_sidebar_list_scroll_by(
        &mut self,
        delta: Pixels,
        cx: &mut Context<Self>,
    ) -> bool {
        if delta.abs() < px(1.0) {
            return false;
        }
        let from = self.native_sidebar.scroll.offset();
        self.native_sidebar.scroll_animation = None;
        self.native_sidebar
            .scroll
            .set_offset(gpui::Point::new(from.x, (from.y - delta).min(px(0.0))));
        cx.notify();
        true
    }
}

/// Where a stop sits in the sidebar's order: a list stop by its top, the chrome above the list
/// before all of it and the bottom bar after all of it.
pub(super) fn stop_order(stop: &FocusedStop, viewport: Bounds<Pixels>) -> f32 {
    match stop.place {
        SidebarStopPlace::List => f32::from(stop.bounds.top()),
        SidebarStopPlace::Fixed if stop.bounds.center().y < viewport.top() => f32::NEG_INFINITY,
        SidebarStopPlace::Fixed => f32::INFINITY,
    }
}

/// The stop a step from `from` to `to` should land on instead, when it passed over unbuilt rows:
/// the nearest row's first stop going forward, its last stop going back, with the row's bounds.
/// A step that wrapped around (Tab from the last stop to the first) passes over nothing.
fn stepped_over_row(
    spans: &[UnbuiltSpan],
    from: f32,
    to: f32,
    forward: bool,
) -> Option<(SharedString, Bounds<Pixels>)> {
    let between = |top: f32| {
        if forward {
            from < top && top < to
        } else {
            to < top && top < from
        }
    };
    let mut best: Option<(&UnbuiltSpan, usize, f32)> = None;
    for span in spans {
        for index in span.unbuilt() {
            let top = f32::from(span.bounds(index).top());
            if !between(top) {
                continue;
            }
            let better = best.is_none_or(|(_, _, best_top)| {
                if forward {
                    top < best_top
                } else {
                    top > best_top
                }
            });
            if better {
                best = Some((span, index, top));
            }
        }
    }
    let (span, index, _) = best?;
    let (first, last) = span.keys(index);
    Some((if forward { first } else { last }, span.bounds(index)))
}

/// The session a session row's stop belongs to, in either sidebar.
pub(super) fn session_row_id(key: &str) -> Option<&str> {
    key.strip_prefix("session:")
        .or_else(|| key.strip_prefix("unified-session:"))
}

/// The stop key of a coordinator row's fold button (decorations.rs).
pub(crate) fn coordinator_fold_stop_key(session_id: &str) -> SharedString {
    format!("coordinator-fold:{session_id}").into()
}

/// The stop key of a Project sidebar session row, built or not.
pub(crate) fn session_stop_key(session_id: &str) -> SharedString {
    format!("session:{session_id}").into()
}

/// The stop key of a project header, shared by its row in the list and its pinned copy.
pub(crate) fn project_header_stop_key(group_id: &str) -> SharedString {
    format!("project-header:{group_id}").into()
}
