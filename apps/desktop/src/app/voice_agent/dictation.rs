//! Dictate for the chat box and the terminal bar: record first (`recorder.rs`, a WAV file this app
//! keeps until gxserver has it), transcribe second (gxserver's `/api/transcribeDictation`, with the
//! engine picked in Settings › Dictation: Hex on that computer, or OpenAI's or xAI's speech-to-text
//! with the key kept on this computer); the text is typed in as the engine wrote it.
//! The two steps are independent: whatever goes
//! wrong while transcribing, the recording stays and Transcribe again or Transcribe previous sends
//! it once more.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "i want to be able to do escape to cancel the voice recording, and we need to show "Transcribing (X)" so user can click on x to cancel the recording but even if he cancels it we need to save so user can hover over the voice button and click "Transcribe previous"". Escape (in the chat box) cancels a recording; after Stop the chat box shows Transcribing with an ✕ that cancels; a cancelled recording is kept as dismissed and Transcribe previous transcribes the newest recording that has no text. A failure says why and offers Transcribe again. The words go into the draft (or the terminal) and are never sent (CDXC:SessionChat 2026-10-08 DECISION). Supersedes the 2026-10-20 streaming dictation through `/api/voice?mode=dictation`, which lost what was said whenever xAI refused or the connection dropped.
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: server/src/voice_agent/recordings.rs (the store and the user's decision on keeping every recording), the phone's `src/chat/native/composer/useDictation.ts`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{Context, EventEmitter, Task};
use serde_json::{Value, json};

use super::recorder::{self, ActiveRecording, LocalRecording};

/// xAI usually answers within a few seconds; a long recording gets longer.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(180);
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the recording pill's clock, waveform and the microphone health are looked at: the
/// waveform takes one bar per tick, as the level meter makes one level per 50 ms.
const TICK: Duration = Duration::from_millis(50);
/// How long a press waits for gxserver to say whether the engine can transcribe before the
/// microphone opens anyway (the recording is kept either way).
const ENGINE_CHECK_TIMEOUT: Duration = Duration::from_secs(2);
/// The bars of the recording pill's waveform (1.2 s of levels).
pub(crate) const WAVEFORM_BARS: usize = 24;
/// Below this level the microphone counts as hearing nothing (-40 dBFS on `level_meter.rs`'s scale).
const QUIET_LEVEL: f32 = 0.25;
/// How long near-silence lasts before the pill says it can't hear (Q2919).
const QUIET_FOR: Duration = Duration::from_secs(3);

/// Where a recording that has no text yet is.
#[derive(Clone, Debug)]
pub(crate) enum Kept {
    /// gxserver has it (its id).
    Server(String),
    /// Only this app has it: gxserver could not be reached.
    Local(LocalRecording),
}

#[derive(Clone, Debug)]
pub(crate) enum DictatePhase {
    Idle,
    /// Waiting for the microphone (and, the first time, for macOS to ask for access).
    Starting,
    Recording,
    /// Stopped: handing the file to gxserver, then waiting for xAI's text.
    Transcribing,
    /// The transcription failed; `failure` says why. The recording is kept.
    Failed {
        kept: Kept,
        failure: DictateFailure,
    },
    /// gxserver said its engine can't transcribe yet (`readDictateEngine`), so nothing was
    /// recorded; `failure` says why (its code and sentence).
    Unavailable {
        failure: DictateFailure,
    },
    /// Something the reader should know that no button can retry: the microphone couldn't start
    /// (`micUnavailable`), the recording had no words (`nothingHeard`), or nothing waited for
    /// Transcribe previous (`nothingWaiting`).
    ///
    /// CDXC:Dictate 2026-11-13 DECISION: User (Q2916 "card"): a refused microphone and "nothing heard" show as the Dictate card above the microphone, which stays until dismissed, instead of a toast that is gone in seconds. The terminal bar, which draws no card, keeps the toast.
    Notice {
        failure: DictateFailure,
    },
    /// The reader cancelled a recording or its transcription: the recording is kept, and Restore
    /// transcribes it into the place it was aimed at. Shown for [`CANCELLED_SHOWN`], then Idle
    /// (the recording stays for Transcribe previous and in Settings > Dictation > Past recordings).
    ///
    /// CDXC:Dictate 2026-11-21 DECISION: User: "if the user cancels transcription by mistake we still make sure we send it to hex to be saved there or we save the audio or something cuz i dont want it to be lost imagine if they had very long transcription they would be very mad to lose it". Every cancel (✕, Escape, a page leaving the screen) already kept the recording in gxserver's store (CDXC:VoiceMode 2026-10-20), but nothing said so: right after a cancel the chat box's pill and a page's box say "Dictation cancelled" with Restore, which transcribes that same recording into the same place. The audio stays in Ghostex's store rather than Hex's history: Hex's local API only transcribes, and the store serves every engine (Hex, OpenAI, xAI, Superwhisper) and the phone.
    Cancelled {
        restore: Restorable,
    },
}

/// How long the "Dictation cancelled · Restore" pill stays after a cancel.
const CANCELLED_SHOWN: Duration = Duration::from_secs(12);
/// How long Restore waits for a cancelled recording to reach gxserver (the upload of a long
/// recording, or a gxserver restart).
const RESTORE_WAIT: Duration = Duration::from_secs(60);

/// Where a run's recording ends up, told once its hand-over to gxserver is done (in the
/// background, so a cancel never waits): Restore waits for it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Restorable(Arc<(Mutex<Option<Option<Kept>>>, std::sync::Condvar)>);

impl Restorable {
    /// `None`: the run had no recording to restore (nothing was waiting).
    fn fill(&self, kept: Option<Kept>) {
        let (slot, ready) = &*self.0;
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(kept);
            ready.notify_all();
        }
    }

    fn filled(&self) -> bool {
        self.0.0.lock().is_ok_and(|slot| slot.is_some())
    }

    /// The recording, once known; `None` when there is none or its hand-over did not end in
    /// `timeout`.
    fn wait(&self, timeout: Duration) -> Option<Kept> {
        let (slot, ready) = &*self.0;
        let slot = slot.lock().ok()?;
        let (slot, _) = ready
            .wait_timeout_while(slot, timeout, |kept| kept.is_none())
            .ok()?;
        slot.clone().flatten()
    }

    fn same(&self, other: &Restorable) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Why a transcription failed: gxserver's `error` on the recording (`recordings.rs` `Problem`),
/// or this app's own when gxserver never got that far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DictateFailure {
    /// gxserver's code (`noKey`, `badKey`, `providerUnavailable`, `rateLimited`,
    /// `paymentRequired`, `badAudio`, `audioMissing`, ...); `notSaved` when gxserver could not take
    /// the recording and `serviceUnavailable` when it could not be asked.
    pub(crate) code: String,
    /// The sentence to show: what went wrong and what to do.
    pub(crate) message: String,
    /// The provider whose key was used (`xai`, `openai`).
    pub(crate) provider: Option<String>,
    /// The provider's own words, when it gave a reason.
    pub(crate) detail: Option<String>,
}

