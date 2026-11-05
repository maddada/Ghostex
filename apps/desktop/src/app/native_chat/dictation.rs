//! The composer's Dictate control: the microphone records to a file first and the recording is
//! transcribed after Stop (`voice_agent::Dictate`, gxserver's `/api/transcribeDictation`); the text
//! is typed into the draft. While it records or transcribes, a small pill just above the
//! microphone shows it with an ✕ that cancels (Escape cancels too); a failed transcription turns it
//! into a small card that says what went wrong and what to do, with Add key or Check key and
//! Transcribe again. Hovering the microphone when a recording has no text yet offers Transcribe
//! previous.
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
use crate::app::voice_agent::{Dictate, DictateEvent, DictateFailure, DictatePhase};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, EntityInputHandler as _,
    FontFeatures, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, canvas,
    deferred, div, px,
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
    #[cfg(target_family = "wasm")]
    {
        crate::app::voice_agent::dictation_preview()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        DICTATION
    }
}

/// The pill's height, unscaled: a 16px line, 4px above and below, a 1px edge.
const PILL_HEIGHT: f32 = 26.0;
/// Space between the pill's bottom edge and the microphone's top edge, unscaled.
const PILL_GAP: f32 = 6.0;
/// The failure card's width, unscaled (narrower when the chat box is).
const FAILURE_CARD_WIDTH: f32 = 300.0;

/// What the failure card offers besides its ✕.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FailureActions {
    /// `(label, provider)`: Add key or Check key, opening that provider's key field.
    pub(super) key: Option<(&'static str, &'static str)>,
    /// A button opening Settings › Voice at Dictate's models (the local engine has no model yet,
    /// or it is still downloading).
    pub(super) models: Option<&'static str>,
    /// Transcribe again: whatever went wrong may be gone by now.
    pub(super) retry: bool,
    /// The card adds "Your recording is kept." (not when its audio is gone).
    pub(super) recording_kept: bool,
}

