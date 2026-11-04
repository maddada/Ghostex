//! The web build's stand-in for the desktop's `app/voice_agent/`: the names the shared chat files
//! call, with no microphone behind them.
//!
//! CDXC:VoiceMode 2026-10-20 WHY: the page has no echo-cancelled microphone path yet (the contract's `AudioWorklet` capture is not built), so it draws neither the Voice button (its header is its own) nor Dictate (`DICTATION` is desktop only); `native_chat/dictation.rs` still names the Dictate state it would record with.

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

/// `?dictatePreview=recording|transcribing|failed` draws the Dictate button on the page, and
/// pressing it shows that state, so the chat box's Dictate pill can be looked at in a browser. No
/// microphone is opened; ✕ (or pressing the button again) goes back to idle. `failed:<code>`
/// shows the failure card for one of gxserver's codes (`failed:noKey`, `failed:providerUnavailable`,
/// ...), worded as gxserver words it (`server/src/voice_agent/transcribe.rs`).
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
        _ => value.strip_prefix("failed:").and_then(preview_failure),
    }
}

fn preview_failure(code: &str) -> Option<DictatePhase> {
    let (provider, message, detail) = match code {
        "noKey" => (
            None,
            "Dictate needs an xAI or OpenAI API key, and neither is set. Add one in Settings > Extensions > Voice agent, then transcribe again.",
            None,
        ),
        "badKey" => (
            Some("xai"),
            "xAI refused the API key. Check it in Settings > Extensions > Voice agent, then transcribe again.",
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
}

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
