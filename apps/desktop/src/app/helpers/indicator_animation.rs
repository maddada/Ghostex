use gpui::{
    AnyElement, App, Bounds, Element, ElementId, EntityId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Window, WindowId,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    time::Duration,
};
use web_time::Instant;

/// CDXC:SessionChat 2026-09-25 WHY:
/// Every repeating indicator (chat working strip, spinners, machine tabs, the sidebar's empty state) advances on one shared thirty-frames-per-second timer. Supersedes the 2026-09-23 display-rate path for the chat: notifying one view still marks its ancestors dirty and the uncached window root renders on every draw, so each chat indicator frame rebuilt the whole window at 120 Hz on ProMotion displays and kept the main thread saturated for as long as an agent worked.
/// A view that registers through `render_indicator_frames_animation_only` keeps its cached content (the chat's transcript, native_chat/transcript_host.rs) on those frames; the others render whole. Seven frames per second (before 2026-09-23) made the compaction bar and spinners visibly step, so do not lower the rate below thirty.
/// SEE-ALSO: apps/desktop/src/app/native_chat/transcript_host.rs, apps/desktop/src/app/native_chat/render.rs, apps/desktop/src/app/native_sidebar/machines.rs.
/// CDXC:SessionChat 2026-10-01 WHY:
/// Each frame used to be armed 33ms after the render that asked for it, so the wait for the display's next frame and the render itself were added on top: measured in the GPUI web build, the account-switch card's bar got a new frame only every 50 to 85ms (about 17 per second) and jumped up to 29px at a time. Frames now land on a fixed grid, one interval after the previous frame was due, so the rate really is thirty. An indicator whose eye-tracked motion needs more (`ThrottledAnimation::frame_interval`) may ask for a shorter interval for itself, or for every display frame; keep that to short-lived indicators, for the whole-window cost above. A 16ms timer is not sixty frames: its wake-up usually misses the display frame it aimed at and draws on the one after, which measured about 27 frames a second.
pub(crate) const INDICATOR_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// A clock idle this many intervals starts over instead of catching up on the frames it missed.
const FRAME_GRID_RESTART_INTERVALS: u32 = 4;

thread_local! {
    /// Views whose indicator frames leave their cached content alone.
    static ANIMATION_ONLY_VIEWS: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// Views that already have an animation-only frame scheduled, with when it is due.
    static FRAME_TICKS_PENDING: RefCell<HashMap<EntityId, Instant>> = RefCell::new(HashMap::new());
    /// When each view's last animation-only frame was due, which the next one is measured from.
    static LAST_VIEW_FRAMES: RefCell<HashMap<EntityId, Instant>> = RefCell::new(HashMap::new());
    /// When each window's last shared-timer frame was due.
    static LAST_WINDOW_FRAMES: RefCell<HashMap<WindowId, Instant>> = RefCell::new(HashMap::new());
    /// Views whose coming render was asked for by an indicator frame and by nothing else.
    static ANIMATION_ONLY_RENDERS: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// Windows with a shared-timer frame scheduled, when it is due, and the views it will notify.
    static TIMER_FRAMES_PENDING: RefCell<Vec<(WindowId, Instant, HashSet<EntityId>)>> = const { RefCell::new(Vec::new()) };
}

/// Lets the view being rendered keep what it draws cached on its indicator frames; call it from
/// that view's render. Only a view whose frame stays cheap when nothing but an indicator changed should.
pub(crate) fn render_indicator_frames_animation_only(view: EntityId) {
    ANIMATION_ONLY_VIEWS.with_borrow_mut(|views| {
        views.insert(view);
    });
}

/// Whether the render `view` is about to do was asked for by an indicator frame alone, so
/// everything it draws cached may stay cached. Consumed by the call.
pub(crate) fn take_animation_only_render(view: EntityId) -> bool {
    ANIMATION_ONLY_RENDERS.with_borrow_mut(|views| views.remove(&view))
}

