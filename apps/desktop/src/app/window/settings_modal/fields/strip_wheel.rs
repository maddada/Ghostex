//! Sideways strips in Settings (the themes row, the backgrounds gallery, extension screenshots)
//! lock a trackpad gesture to one axis, so a sideways swipe moves only the strip and never the
//! page behind it, while a mostly vertical swipe over the strip still scrolls the page.
//!
//! GPUI's own scroll listener moves the strip but never stops the event, so the page above it
//! also scrolled by the swipe's vertical drift. [`strip_wheel`] listens first (capture phase),
//! decides the gesture's dominant axis once, near its start, and keeps it until the gesture
//! ends, its momentum included: a sideways one is taken by the strip and stopped there, a
//! vertical one is left alone for the page.
//! (2026-11-21, User: "scrolling sideways is broken in the settings. Scrolling sideways scrolls me
//! up and down also, so it should lock whenever I'm scrolling the themes. It shouldn't lock me,
//! and it shouldn't scroll me up and down.")
use gpui::{
    Bounds, DispatchPhase, Hitbox, HitboxBehavior, IntoElement, Pixels, ScrollEvent, ScrollHandle,
    TouchPhase, Window, canvas, point, px,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How far a gesture travels before its axis is decided; earlier events follow their own lean.
const DECIDE_DISTANCE: f32 = 4.0;
/// A pause this long between events starts a new gesture where the system sends no phases (a
/// wheel mouse). Momentum events come every frame, so they never reach it.
const GESTURE_GAP: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripRoute {
    /// The strip moves by it and the page does not see it.
    Strip,
    /// The page scrolls by it; the strip stays.
    Page,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockedAxis {
    Sideways,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default)]
struct Gesture {
    axis: Option<LockedAxis>,
    travelled: (f32, f32),
    last_event: Option<Instant>,
}

impl Gesture {
    fn route(&mut self, dx: f32, dy: f32, phase: TouchPhase, now: Instant) -> StripRoute {
        if phase == TouchPhase::Started
            || self
                .last_event
                .is_none_or(|last| now.duration_since(last) >= GESTURE_GAP)
        {
            *self = Gesture::default();
        }
        self.last_event = Some(now);
        if self.axis.is_none() {
            self.travelled.0 += dx;
            self.travelled.1 += dy;
            if self.travelled.0.abs().max(self.travelled.1.abs()) >= DECIDE_DISTANCE {
                self.axis = Some(if self.travelled.0.abs() > self.travelled.1.abs() {
                    LockedAxis::Sideways
                } else {
                    LockedAxis::Vertical
                });
            }
        }
        let sideways = match self.axis {
            Some(axis) => axis == LockedAxis::Sideways,
            None => dx.abs() > dy.abs(),
        };
        if sideways {
            StripRoute::Strip
        } else {
            StripRoute::Page
        }
    }
}

/// The axis a strip's current gesture is locked to; one per strip, kept with its scroll handle.
#[derive(Clone, Default)]
pub(crate) struct StripWheelLock(Rc<Cell<Gesture>>);

/// Listens for the strip's wheel and swipe events while it paints; call it from a canvas' paint
/// step with the hitbox of the strip's bounds.
pub(crate) fn strip_wheel(
    window: &mut Window,
    hitbox: &Hitbox,
    handle: &ScrollHandle,
    lock: &StripWheelLock,
) {
    let hitbox = hitbox.clone();
    let handle = handle.clone();
    let lock = lock.clone();
    window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture || !hitbox.should_handle_scroll(window) {
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height());
        let mut gesture = lock.0.get();
        let route = gesture.route(
            f32::from(delta.x),
            f32::from(delta.y),
            event.touch_phase,
            Instant::now(),
        );
        lock.0.set(gesture);
        if route == StripRoute::Page {
            return;
        }
        cx.stop_propagation();
        let offset = handle.offset();
        let next = (offset.x + delta.x).clamp(-handle.max_offset().x, px(0.0));
        if next != offset.x {
            handle.set_offset(point(next, offset.y));
            window.refresh();
        }
    });
}

/// An invisible layer over a `relative` strip that gives it [`strip_wheel`]'s lock. The strip must
/// track the same `handle`.
pub(crate) fn strip_wheel_layer(handle: &ScrollHandle, lock: &StripWheelLock) -> impl IntoElement {
    use gpui::Styled as _;
    let handle = handle.clone();
    let lock = lock.clone();
    canvas(
        |bounds: Bounds<Pixels>, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| strip_wheel(window, &hitbox, &handle, &lock),
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
