use gpui::{
    AnyElement, App, Bounds, Element, ElementId, EntityId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Window, WindowId,
};
use std::{cell::RefCell, collections::HashSet, time::Duration};
use web_time::Instant;

/// CDXC:SessionChat 2026-09-25 WHY:
/// Every repeating indicator (chat working strip, spinners, machine tabs, the sidebar's empty state) advances on one shared thirty-frames-per-second timer. Supersedes the 2026-09-23 display-rate path for the chat: notifying one view still marks its ancestors dirty and the uncached window root renders on every draw, so each chat indicator frame rebuilt the whole window at 120 Hz on ProMotion displays and kept the main thread saturated for as long as an agent worked.
/// A view that registers through `render_indicator_frames_animation_only` keeps its cached content (the chat's transcript, native_chat/transcript_host.rs) on those frames; the others render whole. Seven frames per second (before 2026-09-23) made the compaction bar and spinners visibly step, so do not lower the rate below thirty.
/// SEE-ALSO: apps/desktop/src/app/native_chat/transcript_host.rs, apps/desktop/src/app/native_chat/render.rs, apps/desktop/src/app/native_sidebar/machines.rs.
pub(crate) const INDICATOR_FRAME_INTERVAL: Duration = Duration::from_millis(33);

thread_local! {
    /// Views whose indicator frames leave their cached content alone.
    static ANIMATION_ONLY_VIEWS: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// Views that already have an animation-only frame scheduled.
    static FRAME_TICKS_PENDING: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// Views whose coming render was asked for by an indicator frame and by nothing else.
    static ANIMATION_ONLY_RENDERS: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// Windows with a shared-timer frame scheduled, with the views it will notify.
    static TIMER_FRAMES_PENDING: RefCell<Vec<(WindowId, HashSet<EntityId>)>> = RefCell::new(Vec::new());
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

/// Schedules the next indicator frame for the view being rendered.
pub(crate) fn request_indicator_frame(window: &mut Window, cx: &mut App) {
    let view = window.current_view();
    if ANIMATION_ONLY_VIEWS.with_borrow(|views| views.contains(&view)) {
        request_animation_only_frame(view, window, cx);
    } else {
        request_timer_frame(view, window, cx);
    }
}

fn request_animation_only_frame(view: EntityId, window: &mut Window, cx: &mut App) {
    if !FRAME_TICKS_PENDING.with_borrow_mut(|pending| pending.insert(view)) {
        return;
    }
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(INDICATOR_FRAME_INTERVAL)
            .await;
        let scheduled = handle.update(cx, |_, window, _| {
            // Runs before the next draw, in its own update, so nothing can notify `view` between
            // the check below and the render it asks for.
            window.on_next_frame(move |window, cx| {
                FRAME_TICKS_PENDING.with_borrow_mut(|pending| {
                    pending.remove(&view);
                });
                // A flag the view does not consume (it left the tree before this draw) is
                // harmless: the cached transcript's element state goes with it, and the next draw
                // renders it whole.
                if !window.view_is_pending_render(view) {
                    ANIMATION_ONLY_RENDERS.with_borrow_mut(|views| {
                        views.insert(view);
                    });
                }
                cx.notify(view);
            });
        });
        if scheduled.is_err() {
            FRAME_TICKS_PENDING.with_borrow_mut(|pending| {
                pending.remove(&view);
            });
        }
    })
    .detach();
}

fn request_timer_frame(view: EntityId, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let window_id = handle.window_id();
    let scheduled = TIMER_FRAMES_PENDING.with_borrow_mut(|pending| {
        if let Some((_, views)) = pending.iter_mut().find(|(id, _)| *id == window_id) {
            views.insert(view);
            true
        } else {
            pending.push((window_id, HashSet::from([view])));
            false
        }
    });
    if scheduled {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(INDICATOR_FRAME_INTERVAL)
            .await;
        let views = TIMER_FRAMES_PENDING.with_borrow_mut(|pending| {
            let index = pending.iter().position(|(id, _)| *id == window_id)?;
            Some(pending.swap_remove(index).1)
        });
        let Some(views) = views else {
            return;
        };
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
            element: Some(self),
            animator: Box::new(animator),
        }
    }
}

impl<E: IntoElement + 'static> ThrottledAnimationExt for E {}

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
                if !reduce_motion {
                    request_indicator_frame(window, cx);
                }
                ((layout_id, element), state)
            },
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        element.prepaint(window, cx);
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
