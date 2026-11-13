//! Read aloud on the host's side: what only the app knows, handed to every chat, and the one
//! speaker the whole app shares.
//!
//! The core (`packages/gx-chat-core/src/read_aloud/`) decides what is said and when. This file
//! tells each chat the facts it cannot know (`setAvailable`, `setAuto`, `setOnScreen`,
//! `setVoiceConversation`, `setDictating`), performs its `speak` and `stopSpeaking` on the app's
//! speaker (`crate::app::read_aloud`, a file twin in the web build), keeps Auto-read for this run
//! of the app (`setReadAloudAuto`) and answers with `speechEnded`. Client contract:
//! `docs/2026-10-20/voice-mode/read-aloud-client-contract.md`.
//!
//! CDXC:VoiceMode 2026-10-22 WHY: it lives in the chat host because the host is the one place that holds every chat: one speaker for the whole app means a `speak` from one chat cuts off another chat's utterance, and that chat has to hear it (`speechEnded` with `interrupted`).

use std::collections::HashMap;
use std::sync::Mutex;

use ghostex_gx_chat_core::{Effect, Event, ReadAloudEvent, UserAction};
use serde_json::{Value, json};

use super::world::{self, World};

/// What the app tells the host about Read aloud (`HostCommand::ReadAloud`).
pub(crate) enum ReadAloudInput {
    /// The chat on screen, by its retention key (`ChatHostHandle::key`), or none.
    OnScreen(Option<String>),
    /// A voice agent conversation is open on this computer.
    VoiceConversation(bool),
    /// Dictate's microphone is recording.
    Dictating(bool),
    /// The speaker finished, or cut off, what it was handed under `token`.
    SpeechEnded { token: u64, interrupted: bool },
    /// The Stop reading hotkey: whichever chat is speaking stops.
    Stop,
}

/// The facts one chat was last told. All false is what a new chat starts from.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Told {
    available: bool,
    auto: bool,
    on_screen: bool,
    voice_conversation: bool,
    dictating: bool,
}

/// The utterance on the speaker: the chat that asked, the core's id, the speaker's token.
struct Speaking {
    key: String,
    id: String,
    token: u64,
}

/// Read aloud's app-wide state, held by the host (`World::read_aloud`).
#[derive(Default)]
pub(super) struct ReadAloudHost {
    /// The feature is on and this device has a voice; `None` until first looked at.
    available: Option<bool>,
    /// Auto-read for this run of the app, never saved (CDXC:VoiceMode 2026-10-21 DECISION in
    /// packages/gx-chat-core/src/read_aloud/events.rs).
    auto: bool,
    on_screen: Option<String>,
    voice_conversation: bool,
    dictating: bool,
    told: HashMap<String, Told>,
    speaking: Option<Speaking>,
    next_token: u64,
    /// What one chat owes another, delivered once the turn's step is over.
    owed: Vec<(String, Event)>,
    /// An app-wide fact changed: every chat is brought up to date after the step.
    stale: bool,
}

/// How many rounds [`flush`] may take: an owed event can make a chat speak, which can owe the
/// chat it cut off an answer. Two rounds is the longest real chain.
const MAX_FLUSH_ROUNDS: usize = 4;

impl ReadAloudHost {
    /// The events that bring chat `key` up to what the app knows now.
    fn events_for(&mut self, key: &str) -> Vec<Event> {
        let available = *self.available.get_or_insert_with(available_now);
        let wanted = Told {
            available,
            auto: self.auto,
            on_screen: self.on_screen.as_deref() == Some(key),
            voice_conversation: self.voice_conversation,
            dictating: self.dictating,
        };
        let told = self.told.entry(key.to_string()).or_default();
        let mut events = Vec::new();
        let mut tell = |changed: bool, event: ReadAloudEvent| {
            if changed {
                events.push(Event::ReadAloud(event));
            }
        };
        tell(
            told.available != wanted.available,
            ReadAloudEvent::SetAvailable {
                on: wanted.available,
            },
        );
        tell(
            told.auto != wanted.auto,
            ReadAloudEvent::SetAuto { on: wanted.auto },
        );
        tell(
            told.voice_conversation != wanted.voice_conversation,
            ReadAloudEvent::SetVoiceConversation {
                open: wanted.voice_conversation,
            },
        );
        tell(
            told.dictating != wanted.dictating,
            ReadAloudEvent::SetDictating {
                on: wanted.dictating,
            },
        );
        tell(
            told.on_screen != wanted.on_screen,
            ReadAloudEvent::SetOnScreen {
                on: wanted.on_screen,
            },
        );
        *told = wanted;
        events
    }
}

