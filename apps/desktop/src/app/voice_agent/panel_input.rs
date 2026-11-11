//! The voice panel's text box, under the transcript, and Space while the panel has the keyboard.
//!
//! CDXC:VoiceMode 2026-11-10 DECISION: User: "Add an input area at the bottom for the voice agent on the phone and on PC, so I can be muted and still type to the voice agent. It controls all of my sessions from the voice agent, and I can type instead of speaking every time. We need a text box at the bottom." A live call shows a text box under the transcript: Enter sends, Shift+Enter is a new line. What is sent goes into the call as the user's turn (the call socket's `text` frame, `server/src/voice_agent/socket.rs`): gxserver shows it as the user's line, cuts a reply being spoken and answers it like a spoken one, by voice and in the transcript, with all its tools. Typing never unmutes and never holds Talk. The phone does its part in `src/voice/`.
//! CDXC:VoiceMode 2026-11-10 DECISION: User, via the coordinator: holding Space to talk keeps working while the voice panel has the keyboard, except while the text box itself is focused, where Space types a space. The panel takes the keyboard from a press on its transcript or its text box (`panel_copy.rs`), which leaves the main window inactive and its own Space hold idle, so the panel holds Talk on Space itself and tells the app when it would, for the header's keycap. A Space hold ends when the panel loses the keyboard, as the main window's does. Escape in the text box gives the keyboard back to the main window (user, 2026-11-11: "also i can't type in the text box"; the fix itself is the first responder kept in `GpuiAppToastWindowChrome.m`).

use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, KeyDownEvent, KeyUpEvent, MouseButton, ParentElement as _, Styled as _,
    Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};

use super::super::conversation::VoicePhase;
use super::super::hold_to_talk::plain_space;
use super::{PanelColors, VoicePanelWindow};

/// The text box with its Enter subscription, and the panel's keyboard-loss watch.
pub(super) fn panel_text_box(
    window: &mut Window,
    cx: &mut Context<VoicePanelWindow>,
) -> (Entity<TextareaState>, Vec<Subscription>) {
    let input = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(1, 5)
            .auto_text_direction(true)
            .submit_on_enter(true)
            .placeholder("Type to the voice agent…")
    });
    // With `submit_on_enter` a plain Enter (or Cmd+Enter) reports `PressEnter`; Shift+Enter stays
    // a new line.
    let enter = cx.subscribe_in(
        &input,
        window,
        |panel: &mut VoicePanelWindow, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                panel.send_typed(window, cx);
            }
        },
    );
    let activation = cx.observe_window_activation(window, |panel, window, cx| {
        if !window.is_window_active() {
            let app = panel.app.clone();
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| app.voice_agent_window_deactivated(cx));
            });
        }
        cx.notify();
    });
    (input, vec![enter, activation])
}

impl VoicePanelWindow {
    fn text_box_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.text_box
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
    }

    /// Sends what was typed into the call and empties the box; kept when no call is live to take
    /// it.
    fn send_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text_box.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let sent = self
            .app
            .update(cx, |app, _| app.send_voice_agent_text(&text))
            .unwrap_or(false);
        if sent {
            self.text_box
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    /// Escape while the text box has the keyboard: the main window gets it back, the way closing a
    /// modal hands it back (`activate_main_window_after_app_modal`); what was typed stays.
    pub(super) fn text_box_escape(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "escape"
            || event.keystroke.modifiers.modified()
            || !self.text_box_focused(window, cx)
        {
            return;
        }
        cx.stop_propagation();
        self.focus.focus(window, cx);
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| app.activate_main_window_after_app_modal(cx));
        });
    }

    /// A key down anywhere in the panel: the text box keeps every key it has, Space holds Talk
    /// elsewhere, and Cmd+C copies the transcript's selection.
    pub(super) fn panel_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text_box_focused(window, cx) {
            return;
        }
        if plain_space(&event.keystroke) {
            cx.stop_propagation();
            let repeat = event.is_held;
            let app = self.app.clone();
            // Deferred: the hold redraws this very panel.
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| app.voice_agent_panel_space(true, repeat, cx));
            });
            return;
        }
        Self::copy_key(event, window, cx);
    }

    pub(super) fn panel_key_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        if event.keystroke.key != "space" {
            return;
        }
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| app.voice_agent_panel_space(false, false, cx));
        });
    }

    /// Tells the app, when it changes, whether Space would hold Talk in the panel now.
    pub(super) fn note_space_free(&mut self, window: &Window, cx: &mut Context<Self>) {
        let free = self.snapshot.phase == VoicePhase::Live
            && window.is_window_active()
            && !self.text_box_focused(window, cx);
        if free == self.space_free_noted {
            return;
        }
        self.space_free_noted = free;
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.note_voice_agent_panel_space_free(free, cx)
            });
        });
    }

    /// The box under the transcript and the status strip, on the buttons. It keeps its height and
    /// grows upward to five lines, then scrolls; the transcript gives way and ends above it. A
    /// press on it gives the panel the keyboard, as a press on the transcript does.
    pub(super) fn text_box(&self, colors: &PanelColors) -> AnyElement {
        div()
            .flex_shrink_0()
            .px(px(12.0))
            .pb(px(8.0))
            .child(
                div()
                    .id("voice-panel-text-box")
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.raised)
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(12.5))
                    .text_color(colors.text)
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        if !window.is_window_active() {
                            window.activate_window();
                        }
                    })
                    .child(
                        Textarea::new(&self.text_box)
                            .appearance(false)
                            .bordered(false)
                            .caret_color(colors.text)
                            .text_size(px(12.5)),
                    ),
            )
            .into_any_element()
    }
}
