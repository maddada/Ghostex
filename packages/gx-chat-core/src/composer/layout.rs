//! Composer geometry: which toolbar controls fit, and when a scroll collapses the box.
//!
//! Port of `packages/shared/session-chat-presentation/composer-layout.ts` and
//! `composer-scroll.ts`. The animation metrics stay in
//! `packages/gx-chat-core/visual/composer-animation.json`, which both renderers
//! already read; nothing copies them here.

use serde::{Deserialize, Serialize};

/// How far a trackpad gesture must travel before the composer collapses.
pub const COMPOSER_SCROLL_THRESHOLD_PX: f64 = 24.0;
/// How long a gesture stays one gesture.
pub const COMPOSER_SCROLL_RESET_MS: f64 = 120.0;
/// How close to the bottom still counts as at the bottom.
pub const COMPOSER_BOTTOM_THRESHOLD_PX: f64 = 10.0;

/// What the renderer measured about its own toolbar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerMeasurements {
    pub available: f64,
    pub options: f64,
    pub actions: f64,
    pub footer_gap: f64,
    pub action_gap: f64,
    pub clearance: f64,
    pub has_overflow_options: bool,
    pub controls: Vec<ComposerControl>,
}

/// One measured toolbar control.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerControl {
    pub id: String,
    pub width: f64,
}

/// Which controls moved into More actions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerFit {
    pub overflowed: Vec<String>,
    pub options_overflowed: bool,
}

/// CDXC:SessionChat 2026-09-17 DECISION:
/// User: GPUI and React chat must stay visually in sync. Both renderers measure their controls and
/// move them into More actions in the same order.
pub fn fit_composer_controls(measurements: &ComposerMeasurements) -> ComposerFit {
    let mut required = measurements.options
        + measurements.actions
        + measurements.footer_gap
        + measurements.clearance;
    let mut overflowed = Vec::new();
    for control in &measurements.controls {
        if required <= measurements.available {
            break;
        }
        overflowed.push(control.id.clone());
        required -= control.width + measurements.action_gap;
    }
    ComposerFit {
        overflowed,
        options_overflowed: required > measurements.available && measurements.has_overflow_options,
    }
}

/// One in-flight scroll gesture over the composer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposerScrollGesture {
    pub accumulated_delta_px: f64,
    pub collapse_suppressed: bool,
    /// `-Infinity` until the first event, so the first one never continues an earlier gesture.
    pub last_event_at: f64,
}

impl Default for ComposerScrollGesture {
    fn default() -> Self {
        Self {
            accumulated_delta_px: 0.0,
            collapse_suppressed: false,
            last_event_at: f64::NEG_INFINITY,
        }
    }
}

impl ComposerScrollGesture {
    /// Back to no gesture at all.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// An expand or a caret move inside the same gesture cancels its collapse.
    pub fn suppress(&mut self, now: f64, gesture_reset_ms: f64) {
        if now - self.last_event_at <= gesture_reset_ms {
            self.collapse_suppressed = true;
        }
    }

    /// Folds one wheel event in and answers whether the composer should collapse now.
    pub fn record(&mut self, input: &ComposerScrollInput) -> bool {
        self.last_event_at = input.now;
        if self.collapse_suppressed
            || !input.collapse_eligible
            || !input.can_scroll_in_gesture_direction
            || input.scrolls_toward_logical_end
        {
            self.accumulated_delta_px = 0.0;
            return false;
        }
        self.accumulated_delta_px += input.delta_px;
        if self.accumulated_delta_px < input.collapse_threshold_px {
            return false;
        }
        self.accumulated_delta_px = 0.0;
        true
    }
}

/// One wheel event over the composer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposerScrollInput {
    pub now: f64,
    pub delta_px: f64,
    pub collapse_threshold_px: f64,
    pub collapse_eligible: bool,
    pub can_scroll_in_gesture_direction: bool,
    pub scrolls_toward_logical_end: bool,
}

/// Whether the composer may collapse at all right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollapseGate {
    pub maximized: bool,
    pub note_open: bool,
    pub suggestions_open: bool,
    pub has_error: bool,
    pub attachment_count: u32,
    pub pending_attachments: u32,
    pub queued_prompts: usize,
}

/// The gate itself: anything that owns the space below the input keeps the box open.
pub fn can_collapse_composer(gate: &CollapseGate) -> bool {
    !gate.maximized
        && !gate.note_open
        && !gate.suggestions_open
        && !gate.has_error
        && gate.attachment_count == 0
        && gate.pending_attachments == 0
        && gate.queued_prompts == 0
}