/// Read aloud is on and this device has a voice, as each app decides it (contract section 2: the
/// desktop's own settings, the page's computer).
fn available_now() -> bool {
    crate::app::read_aloud::available()
}

/// The Voice (`""` is the system voice) and Speed on Settings > Chat.
fn voice_and_speed() -> (String, f64) {
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let object = settings.object();
    let voice = object
        .get("readAloudVoice")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let speed = object
        .get("readAloudRate")
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .filter(|speed| speed.is_finite() && *speed > 0.0)
        .unwrap_or(1.0);
    (voice, speed)
}

fn ended(id: String, interrupted: bool) -> Event {
    Event::ReadAloud(ReadAloudEvent::SpeechEnded { id, interrupted })
}

/// What chat `key` is told when a view attaches to it: everything it has not heard yet.
pub(super) fn attach(world: &mut World, key: &str) -> Vec<Event> {
    world.read_aloud.events_for(key)
}

/// The host's settings pass: the switch, its gate, or the voice changed.
pub(super) fn settings_pass(world: &mut World) {
    let now = available_now();
    let host = &mut world.read_aloud;
    if host.available == Some(now) {
        return;
    }
    host.available = Some(now);
    // Turning the feature off ends Auto-read too (contract section 9).
    if !now {
        host.auto = false;
    }
    host.stale = true;
}

/// One input from the app.
pub(super) fn receive(world: &mut World, input: ReadAloudInput) {
    let host = &mut world.read_aloud;
    match input {
        ReadAloudInput::OnScreen(key) => {
            host.on_screen = key;
            host.stale = true;
        }
        ReadAloudInput::VoiceConversation(open) => {
            host.voice_conversation = open;
            host.stale = true;
        }
        ReadAloudInput::Dictating(on) => {
            host.dictating = on;
            host.stale = true;
        }
        ReadAloudInput::SpeechEnded { token, interrupted } => {
            if host
                .speaking
                .as_ref()
                .is_none_or(|speaking| speaking.token != token)
            {
                // An utterance stopped or replaced on purpose: its chat already forgot it.
                return;
            }
            if let Some(speaking) = host.speaking.take() {
                world::drive(world, &speaking.key, vec![ended(speaking.id, interrupted)]);
            }
        }
        ReadAloudInput::Stop => {
            let Some(key) = host.speaking.as_ref().map(|speaking| speaking.key.clone()) else {
                return;
            };
            if let Ok(action) =
                serde_json::from_value::<UserAction>(json!({"type": "readAloudStop"}))
            {
                world::drive(world, &key, vec![Event::Action(Box::new(action))]);
            }
        }
    }
}

/// Performs one of the core's Read aloud effects for chat `key`.
pub(super) fn perform(world: &mut World, key: &str, effect: Effect) {
    let host = &mut world.read_aloud;
    match effect {
        Effect::Speak { id, segments, .. } => {
            // One speaker for the whole app: the chat that had it hears it was cut off.
            if let Some(previous) = host.speaking.take()
                && previous.key != key
            {
                host.owed.push((previous.key, ended(previous.id, true)));
            }
            let texts: Vec<String> = segments
                .into_iter()
                .map(|segment| segment.text)
                .filter(|text| !text.trim().is_empty())
                .collect();
            if texts.is_empty() {
                crate::app::read_aloud::stop();
                host.owed.push((key.to_string(), ended(id, false)));
                return;
            }
            host.next_token += 1;
            let token = host.next_token;
            host.speaking = Some(Speaking {
                key: key.to_string(),
                id,
                token,
            });
            let (voice, speed) = voice_and_speed();
            crate::app::read_aloud::speak(token, texts, &voice, speed);
        }
        Effect::StopSpeaking { id } => {
            if host
                .speaking
                .as_ref()
                .is_some_and(|speaking| speaking.key == key && speaking.id == id)
            {
                host.speaking = None;
                crate::app::read_aloud::stop();
            }
        }
        Effect::SetReadAloudAuto { on } => {
            host.auto = on;
            // This chat applied it already; every other chat hears it after the step.
            host.told.entry(key.to_string()).or_default().auto = on;
            host.stale = true;
            if on {
                crate::app::read_aloud::prime();
            }
        }
        _ => {}
    }
}

