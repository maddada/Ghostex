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

/// The collapsed toolbar (`...` button) height at scale 1: the compact row is at least this tall
/// (`10 + 28 + 10 = 48px`), matching the resting collapsed height while keeping the editor at the
/// expanded 24px line height centered inside it.
pub(super) const COLLAPSED_ROW_MIN_HEIGHT_PX: f32 = 28.0;

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the composer paints this frame.
#[derive(Clone, Copy)]
pub(super) struct ComposerFrame {
    /// The shape the box paints in: the requested one, except while a collapse is still closing
    /// the box over the expanded content.
    pub collapsed: bool,
    /// The pinned box height while a tween runs; `None` paints the box at its natural height.
    pub height: Option<f32>,
    /// Normalized collapse progress in `0.0..=1.0` (`0.0` = expanded, `1.0` = collapsed) during a
    /// collapse/expand shape transition; `0.0` during expanded auto-grow tweens.
    pub collapse_progress: f32,
    /// The expanded controls' opacity (fades out on collapse, fades in on expand).
    pub controls_opacity: f32,
    /// The expanded controls' downward offset, in unscaled pixels.
    pub controls_offset: f32,
    /// The compact `...` + Send overlay's opacity while transitioning between expanded and
    /// collapsed shapes, so those controls cross-fade cleanly in their exact collapsed slot.
    pub compact_controls_opacity: f32,
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
    /// Active tween duration in seconds (shortened proportionally on mid-flight reversals).
    duration_secs: f32,
    /// The last chat scale seen, used to compute the collapsed height estimate inside `advance`.
    scale: f32,
    /// The shape being painted, which trails a requested collapse until the box has closed.
    collapsed: bool,
    /// A collapse whose box is still closing over the expanded content.
    closing: bool,
    /// True while the active tween is transitioning between collapsed and expanded shapes (false
    /// while an expanded composer auto-grows or shrinks as the user types).
    shape_transition: bool,
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

    /// The shape this frame paints in, for the parts of the box drawn outside `render_composer`.
    pub(super) fn shown_collapsed(&self) -> bool {
        self.collapsed
    }

    /// CDXC:SessionChat 2026-09-28 DECISION: User: the collapsed composer shows one line of the draft or placeholder, and the text is never faded; the line animates into place instead. A collapse closes the box over the expanded content, so the lines below the first slide out of view under its edge, and the one-line row takes over when the box lands. An expansion opens the box over the expanded content, revealing them. Supersedes the same day's fade-out/fade-in of the text.
    /// CDXC:SessionChat 2026-09-28 SEE-ALSO: `apps/mobile/app/src/chat/native/composer/useComposerMotion.ts` runs the same sequence on the phone from the same `composer-animation.json`.
    ///
    /// Note the collapse state the chat asks for. Leaving the collapsed shape also fades the option
    /// pills and the toolbar back in, the way React animated its `CONTROLS` selector on expansion.
    pub(super) fn set_collapsed(&mut self, requested: bool, reduce_motion: bool, scale: f32) {
        self.scale = scale.max(0.01);
        // The first paint of a chat has nothing to move from.
        if reduce_motion || self.current.is_none() {
            self.closing = false;
            self.shape_transition = false;
            if requested != self.collapsed {
                self.flip(requested, true);
            }
            return;
        }
        match (requested, self.collapsed) {
            (true, false) if !self.closing => {
                self.closing = true;
                self.shape_transition = true;
                self.retarget(self.collapsed_estimate(self.scale), false, true);
            }
            // The box has closed: the one-line row takes over at the height it landed on.
            (true, false) if self.started.is_none() => {
                self.closing = false;
                self.shape_transition = false;
                self.flip(true, false);
            }
            // Reversed before the box closed: it opens again over the same content.
            (false, false) if self.closing => {
                self.closing = false;
                self.shape_transition = true;
                self.retarget(self.expanded_natural, false, true);
            }
            (false, true) => {
                self.shape_transition = true;
                self.flip(false, false);
            }
            _ => {}
        }
    }

    fn flip(&mut self, collapsed: bool, reduce_motion: bool) {
        self.collapsed = collapsed;
        if reduce_motion {
            self.shape_transition = false;
        }
        let natural = if collapsed {
            self.collapsed_natural
        } else {
            self.expanded_natural
        };
        if natural > 0.0 {
            self.retarget(natural, reduce_motion, !reduce_motion && !collapsed);
        }
    }

