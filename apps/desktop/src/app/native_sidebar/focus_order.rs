//! The sidebar's keyboard focus order: every always-visible control of the Project and Unified
//! sidebars is a tab stop, Tab and Shift+Tab walk them in the order they are drawn, Enter and Space
//! run the control's own click handler (GPUI fires `on_click` for them on a focused element), and
//! the focused control wears the chat composer's focus ring.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui::Styled as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Bounds, BoxShadow, Context, Div, FocusHandle, Hsla, InteractiveElement as _, MouseButton,
    Pixels, SharedString, Stateful, Window, point, px,
};
use gpui_component::ElementExt as _;

use super::appearance::SidebarAppearance;
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
struct FocusedStop {
    key: SharedString,
    bounds: Bounds<Pixels>,
    place: SidebarStopPlace,
}

/// A Project list row left unbuilt: its first stop (the row), its last (its coordinator fold
/// button when it has one), and where it would be drawn.
#[derive(Clone, Debug)]
pub(crate) struct UnbuiltRow {
    pub(crate) first: SharedString,
    pub(crate) last: SharedString,
    pub(crate) bounds: Bounds<Pixels>,
}

/// A stop Tab stepped to that is not built yet, and the stop focus went to meanwhile: if focus
/// leaves that one (a click, another key), the move is dropped.
struct PendingFocus {
    key: SharedString,
    frames: u8,
    origin: FocusHandle,
}

/// How many frames a stop picked by Tab may take to be built before the move is dropped.
const PENDING_FOCUS_FRAMES: u8 = 3;

