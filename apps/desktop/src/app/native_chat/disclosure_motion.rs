//! How everything in the chat that opens and closes moves: the native form of
//! the React chat's `session-chat-disclosure-body.tsx` and
//! `session-chat-worked-fold.tsx`.
//!
//! A finished turn folds the moment the transcript settles, so the rows the reader was just
//! watching turn into the heading above the final reply instead of vanishing: they dim, their
//! block eases shut while the reply stays where it is, and the heading, its divider and the files
//! line grow in. Every other disclosure (a tool's detail, a tool group, a reasoning row, a card
//! body, a capped block's "Show more", the worked fold's own log) eases its height and opacity
//! between shut and open whenever its open state changes, at the faster manual timing. The
//! timing is `packages/gx-chat-core/visual/worked-fold-animation.json`. This file only keeps time; the renderers paint what [`DisclosureMotions::observe`]
//! reports through [`motion_clip`].
//!
//! Disclosures are watched rather than hooked: each renderer reports the state it is drawing, and
//! a state that differs from the last one drawn starts a motion. That covers a toggle in this
//! view's own sets, a toggle the host answers through the snapshot (the tasks panel), and a
//! reversal part of the way through, with no listener to forget. A disclosure seen for the first
//! time is settled, not opening, so a transcript that loads with its folds open does not animate
//! every one of them.

use serde::Deserialize;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::LazyLock,
};
use web_time::Instant;

use gpui::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, px};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkedFoldMetrics {
    dim_ms: f32,
    dim_opacity: f32,
    collapse_delay_ms: f32,
    collapse_ms: f32,
    heading_delay_ms: f32,
    heading_ms: f32,
    heading_offset_px: f32,
    toggle_ms: f32,
    easing: [f32; 4],
}

static METRICS: LazyLock<WorkedFoldMetrics> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/worked-fold-animation.json"
    ))
    .expect("shared worked-fold animation metrics")
});

/// Which movement a disclosure is making.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FoldMotionKind {
    /// The turn's live rows are folding under a heading that did not exist a moment ago.
    Fold,
    /// The disclosure is opening.
    Open,
    /// The disclosure is closing.
    Close,
}

/// What one frame of a moving disclosure paints. Factors run from 0 (shut) to 1 (fully open).
#[derive(Clone, Copy)]
pub(super) struct FoldFrame {
    pub kind: FoldMotionKind,
    /// The worked fold's heading, divider and files line growing in; 1 outside a fold.
    pub heading: f32,
    /// The heading's upward offset while it arrives, in unscaled pixels.
    pub heading_offset: f32,
    /// The body's height.
    pub log: f32,
    /// The body's opacity.
    pub rows_opacity: f32,
}

struct FoldMotion {
    kind: FoldMotionKind,
    started: Instant,
    /// The body height the motion starts from, so a reversal continues from what is on screen.
    from: f32,
}

/// Every disclosure on screen that is moving, the state each was last drawn in, and the turns the
/// transcript last drew as live rows.
#[derive(Default)]
pub(crate) struct DisclosureMotions {
    live: HashSet<String>,
    observed: HashMap<String, bool>,
    motions: HashMap<String, FoldMotion>,
    heights: HashMap<String, Rc<Cell<f32>>>,
    /// The reader toggled a disclosure while the transcript was following its tail; following
    /// is paused until the motion settles (`disclosure_anchor.rs`).
    pub(super) follow_paused: bool,
    /// The toggled disclosure whose change has not been drawn yet, and when it was pressed: until
    /// it is drawn nothing is moving, and handing following back then would pin the bottom again.
    pub(super) awaiting: Option<(String, Instant)>,
    /// A toggle whose revealed rows ease in above it, whether it is opening, and the height of
    /// those rows the scroll position already accounts for (`None` until a closing group's rows
    /// have been measured, since they start fully grown).
    pub(super) grows_above: Option<(String, bool, Option<f32>)>,
}

fn progress(elapsed_ms: f32, delay_ms: f32, span_ms: f32) -> f32 {
    ((elapsed_ms - delay_ms) / span_ms.max(1.0)).clamp(0.0, 1.0)
}

fn eased(t: f32) -> f32 {
    super::composer_animation::eased(&METRICS.easing, t)
}

impl DisclosureMotions {
    /// The transcript drew this turn's rows unfolded (`work:<user id>`).
    pub(super) fn saw_live(&mut self, key: &str) {
        if !self.live.contains(key) {
            self.live.insert(key.to_string());
        }
    }

    /// A folded turn is being drawn. When the same turn was drawn live before, it has just folded:
    /// start the fold, unless the log stays open (verbose mode) or motion is reduced.
    pub(super) fn arrived(&mut self, key: &str, folds_shut: bool, reduce_motion: bool) {
        if self.live.remove(key) && folds_shut && !reduce_motion {
            self.observed.insert(key.to_string(), false);
            self.motions.insert(
                key.to_string(),
                FoldMotion {
                    kind: FoldMotionKind::Fold,
                    started: Instant::now(),
                    from: 1.0,
                },
            );
        }
    }

