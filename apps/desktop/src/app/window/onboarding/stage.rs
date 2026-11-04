//! The onboarding's fixed 1672x941 artboard: its geometry, the scale it is drawn at, colour
//! helpers, easing curves and the small transition memory the stage animates with.
//! SEE-ALSO: packages/core-ui/onboarding/stage.ts (deleted 2026-10-01) (the React twin of the geometry).
use gpui::{Div, Hsla, Pixels, Rgba, Styled as _, div, px};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) const STAGE_WIDTH: f32 = 1672.0;
pub(crate) const STAGE_HEIGHT: f32 = 941.0;
pub(crate) const PANEL_COUNT: usize = 5;
/// x of the hairline between the copy column and the preview column, per panel (`gr` in the prototype).
pub(crate) const PANEL_DIVIDER_X: [f32; PANEL_COUNT] = [759.0, 796.0, 727.0, 756.0, STAGE_WIDTH];
/// x of the Ghostex lockup and the footer, per panel (`h5` in the prototype).
pub(crate) const PANEL_LOCKUP_X: [f32; PANEL_COUNT] = [46.0, 48.0, 60.0, 46.0, 44.0];
pub(crate) const VEIL_LEFT: f32 = 727.0;
/// Footer row: the Back button and the panel's forward action share this top edge and height.
pub(crate) const FOOT_TOP: f32 = 848.0;
pub(crate) const FOOT_HEIGHT: f32 = 48.0;
/// Distance from the copy/preview divider to the footer's right edge.
pub(crate) const FOOT_RIGHT_INSET: f32 = 35.0;
/// Gap between the copy/preview divider and the progress dots.
pub(crate) const DOTS_INSET: f32 = 40.0;
/// CDXC:Onboarding 2026-09-12 DECISION:
/// User: "the bg behind the right side graphics is too blue please make it less saturated colors for the bg graphic/shader". The veil keeps its shape and motion with most of the colour pulled out: 0 is greyscale, 1 is the shader's own colour.
pub(crate) const VEIL_SATURATION: f32 = 0.32;
pub(crate) const TOAST_MS: u64 = 2400;

/// x of the footer's left edge (the Back button) on every panel.
pub(crate) const FOOT_LEFT: f32 = 46.0;

/// x of the footer's right edge for a panel.
///
/// CDXC:Onboarding 2026-10-28 WHY:
/// The footer used to end at each panel's own divider (759, 796, 727, 756), and Back started at
/// each panel's lockup, so Back and Next jumped sideways between steps and a user paging with the
/// mouse missed them. Panels 1-4 now share one footer that ends left of the narrowest divider;
/// the full-width last panel keeps its footer at the stage's right edge.
pub(crate) fn foot_right_x(panel: usize) -> f32 {
    if panel >= PANEL_COUNT {
        STAGE_WIDTH - FOOT_RIGHT_INSET
    } else {
        PANEL_DIVIDER_X[..PANEL_COUNT - 1]
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min)
            - FOOT_RIGHT_INSET
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stays_put_across_the_first_four_panels() {
        let first = foot_right_x(1);
        for panel in 2..PANEL_COUNT {
            assert_eq!(foot_right_x(panel), first);
        }
        assert_eq!(first, 727.0 - FOOT_RIGHT_INSET);
        assert_eq!(foot_right_x(PANEL_COUNT), STAGE_WIDTH - FOOT_RIGHT_INSET);
    }
}

/// The stage scale: every length on the artboard is multiplied by it, the way the React stage is one
/// `transform: scale()` of a fixed-size box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct S(pub(crate) f32);

impl S {
    pub(crate) fn px(self, value: f32) -> Pixels {
        px(value * self.0)
    }
}

/// An absolutely positioned box on the stage (`box(left, top, width, height)` in stage.ts).
pub(crate) fn abs(s: S, left: f32, top: f32, width: Option<f32>, height: Option<f32>) -> Div {
    let mut element = div().absolute().left(s.px(left)).top(s.px(top));
    if let Some(width) = width {
        element = element.w(s.px(width));
    }
    if let Some(height) = height {
        element = element.h(s.px(height));
    }
    element
}

/// `#rrggbb`.
pub(crate) fn hex(value: u32) -> Hsla {
    gpui::rgb(value).into()
}

/// `rgba(r, g, b, a)`.
pub(crate) fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a,
    }
    .into()
}

pub(crate) fn white(a: f32) -> Hsla {
    rgba(255, 255, 255, a)
}

pub(crate) fn black(a: f32) -> Hsla {
    rgba(0, 0, 0, a)
}

/// CSS `filter: brightness(factor)` on one colour.
pub(crate) fn brightness(color: Hsla, factor: f32) -> Hsla {
    let color: Rgba = color.into();
    Rgba {
        r: (color.r * factor).min(1.0),
        g: (color.g * factor).min(1.0),
        b: (color.b * factor).min(1.0),
        a: color.a,
    }
    .into()
}

