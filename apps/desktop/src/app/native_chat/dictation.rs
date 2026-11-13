//! The composer's Dictate control: the microphone records to a file first and the recording is
//! transcribed after Stop (`voice_agent::Dictate`, gxserver's `/api/transcribeDictation`); the text
//! is typed into the draft, or into the question form box that had the keyboard when the recording
//! started (`aim_dictation`). While it records a small pill just above the microphone shows the
//! clock and a live waveform of the voice reaching the recording, then Transcribing, with an ✕ that
//! cancels (Escape cancels too); a failed transcription turns it into a calm card in the notices'
//! family (`tone_card.rs`) that says what went wrong, that the recording is safe, and Try again
//! with Open settings, Add key or Check key. While the engine can't transcribe the microphone is
//! greyed and a press shows why instead of recording. Hovering the microphone when a recording has
//! no text yet offers Transcribe previous.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "I want dictating and recording and all of this to show up just as a small pop-up right above the dictating button, basically. And escape cancels shouldn't be shown like this. It should be just an X button that appears there. When I hover, it says escape cancels [...] I imagine it to be like a pill above the dictating button that is centered above it." Then, seeing it in a row of its own above the chat box: "um no the pills are wrong spot please move them right above the button with small gap above it". Every Dictate state (recording with its clock, Transcribing, a failure with Transcribe again) is one capsule centred on the microphone, 6px above it, floating over the bottom of the text field: the user's explicit OK for that overlap (AGENTS.md "Native layout and hit-testing discipline"). The pill is a visible element that takes clicks only on itself; the rest of the text field stays usable. Its ✕'s tooltip says Escape cancels. Supersedes the full-width row with "Esc cancels" and the row-above-the-box spot (both earlier on 2026-10-20). The phone's `DictationBar.tsx` still draws the row until the user confirms the phone part.
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "When I'm transcribing, my caret basically isn't shown anymore unless I click on the chat again. please stop doing this. It should keep my caret shown in the chat so I know where the transcription that I'm doing is going to end up." Pressing the microphone or anything on the pill never takes focus from the chat box (the chat surface tracks focus for any press nothing claims, which blurred the field); the dictated text lands at the caret, which then sits right after it.
//!
//! CDXC:SessionChat 2026-10-08 DECISION: User asked for voice input in the chat box, to the left of the Terminal View button, on the desktop and the phone (GPUI Kit's speech input, #3333, on the desktop). Pressing the microphone starts listening and pressing it again stops; the recognized text is typed into the draft and never sent by itself.
//! CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/mobile/app/src/chat/native/composer/useDictation.ts` is the phone's control; the toolbar order is `toolbar.rs` `COMPOSER_CONTROLS` and the phone's `menus.ts`.
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: Escape, the Transcribing ✕, keeping cancelled recordings and Transcribe previous are the user's decision in `apps/desktop/src/app/voice_agent/dictation.rs`.

use super::appearance::ChatAppearance;
use super::cursor::ChatCursor as _;
use super::state::{NativeChatEvent, NativeChatView};
use super::tone_card::{ToneButton, ToneLook, tone_look};
use crate::app::voice_agent::{Dictate, DictateEvent, DictateFailure, DictatePhase};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FontFeatures,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, canvas, deferred,
    div, px,
};
use serde_json::json;
use std::time::Duration;
use std::{cell::Cell, rc::Rc};

/// The one switch for both Dictate buttons, the chat box's and the terminal bar's.
///
/// CDXC:VoiceMode 2026-10-20 DECISION: User: Dictate uses Grok dictation (xAI speech-to-text through gxserver, with the key kept on this computer) instead of the system speech recognizer. Later the same day the user said "Please make the voice agent feature and the dictating feature and all of those features related to voice mode basically there on Linux and on Windows. They're not Mac only, you know what I mean?" so Dictate is shown on macOS, Windows and Linux, superseding "Windows stays hidden" (and the 2026-10-09 decision that hid both Dictate buttons on every OS because the Windows system recognizer never returned words).
/// CDXC:SessionChat 2026-10-09 SEE-ALSO: the phone's switch is `DICTATION_ENABLED` in `apps/mobile/app/src/chat/native/composer/useDictation.ts`.
pub(crate) const DICTATION_ENABLED: bool = true;

/// Whether this build can dictate: the desktop app on macOS, Windows and Linux, where the app's own
/// audio captures the microphone (`voice_agent/audio.rs`). The web build has no microphone, so the
/// control is not drawn there.
pub(super) const DICTATION: bool = DICTATION_ENABLED
    && cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ));

/// Whether the chat box draws the Dictate button. The web build draws it only under its
/// `?dictatePreview=` switch, which shows the pill's states without a microphone.
pub(super) fn dictation_drawn() -> bool {
    #[cfg(ghostex_web)]
    {
        crate::app::voice_agent::dictation_preview()
    }
    #[cfg(not(ghostex_web))]
    {
        DICTATION
    }
}

/// The pill's height, unscaled: a 16px line, 4px above and below, a 1px edge.
const PILL_HEIGHT: f32 = 26.0;
/// Space between the pill's bottom edge and the microphone's top edge, unscaled.
const PILL_GAP: f32 = 6.0;
/// The failure card's width, unscaled (narrower when the chat box is).
const FAILURE_CARD_WIDTH: f32 = 340.0;
/// The waveform's bars, unscaled: as tall as the pill's line, thin with a thin gap.
const WAVE_HEIGHT: f32 = 16.0;
const WAVE_BAR: f32 = 2.0;
const WAVE_GAP: f32 = 2.0;
/// A silent bar's height, as a share of [`WAVE_HEIGHT`].
const WAVE_REST: f32 = 0.2;

/// What the failure card offers besides its ✕.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FailureActions {
    /// `(label, provider)`: Add key or Check key, opening that provider's key field.
    pub(super) key: Option<(&'static str, &'static str)>,
    /// A button opening Settings › Dictation at the engine (Open settings): Hex isn't ready, or
    /// Hex 3 has no build here and another engine must be picked.
    pub(super) models: Option<&'static str>,
    /// Transcribe again: whatever went wrong may be gone by now.
    pub(super) retry: bool,
    /// The card adds "Your recording is kept." (not when its audio is gone).
    pub(super) recording_kept: bool,
}