    /// A renderer is drawing `key` in state `open`. Returns the frame to paint while the
    /// disclosure moves, or `None` once it is at rest (the row then paints its settled form).
    pub(super) fn observe(
        &mut self,
        key: &str,
        open: bool,
        reduce_motion: bool,
    ) -> Option<FoldFrame> {
        let previous = self.observed.insert(key.to_string(), open);
        if previous.is_some_and(|before| before != open)
            && self
                .awaiting
                .as_ref()
                .is_some_and(|(awaited, _)| awaited == key)
        {
            self.awaiting = None;
        }
        match previous {
            Some(before) if before != open => {
                if reduce_motion {
                    self.settle(key);
                    return None;
                }
                let from =
                    self.frame(key)
                        .map(|frame| frame.log)
                        .unwrap_or(if open { 0.0 } else { 1.0 });
                self.motions.insert(
                    key.to_string(),
                    FoldMotion {
                        kind: if open {
                            FoldMotionKind::Open
                        } else {
                            FoldMotionKind::Close
                        },
                        started: Instant::now(),
                        from,
                    },
                );
            }
            // First sight, or unchanged: settled unless a motion (a fold) is already running.
            _ => {}
        }
        self.frame(key)
    }

    /// True while any disclosure still needs frames.
    pub(super) fn running(&self) -> bool {
        !self.motions.is_empty()
    }

    /// The natural height `key`'s moving body measured on its last paint.
    pub(super) fn natural(&self, key: &str) -> f32 {
        self.heights
            .get(&format!("{key}#"))
            .map_or(0.0, |height| height.get())
    }

    /// This frame of the motion on `key`, or `None` once it is at rest.
    pub(super) fn frame(&mut self, key: &str) -> Option<FoldFrame> {
        let motion = self.motions.get(key)?;
        let metrics = &*METRICS;
        let elapsed = motion.started.elapsed().as_secs_f32() * 1000.0;
        let (frame, end) = match motion.kind {
            FoldMotionKind::Fold => {
                let dim = eased(progress(elapsed, 0.0, metrics.dim_ms));
                let collapse = eased(progress(
                    elapsed,
                    metrics.collapse_delay_ms,
                    metrics.collapse_ms,
                ));
                let heading = eased(progress(
                    elapsed,
                    metrics.heading_delay_ms,
                    metrics.heading_ms,
                ));
                (
                    FoldFrame {
                        kind: motion.kind,
                        heading,
                        heading_offset: metrics.heading_offset_px * (1.0 - heading),
                        log: 1.0 - collapse,
                        rows_opacity: 1.0 - (1.0 - metrics.dim_opacity) * dim,
                    },
                    (metrics.collapse_delay_ms + metrics.collapse_ms)
                        .max(metrics.heading_delay_ms + metrics.heading_ms),
                )
            }
            FoldMotionKind::Open | FoldMotionKind::Close => {
                let target = if motion.kind == FoldMotionKind::Open {
                    1.0
                } else {
                    0.0
                };
                // A reversal part of the way through covers only the distance left.
                let span = metrics.toggle_ms * (target - motion.from).abs().max(0.2);
                let log =
                    motion.from + (target - motion.from) * eased(progress(elapsed, 0.0, span));
                (
                    FoldFrame {
                        kind: motion.kind,
                        heading: 1.0,
                        heading_offset: 0.0,
                        log,
                        rows_opacity: metrics.dim_opacity + (1.0 - metrics.dim_opacity) * log,
                    },
                    span,
                )
            }
        };
        if elapsed >= end {
            self.settle(key);
            return None;
        }
        Some(frame)
    }

    /// The natural height one part of a moving disclosure measured on its last paint. `part` is
    /// "" for the body itself, or a name for the worked fold's other parts.
    pub(super) fn height(&mut self, key: &str, part: &str) -> Rc<Cell<f32>> {
        self.heights
            .entry(format!("{key}#{part}"))
            .or_default()
            .clone()
    }

    /// The height a capped block had while settled shut, measured by [`measured`] so a motion
    /// that starts from the cap knows where to start. Kept after the motion ends.
    pub(super) fn floor(&mut self, key: &str) -> Rc<Cell<f32>> {
        self.heights
            .entry(format!("{key}#floor"))
            .or_default()
            .clone()
    }

    fn settle(&mut self, key: &str) {
        self.motions.remove(key);
        let prefix = format!("{key}#");
        self.heights
            .retain(|part, _| !part.starts_with(&prefix) || part.ends_with("#floor"));
    }
}

