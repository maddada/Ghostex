//! The microphone and speaker of a voice conversation and of Grok dictation: 24 kHz mono PCM16
//! both ways, the format gxserver relays to the provider unchanged.
//!
//! One shared front for every system; the platform part is a `VoiceAudioBackend`: macOS's
//! AVAudioEngine (`audio_mac.rs`) and cpal on Windows and Linux (`audio_cpal.rs`, with the echo
//! canceller in `aec.rs` and the speed in `stretch.rs`). The developer test microphone
//! (`test_mic.rs`) replaces what any backend hears.

use std::sync::Mutex;

use super::audio_backend::VoiceAudioBackend;
use super::test_mic::TestMic;

/// Receives captured microphone audio on the audio thread. It must only queue it and return.
pub(crate) type CaptureFn = Box<dyn Fn(&[i16]) + Send + Sync>;
/// Called on an audio thread when the input or output device changed and the engine stopped; the
/// owner starts the audio again.
pub(crate) type DeviceChangedFn = Box<dyn Fn() + Send + Sync>;
/// Called once, on an audio thread, when the microphone gave nothing but digital silence for its
/// first seconds (muted, or blocked by the system). A warning: the audio keeps running, since a
/// headset with a noise gate can be silent too.
pub(crate) type SilentFn = Box<dyn Fn() + Send + Sync>;

/// Why the audio did not start.
#[derive(Clone, Debug)]
pub(crate) enum AudioStartError {
    PermissionDenied,
    NoInput,
    #[cfg_attr(
        any(target_os = "macos", target_os = "linux", target_os = "windows"),
        allow(dead_code)
    )]
    Unsupported,
    Failed(String),
}

/// Where to allow the microphone when the system refused it.
const PERMISSION_HINT: &str = if cfg!(target_os = "windows") {
    "Ghostex can't use the microphone. Turn on Microphone access and Let desktop apps access your microphone in Windows Settings > Privacy & security > Microphone."
} else if cfg!(target_os = "linux") {
    "Ghostex can't use the microphone. Check that this user may use the sound devices (a desktop login does this; otherwise add it to the audio group)."
} else {
    "Ghostex can't use the microphone. Allow Ghostex under Microphone in System Settings > Privacy & Security."
};

/// What to check when the microphone gives only silence.
pub(crate) const SILENT_MICROPHONE: &str = if cfg!(target_os = "windows") {
    "Ghostex hears only silence from the microphone. Check that it isn't muted, and that Microphone access and Let desktop apps access your microphone are on in Windows Settings > Privacy & security > Microphone."
} else {
    "Ghostex hears only silence from the microphone. Check that it isn't muted and that the right input device is chosen in your sound settings."
};

/// What the user sees when the audio stopped after the sound devices changed too many times.
pub(crate) const MICROPHONE_KEPT_STOPPING: &str =
    "The microphone kept stopping. Check your sound devices, then try again.";

impl AudioStartError {
    /// A sentence for the user, naming the setting to change when there is one.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::PermissionDenied => PERMISSION_HINT.to_string(),
            Self::NoInput => "No microphone was found. Connect one, or choose an input device in your sound settings.".to_string(),
            Self::Unsupported => "Voice is not available on this computer yet.".to_string(),
            Self::Failed(message) => message.clone(),
        }
    }
}

/// A running microphone (and, for a conversation, speaker). Dropping it stops both.
pub(crate) struct VoiceAudio {
    backend: Box<dyn VoiceAudioBackend>,
}

impl VoiceAudio {
    /// Opens the microphone (with echo cancellation when `voice_processing`) and, when `playback`,
    /// the speaker. Blocks while macOS asks for microphone access the first time, so call it off
    /// the main thread.
    pub(crate) fn start(
        voice_processing: bool,
        playback: bool,
        capture: CaptureFn,
        device_changed: DeviceChangedFn,
        silent: SilentFn,
    ) -> Result<Self, AudioStartError> {
        Self::start_with_microphone(
            voice_processing,
            playback,
            None,
            capture,
            device_changed,
            silent,
        )
    }

    /// [`Self::start`] recording from the input device named `microphone` (Settings › Voice's
    /// picker); the system default when `None`, or when that device is not connected.
    pub(crate) fn start_with_microphone(
        voice_processing: bool,
        playback: bool,
        microphone: Option<String>,
        capture: CaptureFn,
        device_changed: DeviceChangedFn,
        silent: SilentFn,
    ) -> Result<Self, AudioStartError> {
        let capture = with_test_mic(capture);
        #[cfg(target_os = "macos")]
        let backend: Box<dyn VoiceAudioBackend> = {
            // AVAudioEngine reports a refused microphone at the start, so the Mac never warns.
            drop(silent);
            Box::new(super::audio_mac::MacVoiceAudio::start(
                voice_processing,
                playback,
                microphone,
                capture,
                device_changed,
            )?)
        };
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let backend: Box<dyn VoiceAudioBackend> =
            Box::new(super::audio_cpal::CpalVoiceAudio::start(
                voice_processing,
                playback,
                microphone,
                capture,
                device_changed,
                silent,
            )?);
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let backend: Box<dyn VoiceAudioBackend> = {
            let _ = (voice_processing, playback, microphone, capture, device_changed, silent);
            return Err(AudioStartError::Unsupported);
        };
        Ok(Self { backend })
    }

    /// Queues agent audio (PCM16 24 kHz mono, little-endian bytes) after what is playing.
    pub(crate) fn enqueue(&self, pcm: &[u8]) {
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        self.backend.enqueue(&samples);
    }

    /// Plays the agent's audio `rate` times as fast, pitch kept (the panel's speed buttons).
    pub(crate) fn set_rate(&self, rate: f32) {
        self.backend.set_rate(rate);
    }

    /// Stops the agent's audio at once and drops what was queued. Answers how many samples were
    /// dropped, in the provider's samples whatever the playback rate.
    pub(crate) fn flush(&self) -> u64 {
        self.backend.flush()
    }
}

/// The capture callback, hearing the developer test microphone instead when it is switched on.
fn with_test_mic(capture: CaptureFn) -> CaptureFn {
    let Some(test_mic) = TestMic::from_env() else {
        return capture;
    };
    let test_mic = Mutex::new(test_mic);
    Box::new(move |samples| {
        let mut replaced = vec![0i16; samples.len()];
        if let Ok(mut test_mic) = test_mic.lock() {
            test_mic.fill(&mut replaced);
        }
        capture(&replaced);
    })
}