/// One focus handle per logical sidebar control, kept while the control is drawn.
#[derive(Default)]
pub(crate) struct SidebarFocusStops {
    handles: RefCell<HashMap<SharedString, (FocusHandle, u64)>>,
    frame: Cell<u64>,
    /// The focused stop and where it was drawn this frame, written during prepaint.
    focused: Rc<RefCell<Option<FocusedStop>>>,
    /// The stop the list was last scrolled to, so it is revealed once per visit.
    revealed: RefCell<Option<SharedString>>,
    /// The Project list's session rows it did not build this frame (they are out of view), with
    /// where they would be drawn (session_list.rs).
    unbuilt_rows: RefCell<Vec<UnbuiltRow>>,
    /// The stop focused in the last frame, to tell which rows a Tab or Shift+Tab stepped over.
    last_focused: RefCell<Option<FocusedStop>>,
    /// The direction of the last Tab (`true`) or Shift+Tab (`false`), until a frame reads it.
    traversal: Rc<Cell<Option<bool>>>,
    /// A row Tab picked that is not built yet: focused once the scroll to it builds it.
    pending: RefCell<Option<PendingFocus>>,
    keystrokes: RefCell<Option<gpui::Subscription>>,
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
        self.focused.borrow_mut().take();
        self.unbuilt_rows.borrow_mut().clear();
    }

    /// A Project list row out of view this frame, at the bounds it would be drawn at.
    pub(crate) fn record_unbuilt_row(&self, row: UnbuiltRow) {
        self.unbuilt_rows.borrow_mut().push(row);
    }

    /// The handle of `key` when the current frame built that control.
    fn built(&self, key: &SharedString) -> Option<FocusHandle> {
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
    /// Enter and Space click. Call it after the element's own mouse-down handlers: GPUI runs one
    /// element's mouse-down listeners last-added first, so this one runs before them.
    ///
    /// CDXC:FocusRouting 2026-11-10 DECISION:
    /// User (Q37, approving the sidebar keyboard plan raised by a review of the nested threads change): no control in either sidebar could take Tab focus. There is one sidebar-wide focus order (Project and Unified): every always-visible card, row, header, filter and button is a stop, in the order it is drawn; Enter and Space run the control's existing click handler; the focused control wears the chat composer's ring (3px ring/20 and a 1px ring line). Controls that only appear on hover, and popup menus, are not stops.
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
        let key = key.into();
        let stops = &self.native_sidebar.focus_stops;
        stops.observe_traversal(cx);
        let handle = stops.handle(&key, cx);
        // Inset: GPUI paints an outer shadow as a filled shape under the element, which shows
        // through a control with no fill of its own as a grey slab instead of a ring.
        let ring = sidebar_focus_ring(appearance.ring, appearance.scale, true);
        let press_handle = handle.clone();
        let element = element
            .track_focus(&handle)
            .focus_visible(move |mut style| {
                style.box_shadow = Some(ring.clone());
                style
            })
            .on_any_mouse_down(cx.listener(
                move |app, event: &gpui::MouseDownEvent, window, cx| {
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
                },
            ));
        let focused = stops.focused.clone();
        element.on_prepaint(move |bounds, window, _| {
            if handle.is_focused(window) {
                *focused.borrow_mut() = Some(FocusedStop { key, bounds, place });
            }
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
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(crate) fn native_sidebar_stop_focused(&self, window: &Window) -> bool {
        self.native_sidebar.focus_stops.any_focused(window)
    }

    /// After the whole sidebar prepainted. A Tab or Shift+Tab that stepped over Project list
    /// rows left unbuilt (out of view) goes to the nearest of them instead: the list scrolls to it
    /// and it takes focus once built. Otherwise the focused `List` stop is scrolled into view, once
    /// per visit, unless it is a project header whose pinned copy is showing.
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
        let stops = &self.native_sidebar.focus_stops;
        let traversal = stops.traversal.take();
        let pending = stops.pending.borrow_mut().take();
        // Focus moved on since the step (a click, another Tab): the step is dropped.
        if let Some(pending) = pending.filter(|pending| pending.origin.is_focused(window)) {
            if let Some(handle) = stops.built(&pending.key) {
                let origin = pending.origin;
                cx.defer_in(window, move |_, window, cx| {
                    if origin.is_focused(window) {
                        window.focus(&handle, cx);
                    }
                });
                return;
            }
            if pending.frames > 0 {
                // Still scrolling to it: the stop Tab reached meanwhile is not revealed.
                *stops.pending.borrow_mut() = Some(PendingFocus {
                    frames: pending.frames - 1,
                    ..pending
                });
                window.request_animation_frame();
                return;
            }
        }
        if self.native_sidebar.pending_scroll_offset.is_some() {
            return;
        }
        let viewport = self.native_sidebar.scroll.bounds();
        let current = stops.focused.borrow().clone();
        let last = stops.last_focused.replace(current.clone());
        let stepped_over = match (traversal, last, current.as_ref()) {
            (Some(forward), Some(last), Some(current)) if last.key != current.key => {
                stepped_over_row(
                    &stops.unbuilt_rows.borrow(),
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
                key,
                frames: PENDING_FOCUS_FRAMES,
                origin,
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

    fn scroll_native_sidebar_list_by(
        &mut self,
        delta: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < px(1.0) {
            return;
        }
        let from = self.native_sidebar.scroll.offset();
        self.native_sidebar.scroll_animation = None;
        self.native_sidebar
            .scroll
            .set_offset(gpui::Point::new(from.x, (from.y - delta).min(px(0.0))));
        window.request_animation_frame();
        cx.notify();
    }
}

/// Where a stop sits in the sidebar's order: a list stop by its top, the chrome above the list
/// before all of it and the bottom bar after all of it.
fn stop_order(stop: &FocusedStop, viewport: Bounds<Pixels>) -> f32 {
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
    rows: &[UnbuiltRow],
    from: f32,
    to: f32,
    forward: bool,
) -> Option<(SharedString, Bounds<Pixels>)> {
    let between = |row: &&UnbuiltRow| {
        let top = f32::from(row.bounds.top());
        if forward {
            from < top && top < to
        } else {
            to < top && top < from
        }
    };
    let by_top = |a: &&UnbuiltRow, b: &&UnbuiltRow| {
        f32::from(a.bounds.top()).total_cmp(&f32::from(b.bounds.top()))
    };
    let passed = rows.iter().filter(between);
    if forward {
        passed
            .min_by(by_top)
            .map(|row| (row.first.clone(), row.bounds))
    } else {
        passed
            .max_by(by_top)
            .map(|row| (row.last.clone(), row.bounds))
    }
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