/// Wrap `content` so its laid-out height lands in `height` on every paint.
pub(super) fn measured(height: Rc<Cell<f32>>, content: AnyElement) -> AnyElement {
    let measure = gpui::canvas(
        move |bounds, _, _| height.set(bounds.size.height.as_f32()),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    div()
        .relative()
        .w_full()
        .min_w_0()
        .child(content)
        .child(measure)
        .into_any_element()
}

/// A part whose height eases between `floor` (0 for a body that disappears, the capped height for
/// a block that only shortens) and its natural height, at `frame.log`, fading with
/// `frame.rows_opacity`. The content lays out at its natural height inside a clip and a canvas
/// records that height for the next frame; until the first measurement a part that is mostly
/// open paints unclipped and one that is mostly shut paints at its floor.
///
/// `gap` is the spacing (already scaled) the column around the part puts before it. The part
/// takes that gap back with a negative top margin and carries it inside the clip, so a part eased
/// to nothing leaves no gap behind and the first and last frames match the settled layout.
pub(super) fn motion_clip(
    height: Rc<Cell<f32>>,
    frame: FoldFrame,
    floor: f32,
    gap: f32,
    content: AnyElement,
) -> AnyElement {
    let natural = height.get();
    let factor = frame.log.clamp(0.0, 1.0);
    let clip = div().w_full().min_w_0().overflow_hidden().mt(px(-gap));
    let clip = if factor >= 1.0 || (natural <= 0.0 && factor >= 0.5) {
        clip
    } else if natural <= 0.0 {
        clip.h(px(gap + floor))
    } else {
        clip.h(px(gap + floor + (natural - floor).max(0.0) * factor))
    };
    clip.child(
        div()
            .pt(px(gap))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .opacity(frame.rows_opacity)
            .child(measured(height, content)),
    )
    .into_any_element()
}

/// [`motion_clip`] for a part that is first in its column: the gap it must take back is the one
/// after it, so the negative margin and the carried spacing sit at the bottom instead.
pub(super) fn motion_clip_trailing(
    height: Rc<Cell<f32>>,
    frame: FoldFrame,
    gap: f32,
    content: AnyElement,
) -> AnyElement {
    // The measured height includes the gap the part carries, so a part eased to `f` adds exactly
    // `f` of its full height to the column: the gap after it is taken back with the negative
    // margin, and `disclosure_anchor.rs` scrolls by the same amount to keep the toggle below still.
    let natural = height.get();
    let factor = frame.log.clamp(0.0, 1.0);
    let clip = div().w_full().min_w_0().overflow_hidden().mb(px(-gap));
    let clip = if factor >= 1.0 || (natural <= 0.0 && factor >= 0.5) {
        clip
    } else {
        clip.h(px(natural.max(0.0) * factor))
    };
    clip.child(
        div()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .opacity(frame.rows_opacity)
            .child(measured(
                height,
                div().pb(px(gap)).child(content).into_any_element(),
            )),
    )
    .into_any_element()
}

impl super::state::NativeChatView {
    /// Report the state a disclosure is drawn in; the frame to paint while it moves.
    pub(super) fn disclosure_frame(
        &self,
        key: &str,
        open: bool,
        cx: &gpui::App,
    ) -> Option<FoldFrame> {
        self.disclosure_motion
            .borrow_mut()
            .observe(key, open, cx.reduce_motion())
    }

    /// The cell a disclosure's moving body measures its natural height into.
    pub(super) fn disclosure_height(&self, key: &str) -> Rc<Cell<f32>> {
        self.disclosure_motion.borrow_mut().height(key, "")
    }

    /// The cell a capped block measures its shut height into while settled shut.
    pub(super) fn disclosure_floor(&self, key: &str) -> Rc<Cell<f32>> {
        self.disclosure_motion.borrow_mut().floor(key)
    }

    /// A disclosure's body: as it is when at rest, clipped to this frame while it moves. `gap` is
    /// the scaled spacing the surrounding column puts before it (see [`motion_clip`]).
    pub(super) fn disclosure_body_motion(
        &self,
        key: &str,
        frame: Option<FoldFrame>,
        gap: f32,
        body: AnyElement,
    ) -> AnyElement {
        match frame {
            Some(frame) => motion_clip(self.disclosure_height(key), frame, 0.0, gap, body),
            None => body,
        }
    }

    /// A capped block ("Show more"): shut it shows `floor` pixels, open its natural height; while
    /// it moves the uncapped content is clipped between the two. The caller measures the shut
    /// height into [`Self::disclosure_floor`] while settled shut.
    pub(super) fn capped_body_motion(
        &self,
        key: &str,
        frame: FoldFrame,
        gap: f32,
        body: AnyElement,
    ) -> AnyElement {
        let floor = self.disclosure_floor(key).get();
        motion_clip(self.disclosure_height(key), frame, floor, gap, body)
    }
}
