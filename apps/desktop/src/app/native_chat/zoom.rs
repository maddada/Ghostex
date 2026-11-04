//! Keyboard zoom for the chat: Cmd+= / Cmd+- / Cmd+0 (Ctrl on Windows and Linux).
//!
//! GPUI chat paints everything at `ChatAppearance::scale`, which is the saved
//! `sessionChatZoomPercent` (Settings > Interface > Default chat zoom). The chords step that setting,
//! so every open chat on the computer, and every window drawn from one (the maximized composer,
//! the pickers, the image viewer), changes size together.

use super::state::NativeChatView;
use gpui::{App, Context, Task, Window};
use std::cell::Cell;
use std::time::Duration;

/// CDXC:SessionChat 2026-10-15 DECISION:
/// User: "let's please make the size of the chats all the same (so when i zoom in 1 chat it zoomes all of them not individual one)". The zoom chords step the saved `sessionChatZoomPercent` instead of a per-pane override, so every open chat follows at once and Settings > Chat shows the new value. Cmd+0 sets 100%. Supersedes the 2026-09-19 per-pane temporary override (the `keyboardZoomPercent` snapshot key).
///
/// CDXC:SessionChat 2026-09-19 WHY:
/// The rungs are Chromium's preset zoom factors kept inside the 70% to 200% range Settings offers,
/// so the same number of presses lands on the same percentage as the React chat did on web and mobile, and
/// every rung is also a multiple of the shared 5% step.
/// SEE-ALSO: packages/settings-catalog/src/data/limits.rs owns the range and step; packages/settings-catalog/src/data/defaults.rs the default. The Files text zoom (native_docs/text_zoom.rs) steps the same rungs.
const ZOOM_PERCENT_RUNGS: [f32; 10] = [
    70.0, 75.0, 80.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0,
];

/// Matches the Settings sliders' debounce (`NUMERIC_SETTINGS_DEBOUNCE`): a held chord shows
/// every rung at once and saves once, after the keys go quiet.
const ZOOM_SAVE_DEBOUNCE: Duration = Duration::from_millis(180);

const RESET_ZOOM_PERCENT: f32 = 100.0;

thread_local! {
    /// The zoom the chords asked for that the debounced save has not written yet; every chat
    /// paints at it meanwhile, so the next press steps from what is on screen.
    static PENDING_ZOOM_PERCENT: Cell<Option<f32>> = const { Cell::new(None) };
}

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
pub(crate) enum ZoomStep {
    In,
    Out,
    Reset,
}

/// The zoom one chord lands on from `current`: the next rung up or down, or 100% for a reset.
/// With no rung left in that direction the zoom stays where it is, clamped by the range.
pub(crate) fn step_zoom_percent(current: f32, step: ZoomStep) -> f32 {
    match step {
        ZoomStep::Reset => RESET_ZOOM_PERCENT,
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
    }
}

/// The one debounced save, whichever chat's chord started it; a later press replaces it.
struct ChatZoomSave(#[allow(dead_code)] Task<()>);
impl gpui::Global for ChatZoomSave {}

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
        let chord = super::keyboard::primary_chord;
        bindings.push(crate::app::hotkeys::gpui_runtime_key_binding(
            &chord("="),
            ChatZoomIn,
            Some(context),
            cx,
        ));
        bindings.push(crate::app::hotkeys::gpui_runtime_key_binding(
            &chord("+"),
            ChatZoomIn,
            Some(context),
            cx,
        ));
        bindings.push(crate::app::hotkeys::gpui_runtime_key_binding(
            &chord("-"),
            ChatZoomOut,
            Some(context),
            cx,
        ));
        bindings.push(crate::app::hotkeys::gpui_runtime_key_binding(
            &chord("0"),
            ChatZoomReset,
            Some(context),
            cx,
        ));
    }
    cx.bind_keys(bindings);
}

