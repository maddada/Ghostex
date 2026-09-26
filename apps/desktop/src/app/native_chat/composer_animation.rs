//! The chat box's transition: the native form of React's
//! `use-session-chat-composer-transition.ts`.
//!
//! React tweened the composer's painted height whenever its content changed shape (a scroll
//! collapse, an expansion, the editor auto-growing a line, a queue strip or an attachment strip
//! arriving) and faded the option pills and toolbar in over the second half of an expansion. The
//! same thing happens here: the box measures its natural content height every frame, and while the
//! measured height differs from the one it is painting the box is pinned to an eased value that
//! walks to the new one. Interrupting a tween restarts it from the value on screen, never from the
//! value it started at, so a reversal continues instead of snapping.
//!
//! Timing, easing and the collapsed/expanded metrics come from
//! `packages/gx-chat-core/visual/composer-animation.json`.

use serde::Deserialize;
use std::{cell::Cell, rc::Rc, sync::LazyLock};
use web_time::Instant;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ComposerAnimationMetrics {
    pub duration_ms: f32,
    pub easing: [f32; 4],
    pub min_delta_px: f32,
    pub arrival_delay_fraction: f32,
    pub arrival_duration_fraction: f32,
    pub arrival_translate_y_px: f32,
    pub collapsed_height_px: f32,
    pub collapsed_line_height_px: f32,
    pub collapsed_padding_block_px: f32,
    pub collapsed_row_gap_px: f32,
    pub expanded_line_height_px: f32,
    pub expanded_padding_block_px: f32,
    pub expanded_max_height_px: f32,
    pub expanded_row_gap_px: f32,
    pub constrained_pane_height_px: f32,
}

pub(super) static METRICS: LazyLock<ComposerAnimationMetrics> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/composer-animation.json"
    ))
    .expect("shared composer animation metrics")
});

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `t`: solve x(u) = t, then read y(u).
pub(super) fn eased(easing: &[f32; 4], t: f32) -> f32 {
    let curve = |a: f32, b: f32, u: f32| {
        let v = 1.0 - u;
        3.0 * a * v * v * u + 3.0 * b * v * u * u + u * u * u
    };
    let slope = |a: f32, b: f32, u: f32| {
        let v = 1.0 - u;
        3.0 * a * (v * v - 2.0 * v * u) + 3.0 * b * (2.0 * v * u - u * u) + 3.0 * u * u
    };
    let mut u = t.clamp(0.0, 1.0);
    for _ in 0..6 {
        let derivative = slope(easing[0], easing[2], u);
        if derivative.abs() < 1e-5 {
            break;
        }
        u = (u - (curve(easing[0], easing[2], u) - t) / derivative).clamp(0.0, 1.0);
    }
    curve(easing[1], easing[3], u)
}

/// What the composer paints this frame.
#[derive(Clone, Copy)]
pub(super) struct ComposerFrame {
    /// The pinned box height while a tween runs; `None` paints the box at its natural height.
    pub height: Option<f32>,
    /// The expanded controls' arrival fade (React's delayed opacity keyframes).
    pub controls_opacity: f32,
    /// The arrival fade's downward offset, in unscaled pixels.
    pub controls_offset: f32,
    /// True while the view still needs frames.
    pub running: bool,
    /// CDXC:SessionChat 2026-09-21 WHY: The native chat box is a sibling below the transcript, so a box that shrinks makes the list taller, shortens its scroll range, and near the end that clamped the offset, resumed tail follow, re-expanded the box and shifted the rows by the whole difference on every reversal. The list pads its end by what the box has given up against its expanded height, so the scroll range is the same in every collapse state: React's held inset (`use-session-chat-composer-inset.ts`), computed from the frame being painted rather than measured a frame late.
    pub transcript_inset: f32,
}

#[derive(Default)]
pub(crate) struct ComposerAnimation {
    /// The height the last frame painted, and the value an interruption continues from.
    current: Option<f32>,
    from: f32,
    target: f32,
    started: Option<Instant>,
    /// When the controls that an expansion brings back began their fade.
    arrival: Option<Instant>,
    collapsed: bool,
    /// The natural heights last measured in each shape. A collapse or an expansion starts its tween
    /// towards them in the frame the shape flips; waiting for that frame's measurement painted the
    /// box once at its new height before the tween walked back to it.
    expanded_natural: f32,
    collapsed_natural: f32,
    /// The natural height the last painted frame reported, so a frame that measures the same value
    /// costs nothing. The chat paints far more often than the box changes shape.
    reported: Rc<Cell<f32>>,
}

impl ComposerAnimation {
    fn duration(&self) -> f32 {
        METRICS.duration_ms / 1000.0
    }