impl DictateFailure {
    fn new(code: &str, message: String) -> Self {
        Self {
            code: code.to_string(),
            message,
            provider: None,
            detail: None,
        }
    }

    /// The recording's `error` as gxserver keeps it.
    fn from_problem(error: &Value) -> Self {
        let text = |key: &str| {
            error[key]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            code: text("code").unwrap_or_else(|| "failed".to_string()),
            message: text("message").unwrap_or_else(|| "The transcription failed.".to_string()),
            provider: text("provider"),
            detail: text("detail"),
        }
    }
}

pub(crate) enum DictateEvent {
    /// The finished dictation, for the draft or the terminal.
    Text(String),
    /// Something the reader should hear about where no row shows it (the terminal bar's toast).
    Failure { title: String, message: String },
}

/// One Dictate control's state. The chat box and the terminal bar each own one.
pub(crate) struct Dictate {
    phase: DictatePhase,
    active: Option<ActiveRecording>,
    /// Set when the reader cancels, so work already in flight does not type anything.
    cancelled: Arc<AtomicBool>,
    /// The recording the work in flight is transcribing, once gxserver has it, so a cancel can
    /// dismiss it at once.
    in_flight: Arc<Mutex<Option<String>>>,
    /// Where this run's recording ends up, for Restore after a cancel.
    kept: Restorable,
    work: Option<Task<()>>,
    ticker: Option<Task<()>>,
    elapsed: Duration,
    /// The microphone's level at each tick of this recording, oldest first ([`WAVEFORM_BARS`]).
    levels: VecDeque<f32>,
    /// When this recording last heard more than near-silence (`cant_hear`).
    loud_at: std::time::Instant,
    /// A hotkey hold ended, or the Stop button was pressed, while the microphone was still
    /// opening: stop as soon as it records.
    stop_when_started: bool,
    /// Ends the "Dictation cancelled" pill after [`CANCELLED_SHOWN`].
    cancelled_shown: Option<Task<()>>,
}

