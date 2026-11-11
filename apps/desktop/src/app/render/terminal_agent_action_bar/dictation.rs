// The terminal bar's Dictate button: the microphone records to a file first and the recording is
// transcribed after Stop (`voice_agent::Dictate`); the text is pasted into that session's terminal.
// Pressing the button while it transcribes cancels (the recording is kept as dismissed), and
// right-clicking it transcribes the newest recording that has no text yet (Transcribe previous).
// A failure is a toast that says why; the recording stays for Transcribe previous and in
// Settings › Dictation › Dictation history.
//
// CDXC:SessionChat 2026-10-08 DECISION:
// User: "show the mic in the terminal view bottom bar to the left of the chat view button". It is
// the chat composer's Dictate control (`native_chat/dictation.rs`) on the terminal bar: press to
// listen, press again to stop; what was heard is pasted into the session's terminal and Enter is
// never pressed. One microphone serves the whole window, so pressing Dictate on another session
// while one listens stops that one first, and its words still go to its own terminal.
// CDXC:VoiceMode 2026-10-20 WHY: Escape is not a cancel here: the terminal owns its keys (a native terminal surface takes them before the app sees them) and Escape belongs to the agent, so pressing the button again while it transcribes is the terminal bar's cancel.

use crate::app::model::*;
use crate::app::voice_agent::{Dictate, DictateEvent, DictatePhase};
use crate::*;
use gpui::{AppContext as _, Entity, Subscription, Window};

/// Whether this build can dictate: macOS, Windows and Linux, like the chat box's Dictate
/// (CDXC:VoiceMode 2026-10-20 DECISION in `native_chat/dictation.rs`). Off everywhere while
/// `DICTATION_ENABLED` is.
pub(super) const TERMINAL_DICTATION: bool = crate::app::native_chat::dictation::DICTATION_ENABLED
    && cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ));

/// The session the microphone is working for, and the subscription that pastes its result.
pub(crate) struct TerminalDictation {
    session_id: TerminalSessionId,
    dictate: Entity<Dictate>,
    shown: TerminalDictationState,
    _subscription: Subscription,
}

/// What the terminal bar's Dictate button shows for a session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalDictationState {
    Idle,
    Recording,
    Transcribing,
}

impl GhostexGpuiApp {
    /// What the Dictate button shows for `session_id` (kept current by an observer, so the bar
    /// reads it without the dictation's own state during layout).
    pub(super) fn terminal_dictation_state(
        &self,
        session_id: TerminalSessionId,
    ) -> TerminalDictationState {
        self.terminal_dictation
            .as_ref()
            .filter(|dictation| dictation.session_id == session_id)
            .map_or(TerminalDictationState::Idle, |dictation| dictation.shown)
    }

    /// Start recording for `session_id`, stop a recording (and transcribe it), or cancel a
    /// transcription.
    pub(super) fn toggle_terminal_dictation(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(dictation) = &self.terminal_dictation {
            let dictate = dictation.dictate.clone();
            let (recording, transcribing) = {
                let dictate = dictate.read(cx);
                (dictate.recording(), dictate.transcribing())
            };
            if recording {
                dictate.update(cx, |dictate, cx| dictate.stop(cx));
                cx.notify();
                return;
            }
            if transcribing && dictation.session_id == session_id {
                dictate.update(cx, |dictate, cx| dictate.cancel(cx));
                cx.notify();
                return;
            }
            if transcribing {
                // Another session's words are on their way to its own terminal.
                return;
            }
        }
        let dictate = self.terminal_dictate_for(session_id, window, cx);
        dictate.update(cx, |dictate, cx| dictate.start(cx));
        cx.notify();
    }

    /// Right-click: the newest recording without text, pasted into this session's terminal.
    pub(super) fn transcribe_previous_terminal_dictation(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.terminal_dictation.as_ref().is_some_and(|dictation| {
            let dictate = dictation.dictate.read(cx);
            dictate.recording() || dictate.transcribing()
        }) {
            return;
        }
        let dictate = self.terminal_dictate_for(session_id, window, cx);
        dictate.update(cx, |dictate, cx| dictate.transcribe_previous(cx));
        cx.notify();
    }

    /// A fresh Dictate for `session_id`, replacing an idle one of another session (a running one
    /// keeps its own subscription until it ends, so its words still reach its own terminal).
    fn terminal_dictate_for(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<Dictate> {
        if let Some(dictation) = &self.terminal_dictation
            && dictation.session_id == session_id
        {
            return dictation.dictate.clone();
        }
        let dictate = cx.new(|_| Dictate::new());
        let subscription = cx.subscribe_in(&dictate, window, move |this, _, event, _, cx| {
            match event {
                DictateEvent::Text(text) => this.paste_terminal_dictation(session_id, text, cx),
                DictateEvent::Failure { title, message } => {
                    let message = if title == "Transcription failed" {
                        format!(
                            "{message} Right-click the microphone to transcribe it again, or find it in Settings > Dictation > Past recordings."
                        )
                    } else {
                        message.clone()
                    };
                    let toast =
                        crate::app::native_chat::dictation::dictation_toast(title, &message);
                    this.receive_gpui_app_toast_bridge_message(&toast, cx);
                }
            }
            cx.notify();
        });
        cx.observe(&dictate, |this, dictate, cx| {
            let shown = match dictate.read(cx).phase() {
                DictatePhase::Starting | DictatePhase::Recording => {
                    TerminalDictationState::Recording
                }
                DictatePhase::Transcribing => TerminalDictationState::Transcribing,
                DictatePhase::Idle | DictatePhase::Failed { .. } => TerminalDictationState::Idle,
            };
            if let Some(dictation) = this.terminal_dictation.as_mut()
                && dictation.dictate == dictate
            {
                dictation.shown = shown;
            }
            cx.notify();
        })
        .detach();
        self.terminal_dictation = Some(TerminalDictation {
            session_id,
            dictate: dictate.clone(),
            shown: TerminalDictationState::Idle,
            _subscription: subscription,
        });
        dictate
    }

    /// Paste a finished dictation into the session's terminal, with a trailing space so the next
    /// phrase does not run into it. Enter is never sent.
    fn paste_terminal_dictation(
        &mut self,
        session_id: TerminalSessionId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        let pasted = format!("{text} ");
        view.update(cx, |view, cx| {
            view.paste_text(&pasted, cx);
        });
    }
}