/// The chat zoom every chat paints at: a chord's value still waiting to be saved, else the saved
/// `sessionChatZoomPercent`.
pub(super) fn chat_zoom_percent(settings: &serde_json::Map<String, serde_json::Value>) -> f32 {
    PENDING_ZOOM_PERCENT.with(Cell::get).unwrap_or_else(|| {
        settings
            .get("sessionChatZoomPercent")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(100.0) as f32
    })
}

fn current_chat_zoom_percent() -> f32 {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    chat_zoom_percent(snapshot.object())
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

    /// Drops the transcript's cached row heights when the chat zoom changed since this view last
    /// drew, from a chord in any chat or from Settings: `ListState` keeps a measured height per
    /// row and only re-measures them when the list's width changes, which a zoom does not. A
    /// parked view catches up the next time it draws.
    pub(super) fn sync_chat_zoom(&mut self) {
        let percent = current_chat_zoom_percent();
        if self.drawn_zoom_percent == Some(percent) {
            return;
        }
        let first_draw = self.drawn_zoom_percent.is_none();
        self.drawn_zoom_percent = Some(percent);
        if !first_draw {
            let rows = self.list.item_count();
            self.splice_transcript(0, rows, rows);
        }
    }

    fn step_chat_zoom(&mut self, step: ZoomStep, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let current = current_chat_zoom_percent();
        let next = step_zoom_percent(current, step);
        if next == current {
            return;
        }
        PENDING_ZOOM_PERCENT.with(|pending| pending.set(Some(next)));
        // Every chat window redraws at the new size now; each view re-measures its rows in
        // `sync_chat_zoom` as it draws.
        cx.refresh_windows();
        let save = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_SAVE_DEBOUNCE).await;
            cx.update(|cx| {
                // Saved through the host's Settings patch, which updates the shared settings
                // snapshot in the same effect cycle, so no frame paints the old value between.
                if let Some(percent) = PENDING_ZOOM_PERCENT.with(Cell::take) {
                    let _ = this.update(cx, |this, cx| {
                        this.host(
                            "setChatZoomPercent",
                            serde_json::json!({ "percent": percent }),
                            cx,
                        );
                    });
                }
                cx.refresh_windows();
            });
        });
        cx.set_global(ChatZoomSave(save));
    }
}

#[cfg(test)]
mod tests {
    use super::{ZoomStep, step_zoom_percent};

    #[test]
    fn zoom_steps_through_the_rungs_and_resets_to_100() {
        assert_eq!(step_zoom_percent(100.0, ZoomStep::In), 110.0);
        assert_eq!(step_zoom_percent(110.0, ZoomStep::In), 125.0);
        assert_eq!(step_zoom_percent(100.0, ZoomStep::Out), 90.0);
        assert_eq!(step_zoom_percent(90.0, ZoomStep::Out), 80.0);
        assert_eq!(step_zoom_percent(175.0, ZoomStep::Reset), 100.0);
        assert_eq!(step_zoom_percent(70.0, ZoomStep::Reset), 100.0);
    }

    #[test]
    fn zoom_between_rungs_lands_on_the_next_one() {
        // A Settings slider value off the ladder (5% steps) steps to the nearest rung that way.
        assert_eq!(step_zoom_percent(105.0, ZoomStep::In), 110.0);
        assert_eq!(step_zoom_percent(105.0, ZoomStep::Out), 100.0);
        assert_eq!(step_zoom_percent(135.0, ZoomStep::In), 150.0);
        assert_eq!(step_zoom_percent(135.0, ZoomStep::Out), 125.0);
    }

    #[test]
    fn zoom_stays_inside_70_to_200() {
        assert_eq!(step_zoom_percent(200.0, ZoomStep::In), 200.0);
        assert_eq!(step_zoom_percent(70.0, ZoomStep::Out), 70.0);
        let mut percent = 100.0;
        for _ in 0..20 {
            percent = step_zoom_percent(percent, ZoomStep::In);
        }
        assert_eq!(percent, 200.0);
        for _ in 0..20 {
            percent = step_zoom_percent(percent, ZoomStep::Out);
        }
        assert_eq!(percent, 70.0);
    }
}