impl Drop for Dictate {
    /// A recording still running when its control goes away (its chat or page window closed) is
    /// handed to gxserver now, so it is in Past recordings and Transcribe previous at once rather
    /// than at the next Dictate. Its file is already on disk either way (`recorder.rs`).
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            let _ = std::thread::Builder::new()
                .name("ghostex-dictation-hand-over".into())
                .spawn(move || {
                    let recording = active.finish();
                    let _ = recorder::upload(&recording, None);
                    refresh_previous();
                });
        }
    }
}

impl EventEmitter<DictateEvent> for Dictate {}

impl Dictate {
    pub(crate) fn new() -> Self {
        Self {
            phase: DictatePhase::Idle,
            active: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            in_flight: Arc::new(Mutex::new(None)),
            kept: Restorable::default(),
            work: None,
            ticker: None,
            elapsed: Duration::ZERO,
            levels: resting_levels(),
            loud_at: std::time::Instant::now(),
            stop_when_started: false,
            cancelled_shown: None,
        }
    }

    pub(crate) fn phase(&self) -> &DictatePhase {
        &self.phase
    }

    /// Recording or about to: what Escape cancels and the Stop glyph shows.
    pub(crate) fn recording(&self) -> bool {
        matches!(self.phase, DictatePhase::Starting | DictatePhase::Recording)
    }

    pub(crate) fn transcribing(&self) -> bool {
        matches!(self.phase, DictatePhase::Transcribing)
    }

    /// How long the current recording has run.
    pub(crate) fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// The microphone's recent levels for the waveform, oldest first.
    pub(crate) fn levels(&self) -> &VecDeque<f32> {
        &self.levels
    }

    /// The recording has heard only near-silence for [`QUIET_FOR`]: the pill says "Can't hear
    /// you" until sound comes back.
    ///
    /// CDXC:Dictate 2026-11-13 DECISION: User (Q2919 "keep", with "add the 3 seconds can't hear you thingy"): the waveform keeps its look in a quiet room, and after 3 seconds of near-silence the pill says "Can't hear you" until sound comes back, on the desktop and the phone (`DictationWaveform.tsx`).
    pub(crate) fn cant_hear(&self) -> bool {
        matches!(self.phase, DictatePhase::Recording) && self.loud_at.elapsed() >= QUIET_FOR
    }