/// The buttons for gxserver's failure code (`server/src/voice_agent/transcribe.rs`). A key that
/// is missing or can't be read anymore (`noKey`) offers Add key, a refused one Check key, both on
/// the provider gxserver named (xAI when it named none). The local engine with no model downloaded
/// (`modelMissing`) offers Download a model, and one still downloading Show download; both open
/// Settings › Voice at the models. A recording whose audio is gone has nothing left to try.
///
/// CDXC:VoiceMode 2026-11-05 DECISION: Dictate's engine is the user's explicit choice (Q1259,
/// Q1272) and never falls back by itself, so a Local engine without a model is a failure the card
/// explains, with the way to fix it one press away (Settings › Voice, where the download runs with
/// its progress), and Transcribe again for once it is there.
pub(super) fn failure_actions(failure: &DictateFailure) -> FailureActions {
    let provider = match failure.provider.as_deref() {
        Some("openai") => "openai",
        _ => "xai",
    };
    let (key, models, retry, recording_kept) = match failure.code.as_str() {
        "noKey" => (Some(("Add key", provider)), None, true, true),
        "badKey" => (Some(("Check key", provider)), None, true, true),
        "modelMissing" | "localUnavailable" => (None, Some("Download a model"), true, true),
        "modelDownloading" => (None, Some("Show download"), true, true),
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

/// Settings › Voice, scrolled to Dictate (its engine and models).
pub(crate) fn open_voice_models() -> serde_json::Value {
    json!({
        "type": "open",
        "modal": "settings",
        "initialTab": "voiceAgent",
        "initialSection": "dictate",
    })
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "local" => "The local model",
        _ => "xAI",
    }
}

/// Settings › Voice with `provider`'s key field focused.
pub(super) fn open_voice_agent_key(provider: &str) -> serde_json::Value {
    json!({
        "type": "open",
        "modal": "settings",
        "initialTab": "voiceAgent",
        "initialSection": "keys",
        "initialVoiceAgentKey": provider,
    })
}

fn failure_red(p: &ChatAppearance) -> Hsla {
    if p.light {
        gpui::rgb(0xdc6262).into()
    } else {
        gpui::rgb(0xef9999).into()
    }
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
}

pub(crate) type Dictation = Option<DictationControl>;

impl NativeChatView {
    fn dictate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Dictate> {
        if let Some(control) = &self.dictation {
            return control.dictate.clone();
        }
        let dictate = cx.new(|_| Dictate::new());
        let subscription =
            cx.subscribe_in(&dictate, window, |this, _, event, window, cx| match event {
                DictateEvent::Text(text) => this.insert_dictation(text, window, cx),
                DictateEvent::Failure { title, message } => {
                    // A failed transcription shows in the pill over the microphone with Transcribe
                    // again; only what the pill does not show becomes a toast.
                    let in_row = this.dictation.as_ref().is_some_and(|control| {
                        matches!(
                            control.dictate.read(cx).phase(),
                            DictatePhase::Failed { .. }
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
        });
        dictate
    }

    /// Whether the microphone is recording (the Stop glyph).
    pub(crate) fn dictating(&self, cx: &App) -> bool {
        self.dictation
            .as_ref()
            .is_some_and(|control| control.dictate.read(cx).recording())
    }

    /// Start recording, or stop and transcribe. With the local engine picked and its model not
    /// downloaded, the press starts the download instead (the tooltip says "Click to download")
    /// and opens Settings › Voice, where its progress shows.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dictating(cx)
            && let Some(model) = missing_engine_model()
        {
            cx.background_executor()
                .spawn(async move { crate::app::voice_agent::download_engine_model(&model) })
                .detach();
            cx.emit(NativeChatEvent::Host(open_voice_models()));
            return;
        }
        let dictate = self.dictate(window, cx);
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
        dictate.update(cx, |dictate, cx| dictate.transcribe_previous(cx));
    }

    /// Settings › Voice, scrolled to Dictation history.
    pub(super) fn open_dictation_history(&mut self, cx: &mut Context<Self>) {
        cx.emit(NativeChatEvent::Host(json!({
            "type":"open","modal":"settings","initialTab":"voiceAgent","initialSection":"dictations",
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
        let dictate = self.dictate(window, cx);
        let idle = matches!(
            dictate.read(cx).phase(),
            DictatePhase::Idle | DictatePhase::Failed { .. }
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
        let floating = match &phase {
            DictatePhase::Idle => return None,
            DictatePhase::Failed { failure, .. } => {
                // Never wider than the chat box it floats in. Its width is known before it
                // paints, so the first frame already places it.
                let room = self.composer_bounds.get().size.width - px(2.0);
                let mut width = px(FAILURE_CARD_WIDTH * s);
                if room > px(0.0) {
                    width = width.min(room);
                }
                let mut measured = control.pill.get();
                measured.size.width = width;
                control.pill.set(measured);
                self.render_dictation_failure(failure, width, p, cx).child(
                    close(true, cx)
                        .absolute()
                        .top(px(6.0 * s))
                        .right(px(6.0 * s)),
                )
            }
            _ => {
                let recording = matches!(phase, DictatePhase::Recording);
                let polishing = control.dictate.read(cx).polishing();
                let label = match &phase {
                    DictatePhase::Starting => "Starting…".to_string(),
                    DictatePhase::Recording => {
                        crate::app::voice_agent::dictation_clock(control.dictate.read(cx).elapsed())
                    }
                    _ if polishing => "Polishing".to_string(),
                    _ => "Transcribing".to_string(),
                };
                div()
                    .h(px(PILL_HEIGHT * s))
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .pl(px(if recording { 9.0 } else { 11.0 } * s))
                    .pr(px(4.0 * s))
                    .rounded_full()
                    .whitespace_nowrap()
                    .when(recording, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .size(px(7.0 * s))
                                .rounded_full()
                                .bg(gpui::rgb(0xef4444)),
                        )
                    })
                    .child(
                        div()
                            .when(recording, |this| {
                                this.font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                            })
                            .text_color(p.foreground)
                            .child(label),
                    )
                    // CDXC:VoiceMode 2026-11-05 DECISION: User (Q1273, the mockup): AI polish is on
                    // by default and the chat box shows Polishing with a Skip button, which types
                    // the text as it was before polish at once.
                    .when(polishing, |this| {
                        this.child(
                            div()
                                .id("dictation-skip-polish")
                                .role(gpui::Role::Button)
                                .aria_label("Skip polish")
                                .chat_cursor_pointer()
                                .flex_shrink_0()
                                .h(px(18.0 * s))
                                .px(px(7.0 * s))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .text_color(p.foreground)
                                .bg(p.foreground.opacity(0.08))
                                .hover(|style| style.bg(p.foreground.opacity(0.14)))
                                .tooltip(|window, cx| {
                                    gpui_component::tooltip::Tooltip::new(
                                        "Type the text without AI polish",
                                    )
                                    .build(window, cx)
                                })
                                .child("Skip")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(control) = this.dictation.as_ref() {
                                        let dictate = control.dictate.clone();
                                        dictate.update(cx, |dictate, cx| dictate.skip_polish(cx));
                                    }
                                })),
                        )
                    })
                    .child(close(false, cx))
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
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .text_size(px(12.0 * s))
            .line_height(px(16.0 * s))
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

    /// The failure card: "Couldn't transcribe", gxserver's sentence saying what went wrong and
    /// what to do, and the buttons that help (`failure_actions`). The ✕ is added by the caller,
    /// top right. The provider's own words, when it gave any, are the sentence's tooltip.
    ///
    /// CDXC:VoiceMode 2026-10-30 DECISION: User picked Q779 option B, "a two-line card with the full sentence", for every failure gxserver reports, each in plain words, instead of a pill that only said "Transcription failed" with the reason in a tooltip (Sam on Discord, no xAI key set). Q780 option A: its Add key / Check key button opens the page with that key's field focused (Settings > Voice since 2026-11-05). Supersedes the failure part of the 2026-10-20 one-line pill above; recording and transcribing keep the pill.
    fn render_dictation_failure(
        &self,
        failure: &DictateFailure,
        width: Pixels,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let s = p.scale;
        let actions = failure_actions(failure);
        let sentence = if actions.recording_kept {
            format!("{} Your recording is kept.", failure.message)
        } else {
            failure.message.clone()
        };
        let detail = failure.detail.clone().map(|detail| {
            match failure.provider.as_deref().map(provider_label) {
                Some(label) => format!("{label} said: {detail}"),
                None => detail,
            }
        });
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .flex_shrink_0()
                .px(px(9.0 * s))
                .h(px(22.0 * s))
                .flex()
                .items_center()
                .rounded_full()
                .whitespace_nowrap()
                .text_color(p.foreground)
                .bg(if primary {
                    p.control_primary.opacity(0.3)
                } else {
                    p.foreground.opacity(0.08)
                })
                .hover(move |style| {
                    style.bg(if primary {
                        p.control_primary.opacity(0.42)
                    } else {
                        p.foreground.opacity(0.14)
                    })
                })
                .child(label)
        };
        let mut buttons = div().flex().gap(px(6.0 * s));
        if let Some(label) = actions.models {
            buttons = buttons.child(button("dictation-models", label, true).on_click(cx.listener(
                |_, _, _, cx| {
                    cx.emit(NativeChatEvent::Host(open_voice_models()));
                },
            )));
        }
        if let Some((label, provider)) = actions.key {
            buttons = buttons.child(button("dictation-key", label, true).on_click(cx.listener(
                move |_, _, _, cx| {
                    cx.emit(NativeChatEvent::Host(open_voice_agent_key(provider)));
                },
            )));
        }
        if actions.retry {
            buttons = buttons.child(
                button(
                    "dictation-retry",
                    "Transcribe again",
                    actions.key.is_none() && actions.models.is_none(),
                )
                .on_click(
                    cx.listener(|this, _, _, cx| {
                        if let Some(control) = this.dictation.as_ref() {
                            let dictate = control.dictate.clone();
                            dictate.update(cx, |dictate, cx| dictate.retry(cx));
                        }
                        cx.notify();
                    }),
                ),
            );
        }
        let has_buttons = actions.key.is_some() || actions.models.is_some() || actions.retry;
        div()
            .relative()
            .w(width)
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .pt(px(9.0 * s))
            .pb(px(if has_buttons { 10.0 } else { 9.0 } * s))
            .px(px(12.0 * s))
            .rounded(px(12.0 * s))
            .child(
                div()
                    .pr(px(16.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(failure_red(p))
                    .child("Couldn't transcribe"),
            )
            .child(
                div()
                    .id("dictation-reason")
                    .text_color(p.card_muted)
                    .child(sentence)
                    .when_some(detail, |this, detail| {
                        this.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
                        })
                    }),
            )
            .when(has_buttons, |this| this.child(buttons.mt(px(4.0 * s))))
    }
    /// Whether the toolbar draws the microphone this frame (`toolbar.rs` `render_toolbar`'s rule);
    /// otherwise its last painted bounds are stale.
    fn dictation_button_shown(&self) -> bool {
        !self.composer_animation.shown_collapsed() && self.composer_control_available("dictate")
    }

    /// Type a finished dictation at the caret (over a selection), spaced from the characters on
    /// either side of it (`spaced_at_caret`) while Insert with spacing is on.
    ///
    /// CDXC:VoiceMode 2026-11-05 DECISION: User (Q1264) picked "Spacing at the caret": the space
    /// is decided by the characters next to the caret, not by the end of the draft, so text
    /// dictated into the middle of a sentence gets a space on each side it needs one and none
    /// before a comma or after an opening bracket. `dictateInsertSpacing` off types it as heard.
    fn insert_dictation(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            cx.notify();
            return;
        }
        let Some(input) = self.input.clone() else {
            cx.notify();
            return;
        };
        let spaced = if dictate_insert_spacing() {
            let (before, after) = {
                let input = input.read(cx);
                let value = input.value();
                let range = input.selected_range();
                let start = range.start.min(value.len());
                let end = range.end.clamp(start, value.len());
                (
                    value.get(..start).and_then(|text| text.chars().next_back()),
                    value.get(end..).and_then(|text| text.chars().next()),
                )
            };
            spaced_at_caret(before, after, text)
        } else {
            text.to_owned()
        };
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.replace_text_in_range(None, &spaced, window, cx);
        });
        cx.notify();
    }
}

/// The microphone's tooltip: "Dictate · <engine>" over the hotkey and how it works, or over what
/// is missing (a model not downloaded, no key) when the engine isn't ready.
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

/// The local engine's model when it is picked and not downloaded yet.
fn missing_engine_model() -> Option<String> {
    let engine = crate::app::voice_agent::dictate_engine()?;
    (engine["problem"]["code"] == "modelMissing")
        .then(|| engine["modelId"].as_str().map(str::to_string))
        .flatten()
}

/// `dictateInsertSpacing`, on unless turned off in Settings › Voice.
fn dictate_insert_spacing() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("dictateInsertSpacing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// `text` with the spaces the caret's neighbours need: one before it after a word or a closing
/// mark (not at the start, after a space, or after an opening bracket or quote), and one after
/// it before a word (not before a space or a punctuation mark that hugs the word before it).
pub(super) fn spaced_at_caret(before: Option<char>, after: Option<char>, text: &str) -> String {
    let first = text.chars().next();
    let last = text.chars().next_back();
    let hugs_left = |c: char| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '…' | '%');
    let opens = |c: char| matches!(c, '(' | '[' | '{' | '"' | '\'' | '`' | '/' | '@' | '#' | '“' | '‘');
    let space_before = before.is_some_and(|c| !c.is_whitespace() && !opens(c))
        && first.is_some_and(|c| !c.is_whitespace() && !hugs_left(c));
    let space_after = after.is_some_and(|c| !c.is_whitespace() && !hugs_left(c) && c != '"' && c != '\'')
        && last.is_some_and(|c| !c.is_whitespace() && !opens(c));
    let mut spaced = String::with_capacity(text.len() + 2);
    if space_before {
        spaced.push(' ');
    }
    spaced.push_str(text);
    if space_after {
        spaced.push(' ');
    }
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_names_the_engine_over_the_hotkey() {
        let engine = json!({"engine": "local", "label": "Local, Parakeet v3", "ready": true, "problem": null});
        assert_eq!(
            dictation_tooltip_text(Some(&engine), false, "⌃⌥Space", "auto"),
            "Dictate · Local, Parakeet v3\n⌃⌥Space: tap or hold to talk"
        );
        assert_eq!(
            dictation_tooltip_text(Some(&engine), true, "⌃⌥Space", "auto"),
            "Stop dictating\n⌃⌥Space stops"
        );
        let missing = json!({"label": "Local, Parakeet v3", "ready": false, "problem": {"code": "modelMissing", "message": "Not downloaded yet. Click to download (0.72 GB)"}});
        assert_eq!(
            dictation_tooltip_text(Some(&missing), false, "⌃⌥Space", "auto"),
            "Dictate · Local, Parakeet v3\nNot downloaded yet. Click to download (0.72 GB)"
        );
        // Before gxserver answered, and with no hotkey set.
        assert_eq!(dictation_tooltip_text(None, false, "", "auto"), "Dictate");
        assert_eq!(
            dictation_tooltip_text(None, false, "Ctrl+Alt+Space", "hold"),
            "Dictate\nCtrl+Alt+Space: hold to talk"
        );
    }

    #[test]
    fn a_local_engine_without_a_model_offers_download_a_model() {
        let actions = failure_actions(&failure("modelMissing", Some("local")));
        assert_eq!(actions.models, Some("Download a model"));
        assert_eq!(actions.key, None);
        assert!(actions.retry && actions.recording_kept);
        assert_eq!(
            failure_actions(&failure("modelDownloading", Some("local"))).models,
            Some("Show download")
        );
        assert_eq!(
            open_voice_models(),
            json!({"type": "open", "modal": "settings", "initialTab": "voiceAgent", "initialSection": "dictate"})
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
                "initialTab": "voiceAgent",
                "initialSection": "keys",
                "initialVoiceAgentKey": "openai",
            })
        );
    }

    #[test]
    fn an_empty_draft_or_a_caret_after_a_space_gets_no_leading_space() {
        assert_eq!(spaced_at_caret(None, None, "hello there"), "hello there");
        assert_eq!(spaced_at_caret(Some(' '), None, "hello"), "hello");
        assert_eq!(spaced_at_caret(Some('\n'), None, "hello"), "hello");
    }

    #[test]
    fn a_caret_right_after_a_word_gets_a_space_before() {
        assert_eq!(spaced_at_caret(Some('d'), None, "hello"), " hello");
        assert_eq!(spaced_at_caret(Some('.'), None, "Next one"), " Next one");
    }

    #[test]
    fn a_caret_in_the_middle_of_a_sentence_is_spaced_on_both_sides() {
        // "fix the|bug" with the caret between "the" and "bug".
        assert_eq!(spaced_at_caret(Some('e'), Some('b'), "login"), " login ");
        // "fix the |bug": the space is already before the caret.
        assert_eq!(spaced_at_caret(Some(' '), Some('b'), "login"), "login ");
        // "fix the| bug": the space is already after the caret.
        assert_eq!(spaced_at_caret(Some('e'), Some(' '), "login"), " login");
    }

    #[test]
    fn the_end_of_the_draft_is_not_what_decides() {
        // The draft ends in a word, but the caret sits at its start.
        assert_eq!(spaced_at_caret(None, Some('w'), "Hello"), "Hello ");
    }

    #[test]
    fn punctuation_and_brackets_hug_their_word() {
        assert_eq!(spaced_at_caret(Some('('), Some(')'), "see notes"), "see notes");
        assert_eq!(spaced_at_caret(Some('d'), Some(','), "please"), " please");
        assert_eq!(spaced_at_caret(Some('d'), Some('.'), "now"), " now");
        assert_eq!(spaced_at_caret(Some('d'), None, ", then"), ", then");
        assert_eq!(spaced_at_caret(Some('"'), Some('"'), "quoted"), "quoted");
    }
}
