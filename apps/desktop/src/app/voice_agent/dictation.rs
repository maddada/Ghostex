//! Dictate for the chat box and the terminal bar: record first (`recorder.rs`, a WAV file this app
//! keeps until gxserver has it), transcribe second (gxserver's `/api/transcribeDictation`, xAI's or
//! OpenAI's speech-to-text with the key kept on this computer). The two steps are independent: whatever goes
//! wrong while transcribing, the recording stays and Transcribe again or Transcribe previous sends
//! it once more.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "i want to be able to do escape to cancel the voice recording, and we need to show "Transcribing (X)" so user can click on x to cancel the recording but even if he cancels it we need to save so user can hover over the voice button and click "Transcribe previous"". Escape (in the chat box) cancels a recording; after Stop the chat box shows Transcribing with an ✕ that cancels; a cancelled recording is kept as dismissed and Transcribe previous transcribes the newest recording that has no text. A failure says why and offers Transcribe again. The words go into the draft (or the terminal) and are never sent (CDXC:SessionChat 2026-10-08 DECISION). Supersedes the 2026-10-20 streaming dictation through `/api/voice?mode=dictation`, which lost what was said whenever xAI refused or the connection dropped.
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: server/src/voice_agent/recordings.rs (the store and the user's decision on keeping every recording), the phone's `src/chat/native/composer/useDictation.ts`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{Context, EventEmitter, Task};
use serde_json::{Value, json};

use super::recorder::{self, ActiveRecording, LocalRecording};

/// xAI usually answers within a few seconds; a long recording gets longer.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(180);
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the recording row's clock and the microphone health are looked at.
const TICK: Duration = Duration::from_millis(250);

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
    work: Option<Task<()>>,
    ticker: Option<Task<()>>,
    elapsed: Duration,
}

impl EventEmitter<DictateEvent> for Dictate {}

impl Dictate {
    pub(crate) fn new() -> Self {
        Self {
            phase: DictatePhase::Idle,
            active: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            in_flight: Arc::new(Mutex::new(None)),
            work: None,
            ticker: None,
            elapsed: Duration::ZERO,
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

    /// The microphone button: start, or stop and transcribe.
    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        match self.phase {
            DictatePhase::Idle | DictatePhase::Failed { .. } => self.start(cx),
            DictatePhase::Recording => self.stop(cx),
            DictatePhase::Starting | DictatePhase::Transcribing => {}
        }
    }

    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        if self.recording() || self.transcribing() {
            return;
        }
        self.phase = DictatePhase::Starting;
        self.elapsed = Duration::ZERO;
        self.reset_run();
        let cancelled = self.cancelled.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move { ActiveRecording::start() })
                .await;
            let _ = this.update(cx, |this, cx| match started {
                Ok(active) if cancelled.load(Ordering::SeqCst) => {
                    // Escape before the microphone opened: keep nothing that was never said.
                    let recording = active.finish();
                    keep_dismissed(recording, cx);
                    this.phase = DictatePhase::Idle;
                    cx.notify();
                }
                Ok(active) => {
                    this.active = Some(active);
                    this.phase = DictatePhase::Recording;
                    this.start_ticker(cx);
                    cx.notify();
                }
                Err(error) => {
                    this.phase = DictatePhase::Idle;
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

    /// Stop recording and transcribe what was said.
    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.take() else {
            return;
        };
        self.ticker = None;
        self.phase = DictatePhase::Transcribing;
        let cancelled = self.cancelled.clone();
        let in_flight = self.in_flight.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let recording = cx
                .background_executor()
                .spawn(async move { active.finish() })
                .await;
            let outcome =
                cx.background_executor()
                    .spawn({
                        let cancelled = cancelled.clone();
                        async move {
                            save_and_transcribe(Kept::Local(recording), &cancelled, &in_flight)
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
        match &self.phase {
            DictatePhase::Idle => return,
            DictatePhase::Starting => {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            DictatePhase::Recording => {
                self.cancelled.store(true, Ordering::SeqCst);
                self.ticker = None;
                if let Some(active) = self.active.take() {
                    cx.background_executor()
                        .spawn(async move {
                            let mut recording = active.finish();
                            recording.dismiss();
                            let _ = recorder::upload(&recording, None);
                            refresh_previous();
                        })
                        .detach();
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
                // It finishes on its own and types nothing.
                if let Some(work) = self.work.take() {
                    work.detach();
                }
            }
            DictatePhase::Failed { .. } => {}
        }
        self.phase = DictatePhase::Idle;
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
        self.reset_run();
        let cancelled = self.cancelled.clone();
        let in_flight = self.in_flight.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn({
                    let cancelled = cancelled.clone();
                    async move {
                        match kept {
                            Some(kept) => save_and_transcribe(kept, &cancelled, &in_flight),
                            None => transcribe_latest(&cancelled, &in_flight),
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
            Outcome::Nothing(message) => {
                self.phase = DictatePhase::Idle;
                cx.emit(DictateEvent::Failure {
                    title: "Nothing to transcribe".to_string(),
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

enum Outcome {
    Text(String),
    /// Nothing was waiting (Transcribe previous) or the recording had no words.
    Nothing(String),
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
    if cancelled.load(Ordering::SeqCst) {
        let _ = recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
        refresh_previous();
        return Outcome::Nothing(String::new());
    }
    let answer = recorder::rpc("transcribeDictation", json!({"id": id}), TRANSCRIBE_TIMEOUT);
    if cancelled.load(Ordering::SeqCst) {
        let _ = recorder::rpc("dismissDictation", json!({"id": id}), LIST_TIMEOUT);
    }
    let outcome = outcome_of(answer, Kept::Server(id));
    refresh_previous();
    outcome
}

fn transcribe_latest(cancelled: &AtomicBool, in_flight: &Mutex<Option<String>>) -> Outcome {
    recorder::send_unsent();
    // Named before the call, so a cancel while xAI works can dismiss it.
    if let Some((id, _, _)) = previous_recording()
        && let Ok(mut slot) = in_flight.lock()
    {
        *slot = Some(id);
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
            Outcome::Nothing(message.clone())
        }
        Err(message) => Outcome::Error(message.clone()),
    };
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
                    "No words were heard in the recording. It is kept in Settings > Extensions > Voice agent > Dictation history."
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

/// `m:ss` for a recording's length.
pub(crate) fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