    /// The microphone button: start, or stop and transcribe.
    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        match self.phase {
            DictatePhase::Idle
            | DictatePhase::Failed { .. }
            | DictatePhase::Unavailable { .. }
            | DictatePhase::Notice { .. }
            | DictatePhase::Cancelled { .. } => self.start(cx),
            DictatePhase::Recording => self.stop(cx),
            // The button already shows Stop while the microphone opens (it can take a couple of
            // seconds: the engine check, macOS asking for access), so a press then is a Stop too.
            DictatePhase::Starting => self.stop_when_started = true,
            DictatePhase::Transcribing => {}
        }
    }

    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        if self.recording() || self.transcribing() {
            return;
        }
        self.phase = DictatePhase::Starting;
        self.cancelled_shown = None;
        self.elapsed = Duration::ZERO;
        self.levels = resting_levels();
        self.stop_when_started = false;
        self.reset_run();
        let cancelled = self.cancelled.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            // The engine first: when it can't transcribe, nothing is recorded and the pill says why.
            let started = cx
                .background_executor()
                .spawn(async move {
                    match engine_not_ready() {
                        Some(failure) => Err(Start::Unavailable(failure)),
                        None => ActiveRecording::start().map_err(Start::Microphone),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| match started {
                Err(Start::Unavailable(_)) if cancelled.load(Ordering::SeqCst) => {
                    this.phase = DictatePhase::Idle;
                    cx.notify();
                }
                Err(Start::Unavailable(failure)) => {
                    let message = failure.message.clone();
                    // The phase first, so the chat box knows its card shows this.
                    this.phase = DictatePhase::Unavailable { failure };
                    this.stop_when_started = false;
                    cx.emit(DictateEvent::Failure {
                        title: "Dictation isn't ready".to_string(),
                        message,
                    });
                    cx.notify();
                }
                Ok(active) if cancelled.load(Ordering::SeqCst) => {
                    // Escape before the microphone opened: keep nothing that was never said.
                    let recording = active.finish();
                    keep_dismissed(recording, cx);
                    this.phase = DictatePhase::Idle;
                    cx.notify();
                }
                Ok(active) => {
                    this.active = Some(active);
                    this.loud_at = std::time::Instant::now();
                    this.phase = DictatePhase::Recording;
                    if std::mem::take(&mut this.stop_when_started) {
                        this.stop(cx);
                        return;
                    }
                    this.start_ticker(cx);
                    cx.notify();
                }
                Err(Start::Microphone(error)) => {
                    // The phase first, so the chat box knows its card shows this.
                    this.phase = DictatePhase::Notice {
                        failure: DictateFailure::new("micUnavailable", error.message()),
                    };
                    cx.emit(DictateEvent::Failure {
                        title: "Dictation couldn't start".to_string(),
                        message: error.message(),
                    });
                    cx.notify();
                }
            });
        }));
        // Recordings an earlier run could not hand over go now, off the path of this one.
        cx.background_executor()
            .spawn(async move {
                recorder::send_unsent();
                refresh_previous();
            })
            .detach();
        cx.notify();
    }

    fn start_ticker(&mut self, cx: &mut Context<Self>) {
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this.update(cx, |this, cx| {
                    let Some(active) = this.active.as_ref() else {
                        return false;
                    };
                    this.elapsed = active.elapsed();
                    let level = active.level();
                    this.levels.pop_front();
                    this.levels.push_back(level);
                    if level >= QUIET_LEVEL {
                        this.loud_at = std::time::Instant::now();
                    }
                    if active.take_silent() {
                        // A warning only: a headset with a noise gate is silent too.
                        cx.emit(DictateEvent::Failure {
                            title: "The microphone is silent".to_string(),
                            message: super::audio::SILENT_MICROPHONE.to_string(),
                        });
                    }
                    if let Some(error) = active.take_failure() {
                        // The microphone went away: what was recorded so far is transcribed.
                        cx.emit(DictateEvent::Failure {
                            title: "The microphone stopped".to_string(),
                            message: format!(
                                "{} What was recorded so far is being transcribed.",
                                error.message()
                            ),
                        });
                        this.stop(cx);
                        return false;
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// The hotkey's release: stop a recording, or the one about to start as soon as it does.
    pub(crate) fn stop_recording(&mut self, cx: &mut Context<Self>) {
        match self.phase {
            DictatePhase::Starting => self.stop_when_started = true,
            DictatePhase::Recording => self.stop(cx),
            _ => {}
        }
    }

    /// Stop recording and transcribe what was said.
    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.take() else {
            return;
        };
        self.ticker = None;
        self.phase = DictatePhase::Transcribing;
        let cancelled = self.cancelled.clone();
        let in_flight = self.in_flight.clone();
        let kept = self.kept.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let recording = cx
                .background_executor()
                .spawn(async move { active.finish() })
                .await;
            let outcome = cx
                .background_executor()
                .spawn({
                    let cancelled = cancelled.clone();
                    async move {
                        save_and_transcribe(Kept::Local(recording), &cancelled, &in_flight, &kept)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(outcome, &cancelled, cx));
        }));
        cx.notify();
    }

    /// Escape or ✕: a recording stops and is kept as dismissed; a transcription in flight is
    /// dismissed; a failure row goes away (its recording stays for Transcribe previous).
    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        let mut restorable = None;
        match &self.phase {
            DictatePhase::Idle => return,
            DictatePhase::Starting => {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            DictatePhase::Recording => {
                self.cancelled.store(true, Ordering::SeqCst);
                self.ticker = None;
                if let Some(active) = self.active.take() {
                    let kept = self.kept.clone();
                    cx.background_executor()
                        .spawn(async move {
                            let mut recording = active.finish();
                            recording.dismiss();
                            kept.fill(Some(match recorder::upload(&recording, None) {
                                Ok(dictation) => Kept::Server(
                                    dictation["id"]
                                        .as_str()
                                        .map_or_else(|| recording.id.clone(), str::to_string),
                                ),
                                // Still on this computer; Restore hands it over first.
                                Err(_) => Kept::Local(recording),
                            }));
                            refresh_previous();
                        })
                        .detach();
                    restorable = Some(self.kept.clone());
                }
            }
            DictatePhase::Transcribing => {
                // gxserver keeps it as dismissed even if xAI answers a moment later; work that
                // has not reached gxserver yet sees the flag and dismisses it there itself.
                self.cancelled.store(true, Ordering::SeqCst);
                if let Some(id) = self.in_flight.lock().ok().and_then(|id| id.clone()) {
                    cx.background_executor()
                        .spawn(async move {
                            let _ =
                                recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
                            refresh_previous();
                        })
                        .detach();
                }
                // It finishes on its own and types nothing; it tells `kept` where the recording
                // went.
                if let Some(work) = self.work.take() {
                    work.detach();
                }
                restorable = Some(self.kept.clone());
            }
            DictatePhase::Failed { .. }
            | DictatePhase::Unavailable { .. }
            | DictatePhase::Notice { .. }
            | DictatePhase::Cancelled { .. } => {}
        }
        self.cancelled_shown = None;
        self.phase = match restorable {
            Some(restore) => {
                self.cancelled_shown = Some(cx.spawn({
                    let restore = restore.clone();
                    async move |this, cx| {
                        cx.background_executor().timer(CANCELLED_SHOWN).await;
                        let _ = this.update(cx, |this, cx| {
                            if matches!(&this.phase, DictatePhase::Cancelled { restore: shown } if shown.same(&restore))
                            {
                                this.phase = DictatePhase::Idle;
                                cx.notify();
                            }
                        });
                    }
                }));
                DictatePhase::Cancelled { restore }
            }
            None => DictatePhase::Idle,
        };
        cx.notify();
    }

    /// Restore after a cancel: the cancelled recording, transcribed into the place it was aimed
    /// at (CDXC:Dictate 2026-11-21 on [`DictatePhase::Cancelled`]).
    pub(crate) fn restore(&mut self, cx: &mut Context<Self>) {
        let DictatePhase::Cancelled { restore } = &self.phase else {
            return;
        };
        let restore = restore.clone();
        self.cancelled_shown = None;
        self.phase = DictatePhase::Transcribing;
        self.reset_run();
        let cancelled = self.cancelled.clone();
        let in_flight = self.in_flight.clone();
        let kept = self.kept.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn({
                    let cancelled = cancelled.clone();
                    async move {
                        match restore.wait(RESTORE_WAIT) {
                            Some(recording) => {
                                save_and_transcribe(recording, &cancelled, &in_flight, &kept)
                            }
                            None => {
                                kept.fill(None);
                                Outcome::Error(
                                    "The cancelled recording hasn't reached Ghostex's service yet. It is kept on this computer; find it in Settings > Dictation > Past recordings."
                                        .to_string(),
                                )
                            }
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(outcome, &cancelled, cx));
        }));
        cx.notify();
    }

    /// Transcribe again after a failure.
    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        let DictatePhase::Failed { kept, .. } = &self.phase else {
            return;
        };
        let kept = kept.clone();
        self.transcribe_kept(Some(kept), cx);
    }

    /// The newest recording that has no text yet (cancelled, dismissed or failed), typed in like
    /// a dictation.
    pub(crate) fn transcribe_previous(&mut self, cx: &mut Context<Self>) {
        if self.recording() || self.transcribing() {
            return;
        }
        self.transcribe_kept(None, cx);
    }

    fn transcribe_kept(&mut self, kept: Option<Kept>, cx: &mut Context<Self>) {
        self.phase = DictatePhase::Transcribing;
        self.cancelled_shown = None;
        self.reset_run();
        let cancelled = self.cancelled.clone();
        let in_flight = self.in_flight.clone();
        let slot = self.kept.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn({
                    let cancelled = cancelled.clone();
                    async move {
                        match kept {
                            Some(kept) => save_and_transcribe(kept, &cancelled, &in_flight, &slot),
                            None => transcribe_latest(&cancelled, &in_flight, &slot),
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(outcome, &cancelled, cx));
        }));
        cx.notify();
    }

    /// A fresh cancel flag and in-flight slot for the next run; a detached run keeps its own.
    fn reset_run(&mut self) {
        self.cancelled = Arc::new(AtomicBool::new(false));
        self.in_flight = Arc::new(Mutex::new(None));
        self.kept = Restorable::default();
    }

    fn finish(&mut self, outcome: Outcome, cancelled: &AtomicBool, cx: &mut Context<Self>) {
        if cancelled.load(Ordering::SeqCst) {
            return;
        }
        self.work = None;
        match outcome {
            Outcome::Text(text) => {
                self.phase = DictatePhase::Idle;
                cx.emit(DictateEvent::Text(text));
            }
            Outcome::Nothing(_, message) if message.is_empty() => {
                self.phase = DictatePhase::Idle;
            }
            Outcome::Nothing(code, message) => {
                self.phase = DictatePhase::Notice {
                    failure: DictateFailure::new(code, message.clone()),
                };
                cx.emit(DictateEvent::Failure {
                    title: if code == "nothingHeard" {
                        "No words heard".to_string()
                    } else {
                        "Nothing to transcribe".to_string()
                    },
                    message,
                });
            }
            Outcome::Error(message) => {
                self.phase = DictatePhase::Idle;
                cx.emit(DictateEvent::Failure {
                    title: "Transcription failed".to_string(),
                    message,
                });
            }
            Outcome::Failed(kept, failure) => {
                let message = failure.message.clone();
                // The phase first, so the chat box knows its card shows this.
                self.phase = DictatePhase::Failed { kept, failure };
                cx.emit(DictateEvent::Failure {
                    title: "Transcription failed".to_string(),
                    message: format!("{message} Your recording is kept."),
                });
            }
        }
        cx.notify();
    }
}

