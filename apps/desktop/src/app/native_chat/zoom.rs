//! Keyboard zoom for the chat pane: Cmd+= / Cmd+- / Cmd+0 (Ctrl on Windows and Linux).
//!
//! GPUI chat paints everything at `ChatAppearance::scale`, so the chords step a per-pane override
//! that the appearance prefers over the configured `sessionChatZoomPercent`. Like Chromium's page
//! zoom the override is temporary: it belongs to this view, is never written to Settings, and a
//! change to the configured default drops it.

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::{App, Context, KeyBinding, Window};
use serde_json::Value;
use std::sync::Arc;

/// The override rides in the snapshot so every window drawn from it (the maximized composer, the
/// pickers, the image viewer) paints at the pane's size without a second zoom channel.
const ZOOM_PERCENT_KEY: &str = "keyboardZoomPercent";

/// CDXC:SessionChat 2026-09-19 WHY:
/// The rungs are Chromium's preset zoom factors kept inside the 70% to 200% range Settings offers,
/// so the same number of presses lands on the same percentage as the React chat did on web and mobile, and
/// every rung is also a multiple of the shared 5% step. Cmd+0 drops the override and returns the pane to the
/// configured default instead of to a flat 100%, which is what a reset means once the default is a
/// user setting.
/// SEE-ALSO: packages/shared/ghostex-settings/types.ts owns the range, step and default.
const ZOOM_PERCENT_RUNGS: [f32; 10] = [
    70.0, 75.0, 80.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0,
];

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ChatZoomIn;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ChatZoomOut;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ChatZoomReset;

#[derive(Clone, Copy)]
enum ZoomStep {
    In,
    Out,
    Reset,
}

/// The temporary zoom this pane's keyboard asked for, and the configured default it was stepped
/// from, so a later change to that default can drop it.
#[derive(Default)]
pub(super) struct ChatZoomState {
    override_percent: Option<f32>,
    default_percent: Option<f32>,
}

struct ChatZoomRegistered;
impl gpui::Global for ChatZoomRegistered {}

/// The chords are bound in the pane's context and in the focused field's, the way transcript
/// search claims Cmd+F: a binding on `NativeChat` alone loses by depth to the input while the
/// composer has focus. The shell's own zoom chords for the focused terminal or CEF pane stay bound
/// at the window root, where this deeper binding wins while the chat pane holds the keyboard and
/// every other surface keeps its own zoom.
pub(super) fn register(cx: &mut App) {
    if cx.has_global::<ChatZoomRegistered>() {
        return;
    }
    cx.set_global(ChatZoomRegistered);
    let mut bindings = Vec::new();
    for context in ["NativeChat", "NativeChat > Input"] {
        bindings.push(KeyBinding::new("secondary-=", ChatZoomIn, Some(context)));
        bindings.push(KeyBinding::new("secondary-+", ChatZoomIn, Some(context)));
        bindings.push(KeyBinding::new("secondary--", ChatZoomOut, Some(context)));
        bindings.push(KeyBinding::new("secondary-0", ChatZoomReset, Some(context)));
    }
    cx.bind_keys(bindings);
}

/// The temporary zoom a snapshot carries, if its pane has one.
pub(super) fn keyboard_zoom_percent(state: &Value) -> Option<f32> {
    state[ZOOM_PERCENT_KEY]
        .as_f64()
        .map(|percent| percent as f32)
}

fn write_keyboard_zoom_percent(state: &mut Value, percent: Option<f32>) {
    let Some(object) = state.as_object_mut() else {
        return;
    };
    match percent {
        Some(percent) => {
            object.insert(ZOOM_PERCENT_KEY.to_string(), serde_json::json!(percent));
        }
        None => {
            object.remove(ZOOM_PERCENT_KEY);
        }
    }
}

impl NativeChatView {
    pub(super) fn chat_zoom_in_action(
        &mut self,
        _: &ChatZoomIn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_chat_zoom(ZoomStep::In, cx);
    }

    pub(super) fn chat_zoom_out_action(
        &mut self,
        _: &ChatZoomOut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_chat_zoom(ZoomStep::Out, cx);
    }

    pub(super) fn chat_zoom_reset_action(
        &mut self,
        _: &ChatZoomReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_chat_zoom(ZoomStep::Reset, cx);
    }

    /// Carries this pane's temporary zoom into a freshly published snapshot.
    pub(super) fn apply_chat_zoom(&self, state: &mut Value) {
        write_keyboard_zoom_percent(state, self.zoom.override_percent);
    }

    /// Drops the temporary zoom when the configured default changes, so Settings > Chat stays in
    /// charge of what this pane returns to.
    pub(super) fn sync_chat_zoom_default(&mut self) {
        let default = ChatAppearance::default_zoom_percent(&self.snapshot);
        if self.zoom.default_percent == Some(default) {
            return;
        }
        self.zoom.default_percent = Some(default);
        if self.zoom.override_percent.take().is_some() {
            self.rescale_chat();
        }
    }

    /// Publishes the new size to this pane's snapshot and drops the transcript's cached row
    /// heights: `ListState` keeps a measured height per row and only re-measures them when the
    /// list's width changes, which a zoom does not.
    fn rescale_chat(&mut self) {
        let percent = self.zoom.override_percent;
        write_keyboard_zoom_percent(Arc::make_mut(&mut self.snapshot), percent);
        let rows = self.list.item_count();
        self.splice_transcript(0, rows, rows);
    }

    fn step_chat_zoom(&mut self, step: ZoomStep, cx: &mut Context<Self>) {
        let default = ChatAppearance::default_zoom_percent(&self.snapshot);
        self.zoom.default_percent = Some(default);
        let current = self.zoom.override_percent.unwrap_or(default);
        let next = match step {
            ZoomStep::Reset => default,
            // No rung left in that direction: the pane stays where it is, clamped by the range.
            ZoomStep::In => ZOOM_PERCENT_RUNGS
                .iter()
                .copied()
                .find(|rung| *rung > current + 0.01)
                .unwrap_or(current),
            ZoomStep::Out => ZOOM_PERCENT_RUNGS
                .iter()
                .rev()
                .copied()
                .find(|rung| *rung < current - 0.01)
                .unwrap_or(current),
        };
        self.zoom.override_percent = (next != default).then_some(next);
        self.rescale_chat();
        cx.stop_propagation();
        cx.notify();
    }
}