    /// The collapsed box's inner height (inside the outer card's border): as last measured, or
    /// from the shared metrics before the first collapse.
    pub(super) fn collapsed_estimate(&self, scale: f32) -> f32 {
        if self.collapsed_natural > 0.0 {
            self.collapsed_natural
        } else {
            (METRICS.collapsed_height_px.max(COLLAPSED_ROW_MIN_HEIGHT_PX)
                + METRICS.collapsed_padding_block_px * 2.0)
                * scale
        }
    }

    /// Walk from the height on screen to `natural`. Returns true when a tween started.
    fn retarget(&mut self, natural: f32, reduce_motion: bool, scale_for_reversal: bool) -> bool {
        let current = self.current.unwrap_or(natural);
        self.target = natural;
        if reduce_motion || (natural - current).abs() < METRICS.min_delta_px {
            self.current = Some(natural);
            self.started = None;
            self.shape_transition = false;
            return false;
        }
        let base = self.duration();
        self.duration_secs = if scale_for_reversal {
            let full = self.collapse_travel(self.scale.max(0.01));
            if full > 1.0 {
                let ratio = ((natural - current).abs() / full).clamp(0.0, 1.0);
                base * ratio.sqrt().clamp(0.5, 1.0)
            } else {
                base
            }
        } else {
            base
        };
        self.from = current;
        self.started = Some(Instant::now());
        true
    }

    /// Advance the tween and report what to paint.
    pub(super) fn advance(&mut self, reduce_motion: bool) -> ComposerFrame {
        if reduce_motion {
            self.started = None;
            self.closing = false;
            self.shape_transition = false;
            if self.current.is_some() {
                self.current = Some(self.target);
            }
            return ComposerFrame {
                collapsed: self.collapsed,
                height: None,
                collapse_progress: if self.collapsed { 1.0 } else { 0.0 },
                controls_opacity: 1.0,
                controls_offset: 0.0,
                compact_controls_opacity: 0.0,
                running: false,
                transcript_inset: self.transcript_inset(self.target),
            };
        }
        let mut height = None;
        if let Some(started) = self.started {
            let duration = if self.duration_secs > 0.0 {
                self.duration_secs
            } else {
                self.duration()
            };
            let progress = started.elapsed().as_secs_f32() / duration;
            if progress >= 1.0 {
                self.started = None;
                self.current = Some(self.target);
                self.shape_transition = false;
                // Hand off immediately when a collapse lands so we never paint an unclipped
                // expanded frame between the tween ending and the collapsed row taking over.
                if self.closing {
                    self.closing = false;
                    self.collapsed = true;
                }
            } else {
                let value =
                    self.from + (self.target - self.from) * eased(&METRICS.easing, progress);
                self.current = Some(value);
                height = Some(value);
            }
        } else if self.shape_transition && !self.collapsed && self.expanded_natural <= 0.0 {
            // Very first expand of a chat that opened collapsed: hold the collapsed height for the
            // single layout pass that measures the expanded natural height so it never flashes open.
            height = self.current;
        }
        let (collapse_progress, controls_opacity, controls_offset, compact_controls_opacity) =
            if self.shape_transition
                && let Some(painted) = height
            {
                let h_col = self.collapsed_estimate(self.scale.max(0.01));
                let h_exp = self.expanded_natural.max(h_col + 1.0);
                let collapse_progress = ((h_exp - painted) / (h_exp - h_col)).clamp(0.0, 1.0);
                let controls_opacity = smoothstep(1.0 - collapse_progress / 0.45);
                let controls_offset = METRICS.arrival_translate_y_px * (1.0 - controls_opacity);
                let compact_controls_opacity = smoothstep((collapse_progress - 0.60) / 0.40);
                (
                    collapse_progress,
                    controls_opacity,
                    controls_offset,
                    compact_controls_opacity,
                )
            } else {
                (if self.collapsed { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0)
            };
        ComposerFrame {
            collapsed: self.collapsed,
            height,
            collapse_progress,
            controls_opacity,
            controls_offset,
            compact_controls_opacity,
            running: self.started.is_some() || self.closing,
            transcript_inset: self.transcript_inset(height.unwrap_or(self.target)),
        }
    }

    /// How much taller the expanded box is than the collapsed one: the rows a collapse uncovers.
    /// Before the first collapse has been measured the collapsed height comes from the shared metrics.
    pub(super) fn collapse_travel(&self, scale: f32) -> f32 {
        (self.expanded_natural - self.collapsed_estimate(scale)).max(0.0)
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
        // The expanded content stays under a closing box; only the row it lands on is measured anew.
        if self.closing {
            return false;
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
        let keep_shape = self.shape_transition;
        self.retarget(natural, reduce_motion, keep_shape);
        if !keep_shape {
            self.shape_transition = false;
        }
        true
    }
}