/// Why a press did not start recording.
enum Start {
    Unavailable(DictateFailure),
    Microphone(super::audio::AudioStartError),
}

fn resting_levels() -> VecDeque<f32> {
    std::iter::repeat_n(0.0, WAVEFORM_BARS).collect()
}

/// Asks gxserver whether its engine can transcribe now, with `prepare` so a closed Hex opens
/// while the user speaks (blocking, short). `Some` with why when it can't; `None` when it can,
/// or when gxserver didn't answer in time (the recording is kept and transcribed later).
///
/// CDXC:Dictate 2026-11-13 DECISION: User: "Don't let me dictate all of this and then fail to send: tell me Hex isn't set up on the computer, or grey out the button." The microphone is greyed while the engine can't transcribe (`toolbar.rs`), and a press asks again first: an engine that still can't shows the card with what to do instead of recording.
fn engine_not_ready() -> Option<DictateFailure> {
    let engine = recorder::rpc_waiting(
        "readDictateEngine",
        json!({"prepare": true}),
        ENGINE_CHECK_TIMEOUT,
        Duration::ZERO,
    )
    .ok()?;
    if let Ok(mut slot) = ENGINE.lock() {
        *slot = Some(engine.clone());
    }
    if engine["ready"].as_bool() != Some(false) {
        return None;
    }
    let problem = &engine["problem"];
    Some(DictateFailure::new(
        problem["code"].as_str().unwrap_or("notReady"),
        problem["message"]
            .as_str()
            .unwrap_or("Dictation on this computer needs setting up first.")
            .to_string(),
    ))
}