/// Schedules the next indicator frame for the view being rendered, `interval` after its last one.
pub(crate) fn request_indicator_frame(interval: Duration, window: &mut Window, cx: &mut App) {
    let view = window.current_view();
    if ANIMATION_ONLY_VIEWS.with_borrow(|views| views.contains(&view)) {
        request_animation_only_frame(view, interval, window, cx);
    } else {
        request_timer_frame(view, interval, window, cx);
    }
}

/// When the next frame of `key` is due: an interval after its last one, or after `now` for a clock
/// that has not drawn lately.
fn next_frame_due<K: Eq + Hash>(
    last_frames: &mut HashMap<K, Instant>,
    key: &K,
    interval: Duration,
    now: Instant,
) -> Instant {
    let idle = |interval: Duration, last: &Instant| {
        now.saturating_duration_since(*last) >= interval * FRAME_GRID_RESTART_INTERVALS
    };
    last_frames.retain(|_, last| !idle(INDICATOR_FRAME_INTERVAL.max(interval), last));
    match last_frames.get(key).filter(|last| !idle(interval, last)) {
        // A render that ran past the grid is followed at once rather than an interval later.
        Some(last) => (*last + interval).max(now),
        None => now + interval,
    }
}

fn request_animation_only_frame(
    view: EntityId,
    interval: Duration,
    window: &mut Window,
    cx: &mut App,
) {
    let now = Instant::now();
    let due = LAST_VIEW_FRAMES.with_borrow_mut(|last| next_frame_due(last, &view, interval, now));
    // One frame per view: a later request only replaces a pending one that is due later.
    let replaced = FRAME_TICKS_PENDING.with_borrow_mut(|pending| {
        if pending.get(&view).is_some_and(|at| *at <= due) {
            return false;
        }
        pending.insert(view, due);
        true
    });
    if !replaced {
        return;
    }
    if interval.is_zero() {
        draw_view_next_frame(view, due, window);
        return;
    }
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(due.saturating_duration_since(Instant::now()))
            .await;
        if FRAME_TICKS_PENDING.with_borrow(|pending| pending.get(&view) != Some(&due)) {
            // A sooner frame took this one's place.
            return;
        }
        let scheduled = handle.update(cx, |_, window, _| draw_view_next_frame(view, due, window));
        if scheduled.is_err() {
            FRAME_TICKS_PENDING.with_borrow_mut(|pending| {
                pending.remove(&view);
            });
        }
    })
    .detach();
}

/// Draws `view`'s animation-only frame `due` with the window's next frame.
fn draw_view_next_frame(view: EntityId, due: Instant, window: &mut Window) {
    // Runs before the next draw, in its own update, so nothing can notify `view` between the
    // check below and the render it asks for.
    window.on_next_frame(move |window, cx| {
        FRAME_TICKS_PENDING.with_borrow_mut(|pending| {
            pending.remove(&view);
        });
        LAST_VIEW_FRAMES.with_borrow_mut(|last| {
            last.insert(view, due);
        });
        // A flag the view does not consume (it left the tree before this draw) is harmless: the
        // cached transcript's element state goes with it, and the next draw renders it whole.
        if !window.view_is_pending_render(view) {
            ANIMATION_ONLY_RENDERS.with_borrow_mut(|views| {
                views.insert(view);
            });
        }
        cx.notify(view);
    });
}

