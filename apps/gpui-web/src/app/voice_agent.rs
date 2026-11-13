//! The web build's stand-in for the desktop's `app/voice_agent/`: the names the shared chat files
//! call, with no microphone behind them.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the page has no echo-cancelled microphone path yet (the contract's `AudioWorklet` capture is not built), so it draws neither the Voice button (its header is its own) nor Dictate (`DICTATION` is desktop only); `native_chat/dictation.rs` still names the Dictate state it would record with.

use std::collections::VecDeque;
use std::time::Duration;

use gpui::{Context, EventEmitter};

/// Dictate's phases, as the desktop's `voice_agent::DictatePhase` names them.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum DictatePhase {
    Idle,
    Starting,
    Recording,
    Transcribing,
    Failed { kept: (), failure: DictateFailure },
    Unavailable { failure: DictateFailure },
}

/// The desktop's `voice_agent::dictation::WAVEFORM_BARS`.
pub(crate) const WAVEFORM_BARS: usize = 24;

/// A spoken phrase's levels for the preview's waveform (no microphone here): syllables rising and
/// falling with a short pause, so the pill can be looked at as it reads while someone speaks.
fn preview_levels() -> VecDeque<f32> {
    [
        0.05, 0.08, 0.35, 0.62, 0.48, 0.71, 0.55, 0.3, 0.12, 0.05, 0.04, 0.22, 0.58, 0.83, 0.66,
        0.42, 0.6, 0.74, 0.5, 0.26, 0.4, 0.64, 0.45, 0.2,
    ]
    .into_iter()
    .take(WAVEFORM_BARS)
    .collect()
}

/// Why a transcription failed, as the desktop's `voice_agent::DictateFailure` names it.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DictateFailure {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) provider: Option<String>,
    pub(crate) detail: Option<String>,
}

#[allow(dead_code)]
pub(crate) enum DictateEvent {
    Text(String),
    Failure { title: String, message: String },
}

/// `?dictatePreview=recording|transcribing|failed|unavailable` draws the Dictate button on the page, and
/// pressing it shows that state, so the chat box's Dictate pill can be looked at in a browser. No
/// microphone is opened; ✕ (or pressing the button again) goes back to idle. `failed:<code>`
/// shows the failure card for one of gxserver's codes (`failed:noKey`, `failed:providerUnavailable`,
/// ...), worded as gxserver words it (`server/src/voice_agent/transcribe.rs`);
/// `unavailable:<code>` the card a press shows while the engine can't transcribe (the microphone
/// greyed meanwhile, `dictation_unready`).
pub(crate) fn dictation_preview() -> bool {
    preview_phase().is_some()
}

fn preview_phase() -> Option<DictatePhase> {
    let search = web_sys::window().and_then(|window| window.location().search().ok())?;
    let value = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("dictatePreview="))?;
    match value {
        "recording" => Some(DictatePhase::Recording),
        "transcribing" => Some(DictatePhase::Transcribing),
        "failed" => preview_failure("paymentRequired"),
        "unavailable" => preview_unavailable("hexNotInstalled"),
        _ => value
            .strip_prefix("failed:")
            .and_then(preview_failure)
            .or_else(|| {
                value
                    .strip_prefix("unavailable:")
                    .and_then(preview_unavailable)
            }),
    }
}

fn preview_unavailable(code: &str) -> Option<DictatePhase> {
    match preview_failure(code)? {
        DictatePhase::Failed { failure, .. } => Some(DictatePhase::Unavailable { failure }),
        _ => None,
    }
}

/// The engine the preview reports: not ready under `unavailable`, so the microphone shows greyed.
fn preview_engine() -> Option<serde_json::Value> {
    match preview_phase()? {
        DictatePhase::Unavailable { failure } => Some(serde_json::json!({
            "engine": "hex", "label": "Hex", "ready": false,
            "problem": {"code": failure.code, "message": failure.message},
        })),
        _ => None,
    }
}