enum Outcome {
    Text(String),
    /// Nothing was waiting (Transcribe previous) or the recording had no words.
    /// Its code (`nothingHeard`, `nothingWaiting`) and sentence; an empty sentence is a run the
    /// reader cancelled, which says nothing.
    Nothing(&'static str, String),
    /// Transcribe previous could not reach gxserver; nothing changed.
    Error(String),
    Failed(Kept, DictateFailure),
}

/// Hands a recording to gxserver if only this app has it, then transcribes it (blocking). When the
/// reader cancelled meanwhile, gxserver keeps it as dismissed instead.
fn save_and_transcribe(
    kept: Kept,
    cancelled: &AtomicBool,
    in_flight: &Mutex<Option<String>>,
    slot: &Restorable,
) -> Outcome {
    let id = match kept {
        Kept::Server(id) => id,
        Kept::Local(mut recording) => {
            if cancelled.load(Ordering::SeqCst) {
                recording.dismiss();
            }
            match recorder::upload(&recording, None) {
                Ok(dictation) => match dictation["id"].as_str() {
                    Some(id) => id.to_string(),
                    None => recording.id.clone(),
                },
                Err(error) => {
                    slot.fill(Some(Kept::Local(recording.clone())));
                    return Outcome::Failed(
                        Kept::Local(recording),
                        DictateFailure::new(
                            "notSaved",
                            format!(
                                "Ghostex's service couldn't take the recording ({}). It is kept on this computer; transcribe again.",
                                error.trim_end_matches('.')
                            ),
                        ),
                    );
                }
            }
        }
    };
    if let Ok(mut slot) = in_flight.lock() {
        *slot = Some(id.clone());
    }
    slot.fill(Some(Kept::Server(id.clone())));
    if cancelled.load(Ordering::SeqCst) {
        let _ = recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
        refresh_previous();
        return Outcome::Nothing("nothingWaiting", String::new());
    }
    let answer = recorder::rpc("transcribeDictation", json!({"id": id}), TRANSCRIBE_TIMEOUT);
    if cancelled.load(Ordering::SeqCst) {
        let _ = recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
    }
    let outcome = outcome_of(answer, Kept::Server(id));
    refresh_previous();
    outcome
}

fn transcribe_latest(
    cancelled: &AtomicBool,
    in_flight: &Mutex<Option<String>>,
    slot: &Restorable,
) -> Outcome {
    recorder::send_unsent();
    // Named before the call, so a cancel while xAI works can dismiss it (and Restore take it).
    if let Some((id, _, _)) = previous_recording() {
        if let Ok(mut in_flight) = in_flight.lock() {
            *in_flight = Some(id.clone());
        }
        slot.fill(Some(Kept::Server(id)));
    }
    let answer = recorder::rpc(
        "transcribeDictation",
        json!({"latest": true}),
        TRANSCRIBE_TIMEOUT,
    );
    if let (true, Ok(answer)) = (cancelled.load(Ordering::SeqCst), &answer)
        && let Some(id) = answer["dictation"]["id"].as_str()
    {
        let _ = recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
    }
    let outcome = match &answer {
        Ok(answer) => outcome_of(
            Ok(answer.clone()),
            Kept::Server(
                answer["dictation"]["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
        Err(message) if message.contains("no recording waiting") => {
            Outcome::Nothing("nothingWaiting", message.clone())
        }
        Err(message) => Outcome::Error(message.clone()),
    };
    if !slot.filled() {
        slot.fill(
            answer
                .ok()
                .and_then(|answer| answer["dictation"]["id"].as_str().map(str::to_string))
                .map(Kept::Server),
        );
    }
    refresh_previous();
    outcome
}

fn outcome_of(answer: Result<Value, String>, kept: Kept) -> Outcome {
    let dictation = match answer {
        Ok(answer) => answer["dictation"].clone(),
        Err(message) => {
            return Outcome::Failed(kept, DictateFailure::new("serviceUnavailable", message));
        }
    };
    match dictation["status"].as_str() {
        Some("transcribed") => {
            let text = dictation["text"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string();
            if text.is_empty() {
                Outcome::Nothing(
                    "nothingHeard",
                    "No words were heard in the recording. It is kept in Settings > Dictation > Past recordings."
                        .to_string(),
                )
            } else {
                Outcome::Text(text)
            }
        }
        _ => Outcome::Failed(kept, DictateFailure::from_problem(&dictation["error"])),
    }
}

/// A recording cancelled before it really began: kept as dismissed like any other.
fn keep_dismissed(mut recording: LocalRecording, cx: &mut Context<Dictate>) {
    recording.dismiss();
    cx.background_executor()
        .spawn(async move {
            let _ = recorder::upload(&recording, None);
            refresh_previous();
        })
        .detach();
}

/// Hands gxserver the recordings an earlier run of this app left on this computer (it quit or
/// crashed while recording or uploading), so they are in Past recordings and Transcribe previous
/// from the start rather than after the next Dictate (blocking).
pub(crate) fn hand_over_unsent() {
    if recorder::send_unsent() > 0 {
        refresh_previous();
    }
}

/// The newest recording without text, as the last refresh found it (for the microphone's hover
/// menu): `(id, durationMs, createdAt)`.
static PREVIOUS: Mutex<Option<(String, u64, i64)>> = Mutex::new(None);

pub(crate) fn previous_recording() -> Option<(String, u64, i64)> {
    PREVIOUS.lock().ok().and_then(|previous| previous.clone())
}

/// Asks gxserver which recording Transcribe previous would take (blocking).
pub(crate) fn refresh_previous() {
    let latest = recorder::rpc("listDictations", json!({"limit": 0}), LIST_TIMEOUT)
        .ok()
        .and_then(|answer| {
            let latest = &answer["latestUntranscribed"];
            Some((
                latest["id"].as_str()?.to_string(),
                latest["durationMs"].as_u64().unwrap_or(0),
                latest["createdAt"].as_i64().unwrap_or(0),
            ))
        });
    if let Ok(mut previous) = PREVIOUS.lock() {
        *previous = latest;
    }
}

/// What will transcribe (`/api/readDictateEngine`), as the last refresh found it: the
/// microphone's tooltip reads it.
static ENGINE: Mutex<Option<Value>> = Mutex::new(None);

pub(crate) fn dictate_engine() -> Option<Value> {
    ENGINE.lock().ok().and_then(|engine| engine.clone())
}

/// Asks gxserver what will transcribe (blocking, cheap: no disk scan).
pub(crate) fn refresh_engine() {
    if let Ok(engine) = recorder::rpc("readDictateEngine", json!({}), LIST_TIMEOUT)
        && let Ok(mut slot) = ENGINE.lock()
    {
        *slot = Some(engine);
    }
}

/// `m:ss` for a recording's length.
pub(crate) fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_waits_for_the_cancelled_recording_to_reach_gxserver() {
        let restorable = Restorable::default();
        let filler = restorable.clone();
        let hand_over = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            filler.fill(Some(Kept::Server("rec-1".to_string())));
        });
        match restorable.wait(Duration::from_secs(5)) {
            Some(Kept::Server(id)) => assert_eq!(id, "rec-1"),
            other => panic!("expected the recording gxserver took, got {other:?}"),
        }
        hand_over.join().unwrap();
    }

    #[test]
    fn restore_gives_up_when_the_hand_over_never_ends() {
        let restorable = Restorable::default();
        assert!(restorable.wait(Duration::from_millis(20)).is_none());
        assert!(!restorable.filled());
    }

    #[test]
    fn a_run_with_nothing_to_restore_answers_at_once() {
        let restorable = Restorable::default();
        restorable.fill(None);
        assert!(restorable.filled());
        let started = std::time::Instant::now();
        assert!(restorable.wait(Duration::from_secs(5)).is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_recording_gxserver_could_not_take_is_restored_from_this_computer() {
        let restorable = Restorable::default();
        let recording = LocalRecording {
            id: "rec-2".to_string(),
            path: std::path::PathBuf::from("/nonexistent/rec-2.wav"),
            created_at: 1,
            duration_ms: 60_000,
            dismissed: true,
        };
        restorable.fill(Some(Kept::Local(recording)));
        match restorable.wait(Duration::from_secs(1)) {
            Some(Kept::Local(recording)) => assert_eq!(recording.id, "rec-2"),
            other => panic!("expected the local recording, got {other:?}"),
        }
        assert!(restorable.same(&restorable.clone()));
        assert!(!restorable.same(&Restorable::default()));
    }
}
