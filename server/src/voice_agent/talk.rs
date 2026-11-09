//! Holding to talk, for every client at once: what a press, a release, a cancel, a mute change and
//! a dropped link do to the turn, decided here and carried out by `conversation.rs`. Push-to-talk
//! calls listen only while the hold lasts; a Regular call that is muted listens while it lasts.
//!
//! CDXC:VoiceMode 2026-10-24 DECISION: User, asked "What should the hold button and Space do in a Regular call?", picked "While muted, holding unmutes for as long as you hold" ("Like Zoom's hold Space to unmute. The provider still decides when you finished, and Grok still bills the whole call"). Supersedes "No need to switch when I'm muted" (2026-10-21, `talk_start` in conversation.rs) for this one case: the call never changes mode, the hold only opens the microphone. Releasing while still muted streams silence so the provider's turn detection hears you stop.
//! CDXC:VoiceMode 2026-10-24 DECISION: User, asked "How should Mute and push-to-talk work together?", picked "Mute wins: the hold button greys out and says "Unmute to talk"" ("Mute becomes a lock against talking by accident"). gxserver refuses a press while muted (`refused`) and muting during a hold cancels it, whatever the client drew.
//! CDXC:VoiceMode 2026-10-24 DECISION: User, asked "What should a very short press (a tap) do?", picked "Stop the agent if it's speaking, then show "Hold to talk" for a moment" ("Nothing is sent"). A hold with less than `MIN_HOLD_AUDIO_MS` of audio answers `tooShort` and sends nothing; the press itself already cut the agent off.
//! CDXC:VoiceMode 2026-10-24 WHY: a link that drops while Talk is held used to leave gxserver hearing you until the next press. A push-to-talk hold is cancelled when its client goes (half a sentence is not a turn); a Regular call's hold ends as a release would.
//! SEE-ALSO: apps/desktop/src/app/voice_agent/hold_to_talk.rs, apps/mobile/app/src/voice/conversation.ts (`setVoiceTalking`), GhostexVoice.kt (`setTalking`).

use std::time::{Duration, Instant};

/// A hold with less audio than this held no speech worth answering.
pub(super) const MIN_HOLD_AUDIO_MS: u64 = 200;
/// The longest gxserver streams silence after a Regular call's hold, waiting for the provider to
/// hear the turn end.
pub(super) const SILENCE_AFTER_HOLD: Duration = Duration::from_secs(3);

/// What a press does.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Press {
    /// A push-to-talk turn starts: cut the agent off and clear the provider's buffer.
    Talk,
    /// A muted Regular call listens for as long as the hold lasts.
    Unmute,
    /// A muted push-to-talk call: nothing is heard until the user unmutes.
    RefusedMuted,
    /// Already held, or a Regular call that already listens.
    Ignored,
}

/// What a release does.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Release {
    /// Nothing was held.
    Ignored,
    /// Too short to hold speech: nothing is sent. `clear` drops the push-to-talk buffer.
    TooShort { clear: bool },
    /// Push-to-talk: commit what was said and ask for the reply.
    Commit,
    /// A Regular call's hold ended; the provider's turn detection finishes the turn (with silence
    /// streamed after it while the call is still muted).
    Unmuted,
}

/// What cancelling a hold does.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Cancel {
    /// Nothing was held.
    Ignored,
    /// The hold is dropped and the provider's buffer cleared; nothing is answered.
    Cleared,
}

pub(super) struct Talk {
    push_to_talk: bool,
    /// The client's Mute (`mute` frame); gxserver drops the microphone's audio while it is on.
    muted: bool,
    held: bool,
    held_audio_ms: u64,
    /// A Regular call's hold ended while muted: silence streams until this, or until the provider
    /// hears the turn end.
    silence_until: Option<Instant>,
}

impl Talk {
    pub(super) fn new(push_to_talk: bool) -> Self {
        Self {
            push_to_talk,
            muted: false,
            held: false,
            held_audio_ms: 0,
            silence_until: None,
        }
    }

    pub(super) fn held(&self) -> bool {
        self.held
    }

    /// The attached client's Mute is on.
    pub(super) fn muted(&self) -> bool {
        self.muted
    }

    pub(super) fn press(&mut self) -> Press {
        if self.held {
            return Press::Ignored;
        }
        if self.push_to_talk {
            if self.muted {
                return Press::RefusedMuted;
            }
        } else if !self.muted {
            return Press::Ignored;
        }
        self.held = true;
        self.held_audio_ms = 0;
        self.silence_until = None;
        if self.push_to_talk {
            Press::Talk
        } else {
            Press::Unmute
        }
    }

    pub(super) fn release(&mut self, now: Instant) -> Release {
        if !std::mem::take(&mut self.held) {
            return Release::Ignored;
        }
        let short = self.held_audio_ms < MIN_HOLD_AUDIO_MS;
        if self.push_to_talk {
            return if short {
                Release::TooShort { clear: true }
            } else {
                Release::Commit
            };
        }
        if short {
            return Release::TooShort { clear: false };
        }
        if self.muted {
            self.silence_until = Some(now + SILENCE_AFTER_HOLD);
        }
        Release::Unmuted
    }

    /// The user slid off the button, or muted a push-to-talk call mid-hold.
    pub(super) fn cancel(&mut self) -> Cancel {
        if std::mem::take(&mut self.held) {
            Cancel::Cleared
        } else {
            Cancel::Ignored
        }
    }

    /// The client's Mute changed. Muting a push-to-talk call mid-hold cancels the hold (Mute wins);
    /// unmuting stops the silence after a Regular hold, since the microphone speaks again.
    pub(super) fn set_muted(&mut self, muted: bool) -> Cancel {
        self.muted = muted;
        if !muted {
            self.silence_until = None;
        }
        if muted && self.push_to_talk {
            return self.cancel();
        }
        Cancel::Ignored
    }

    /// The client's link dropped: a push-to-talk hold is cancelled, a Regular one ends as a release.
    pub(super) fn detached(&mut self, now: Instant) -> Option<Release> {
        if !self.held {
            return None;
        }
        if self.push_to_talk {
            self.cancel();
            return Some(Release::TooShort { clear: true });
        }
        Some(self.release(now))
    }

    /// A client attached: its own Mute arrives in a `mute` frame after `ready`, so a device taking
    /// over never inherits the last one's.
    pub(super) fn attached(&mut self) {
        self.muted = false;
        self.held = false;
        self.silence_until = None;
    }

    /// Whether the microphone's audio reaches the provider, counting what a hold carries.
    pub(super) fn passes_audio(&mut self, bytes_ms: u64) -> bool {
        let open = (!self.muted || self.held) && (!self.push_to_talk || self.held);
        if open && self.held {
            self.held_audio_ms += bytes_ms;
        }
        open
    }

    /// Whether gxserver should stream another moment of silence (a Regular hold ended while muted).
    pub(super) fn wants_silence(&mut self, now: Instant) -> bool {
        match self.silence_until {
            Some(until) if now < until && self.muted && !self.held => true,
            Some(_) => {
                self.silence_until = None;
                false
            }
            None => false,
        }
    }

    /// The provider heard the turn end: no more silence is needed.
    pub(super) fn speech_stopped(&mut self) {
        self.silence_until = None;
    }
}