/// The buttons for gxserver's failure code (`server/src/voice_agent/transcribe.rs`). A key that
/// is missing or can't be read anymore (`noKey`) offers Add key, a refused one Check key, both on
/// the provider gxserver named (xAI when it named none). Hex that isn't installed, running or
/// ready, and a computer Hex 3 has no build for, offer Open settings (Settings › Dictation at the
/// engine, as on the phone). A recording whose audio is gone has nothing left to try.
///
/// CDXC:VoiceMode 2026-11-08 DECISION: Dictate's engine is the user's explicit choice (Q1259,
/// Q1272) and never falls back by itself, so Hex that isn't ready is a failure the card explains,
/// with the way to fix it one press away (Settings › Dictation, where Install Hex runs), and
/// Transcribe again for once it is there. Supersedes the 2026-11-05 Download a model for the
/// retired local engine (CDXC:Dictate 2026-11-08 in server/src/voice_agent/speech/retired.rs).
pub(super) fn failure_actions(failure: &DictateFailure) -> FailureActions {
    let provider = match failure.provider.as_deref() {
        Some("openai") => "openai",
        _ => "xai",
    };
    let (key, models, retry, recording_kept) = match failure.code.as_str() {
        "noKey" => (Some(("Add key", provider)), None, true, true),
        "badKey" => (Some(("Check key", provider)), None, true, true),
        "hexUnsupported" | "hexNotInstalled" | "hexNotRunning" | "hexNoModels"
        | "hexModelNotReady" | "hexVersion" | "keyUnreadable" | "notReady" => {
            (None, Some("Open settings"), true, true)
        }
        "audioMissing" => (None, None, false, false),
        _ => (None, None, true, true),
    };
    FailureActions {
        key,
        models,
        retry,
        recording_kept,
    }
}

/// Settings › Dictation, scrolled to its engine.
pub(crate) fn open_voice_models() -> serde_json::Value {
    json!({
        "type": "open",
        "modal": "settings",
        "initialTab": "dictation",
        "initialSection": "dictate",
    })
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "hex" => "Hex",
        _ => "xAI",
    }
}

/// Settings › Dictation with `provider`'s key field (under the engine) focused.
pub(super) fn open_voice_agent_key(provider: &str) -> serde_json::Value {
    json!({
        "type": "open",
        "modal": "settings",
        "initialTab": "dictation",
        "initialSection": "dictate",
        "initialVoiceAgentKey": provider,
    })
}

/// Which Dictate card shows (`render_dictation_failure`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CardKind {
    Failed,
    Unavailable,
    Notice,
}

/// A notice card's title (the phone's `noticeTitle` in `DictationBar.tsx`).
pub(super) fn notice_title(code: &str) -> &'static str {
    match code {
        "micUnavailable" => "Can't use the microphone",
        "nothingHeard" => "No words heard",
        _ => "Nothing to transcribe",
    }
}

/// gxserver's sentence for a card that has Try again: without its trailing "then transcribe
/// again" (or a closing "Transcribe again."), which the button says (Q2915). Older Ghostex
/// versions keep reading the full sentences from gxserver.
pub(crate) fn card_sentence(message: &str) -> String {
    let message = message.trim();
    for tail in [
        ", then transcribe again.",
        " Then transcribe again.",
        ", then try again.",
    ] {
        if let Some(head) = message.strip_suffix(tail) {
            return format!("{head}.");
        }
    }
    match message.strip_suffix(" Transcribe again.") {
        Some(head) if !head.is_empty() => head.to_string(),
        _ => message.to_string(),
    }
}

/// A card title for why the engine can't transcribe yet, in 2 to 6 words (the phone's
/// `unavailableTitle` in `DictationBar.tsx`).
pub(super) fn unavailable_title(code: &str) -> &'static str {
    match code {
        "hexNotInstalled" => "Hex isn't installed",
        "hexNotRunning" => "Hex isn't running",
        "hexNoModels" | "hexModelNotReady" => "Hex has no model ready",
        "hexUnsupported" => "Hex can't run here",
        "noKey" => "No API key added",
        "keyUnreadable" => "The API key can't be read",
        _ => "Dictation isn't ready",
    }
}

/// Whether gxserver last said Dictate's engine can't transcribe (the microphone shows greyed).
/// The first call asks gxserver, so the microphone is greyed soon after a chat first draws it; a
/// hover or a press asks again.
pub(super) fn dictation_unready(cx: &App) -> bool {
    static ASKED: std::sync::Once = std::sync::Once::new();
    ASKED.call_once(|| {
        cx.background_executor()
            .spawn(async { crate::app::voice_agent::refresh_engine() })
            .detach();
    });
    crate::app::voice_agent::dictate_engine().is_some_and(|engine| engine["ready"] == false)
}

/// The recording pill's waveform: `levels` (0 to 1, oldest first) as thin bars that grow with the
/// voice, the newest on the right.
fn waveform<'a>(levels: impl Iterator<Item = &'a f32>, p: &ChatAppearance) -> gpui::Div {
    let s = p.scale;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(WAVE_GAP * s))
        .h(px(WAVE_HEIGHT * s))
        .children(levels.map(|&level| {
            let level = level.clamp(0.0, 1.0);
            div()
                .w(px(WAVE_BAR * s))
                .h(px(WAVE_HEIGHT * s * level.max(WAVE_REST)))
                .rounded_full()
                .bg(p.foreground.opacity(0.3 + level * 0.55))
        }))
}

/// Where the pill sits inside the chat box, from the box's and the microphone's last paint.
struct Placement {
    /// The box's border box (`composer_bounds`, its top may reach up over stacked panels).
    card: Rc<Cell<Bounds<Pixels>>>,
    microphone: Rc<Cell<Bounds<Pixels>>>,
    pill: Rc<Cell<Bounds<Pixels>>>,
    microphone_shown: bool,
    scale: f32,
}

impl Placement {
    /// `(left, bottom)` inside the box's padding box: centred on the microphone and
    /// [`PILL_GAP`] above it, kept inside the box when it is too wide to centre. Flush right
    /// above the toolbar before the first paint, or while the microphone is folded into More
    /// actions.
    fn spot(&self) -> (Pixels, Pixels) {
        let card = self.card.get();
        let microphone = self.microphone.get();
        let width = self.pill.get().size.width;
        // Absolute children are placed in the padding box, one border width inside the card.
        let inner_left = card.left() + px(1.0);
        let inner_bottom = card.bottom() - px(1.0);
        let room = (card.size.width - px(2.0) - width).max(px(0.0));
        let gap = px(PILL_GAP * self.scale);
        if self.microphone_shown && microphone.size.width > px(0.0) && card.size.width > px(0.0) {
            let left = (microphone.center().x - inner_left - width / 2.0).clamp(px(0.0), room);
            let bottom = (inner_bottom - microphone.top() + gap).max(px(0.0));
            (left, bottom)
        } else {
            (room, px(40.0 * self.scale))
        }
    }
}

/// How long the pointer rests on the microphone before Transcribe previous is offered.
const HOVER_DELAY: Duration = Duration::from_millis(350);

/// The app toast for a dictation problem no composer row shows (the microphone refused, nothing
/// waiting for Transcribe previous), from the composer or the terminal bar.
///
/// CDXC:SessionChat 2026-10-09 WHY: User: "pls fix dictate feature it doesnt work at all (at least on windows)". A `sessionChatHostAction` "toast" is dropped by the app, so pressing the microphone did nothing visible. This raw `{type: "toast"}` message reaches the app's toast window from both buttons.
pub(crate) fn dictation_toast(title: &str, message: &str) -> serde_json::Value {
    json!({
        "type": "toast",
        "level": "error",
        "title": title,
        "description": message,
        "durationMs": 12_000,
    })
}