/// Mixes two colours in sRGB, the way CSS interpolates a two-stop gradient.
pub(crate) fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let a: Rgba = a.into();
    let b: Rgba = b.into();
    let t = t.clamp(0.0, 1.0);
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` evaluated at progress `x`.
pub(crate) fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 || x >= 1.0 {
        return x;
    }
    let sample = |a1: f32, a2: f32, t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * a1 + 3.0 * mt * t * t * a2 + t * t * t
    };
    let slope = |a1: f32, a2: f32, t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * a1 + 6.0 * mt * t * (a2 - a1) + 3.0 * t * t * (1.0 - a2)
    };
    let mut t = x;
    for _ in 0..8 {
        let error = sample(x1, x2, t) - x;
        let d = slope(x1, x2, t);
        if error.abs() < 1e-5 || d.abs() < 1e-6 {
            break;
        }
        t = (t - error / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    if (sample(x1, x2, t) - x).abs() > 1e-3 {
        for _ in 0..30 {
            t = (lo + hi) / 2.0;
            if sample(x1, x2, t) < x {
                lo = t;
            } else {
                hi = t;
            }
        }
    }
    sample(y1, y2, t)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Ease {
    /// CSS `ease`.
    Ease,
    EaseOut,
    EaseInOut,
    Bezier(f32, f32, f32, f32),
}

impl Ease {
    pub(crate) fn apply(self, x: f32) -> f32 {
        match self {
            Ease::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, x),
            Ease::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, x),
            Ease::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, x),
            Ease::Bezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, x),
        }
    }
}

/// The stage's move curve (`cubic-bezier(0.3, 0.7, 0.2, 1)`, 0.6s) for the divider, lockup, footer and dots.
pub(crate) const STAGE_MOVE: (Duration, Ease) =
    (Duration::from_millis(600), Ease::Bezier(0.3, 0.7, 0.2, 1.0));

/// Progress of a one-shot animation that started at `started`, eased.
pub(crate) fn progress(started: Instant, now: Instant, duration: Duration, ease: Ease) -> f32 {
    let elapsed = now.saturating_duration_since(started).as_secs_f32();
    ease.apply(elapsed / duration.as_secs_f32().max(1e-3))
}

/// `gxob-breathe`: opacity dips to 0.35 at the half of every `period`.
pub(crate) fn breathe(epoch: Instant, now: Instant, period: f32, delay: f32) -> f32 {
    let t = (now.saturating_duration_since(epoch).as_secs_f32() - delay).max(0.0) / period;
    let phase = t.fract();
    let half = if phase < 0.5 {
        phase * 2.0
    } else {
        (1.0 - phase) * 2.0
    };
    1.0 - 0.65 * Ease::EaseInOut.apply(half)
}

/// `gxob-blinkc` (`steps(2)`): visible for the first half of every second.
pub(crate) fn blink(epoch: Instant, now: Instant) -> bool {
    now.saturating_duration_since(epoch).as_secs_f32().fract() < 0.5
}

/// `useCycle(count, interval, rest)`: the frame a looping demo is on, plus when that frame and
/// the current loop began (the React demos key their entrance animations on those moments).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cycle {
    pub(crate) frame: usize,
    pub(crate) frame_started: Instant,
    pub(crate) cycle_started: Instant,
}

impl Cycle {
    /// When `frame` of the current loop began.
    pub(crate) fn frame_at(&self, frame: usize, interval_ms: u64) -> Instant {
        self.cycle_started + Duration::from_millis(interval_ms * frame as u64)
    }
}

pub(crate) fn cycle(
    mounted: Instant,
    now: Instant,
    count: usize,
    interval_ms: u64,
    rest: usize,
) -> Cycle {
    let elapsed = now.saturating_duration_since(mounted).as_millis() as u64;
    let tick = (elapsed / interval_ms) as usize;
    let period = count + rest;
    let wrapped = tick % period;
    let frame = wrapped.min(count - 1);
    let cycle_started = mounted + Duration::from_millis(((tick - wrapped) as u64) * interval_ms);
    Cycle {
        frame,
        frame_started: cycle_started + Duration::from_millis(frame as u64 * interval_ms),
        cycle_started,
    }
}

/// `gxob-lineIn` (and friends): opacity and a leftward offset easing in over `ms`.
pub(crate) fn entrance(started: Instant, now: Instant, ms: u64) -> f32 {
    progress(started, now, Duration::from_millis(ms), Ease::EaseOut)
}

/// `gxob-glowOnce`: a blue glow that fades over 1.4s.
pub(crate) fn glow_once(started: Instant, now: Instant) -> f32 {
    1.0 - progress(started, now, Duration::from_millis(1400), Ease::EaseOut)
}

/// Remembers where each animated value is heading so a CSS `transition` can be replayed: when the
/// target of `id` changes, the value eases from wherever it was toward the new target.
#[derive(Default)]
pub(crate) struct Transitions {
    values: RefCell<HashMap<String, TransitionState>>,
}

struct TransitionState {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
    ease: Ease,
}

impl TransitionState {
    fn value(&self, now: Instant) -> f32 {
        let t = progress(self.started, now, self.duration, self.ease);
        self.from + (self.to - self.from) * t
    }
}

impl Transitions {
    pub(crate) fn value(
        &self,
        id: &str,
        target: f32,
        (duration, ease): (Duration, Ease),
        now: Instant,
    ) -> f32 {
        let mut values = self.values.borrow_mut();
        match values.get_mut(id) {
            Some(state) => {
                if (state.to - target).abs() > f32::EPSILON {
                    let current = state.value(now);
                    *state = TransitionState {
                        from: current,
                        to: target,
                        started: now,
                        duration,
                        ease,
                    };
                }
                state.value(now)
            }
            None => {
                values.insert(
                    id.to_string(),
                    TransitionState {
                        from: target,
                        to: target,
                        started: now,
                        duration,
                        ease,
                    },
                );
                target
            }
        }
    }
}

pub(crate) fn clock_stamp(at: chrono::DateTime<chrono::Local>) -> String {
    at.format("%H:%M:%S").to_string()
}

/// The last path segment of a folder, `/` or `\` separated.
pub(crate) fn folder_basename(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}