fn preview_failure(code: &str) -> Option<DictatePhase> {
    let (provider, message, detail) = match code {
        "noKey" => (
            None,
            "Dictate needs an xAI or OpenAI API key, and neither is set. Add one in Settings > Dictation, then transcribe again.",
            None,
        ),
        "badKey" => (
            Some("xai"),
            "xAI refused the API key. Check it in Settings > Dictation, then transcribe again.",
            Some("Incorrect API key provided."),
        ),
        "providerUnavailable" => (
            Some("xai"),
            "Couldn't reach xAI. Check this computer's internet connection, then transcribe again.",
            Some("io: dns error: failed to lookup address information"),
        ),
        "rateLimited" => (
            Some("openai"),
            "OpenAI is limiting requests right now. Wait a moment, then transcribe again.",
            Some("Rate limit reached for gpt-4o-transcribe."),
        ),
        "paymentRequired" => (
            Some("xai"),
            "xAI didn't accept the request (payment required). Check your credit or plan usage at console.x.ai, then transcribe again.",
            Some("xAI answered HTTP 402."),
        ),
        "badAudio" => (
            Some("openai"),
            "OpenAI couldn't read the recording. Transcribe again; if it keeps failing, record it again.",
            Some("Audio file might be corrupted or unsupported."),
        ),
        "audioMissing" => (
            None,
            "The recording's audio file is gone, so it can't be transcribed.",
            None,
        ),
        "hexNotInstalled" => (
            Some("hex"),
            "Hex 3 isn't installed on this computer. Install it to transcribe with Hex.",
            None,
        ),
        "hexTimeout" => (
            Some("hex"),
            "Hex didn't answer within 112 seconds. Check that Hex is working (try dictating in Hex itself), then transcribe again.",
            None,
        ),
        _ => return None,
    };
    Some(DictatePhase::Failed {
        kept: (),
        failure: DictateFailure {
            code: code.to_string(),
            message: message.to_string(),
            provider: provider.map(str::to_string),
            detail: detail.map(str::to_string),
        },
    })
}

/// A Dictate that never records; under `?dictatePreview=` it shows the asked-for state.
pub(crate) struct Dictate {
    phase: DictatePhase,
}

impl EventEmitter<DictateEvent> for Dictate {}

#[allow(dead_code)]
impl Dictate {
    pub(crate) fn new() -> Self {
        Self {
            phase: DictatePhase::Idle,
        }
    }

    pub(crate) fn phase(&self) -> &DictatePhase {
        &self.phase
    }

    pub(crate) fn recording(&self) -> bool {
        matches!(self.phase, DictatePhase::Recording)
    }

    pub(crate) fn transcribing(&self) -> bool {
        matches!(self.phase, DictatePhase::Transcribing)
    }

    pub(crate) fn elapsed(&self) -> Duration {
        Duration::from_secs(7)
    }

    pub(crate) fn levels(&self) -> VecDeque<f32> {
        preview_levels()
    }

    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        self.phase = match self.phase {
            DictatePhase::Idle => preview_phase().unwrap_or(DictatePhase::Idle),
            _ => DictatePhase::Idle,
        };
        cx.notify();
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.phase = DictatePhase::Idle;
        cx.notify();
    }

    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        self.phase = DictatePhase::Transcribing;
        cx.notify();
    }

    pub(crate) fn transcribe_previous(&mut self, _cx: &mut Context<Self>) {}

    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase, DictatePhase::Idle) {
            self.toggle(cx);
        }
    }

    pub(crate) fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording() {
            self.toggle(cx);
        }
    }
}

/// What will transcribe: the page asks no gxserver for it, so the tooltip says only "Dictate"
/// (and the microphone greys under `?dictatePreview=unavailable`).
pub(crate) fn dictate_engine() -> Option<serde_json::Value> {
    preview_engine()
}

pub(crate) fn refresh_engine() {}

pub(crate) fn previous_recording() -> Option<(String, u64, i64)> {
    None
}

pub(crate) fn refresh_previous() {}

pub(crate) fn dictation_clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl crate::GhostexGpuiApp {
    /// The desktop's headset on the sidebar row a voice call is about; the page holds no calls.
    pub(crate) fn render_voice_call_badge(
        &self,
        _sidebar_session_id: &str,
        _appearance: &crate::app::native_sidebar::appearance::SidebarAppearance,
    ) -> Option<gpui::AnyElement> {
        None
    }
}