    /// The cell a painting frame writes its measured natural height into.
    pub(super) fn reported(&self) -> Rc<Cell<f32>> {
        self.reported.clone()
    }

    /// Note the collapse state this frame renders in; leaving the collapsed shape fades the option
    /// pills and the toolbar back in, the way React animated its `CONTROLS` selector on expansion.
    pub(super) fn set_collapsed(&mut self, collapsed: bool, reduce_motion: bool) {
        if collapsed == self.collapsed {
            return;
        }
        self.collapsed = collapsed;
        self.arrival = (!collapsed && !reduce_motion).then(Instant::now);
        let natural = if collapsed {
            self.collapsed_natural
        } else {
            self.expanded_natural
        };
        if natural > 0.0 {
            self.retarget(natural, reduce_motion);
        }
    }

    /// Walk from the height on screen to `natural`. Returns true when a tween started.
    fn retarget(&mut self, natural: f32, reduce_motion: bool) -> bool {
        let current = self.current.unwrap_or(natural);
        self.target = natural;
        if reduce_motion || (natural - current).abs() < METRICS.min_delta_px {
            self.current = Some(natural);
            self.started = None;
            return false;
        }
        self.from = current;
        self.started = Some(Instant::now());
        true
    }

    /// Advance the tween and report what to paint.
    pub(super) fn advance(&mut self, reduce_motion: bool) -> ComposerFrame {
        if reduce_motion {
            self.started = None;
            self.arrival = None;
            if self.current.is_some() {
                self.current = Some(self.target);
            }
            return ComposerFrame {
                height: None,
                controls_opacity: 1.0,
                controls_offset: 0.0,
                running: false,
                transcript_inset: self.transcript_inset(self.target),
            };
        }
        let duration = self.duration();
        let mut height = None;
        if let Some(started) = self.started {
            let progress = started.elapsed().as_secs_f32() / duration;
            if progress >= 1.0 {
                self.started = None;
                self.current = Some(self.target);
            } else {
                let value =
                    self.from + (self.target - self.from) * eased(&METRICS.easing, progress);
                self.current = Some(value);
                height = Some(value);
            }
        }
        let mut controls_opacity = 1.0;
        let mut controls_offset = 0.0;
        if let Some(arrival) = self.arrival {
            let delay = duration * METRICS.arrival_delay_fraction;
            let span = (duration * METRICS.arrival_duration_fraction).max(0.001);
            let progress = (arrival.elapsed().as_secs_f32() - delay) / span;
            if progress >= 1.0 {
                self.arrival = None;
            } else {
                let fade = eased(&METRICS.easing, progress.max(0.0));
                controls_opacity = fade;
                controls_offset = METRICS.arrival_translate_y_px * (1.0 - fade);
            }
        }
        ComposerFrame {
            height,
            controls_opacity,
            controls_offset,
            running: self.started.is_some() || self.arrival.is_some(),
            transcript_inset: self.transcript_inset(height.unwrap_or(self.target)),
        }
    }

    /// How much taller the expanded box is than the collapsed one: the rows a collapse uncovers.
    /// Before the first collapse has been measured the collapsed height comes from the shared metrics.
    pub(super) fn collapse_travel(&self, scale: f32) -> f32 {
        let collapsed = if self.collapsed_natural > 0.0 {
            self.collapsed_natural
        } else {
            (METRICS.collapsed_height_px + METRICS.collapsed_padding_block_px * 2.0) * scale + 2.0
        };
        (self.expanded_natural - collapsed).max(0.0)
    }

    /// The inset the last painted frame gave the transcript, for readers between frames.
    pub(super) fn current_transcript_inset(&self) -> f32 {
        let painted = if self.started.is_some() {
            self.current.unwrap_or(self.target)
        } else {
            self.target
        };
        self.transcript_inset(painted)
    }

    fn transcript_inset(&self, painted: f32) -> f32 {
        (self.expanded_natural - painted).max(0.0)
    }

    /// Adopt the natural height the frame just measured. Returns true when a redraw is owed.
    pub(super) fn measured(&mut self, natural: f32, reduce_motion: bool) -> bool {
        if !natural.is_finite() || natural <= 0.0 {
            return false;
        }
        if self.collapsed {
            self.collapsed_natural = natural;
        } else {
            self.expanded_natural = natural;
        }
        if self.current.is_none() {
            // The first paint of a chat has nothing to move from, so it starts at rest.
            self.current = Some(natural);
            self.target = natural;
            return false;
        }
        if (natural - self.target).abs() < METRICS.min_delta_px {
            return false;
        }
        // A redraw is owed even when nothing tweens: the transcript's inset follows the new height.
        self.retarget(natural, reduce_motion);
        true
    }
}