/// The composer's Dictate state, made the first time the reader uses it.
pub(crate) struct DictationControl {
    dictate: Entity<Dictate>,
    _subscription: Subscription,
    /// The pointer is on the microphone; the hover menu opens after [`HOVER_DELAY`].
    hover: Option<Task<()>>,
    /// The pill at its last paint, and the spot it was last placed at (`Placement`).
    pill: Rc<Cell<Bounds<Pixels>>>,
    placed: Rc<Cell<(Pixels, Pixels)>>,
    /// The question form box (its `form_inputs` key) the running dictation types into; `None` is
    /// the chat box (`aim_dictation`).
    target: Option<String>,
    /// The target's text and selection when the run was aimed (`aim_dictation`): its words go
    /// there while the text is the same when they arrive.
    anchor: Option<(String, Option<std::ops::Range<usize>>)>,
}

pub(crate) type Dictation = Option<DictationControl>;

impl NativeChatView {
    fn dictate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Dictate> {
        if let Some(control) = &self.dictation {
            return control.dictate.clone();
        }
        let dictate = cx.new(|_| Dictate::new());
        let subscription = cx.subscribe_in(&dictate, window, |this, _, event, _, cx| match event {
            DictateEvent::Text(text) => this.insert_dictation(text, cx),
            DictateEvent::Failure { title, message } => {
                // A failed transcription shows in the pill over the microphone with Transcribe
                // again; only what the pill does not show becomes a toast.
                let in_row = this.dictation.as_ref().is_some_and(|control| {
                    matches!(
                        control.dictate.read(cx).phase(),
                        DictatePhase::Failed { .. }
                            | DictatePhase::Unavailable { .. }
                            | DictatePhase::Notice { .. }
                    )
                });
                if !in_row {
                    cx.emit(NativeChatEvent::Host(dictation_toast(title, message)));
                }
                cx.notify();
            }
        });
        cx.observe(&dictate, |_, _, cx| cx.notify()).detach();
        self.dictation = Some(DictationControl {
            dictate: dictate.clone(),
            _subscription: subscription,
            hover: None,
            pill: Default::default(),
            placed: Rc::new(Cell::new((px(0.0), px(0.0)))),
            target: None,
            anchor: None,
        });
        dictate
    }

    /// Sets where the words of the run `dictate` is about to begin are typed: `target`, a question
    /// form box, or the chat box. A new recording aims at the box that has the keyboard
    /// (`focused_form_box`); a recording transcribed later (Transcribe again, Transcribe previous)
    /// at the chat box. Nothing changes while one records or transcribes, so a box focused
    /// meanwhile does not move it.
    ///
    /// CDXC:Dictate 2026-11-10 WHY: Dictate typed into the chat box even when the caret was in a question form's note, text answer or "Anything else?" box, so the words had to be cut from the draft and pasted into the box. The box that has the keyboard when the recording starts gets them, at its caret with the chat box's spacing; if its form was sent or answered meanwhile they go to the chat box, so a dictation is never lost. This widens "typed into the draft" of the SessionChat 2026-10-08 DECISION and follows the VoiceMode 2026-10-20 one on the caret showing where the words will land (both in this file's head).
    /// CDXC:Dictate 2026-11-10 SEE-ALSO: the chat core writes the words and decides whether the box still takes them (`insert` in packages/gx-chat-core/src/questions/dictation.rs); the phone's rule is `apps/mobile/app/src/chat/native/composer/dictationTarget.ts`; the box shows it is the target in `question_form.rs` `note_box`.
    ///
    /// CDXC:Dictate 2026-11-13 DECISION: User: "when we dictate with the caret in the middle or some text selected, we should enter the text to replace what's selected or insert it at the caret." The target's selection (or caret) is kept here with its text when the recording starts, and the words go over it while the text is the same when they arrive: moving the caret while it records or transcribes does not move them, typing in the box does (they then go where the caret is, as the text they were aimed into is gone). Same rule on the phone (`dictationAnchor` in apps/mobile/app/src/chat/rust/host.ts, `placeIn` in dictationTarget.ts).
    fn aim_dictation(&mut self, dictate: &Entity<Dictate>, target: Option<String>, cx: &App) {
        let dictate = dictate.read(cx);
        if dictate.recording() || dictate.transcribing() {
            return;
        }
        let anchor = match target.as_deref() {
            Some(key) => self.form_inputs.get(key).map(|entry| {
                let input = entry.input.read(cx);
                (input.value().to_string(), Some(input.selected_range()))
            }),
            None => Some((self.draft.clone(), self.composer_selection(cx))),
        };
        if let Some(control) = self.dictation.as_mut() {
            control.target = target;
            control.anchor = anchor;
        }
    }

    /// Where the words go in `text` (UTF-16 offsets): the aimed selection while `text` is what it
    /// was when the run was aimed, else `now`.
    fn dictation_place(
        anchor: &Option<(String, Option<std::ops::Range<usize>>)>,
        text: &str,
        now: Option<std::ops::Range<usize>>,
    ) -> (usize, usize) {
        match anchor {
            Some((aimed, range)) if aimed == text => utf16_selection(text, range.clone()),
            _ => utf16_selection(text, now),
        }
    }

    /// The form box the running dictation will type into, while it records or transcribes.
    pub(super) fn dictation_target(&self, cx: &App) -> Option<&str> {
        let control = self.dictation.as_ref()?;
        let dictate = control.dictate.read(cx);
        (dictate.recording() || dictate.transcribing())
            .then_some(control.target.as_deref())
            .flatten()
    }

    /// Whether the microphone is recording (the Stop glyph).
    pub(crate) fn dictating(&self, cx: &App) -> bool {
        self.dictation
            .as_ref()
            .is_some_and(|control| control.dictate.read(cx).recording())
    }

