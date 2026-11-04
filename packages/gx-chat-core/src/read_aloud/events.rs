//! What the host tells Read aloud: the inputs only the app knows, and the speech engine's answers.

use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::read_aloud::playback;
use crate::state::{ChatContext, ChatState};

/// One host input, as [`crate::Event::ReadAloud`] carries it.
///
/// The phone sends it as `{"type": "readAloud", "event": {"kind": "setAvailable", "on": true}}`
/// (`packages/gx-chat-json/src/event_json.rs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReadAloudEvent {
    /// The built-in switch is on for this chat's computer AND this device has a voice. Off stops
    /// the speech and hides every control.
    SetAvailable { on: bool },
    /// Auto-read is on for this run of the app (every chat hears the same value).
    SetAuto { on: bool },
    /// This chat is the one on screen. Off stops its speech and forgets what was waiting.
    SetOnScreen { on: bool },
    /// A voice agent conversation is open on this device.
    SetVoiceConversation { open: bool },
    /// The microphone is recording (Dictate).
    SetDictating { on: bool },
    /// The engine finished utterance `id`, or cut it off (`interrupted`).
    SpeechEnded {
        id: String,
        #[serde(default)]
        interrupted: bool,
    },
}

/// Applies one host input. Effects go to the outbox (`playback.rs`), so this returns none.
pub fn handle_event(
    state: &mut ChatState,
    event: &ReadAloudEvent,
    _context: &ChatContext,
) -> Vec<Effect> {
    match event {
        ReadAloudEvent::SetAvailable { on } => {
            state.read_aloud.available = *on;
            if !on {
                playback::stop_all(state);
            }
        }
        ReadAloudEvent::SetAuto { on } => set_auto(state, *on),
        ReadAloudEvent::SetOnScreen { on } => {
            state.read_aloud.on_screen = *on;
            // CDXC:VoiceMode 2026-10-20 DECISION: User: switching to another chat, closing the chat or leaving the session stops the speech and keeps nothing waiting; Auto-read stays on and applies to the chat now on screen.
            if !on {
                playback::stop_all(state);
            }
        }
        /*
        CDXC:VoiceMode 2026-10-21 DECISION:
        User (Q35): while a voice agent conversation is open, auto-read stays quiet, and it comes back when you hang up. The voice agent already announced the replies that finished during the call, so they are not read afterwards: hanging up takes a new baseline (`auto_read.rs`). Opening the call also stops anything being read, so it never talks over the voice agent.
        */
        ReadAloudEvent::SetVoiceConversation { open } => {
            state.read_aloud.voice_conversation = *open;
            if *open {
                playback::stop_all(state);
            }
        }
        // CDXC:VoiceMode 2026-10-22 WHY: starting to dictate stops the speech at once, and replies that arrive while the microphone is on wait until it stops, so the recognizer never hears the device's own voice and types it into the draft.
        ReadAloudEvent::SetDictating { on } => {
            state.read_aloud.dictating = *on;
            if *on {
                playback::stop_current(state);
            } else {
                playback::play_next(state);
            }
        }
        ReadAloudEvent::SpeechEnded { id, interrupted } => {
            playback::speech_ended(state, id, *interrupted);
        }
    }
    Vec::new()
}

/// Auto-read switched on or off for this chat.
///
/// CDXC:VoiceMode 2026-10-21 DECISION:
/// User (Q28): Auto-read never turns itself back on after the app was closed, on the phone and on the desktop. It is state of this run of the app, held by the host and pushed into every chat (`setAuto`), not a saved setting; the spec's `readAloudAutoRead` key is gone. Turning it off stops what auto-read was saying and forgets what it lined up; a reply you asked for yourself keeps playing.
pub(crate) fn set_auto(state: &mut ChatState, on: bool) {
    state.read_aloud.auto_read = on;
    if on {
        return;
    }
    if state
        .read_aloud
        .current
        .as_ref()
        .is_some_and(|playing| playing.auto)
    {
        playback::stop_current(state);
    }
    playback::forget_waiting(state);
}
