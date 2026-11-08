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

use gpui::prelude::FluentBuilder as _;
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
    /// A disclosure opened by a reader scrolled up, whose end is scrolled into view as it grows
    /// (`disclosure_anchor.rs`, `disclosure_reveal.rs`).
    pub(super) reveal: Option<Reveal>,
}

/// The opened disclosure being revealed, and where its moving part was last painted.
pub(crate) struct Reveal {
    pub(super) key: String,
    pub(super) extent: RevealExtent,
    /// The header's height to keep in view above a body's top.
    pub(super) room: f32,
    pub(super) pressed: Instant,
    /// The motion has ended; one more paint lands on the settled height.
    pub(super) settled: bool,
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

    /// True while `key` is opening or closing.
    pub(super) fn is_moving(&self, key: &str) -> bool {
        self.motions.contains_key(key)
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

    /// How far `key` is open for sizing what holds it: 0 shut to 1 open, following its motion. A
    /// state that differs from the one last drawn counts as not yet started, so the holder does
    /// not jump ahead of the drawing.
    pub(super) fn open_factor(&mut self, key: &str, open: bool) -> f32 {
        if let Some(frame) = self.frame(key) {
            return frame.log.clamp(0.0, 1.0);
        }
        let shown = self.observed.get(key).copied().unwrap_or(open);
        if shown { 1.0 } else { 0.0 }
    }

    /// Forget every disclosure whose key starts with `prefix`: the next time one is drawn it is
    /// seen for the first time, settled in whatever state it is in.
    pub(super) fn forget(&mut self, prefix: &str) {
        self.observed.retain(|key, _| !key.starts_with(prefix));
        self.motions.retain(|key, _| !key.starts_with(prefix));
        self.heights.retain(|key, _| !key.starts_with(prefix));
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
    motion_clip_revealed(height, frame, floor, gap, content, None)
}

/// [`motion_clip`] that also records where the clip paints into `extent`, for the disclosure being
/// revealed (`disclosure_anchor.rs`).
fn motion_clip_revealed(
    height: Rc<Cell<f32>>,
    frame: FoldFrame,
    floor: f32,
    gap: f32,
    content: AnyElement,
    extent: Option<RevealExtent>,
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
    .when_some(extent, |clip, extent| {
        clip.relative().child(extent_canvas(extent, false))
    })
    .into_any_element()
}

/// Where the part being revealed last painted, in window pixels. Taken when applied, so one paint
/// scrolls once.
pub(super) type RevealExtent = Rc<Cell<Option<PaintedPart>>>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaintedPart {
    /// The horizontal middle, to tell a transcript row from a panel beside or below the list.
    pub(super) center_x: f32,
    pub(super) top: f32,
    pub(super) bottom: f32,
    /// The part starts with its own header rather than being the body under one.
    pub(super) header_inside: bool,
}

/// A canvas filling its parent that records the parent's window bounds into `extent`.
fn extent_canvas(extent: RevealExtent, header_inside: bool) -> AnyElement {
    gpui::canvas(
        move |bounds, _, _| {
            extent.set(Some(PaintedPart {
                center_x: bounds.center().x.as_f32(),
                top: bounds.top().as_f32(),
                bottom: bounds.bottom().as_f32(),
                header_inside,
            }))
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
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
            Some(frame) => motion_clip_revealed(
                self.disclosure_height(key),
                frame,
                0.0,
                gap,
                body,
                self.revealing(key),
            ),
            None => self.reveal_extent(key, false, body),
        }
    }

    /// The extent cell of `key` while it is the disclosure being revealed.
    fn revealing(&self, key: &str) -> Option<RevealExtent> {
        self.disclosure_motion
            .borrow()
            .reveal
            .as_ref()
            .filter(|reveal| reveal.key == key)
            .map(|reveal| reveal.extent.clone())
    }

    /// Record where `part` of disclosure `key` paints while it is the one being revealed
    /// (`disclosure_anchor.rs`); any other disclosure is returned as it is. `header_inside` says
    /// the part starts with its own header (a tool group whose toggle tops it) rather than being
    /// the body below one.
    pub(super) fn reveal_extent(
        &self,
        key: &str,
        header_inside: bool,
        part: AnyElement,
    ) -> AnyElement {
        let Some(extent) = self.revealing(key) else {
            return part;
        };
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(part)
            .child(extent_canvas(extent, header_inside))
            .into_any_element()
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
        motion_clip_revealed(
            self.disclosure_height(key),
            frame,
            floor,
            gap,
            body,
            self.revealing(key),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fold_seen_first_is_settled_and_a_change_moves_it() {
        let mut motions = DisclosureMotions::default();
        assert!(
            motions
                .observe("session-details:tasks", true, false)
                .is_none()
        );
        assert!(!motions.running());
        let frame = motions.observe("session-details:tasks", false, false);
        assert!(frame.is_some_and(|frame| frame.kind == FoldMotionKind::Close));
        assert!(motions.running());
    }

    #[test]
    fn a_fold_is_still_when_motion_is_reduced() {
        let mut motions = DisclosureMotions::default();
        motions.observe("session-details:tasks", true, true);
        assert!(
            motions
                .observe("session-details:tasks", false, true)
                .is_none()
        );
        assert!(!motions.running());
    }

    #[test]
    fn forgetting_a_prefix_makes_the_next_draw_a_first_sight() {
        let mut motions = DisclosureMotions::default();
        motions.observe("session-details:tasks", true, false);
        motions.observe("session-details:tasks", false, false);
        motions.observe("tool-card:1", true, false);
        motions.forget("session-details:");
        assert!(!motions.running());
        assert!(
            motions
                .observe("session-details:tasks", true, false)
                .is_none()
        );
        assert!(!motions.running());
        // Other disclosures keep watching their state.
        assert!(motions.observe("tool-card:1", false, false).is_some());
    }
}