fn request_timer_frame(view: EntityId, interval: Duration, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let window_id = handle.window_id();
    let now = Instant::now();
    let due =
        LAST_WINDOW_FRAMES.with_borrow_mut(|last| next_frame_due(last, &window_id, interval, now));
    // One frame per window for all its views: a later request joins it, and moves it sooner when
    // it asks for a sooner frame.
    let replaced = TIMER_FRAMES_PENDING.with_borrow_mut(|pending| {
        if let Some((_, at, views)) = pending.iter_mut().find(|(id, _, _)| *id == window_id) {
            views.insert(view);
            if *at <= due {
                return false;
            }
            *at = due;
        } else {
            pending.push((window_id, due, HashSet::from([view])));
        }
        true
    });
    if !replaced {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(due.saturating_duration_since(Instant::now()))
            .await;
        let views = TIMER_FRAMES_PENDING.with_borrow_mut(|pending| {
            let index = pending
                .iter()
                .position(|(id, at, _)| *id == window_id && *at == due)?;
            Some(pending.swap_remove(index).2)
        });
        let Some(views) = views else {
            // A sooner frame took this one's place.
            return;
        };
        LAST_WINDOW_FRAMES.with_borrow_mut(|last| {
            last.insert(window_id, due);
        });
        let _ = handle.update(cx, |_, _, cx| {
            for view in views {
                cx.notify(view);
            }
        });
    })
    .detach();
}

/// A repeating animation element that advances at the indicator rate of the view it is in.
pub(crate) struct ThrottledAnimation<E> {
    id: ElementId,
    period: Duration,
    frame_interval: Duration,
    element: Option<E>,
    animator: Box<dyn Fn(E, f32) -> E + 'static>,
}

struct ThrottledAnimationState {
    start: Instant,
}

pub(crate) trait ThrottledAnimationExt: Sized {
    /// `animator` receives the progress 0..1 through `period`; the animation repeats.
    fn with_throttled_animation(
        self,
        id: impl Into<ElementId>,
        period: Duration,
        animator: impl Fn(Self, f32) -> Self + 'static,
    ) -> ThrottledAnimation<Self> {
        ThrottledAnimation {
            id: id.into(),
            period,
            frame_interval: INDICATOR_FRAME_INTERVAL,
            element: Some(self),
            animator: Box::new(animator),
        }
    }
}

impl<E: IntoElement + 'static> ThrottledAnimationExt for E {}

impl<E> ThrottledAnimation<E> {
    /// Advances this animation every `interval` instead of at the shared indicator rate, and on
    /// every display frame for `Duration::ZERO`. Only for short-lived motion the eye follows, such
    /// as a bar sweeping across a track (see `INDICATOR_FRAME_INTERVAL` for what each frame costs).
    pub(crate) fn frame_interval(mut self, interval: Duration) -> Self {
        self.frame_interval = interval;
        self
    }
}

impl<E: IntoElement + 'static> IntoElement for ThrottledAnimation<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: IntoElement + 'static> Element for ThrottledAnimation<E> {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_element_state(
            id.expect("a throttled animation carries an element id"),
            |state: Option<ThrottledAnimationState>, window| {
                let state = state.unwrap_or_else(|| ThrottledAnimationState {
                    start: Instant::now(),
                });
                let reduce_motion = cx.reduce_motion();
                let progress = if reduce_motion {
                    0.0
                } else {
                    (state.start.elapsed().as_secs_f32() / self.period.as_secs_f32()).fract()
                };
                let element = self
                    .element
                    .take()
                    .expect("a throttled animation lays out once per frame");
                let mut element = (self.animator)(element, progress).into_any_element();
                let layout_id = element.request_layout(window, cx);
                ((layout_id, element), state)
            },
        )
    }

    /// CDXC:SessionChat 2026-10-23 WHY:
    /// The next frame is asked for here rather than at layout, and only while the indicator is
    /// inside the clip it is painted in. One scrolled out of a list (a busy machine's spinner in a
    /// long sidebar, a loading row overdrawn past the transcript's edge) used to keep its window
    /// drawing thirty times a second for nothing anyone could see. Scrolling it back in redraws
    /// the view, whose prepaint asks again. A window that is minimized or covered draws no frames,
    /// so its indicators stop after the frame already asked for.
    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        element.prepaint(window, cx);
        if !cx.reduce_motion() && bounds.intersects(&window.content_mask().bounds) {
            request_indicator_frame(self.frame_interval, window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}