/// Delivers what chats owe each other and brings every chat up to date. Runs after every step.
pub(super) fn flush(world: &mut World) {
    for _ in 0..MAX_FLUSH_ROUNDS {
        let stale = std::mem::take(&mut world.read_aloud.stale);
        let owed = std::mem::take(&mut world.read_aloud.owed);
        if !stale && owed.is_empty() {
            return;
        }
        for (key, event) in owed {
            world::drive(world, &key, vec![event]);
        }
        if stale {
            sync_all(world);
        }
    }
}

fn sync_all(world: &mut World) {
    let mut keys = world.store.keys();
    // The chat leaving the screen hears it before the one arriving (contract section 5).
    let on_screen = world.read_aloud.on_screen.clone();
    keys.sort_by_key(|key| on_screen.as_ref() == Some(key));
    for key in keys {
        let events = world.read_aloud.events_for(&key);
        if !events.is_empty() {
            world::drive(world, &key, events);
        }
    }
}

/// Chat `key` is gone from the host: what it was saying stops.
pub(super) fn forget(world: &mut World, key: &str) {
    let host = &mut world.read_aloud;
    host.told.remove(key);
    host.owed.retain(|(owed, _)| owed != key);
    if host
        .speaking
        .as_ref()
        .is_some_and(|speaking| speaking.key == key)
    {
        host.speaking = None;
        crate::app::read_aloud::stop();
    }
}

/// What the app last sent, so a render that changes nothing sends nothing.
struct Sent {
    on_screen: Option<String>,
    voice_conversation: bool,
    dictating: bool,
}

static SENT: Mutex<Sent> = Mutex::new(Sent {
    on_screen: None,
    voice_conversation: false,
    dictating: false,
});

fn sent() -> std::sync::MutexGuard<'static, Sent> {
    SENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The chat on screen changed (`setOnScreen`): its retention key, or none.
pub(crate) fn read_aloud_on_screen(key: Option<String>) {
    {
        let mut sent = sent();
        if sent.on_screen == key {
            return;
        }
        sent.on_screen = key.clone();
    }
    super::worker::send_read_aloud(ReadAloudInput::OnScreen(key));
}

/// A voice agent conversation opened or ended (`setVoiceConversation`).
// The web build has no voice agent, Dictate or Read aloud hotkeys.
#[cfg_attr(ghostex_web, allow(dead_code))]
pub(crate) fn read_aloud_voice_conversation(open: bool) {
    {
        let mut sent = sent();
        if sent.voice_conversation == open {
            return;
        }
        sent.voice_conversation = open;
    }
    super::worker::send_read_aloud(ReadAloudInput::VoiceConversation(open));
}

/// Dictate's microphone started or stopped recording (`setDictating`).
// The web build has no voice agent, Dictate or Read aloud hotkeys.
#[cfg_attr(ghostex_web, allow(dead_code))]
pub(crate) fn read_aloud_dictating(on: bool) {
    {
        let mut sent = sent();
        if sent.dictating == on {
            return;
        }
        sent.dictating = on;
    }
    super::worker::send_read_aloud(ReadAloudInput::Dictating(on));
}

/// The speaker finished, or cut off, what it was handed under `token`. Any thread.
pub(crate) fn read_aloud_speech_ended(token: u64, interrupted: bool) {
    super::worker::send_read_aloud(ReadAloudInput::SpeechEnded { token, interrupted });
}

/// The Stop reading hotkey: whatever Read aloud is saying stops, in whichever chat.
// The web build has no voice agent, Dictate or Read aloud hotkeys.
#[cfg_attr(ghostex_web, allow(dead_code))]
pub(crate) fn read_aloud_stop() {
    super::worker::send_read_aloud(ReadAloudInput::Stop);
}