    /// Start recording, or stop and transcribe.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dictate = self.dictate(window, cx);
        self.aim_dictation(&dictate, self.focused_form_box(window, cx), cx);
        dictate.update(cx, |dictate, cx| dictate.toggle(cx));
        cx.notify();
    }

    /// Whether the Dictate hotkey has a microphone to press here: false where the chat box draws
    /// none, so the chord keeps its ordinary meaning.
    pub(crate) fn dictation_hotkey_available(&self) -> bool {
        self.composer_control_available("dictate")
    }

    /// The Dictate hotkey went down: start, or stop and transcribe (`voice_agent::dictate_hotkey`
    /// decides which).
    pub(crate) fn dictation_hotkey_step(
        &mut self,
        start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dictate = self.dictate(window, cx);
        if start {
            self.aim_dictation(&dictate, self.focused_form_box(window, cx), cx);
        }
        dictate.update(cx, |dictate, cx| {
            if start {
                dictate.start(cx);
            } else {
                dictate.stop_recording(cx);
            }
        });
        cx.notify();
    }

    /// Escape: cancels a recording or a transcription (the recording is kept). Answers whether
    /// there was one.
    pub(super) fn cancel_dictation(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(control) = &self.dictation else {
            return false;
        };
        let dictate = control.dictate.clone();
        let active = {
            let dictate = dictate.read(cx);
            dictate.recording() || dictate.transcribing()
        };
        if active {
            dictate.update(cx, |dictate, cx| dictate.cancel(cx));
        }
        active
    }

    pub(super) fn transcribe_previous_dictation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dictate = self.dictate(window, cx);
        self.aim_dictation(&dictate, None, cx);
        dictate.update(cx, |dictate, cx| dictate.transcribe_previous(cx));
    }

    /// Settings › Dictation, scrolled to Dictation history.
    pub(super) fn open_dictation_history(&mut self, cx: &mut Context<Self>) {
        cx.emit(NativeChatEvent::Host(json!({
            "type":"open","modal":"settings","initialTab":"dictation","initialSection":"dictations",
        })));
    }

    /// The microphone's menu: Transcribe previous when a recording has no text yet, and the
    /// history. Opened by resting the pointer on the microphone (only when there is something to
    /// transcribe) or by right-clicking it.
    pub(super) fn show_dictation_menu(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut rows = Vec::new();
        if let Some((_, duration_ms, created_at)) = crate::app::voice_agent::previous_recording() {
            let ago = chrono::Utc::now()
                .timestamp_millis()
                .saturating_sub(created_at)
                / 60_000;
            let when = match ago {
                0 => "just now".to_string(),
                1 => "1 min ago".to_string(),
                minutes if minutes < 60 => format!("{minutes} min ago"),
                minutes => format!("{} h ago", minutes / 60),
            };
            rows.push(json!({
                "label": format!(
                    "Transcribe previous ({}, {when})",
                    crate::app::voice_agent::dictation_clock(Duration::from_millis(duration_ms))
                ),
                "command": {"type":"composerHost","action":"transcribePreviousDictation"},
            }));
        }
        rows.push(json!({
            "label": "Dictation history…",
            "command": {"type":"composerHost","action":"dictationHistory"},
        }));
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        let trigger =
            super::toolbar::button_trigger(self.composer_button_bounds.dictate.get(), position);
        self.show_chat_menu(rows, trigger, 260.0, window, cx);
    }

    /// The microphone was pressed: the pointer on it is no longer one that came to rest there, so
    /// the hover menu stays shut until it has left the button and come back (`hover_dictation`).
    ///
    /// CDXC:Dictate 2026-11-10 WHY: With a recording waiting for Transcribe previous, the hover menu that was armed when the pointer reached the microphone was not called off by a press, so a click made within its delay started a recording and the menu then opened over it. That menu is a window of its own that takes the keyboard from the main window, where the caret stops being drawn, and a click that closes it leaves the keyboard where the click landed: the box being dictated into could be left without it (user: "bazen fokus orada değil başka bir şeyde oldu"). A press also makes GPUI report the button as not hovered, so the pointer can count as entering it again while it never left, possibly once the words are in. So a press calls off a waiting menu and none is armed until the pointer has really left the button, which that report cannot tell from a press: the button's own bounds and the window's exit say so (composer.rs). It is the same for the chat box, as the 2026-10-20 decision at the top of this file asks ("Pressing the microphone … never takes focus from the chat box").
    pub(super) fn press_dictation(&mut self) {
        self.dictate_pressed.set(true);
        if let Some(control) = self.dictation.as_mut() {
            control.hover = None;
        }
    }

    /// The pointer entered or left the microphone.
    pub(super) fn hover_dictation(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !dictation_drawn() {
            return;
        }
        // Still the pointer that pressed the microphone (`press_dictation`).
        if hovered && self.dictate_pressed.get() {
            return;
        }
        let dictate = self.dictate(window, cx);
        let idle = matches!(
            dictate.read(cx).phase(),
            DictatePhase::Idle
                | DictatePhase::Failed { .. }
                | DictatePhase::Unavailable { .. }
                | DictatePhase::Notice { .. }
        );
        if !hovered || !idle {
            if let Some(control) = self.dictation.as_mut() {
                control.hover = None;
            }
            return;
        }
        // The hover asks gxserver what is waiting, so a recording the phone or the terminal bar
        // left is offered too.
        let refresh = cx.background_executor().spawn(async {
            crate::app::voice_agent::refresh_previous();
            crate::app::voice_agent::refresh_engine();
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            refresh.await;
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let still_hovered = this
                    .dictation
                    .as_ref()
                    .is_some_and(|control| control.hover.is_some());
                if still_hovered && crate::app::voice_agent::previous_recording().is_some() {
                    let position = window.mouse_position();
                    this.show_dictation_menu(position, window, cx);
                }
                if let Some(control) = this.dictation.as_mut() {
                    control.hover = None;
                }
            });
        });
        if let Some(control) = self.dictation.as_mut() {
            control.hover = Some(task);
        }
    }

    /// Dictate's pill: a capsule centred over the microphone while it records or transcribes, and
    /// the failure card when a transcription failed, floating [`PILL_GAP`] above the button over the
    /// bottom of the chat box's text field (an absolute child of the box, drawn after its content).
    /// It takes clicks only on itself.
    pub(super) fn render_dictation_pill(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = self.dictation.as_ref()?;
        let phase = control.dictate.read(cx).phase().clone();
        let s = p.scale;
        let close = |failed: bool, cx: &mut Context<Self>| {
            div()
                .id("dictation-cancel")
                .role(gpui::Role::Button)
                .aria_label(if failed { "Dismiss" } else { "Cancel" })
                .chat_cursor_pointer()
                .flex_shrink_0()
                .size(px(18.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .hover(|style| style.bg(p.foreground.opacity(0.08)))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(if failed {
                        "Dismiss (the recording is kept)"
                    } else {
                        "Escape cancels"
                    })
                    .build(window, cx)
                })
                .child(
                    gpui::svg()
                        .path("titlebar/x.svg")
                        .size(px(11.0 * s))
                        .text_color(p.card_muted),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(control) = this.dictation.as_ref() {
                        let dictate = control.dictate.clone();
                        dictate.update(cx, |dictate, cx| dictate.cancel(cx));
                    }
                    cx.notify();
                }))
        };
        let card_width = || {
            // Never wider than the chat box it floats in. Its width is known before it paints,
            // so the first frame already places it.
            let room = self.composer_bounds.get().size.width - px(2.0);
            let mut width = px(FAILURE_CARD_WIDTH * s);
            if room > px(0.0) {
                width = width.min(room);
            }
            let mut measured = control.pill.get();
            measured.size.width = width;
            control.pill.set(measured);
            width
        };
        let is_card = matches!(
            phase,
            DictatePhase::Failed { .. }
                | DictatePhase::Unavailable { .. }
                | DictatePhase::Notice { .. }
        );
        let floating = match &phase {
            DictatePhase::Idle => return None,
            DictatePhase::Failed { failure, .. } => {
                let width = card_width();
                div().w(width).child(self.render_dictation_failure(
                    failure,
                    CardKind::Failed,
                    p,
                    cx,
                ))
            }
            DictatePhase::Unavailable { failure } => {
                let width = card_width();
                div().w(width).child(self.render_dictation_failure(
                    failure,
                    CardKind::Unavailable,
                    p,
                    cx,
                ))
            }
            DictatePhase::Notice { failure } => {
                let width = card_width();
                div().w(width).child(self.render_dictation_failure(
                    failure,
                    CardKind::Notice,
                    p,
                    cx,
                ))
            }
            _ => {
                let recording = matches!(phase, DictatePhase::Recording);
                let label = match &phase {
                    DictatePhase::Starting => "Starting…".to_string(),
                    DictatePhase::Recording => {
                        crate::app::voice_agent::dictation_clock(control.dictate.read(cx).elapsed())
                    }
                    _ => "Transcribing".to_string(),
                };
                if recording {
                    // CDXC:Dictate 2026-11-13 DECISION: The recording pill: ✕ cancel on the left, the clock and the waveform, and "Can't hear you" after 3 seconds of near-silence (Q2919). User (Q2918 "pill"): no ✓ finish in it; the microphone's Stop finishes.
                    let (levels, quiet) = {
                        let dictate = control.dictate.read(cx);
                        (
                            dictate.levels().iter().copied().collect::<Vec<f32>>(),
                            dictate.cant_hear(),
                        )
                    };
                    div()
                        .h(px(PILL_HEIGHT * s))
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        // The ✕'s 11px mark, centred in its 18px button 4px in, starts 7.5px in;
                        // the waveform or "Can't hear you" ends as far from the other end.
                        .pl(px(4.0 * s))
                        .pr(px(8.0 * s))
                        .rounded_full()
                        .whitespace_nowrap()
                        .child(close(false, cx))
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(px(7.0 * s))
                                .rounded_full()
                                .bg(gpui::rgb(0xef4444)),
                        )
                        .child(
                            div()
                                .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                                .text_color(p.foreground)
                                .child(label),
                        )
                        .child(waveform(levels.iter(), p).ml(px(2.0 * s)))
                        .when(quiet, |this| {
                            this.child(
                                div()
                                    .text_color(
                                        super::tone_card::tone_look("warning", p)
                                            .hue
                                            .unwrap_or(p.muted),
                                    )
                                    .child("Can't hear you"),
                            )
                        })
                } else {
                    div()
                        .h(px(PILL_HEIGHT * s))
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        // The text starts 9px in; the ✕'s 11px mark, centred in its 18px button,
                        // ends 9px in, so both ends are inset alike.
                        .pl(px(9.0 * s))
                        .pr(px(5.5 * s))
                        .rounded_full()
                        .whitespace_nowrap()
                        .child(div().text_color(p.foreground).child(label))
                        .child(close(false, cx))
                }
            }
        };
        let microphone_shown = self.dictation_button_shown();
        let placement = Placement {
            card: self.composer_bounds.clone(),
            microphone: self.composer_button_bounds.dictate.clone(),
            pill: control.pill.clone(),
            microphone_shown,
            scale: s,
        };
        let (left, bottom) = placement.spot();
        control.placed.set((left, bottom));
        let placed = control.placed.clone();
        let measured_pill = control.pill.clone();
        let pill = floating
            .id("dictation-pill")
            .role(gpui::Role::Status)
            .absolute()
            .left(left)
            .bottom(bottom)
            .occlude()
            // A press on the pill (its ✕, Transcribe again, or the pill itself) must not move focus
            // to the chat surface, which tracks focus for presses nothing claimed: the chat box
            // keeps its caret where the dictated text will land.
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.prevent_default()
            })
            // A card draws its own shell (`tone_card`); the pill is a capsule.
            .when(!is_card, |this| {
                this.border_1()
                    .border_color(p.input_border)
                    .bg(p.card_panel)
                    .text_size(px(12.0 * s))
                    .line_height(px(16.0 * s))
            })
            .child(
                // The pill's border box; once the box and the microphone have painted this frame,
                // a spot that moved (a resize, a growing draft, a wider label) is drawn again.
                canvas(
                    move |bounds, window, _| {
                        measured_pill.set(bounds);
                        if placement.spot() != placed.get() {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset(px(-1.0)),
            );
        // Painted after the box: the card reaches above the box's top edge, and the box draws its
        // border after its children.
        Some(deferred(pill).with_priority(1).into_any_element())
    }

    /// The Dictate card, compact: a mark and a title, gxserver's sentence (shortened where a
    /// button says it, `card_sentence`) with what became of the recording, and the buttons that
    /// help, Try again filled. `Failed` is a transcription that failed ("Couldn't transcribe",
    /// "Your recording is safe.", `failure_actions`); `Unavailable` the card a press shows while
    /// the engine can't transcribe ("Nothing was recorded.", the way to fix it); `Notice` what no
    /// button retries (a microphone that couldn't start, no words heard, nothing waiting). The
    /// provider's own words, when it gave any, are the sentence's tooltip. Its ✕ sits on the
    /// card's corner while the pointer is over it (`card_corner_close`).
    ///
    /// CDXC:VoiceMode 2026-10-30 DECISION: User picked Q779 option B, "a two-line card with the full sentence", for every failure gxserver reports, each in plain words, instead of a pill that only said "Transcription failed" with the reason in a tooltip (Sam on Discord, no xAI key set). Q780 option A: its Add key / Check key button opens the page with that key's field focused (Settings > Voice since 2026-11-05). Supersedes the failure part of the 2026-10-20 one-line pill above; recording and transcribing keep the pill.
    /// CDXC:Dictate 2026-11-13 DECISION: User, of the phone's failure: "the error you're showing is so ugly. Make it look nicer": a calm card, what happened in plain words, that the recording is safe, and one or two buttons (Try again, Open settings), on the desktop and the phone alike (`DictationBar.tsx`). Then (Q2915) the sentence drops its trailing "then transcribe again" where Try again shows, and (Q2914, "please we should never ever lose the user's transcript") a card whose recording has no words yet says where it is kept: Settings > Dictation > Past recordings.
    fn render_dictation_failure(
        &self,
        failure: &DictateFailure,
        kind: CardKind,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let actions = match kind {
            CardKind::Notice => FailureActions {
                key: None,
                models: (failure.code == "nothingHeard").then_some("Past recordings"),
                retry: false,
                recording_kept: false,
            },
            _ => failure_actions(failure),
        };
        let detail = failure.detail.clone().map(|detail| {
            match failure.provider.as_deref().map(provider_label) {
                Some(label) => format!("{label} said: {detail}"),
                None => detail,
            }
        });
        let (icon, tone) = match (kind, failure.code.as_str()) {
            (CardKind::Unavailable, _) | (CardKind::Notice, "micUnavailable") => {
                ("titlebar/microphone-off.svg", "warning")
            }
            (CardKind::Notice, _) => ("titlebar/microphone.svg", "terminal"),
            _ => ("titlebar/microphone.svg", "warning"),
        };
        let look = ToneLook {
            icon: icon.to_string(),
            ..tone_look(tone, p)
        };
        let title = match kind {
            CardKind::Unavailable => unavailable_title(&failure.code).to_string(),
            CardKind::Notice => notice_title(&failure.code).to_string(),
            CardKind::Failed => "Couldn't transcribe".to_string(),
        };
        let retry = actions.retry && kind == CardKind::Failed;
        let message = if retry {
            card_sentence(&failure.message)
        } else {
            failure.message.clone()
        };
        let sentence = (
            match kind {
                CardKind::Unavailable => format!("{message} Nothing was recorded."),
                CardKind::Failed if actions.recording_kept => {
                    format!("{message} Your recording is safe in Past recordings.")
                }
                _ => message,
            },
            detail,
        );
        let mut footer = Vec::new();
        let settings_label = match kind {
            CardKind::Unavailable if actions.key.is_none() => Some("Open settings"),
            _ => actions.models.filter(|_| actions.key.is_none()),
        };
        if let Some(label) = settings_label {
            let kind = if retry {
                ToneButton::Secondary
            } else {
                ToneButton::Primary
            };
            let history = label == "Past recordings";
            footer.push(
                self.tone_button("dictation-models", label, None, kind, None, false, p)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if history {
                            this.open_dictation_history(cx);
                        } else {
                            cx.emit(NativeChatEvent::Host(open_voice_models()));
                        }
                    }))
                    .into_any_element(),
            );
        }
        if let Some((label, provider)) = actions.key {
            let kind = if retry {
                ToneButton::Secondary
            } else {
                ToneButton::Primary
            };
            footer.push(
                self.tone_button("dictation-key", label, None, kind, None, false, p)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(NativeChatEvent::Host(open_voice_agent_key(provider)));
                    }))
                    .into_any_element(),
            );
        }
        if retry {
            footer.push(
                self.tone_button(
                    "dictation-retry",
                    "Try again",
                    None,
                    ToneButton::Primary,
                    None,
                    false,
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(control) = this.dictation.as_ref() {
                        let dictate = control.dictate.clone();
                        this.aim_dictation(&dictate, None, cx);
                        dictate.update(cx, |dictate, cx| dictate.retry(cx));
                    }
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        let close_label = if kind == CardKind::Failed {
            "Dismiss (the recording is kept)"
        } else {
            "Dismiss"
        };
        let (sentence_text, sentence_detail) = sentence;
        let hue = look.hue.unwrap_or(p.muted);
        div()
            .id("dictation-card")
            .group("dictation-card")
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .px(px(12.0 * s))
            .pt(px(9.0 * s))
            .pb(px(10.0 * s))
            .rounded(px(14.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .child(
                        gpui::svg()
                            .path(look.icon.clone())
                            .flex_shrink_0()
                            .size(px(14.0 * s))
                            .text_color(hue),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.0 * s))
                            .line_height(px(18.0 * s))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(p.foreground)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .id("dictation-reason")
                    .text_size(px(12.5 * s))
                    .line_height(px(17.0 * s))
                    .text_color(p.card_muted)
                    .child(sentence_text)
                    .when_some(sentence_detail, |this, detail| {
                        this.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
                        })
                    }),
            )
            .when(!footer.is_empty(), |card| {
                card.child(
                    div()
                        .mt(px(4.0 * s))
                        .flex()
                        .justify_end()
                        .gap(px(6.0 * s))
                        .children(footer),
                )
            })
            .child(self.card_corner_close(close_label, p, cx))
            .into_any_element()
    }

    /// The Dictate card's ✕: the toast's round button centred on the card's top corner (the
    /// left on macOS unless Close buttons on the right is on, like every dialog), shown while the
    /// pointer is over the card. It dismisses the card; a recording stays kept.
    ///
    /// CDXC:Dictate 2026-11-13 DECISION: User (Q2913, Other): "when I hover, it's shown on the top right or top left corner, just like we do for Settings". Supersedes the filled ✕ inside the card's header.
    /// SEE-ALSO: window/round_close_button.rs (`close_buttons_on_left`), native_chat/coordinator_board.rs (the board card's remove button, the same look).
    fn card_corner_close(
        &self,
        label: &'static str,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let size = crate::app::window::round_close_button::ROUND_CLOSE_BUTTON_SIZE * s;
        let outset = px(-size / 2.0 + 2.0 * s);
        let light = p.light;
        div()
            .id("dictation-cancel")
            .role(gpui::Role::Button)
            .aria_label(label)
            .absolute()
            .top(outset)
            .map(|button| {
                if crate::app::window::round_close_button::close_buttons_on_left() {
                    button.left(outset)
                } else {
                    button.right(outset)
                }
            })
            .size(px(size))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(px(s))
            .border_color(
                crate::app::window::toast::gpui_app_toast_close_button_border_color(light),
            )
            .bg(crate::app::window::toast::gpui_app_toast_close_button_color(light))
            .chat_cursor_pointer()
            .opacity(0.0)
            .group_hover("dictation-card", |style| style.opacity(1.0))
            .hover(move |style| {
                style
                    .opacity(1.0)
                    .bg(crate::app::window::toast::gpui_app_toast_close_button_hover_color(light))
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(crate::app::window::round_close_button::round_close_x_icon(
                s,
                crate::app::window::toast::gpui_app_toast_close_button_icon_color(light),
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(control) = this.dictation.as_ref() {
                    let dictate = control.dictate.clone();
                    dictate.update(cx, |dictate, cx| dictate.cancel(cx));
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// Whether the toolbar draws the microphone this frame (`toolbar.rs` `render_toolbar`'s rule);
    /// otherwise its last painted bounds are stale.
    fn dictation_button_shown(&self) -> bool {
        !self.composer_animation.shown_collapsed() && self.composer_control_available("dictate")
    }

    /// Hands a finished dictation to the chat core (`insertDictation`, `questions/dictation.rs`
    /// in gx-chat-core), which types it at the caret (over a selection), spaced from the
    /// characters on either side of it while Insert with spacing is on (the VoiceMode 2026-11-05
    /// DECISION on its `spaced_at_caret`): into the form box the dictation was aimed at
    /// (`aim_dictation`) while that box still takes text, into the chat box otherwise. The view
    /// says which box and where each caret is; the core answers with `dictation_inserted`.
    fn insert_dictation(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        let (target, anchor) = self
            .dictation
            .as_mut()
            .map(|control| (control.target.take(), control.anchor.take()))
            .unwrap_or_default();
        cx.notify();
        if text.is_empty() {
            return;
        }
        let chat_anchor = if target.is_none() {
            anchor.clone()
        } else {
            None
        };
        let (start, end) =
            Self::dictation_place(&chat_anchor, &self.draft, self.composer_selection(cx));
        let mut action = json!({
            "type": "insertDictation",
            "text": text,
            "spacing": dictate_insert_spacing(),
            "draft": self.draft,
            "start": start,
            "end": end,
        });
        if let Some((key, (form, slot))) = target
            .as_deref()
            .and_then(|key| Some((key, key.rsplit_once(':')?)))
        {
            action["form"] = json!(form);
            action["slot"] = slot
                .parse::<usize>()
                .map_or_else(|_| json!(slot), |index| json!(index));
            if let Some(entry) = self.form_inputs.get(key) {
                let input = entry.input.read(cx);
                let shown = input.value().to_string();
                let (start, end) =
                    Self::dictation_place(&anchor, &shown, Some(input.selected_range()));
                action["shown"] = json!(shown);
                action["shownStart"] = json!(start);
                action["shownEnd"] = json!(end);
            }
        }
        self.invoke(action, cx);
    }

    /// The chat box's selection in `self.draft`, while the field shows that text.
    fn composer_selection(&self, cx: &App) -> Option<std::ops::Range<usize>> {
        let input = self.input.as_ref()?.read(cx);
        (input.value().to_string() == self.draft).then(|| input.selected_range())
    }

    /// The core's `dictationInserted`: Dictate's words are in a draft, the chat box's or a form
    /// box's, and `content` is that draft's text now. The chat box shows it at once, or asks again
    /// when the reader typed in it meanwhile (the chat box's draft is this view's, not the
    /// core's); a form box at its next draw, where its window is at hand (`show_dictated`), in the
    /// transcript and in the Resources modal's copy alike.
    pub(super) fn dictation_inserted(
        &mut self,
        params: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(dictated) = Dictated::of(params) else {
            return;
        };
        let Some(form) = params["form"].as_str() else {
            if dictated.supersedes.contains(&self.draft) {
                self.replace_draft(&dictated.content, false, false, cx);
                self.input_caret = Some(dictated.caret);
            } else if let Some(text) = params["text"].as_str() {
                // The reader typed in the chat box after the words were asked for: the core
                // places them again, in the text and at the caret as they are now.
                let (start, end) = utf16_selection(&self.draft, self.composer_selection(cx));
                self.invoke(
                    json!({
                        "type": "insertDictation",
                        "text": text,
                        "spacing": params["spacing"],
                        "id": params["id"],
                        "draft": self.draft,
                        "start": start,
                        "end": end,
                    }),
                    cx,
                );
            }
            return;
        };
        let key = match &params["slot"] {
            serde_json::Value::String(slot) => format!("{form}:{slot}"),
            slot => format!("{form}:{}", slot.as_u64().unwrap_or_default()),
        };
        for modal in [false, true] {
            // A copy that is drawn takes the answer. The transcript's box takes words just heard
            // also while it has no input: it is the dictation's own target, and the sync that
            // makes its input shows them (`sync_form_input_in`), or drops them with the box.
            if self.inputs_mut(modal).contains_key(&key) || (!modal && dictated.heard) {
                let copy = (modal, key.clone());
                let dictated = match self.dictated_boxes.remove(&copy) {
                    Some(waiting) => dictated.clone().after(waiting),
                    None => dictated.clone(),
                };
                self.dictated_boxes.insert(copy, dictated);
            }
        }
        cx.notify();
    }

    /// Shows the text the core says this form box's draft has now (`dictation_inserted`). A copy
    /// of the box that shows one of the texts it supersedes takes it, with the caret after the
    /// words, and the input's report of it is not sent back as an edit (`FormInput::adopted`); a
    /// copy showing anything else holds an edit the core has not answered yet, whose own answer
    /// brings its text. The transcript's box, where a dictation started, takes the keyboard unless
    /// the Resources modal is open.
    ///
    /// CDXC:Dictate 2026-11-10 WHY: After a dictation into a box nobody had typed in, the caret stood before the words instead of after them (user: "cursor'ı cümlenin başına indiriyor. Aslında en sonuna indirmesi lazım"). Such a box is still waiting for its saved draft (`FormInput::settled`), so the document that carried the words filled it through `set_value`, which puts a multi-line input's caret at the start, and this function then found the words already shown and left the caret alone. A box's sync therefore runs this before that fill (`sync_form_input_in`), and a copy that already shows words just heard gets the core's caret too, for the document that arrives before the answer. The same happened to a target whose input was made after the answer: a form sent while the recording ran loses its inputs, a refused send opens it again, and words that arrived before the next draw found no input to keep their answer for, so the new input was made from the draft with the caret at the start and without the keyboard. The answer for the transcript's box is therefore kept until that box's next sync (`dictation_inserted`), which shows it in the input it makes or drops it with a box that is no longer there; an answer is never kept for a copy that is opened later.
    pub(super) fn show_dictated(
        &mut self,
        modal: bool,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dictated_boxes.is_empty() {
            return;
        }
        let Some(dictated) = self.dictated_boxes.remove(&(modal, key.to_string())) else {
            return;
        };
        let Some(entry) = self.inputs_mut(modal).get_mut(key) else {
            return;
        };
        let input = entry.input.clone();
        let current = input.read(cx).value().to_string();
        let adopts = dictated.supersedes.contains(&current);
        let shows = current == dictated.content;
        if adopts || shows {
            entry.settled = true;
            entry.core = dictated.core.clone();
            entry.rev = dictated.rev;
        }
        if adopts {
            entry.adopted = Some(dictated.content.clone());
        }
        // Not while the Resources modal is open: the reader may be typing in its copy.
        let takes_keyboard =
            dictated.heard && !modal && self.snapshot["resourceModal"]["id"].is_null();
        input.update(cx, |input, cx| {
            let caret = super::form_attachments::utf16_to_byte(&dictated.content, dictated.caret);
            if adopts {
                input.replace_all(dictated.content, window, cx);
            }
            if adopts || (shows && dictated.heard) {
                input.set_selected_range(caret..caret, cx);
            }
            if takes_keyboard {
                input.focus(window, cx);
            }
        });
    }
}

/// The core's `dictationInserted` (`dictation_inserted`), kept for a form box until its next
/// draw.
#[derive(Clone)]
pub(crate) struct Dictated {
    /// The texts a copy of the target may show to take `content` in their place.
    supersedes: Vec<String>,
    /// The target's text now, and the UTF-16 offset after the words in it.
    content: String,
    caret: usize,
    /// A form box's draft now: the core that holds it and how many dictations it has.
    core: String,
    rev: u64,
    /// The words were just heard; false when another copy's edit moved them.
    heard: bool,
}

impl Dictated {
    fn of(params: &serde_json::Value) -> Option<Self> {
        Some(Self {
            supersedes: params["supersedes"]
                .as_array()?
                .iter()
                .filter_map(|text| text.as_str().map(str::to_owned))
                .collect(),
            content: params["content"].as_str()?.to_owned(),
            caret: params["caret"].as_u64()? as usize,
            core: params["core"].as_str().unwrap_or_default().to_owned(),
            rev: params["rev"].as_u64().unwrap_or(0),
            heard: params["text"].is_string(),
        })
    }

    /// This answer in place of `waiting`, one the copy has not drawn yet: when it builds on
    /// `waiting`'s text, a copy that would have taken that text takes this one's.
    fn after(mut self, waiting: Self) -> Self {
        if self.supersedes.contains(&waiting.content) {
            self.supersedes.extend(waiting.supersedes);
            self.heard |= waiting.heard;
        }
        self
    }
}

/// `range` (byte offsets into `text`) as UTF-16 offsets; the end of the text without one.
fn utf16_selection(text: &str, range: Option<std::ops::Range<usize>>) -> (usize, usize) {
    let units = |byte: usize| text.get(..byte).unwrap_or(text).encode_utf16().count();
    match range {
        Some(range) => (units(range.start), units(range.end.max(range.start))),
        None => (units(text.len()), units(text.len())),
    }
}

/// The microphone's tooltip: "Dictate · <engine>" over the hotkey and how it works, or over what
/// is missing (Hex not running, no key) when the engine isn't ready.
///
/// CDXC:VoiceMode 2026-11-05 DECISION: User (Q1259): "we need to also mention in a tooltip what we
/// use"; the mockup's two lines, "Dictate · Local, Parakeet v3" then "⌃⌥Space: tap or hold to
/// talk", or the engine's problem in place of the second line.
pub(super) fn dictation_tooltip_text(
    engine: Option<&serde_json::Value>,
    recording: bool,
    chord: &str,
    mode: &str,
) -> String {
    let label = engine.and_then(|engine| engine["label"].as_str());
    let first = match (recording, label) {
        (true, _) => "Stop dictating".to_string(),
        (false, Some(label)) => format!("Dictate · {label}"),
        (false, None) => "Dictate".to_string(),
    };
    let problem = engine
        .filter(|engine| engine["ready"] != true)
        .and_then(|engine| engine["problem"]["message"].as_str());
    let hint = match mode {
        "toggle" => "press to start or stop",
        "hold" => "hold to talk",
        _ => "tap or hold to talk",
    };
    let second = match problem {
        Some(problem) if !recording => problem.to_string(),
        _ if chord.is_empty() => String::new(),
        _ if recording => format!("{chord} stops"),
        _ => format!("{chord}: {hint}"),
    };
    if second.is_empty() {
        first
    } else {
        format!("{first}\n{second}")
    }
}

/// `dictateHotkeyMode` (`auto`, `toggle`, `hold`).
pub(super) fn dictate_hotkey_mode() -> String {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("dictateHotkeyMode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("auto")
        .to_string()
}

/// `dictateInsertSpacing`, on unless turned off in Settings › Dictation.
fn dictate_insert_spacing() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("dictateInsertSpacing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_names_the_engine_over_the_hotkey() {
        let engine =
            json!({"engine": "hex", "label": "Hex, Parakeet v3", "ready": true, "problem": null});
        assert_eq!(
            dictation_tooltip_text(Some(&engine), false, "⌃⌥Space", "auto"),
            "Dictate · Hex, Parakeet v3\n⌃⌥Space: tap or hold to talk"
        );
        assert_eq!(
            dictation_tooltip_text(Some(&engine), true, "⌃⌥Space", "auto"),
            "Stop dictating\n⌃⌥Space stops"
        );
        let missing = json!({"label": "Hex", "ready": false, "problem": {"code": "hexNotRunning", "message": "Hex isn't running on this computer. Open Hex."}});
        assert_eq!(
            dictation_tooltip_text(Some(&missing), false, "⌃⌥Space", "auto"),
            "Dictate · Hex\nHex isn't running on this computer. Open Hex."
        );
        // Before gxserver answered, and with no hotkey set.
        assert_eq!(dictation_tooltip_text(None, false, "", "auto"), "Dictate");
        assert_eq!(
            dictation_tooltip_text(None, false, "Ctrl+Alt+Space", "hold"),
            "Dictate\nCtrl+Alt+Space: hold to talk"
        );
    }

    #[test]
    fn hex_that_isnt_ready_offers_open_settings() {
        for code in ["hexNotInstalled", "hexNotRunning", "hexModelNotReady"] {
            let actions = failure_actions(&failure(code, Some("hex")));
            assert_eq!(actions.models, Some("Open settings"), "{code}");
            assert_eq!(actions.key, None);
            assert!(actions.retry && actions.recording_kept);
        }
        assert_eq!(
            failure_actions(&failure("hexUnsupported", Some("hex"))).models,
            Some("Open settings")
        );
        assert_eq!(
            open_voice_models(),
            json!({"type": "open", "modal": "settings", "initialTab": "dictation", "initialSection": "dictate"})
        );
    }

    fn failure(code: &str, provider: Option<&str>) -> DictateFailure {
        DictateFailure {
            code: code.to_string(),
            message: String::new(),
            provider: provider.map(str::to_string),
            detail: None,
        }
    }

    #[test]
    fn a_missing_key_offers_add_key_on_xai_unless_gxserver_named_openai() {
        let actions = failure_actions(&failure("noKey", None));
        assert_eq!(actions.key, Some(("Add key", "xai")));
        assert!(actions.retry && actions.recording_kept);
        // A saved OpenAI key that can't be read anymore.
        assert_eq!(
            failure_actions(&failure("noKey", Some("openai"))).key,
            Some(("Add key", "openai"))
        );
    }

    #[test]
    fn a_refused_key_offers_check_key_on_that_provider() {
        assert_eq!(
            failure_actions(&failure("badKey", Some("openai"))).key,
            Some(("Check key", "openai"))
        );
        assert_eq!(
            failure_actions(&failure("badKey", Some("xai"))).key,
            Some(("Check key", "xai"))
        );
    }

    #[test]
    fn failures_time_can_fix_offer_transcribe_again_only() {
        for code in [
            "providerUnavailable",
            "rateLimited",
            "paymentRequired",
            "badAudio",
            "providerError",
            "interrupted",
            "notSaved",
            "serviceUnavailable",
        ] {
            let actions = failure_actions(&failure(code, Some("xai")));
            assert_eq!(
                actions,
                FailureActions {
                    key: None,
                    models: None,
                    retry: true,
                    recording_kept: true
                },
                "{code}"
            );
        }
    }

    #[test]
    fn missing_audio_offers_nothing_and_does_not_promise_the_recording() {
        assert_eq!(
            failure_actions(&failure("audioMissing", None)),
            FailureActions {
                key: None,
                models: None,
                retry: false,
                recording_kept: false
            }
        );
    }

    #[test]
    fn add_key_opens_the_voice_agent_key_field() {
        assert_eq!(
            open_voice_agent_key("openai"),
            json!({
                "type": "open",
                "modal": "settings",
                "initialTab": "dictation",
                "initialSection": "dictate",
                "initialVoiceAgentKey": "openai",
            })
        );
    }
}
