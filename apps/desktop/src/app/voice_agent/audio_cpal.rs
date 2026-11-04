//! The microphone and speaker of a voice conversation and of dictation on Windows and Linux, through
//! cpal (WASAPI on Windows; ALSA on Linux, which reaches PipeWire or PulseAudio through their ALSA
//! plugins as the "default" device). The Mac has its own AVAudioEngine bridge.
//!
//! Capture is resampled from the device's rate to the provider's 24 kHz mono PCM16. For a
//! conversation the speaker plays the agent's audio from the same object: it is time-stretched for
//! the speed buttons (`stretch.rs`), handed to the echo canceller as the reference of what is about
//! to be heard (`aec.rs`), resampled to the device's rate and copied to every output channel. The
//! echo canceller runs on the capture thread: the output callback queues the reference, and each
//! capture callback first feeds the queued reference, then cleans the microphone audio.
//!
//! cpal streams cannot move between threads, so they are built on, and owned by, one thread of
//! their own that keeps them until this object is dropped.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream,
    SupportedStreamConfig, SupportedStreamConfigRange,
};
use sonora_common_audio::push_resampler::PushResampler;

use super::aec::{self, EchoCanceller};
use super::audio::{AudioStartError, CaptureFn, DeviceChangedFn, SilentFn};
use super::audio_backend::VoiceAudioBackend;
use super::stretch::Stretcher;

/// Device rates tried in order; the resampler needs a whole number of samples per 10 ms.
const PREFERRED_RATES: [u32; 6] = [48_000, 24_000, 44_100, 32_000, 16_000, 96_000];
/// Sample formats the callbacks convert, best first.
const FORMATS: [SampleFormat; 4] = [
    SampleFormat::F32,
    SampleFormat::I16,
    SampleFormat::I32,
    SampleFormat::U16,
];
/// The most reference audio kept for the echo canceller when capture falls behind (1 s).
const MAX_REFERENCE: usize = aec::RATE as usize;
/// How often the default input and output devices are compared, to follow a headset being
/// plugged in or a new default chosen in the sound settings.
const DEVICE_POLL: Duration = Duration::from_secs(2);
/// How often the watcher looks at the streams.
const WATCH_TICK: Duration = Duration::from_millis(500);
/// A stream whose callback has not run for this long has stopped (a driver or sound server
/// dropped it without saying so); the audio is started again as for a device change.
const STALLED: Duration = Duration::from_secs(3);
/// Digital silence for this long from the start means a muted or blocked microphone.
const SILENT_SECONDS: usize = 5;
/// How long building the streams may take before start gives up.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// The agent's audio on its way to the speaker.
struct Playback {
    stretch: Stretcher,
    /// Resampled audio at the device's rate, not yet handed to the device.
    device: VecDeque<f32>,
    device_rate: u32,
    resampler: PushResampler<f32>,
    frame: Vec<f32>,
    resampled: Vec<f32>,
}

impl Playback {
    fn new(device_rate: u32) -> Self {
        let device_frame = (device_rate / 100) as usize;
        Self {
            stretch: Stretcher::new(),
            device: VecDeque::with_capacity(device_frame * 4),
            device_rate,
            resampler: PushResampler::new(aec::FRAME, device_frame, 1),
            frame: vec![0.0; aec::FRAME],
            resampled: vec![0.0; device_frame],
        }
    }

    /// Prepares the next 10 ms for the device: the agent's audio, or silence.
    fn render_frame(&mut self, reference: Option<&Mutex<VecDeque<f32>>>) {
        let written = self.stretch.pull(&mut self.frame);
        self.frame[written..].fill(0.0);
        if let Some(reference) = reference
            && let Ok(mut reference) = reference.lock()
        {
            reference.extend(self.frame.iter().copied());
            let excess = reference.len().saturating_sub(MAX_REFERENCE);
            reference.drain(..excess);
        }
        self.resampler
            .resample_mono(&self.frame, &mut self.resampled);
        self.device.extend(self.resampled.iter().copied());
    }
}

struct Shared {
    playback: Option<Mutex<Playback>>,
    /// What the speaker is about to play, for the echo canceller (24 kHz).
    reference: Option<Mutex<VecDeque<f32>>>,
    device_changed: DeviceChangedFn,
    /// The device change was reported; it is reported once.
    changed: AtomicBool,
    silent: SilentFn,
    started: Instant,
    /// When each callback last ran, in milliseconds after `started`.
    last_capture: AtomicU64,
    last_output: AtomicU64,
}

impl Shared {
    fn report_change(&self) {
        if !self.changed.swap(true, Ordering::SeqCst) {
            (self.device_changed)();
        }
    }

    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Whether a callback that last ran at `last` has stopped.
    fn stalled(&self, last: &AtomicU64) -> bool {
        self.now().saturating_sub(last.load(Ordering::Relaxed)) > STALLED.as_millis() as u64
    }
}

/// The microphone's side: device audio in, cleaned 24 kHz PCM16 out.
struct Capture {
    channels: usize,
    device_frame: usize,
    pending: Vec<f32>,
    resampler: PushResampler<f32>,
    frame: Vec<f32>,
    echo: Option<EchoCanceller>,
    reference: Vec<f32>,
    cleaned: Vec<f32>,
    pcm: Vec<i16>,
    capture: CaptureFn,
    shared: Arc<Shared>,
    /// Device samples of exact silence since the start, until the first sound.
    silent_samples: usize,
    heard: bool,
}

impl Capture {
    /// Warns once when the microphone gives nothing but exact zeros for its first seconds: a real
    /// microphone always hears some noise, a muted or blocked one gives digital silence.
    fn watch_silence<T>(&mut self, data: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        if self.heard {
            return;
        }
        if data.iter().any(|sample| f32::from_sample(*sample) != 0.0) {
            self.heard = true;
            return;
        }
        self.silent_samples += data.len() / self.channels;
        if self.silent_samples >= self.device_frame * 100 * SILENT_SECONDS {
            self.heard = true;
            (self.shared.silent)();
        }
    }

    fn process<T>(&mut self, data: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        self.shared
            .last_capture
            .store(self.shared.now(), Ordering::Relaxed);
        self.watch_silence(data);
        // The first channel is the microphone; arrays that report several are not mixed, which
        // could cancel parts of the voice out.
        self.pending.extend(
            data.chunks(self.channels)
                .map(|frame| f32::from_sample(frame[0])),
        );
        let mut offset = 0;
        self.cleaned.clear();
        while self.pending.len() - offset >= self.device_frame {
            self.resampler.resample_mono(
                &self.pending[offset..offset + self.device_frame],
                &mut self.frame,
            );
            offset += self.device_frame;
            match &mut self.echo {
                Some(echo) => {
                    if let Some(reference) = &self.shared.reference
                        && let Ok(mut reference) = reference.lock()
                    {
                        self.reference.clear();
                        self.reference.extend(reference.drain(..));
                    }
                    echo.render(&self.reference);
                    self.reference.clear();
                    echo.capture(&self.frame, &mut self.cleaned);
                }
                None => self.cleaned.extend_from_slice(&self.frame),
            }
        }
        self.pending.drain(..offset);
        if self.cleaned.is_empty() {
            return;
        }
        self.pcm.clear();
        self.pcm.extend(
            self.cleaned
                .iter()
                .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16),
        );
        (self.capture)(&self.pcm);
    }
}

/// A running microphone (and, for a conversation, speaker). Dropping it stops both.
pub(crate) struct CpalVoiceAudio {
    shared: Arc<Shared>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl CpalVoiceAudio {
    /// Opens the default microphone (with echo cancellation when `voice_processing`) and, when
    /// `playback`, the default speaker. Blocks until both run.
    pub(crate) fn start(
        voice_processing: bool,
        playback: bool,
        capture: CaptureFn,
        device_changed: DeviceChangedFn,
        silent: SilentFn,
    ) -> Result<Self, AudioStartError> {
        let (started_sender, started) = mpsc::channel();
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("ghostex-voice-audio".into())
            .spawn(move || {
                let running =
                    open_streams(voice_processing, playback, capture, device_changed, silent);
                let Running {
                    streams,
                    shared,
                    input_name,
                    output_name,
                } = match running {
                    Ok(running) => running,
                    Err(error) => {
                        let _ = started_sender.send(Err(error));
                        return;
                    }
                };
                let _ = started_sender.send(Ok(shared.clone()));
                watch_devices(&stopped, &shared, input_name, output_name, playback);
                drop(streams);
            })
            .map_err(|error| {
                AudioStartError::Failed(format!("The audio could not start: {error}"))
            })?;
        match started.recv_timeout(START_TIMEOUT) {
            Ok(Ok(shared)) => Ok(Self {
                shared,
                stop: Some(stop),
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(AudioStartError::Failed(
                "The microphone did not start in time.".to_string(),
            )),
        }
    }
}

impl VoiceAudioBackend for CpalVoiceAudio {
    fn enqueue(&self, samples: &[i16]) {
        let Some(playback) = &self.shared.playback else {
            return;
        };
        let samples: Vec<f32> = samples
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();
        if let Ok(mut playback) = playback.lock() {
            playback.stretch.push(&samples);
        }
    }

    fn set_rate(&self, rate: f32) {
        if let Some(playback) = &self.shared.playback
            && let Ok(mut playback) = playback.lock()
        {
            playback.stretch.set_rate(rate);
        }
    }

    fn flush(&self) -> u64 {
        let Some(playback) = &self.shared.playback else {
            return 0;
        };
        let Ok(mut playback) = playback.lock() else {
            return 0;
        };
        // What waits at the device's rate is under 10 ms; it is counted at 1x.
        let waiting = playback.device.len() as u64 * aec::RATE as u64 / playback.device_rate as u64;
        let dropped = playback.stretch.pending_input_samples() + waiting;
        playback.stretch.clear();
        playback.device.clear();
        dropped
    }
}

impl Drop for CpalVoiceAudio {
    fn drop(&mut self) {
        // Dropping the streams on their thread stops them and waits for their callbacks.
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The streams of a started audio, with the device names the watcher compares against.
struct Running {
    streams: Vec<Stream>,
    shared: Arc<Shared>,
    input_name: Option<String>,
    output_name: Option<String>,
}

fn open_streams(
    voice_processing: bool,
    playback: bool,
    capture: CaptureFn,
    device_changed: DeviceChangedFn,
    silent: SilentFn,
) -> Result<Running, AudioStartError> {
    let host = cpal::default_host();
    let input = host
        .default_input_device()
        .ok_or(AudioStartError::NoInput)?;
    let input_config = choose_config(
        input
            .supported_input_configs()
            .map(|configs| configs.collect()),
        input.default_input_config().ok(),
    )
    .ok_or_else(|| start_failure(&input, Side::Microphone))?;
    let output = match playback {
        true => {
            let output = host
                .default_output_device()
                .ok_or_else(|| AudioStartError::Failed("No speaker was found.".to_string()))?;
            let config = choose_config(
                output
                    .supported_output_configs()
                    .map(|configs| configs.collect()),
                output.default_output_config().ok(),
            )
            .ok_or_else(|| start_failure(&output, Side::Speaker))?;
            Some((output, config))
        }
        false => None,
    };
    let shared = Arc::new(Shared {
        playback: output
            .as_ref()
            .map(|(_, config)| Mutex::new(Playback::new(config.sample_rate()))),
        reference: (voice_processing && playback)
            .then(|| Mutex::new(VecDeque::with_capacity(MAX_REFERENCE))),
        device_changed,
        changed: AtomicBool::new(false),
        silent,
        started: Instant::now(),
        last_capture: AtomicU64::new(0),
        last_output: AtomicU64::new(0),
    });
    let mut streams = Vec::new();
    // The speaker starts first, so the echo canceller has its reference before the microphone's
    // first frame.
    if let Some((output, config)) = &output {
        let stream = build_output(output, config, shared.clone())?;
        stream.play().map_err(|error| {
            AudioStartError::Failed(format!("The speaker could not start: {error}"))
        })?;
        streams.push(stream);
    }
    let device_frame = (input_config.sample_rate() / 100) as usize;
    let state = Capture {
        channels: input_config.channels().max(1) as usize,
        device_frame,
        pending: Vec::with_capacity(device_frame * 4),
        resampler: PushResampler::new(device_frame, aec::FRAME, 1),
        frame: vec![0.0; aec::FRAME],
        echo: shared.reference.is_some().then(EchoCanceller::new),
        reference: Vec::with_capacity(MAX_REFERENCE),
        cleaned: Vec::with_capacity(aec::FRAME * 4),
        pcm: Vec::with_capacity(aec::FRAME * 4),
        capture,
        shared: shared.clone(),
        silent_samples: 0,
        heard: false,
    };
    let stream = build_input(&input, &input_config, state, shared.clone())?;
    stream
        .play()
        .map_err(|error| play_error(&error, Side::Microphone))?;
    streams.push(stream);
    let input_name = device_name(&input);
    let output_name = output.and_then(|(output, _)| device_name(&output));
    Ok(Running {
        streams,
        shared,
        input_name,
        output_name,
    })
}

/// Keeps the streams until stopped, reporting a change of default device or a stream that stopped
/// calling back.
fn watch_devices(
    stopped: &mpsc::Receiver<()>,
    shared: &Shared,
    input_name: Option<String>,
    output_name: Option<String>,
    playback: bool,
) {
    let host = cpal::default_host();
    let mut last_poll = Instant::now();
    while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(WATCH_TICK) {
        if shared.stalled(&shared.last_capture) || (playback && shared.stalled(&shared.last_output))
        {
            shared.report_change();
        }
        if last_poll.elapsed() < DEVICE_POLL {
            continue;
        }
        last_poll = Instant::now();
        let input_now = host
            .default_input_device()
            .and_then(|device| device_name(&device));
        let output_now = playback
            .then(|| {
                host.default_output_device()
                    .and_then(|device| device_name(&device))
            })
            .flatten();
        if input_now != input_name || output_now != output_name {
            shared.report_change();
        }
    }
}

/// The name the system shows for the device, which changes when the default device does.
fn device_name(device: &Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

/// The device's own format when the resampler can take its rate, else the first preferred rate
/// it supports, in a format the callbacks convert.
fn choose_config(
    ranges: Result<Vec<SupportedStreamConfigRange>, cpal::Error>,
    default: Option<SupportedStreamConfig>,
) -> Option<SupportedStreamConfig> {
    if let Some(default) = &default
        && PREFERRED_RATES.contains(&default.sample_rate())
        && FORMATS.contains(&default.sample_format())
    {
        return Some(default.clone());
    }
    let ranges = ranges.ok()?;
    for rate in PREFERRED_RATES {
        for format in FORMATS {
            if let Some(range) = ranges.iter().find(|range| {
                range.sample_format() == format
                    && range.min_sample_rate() <= rate
                    && rate <= range.max_sample_rate()
            }) {
                return Some(range.with_sample_rate(rate));
            }
        }
    }
    None
}

fn build_output(
    device: &Device,
    config: &SupportedStreamConfig,
    shared: Arc<Shared>,
) -> Result<Stream, AudioStartError> {
    match config.sample_format() {
        SampleFormat::F32 => build_output_typed::<f32>(device, config, shared),
        SampleFormat::I16 => build_output_typed::<i16>(device, config, shared),
        SampleFormat::I32 => build_output_typed::<i32>(device, config, shared),
        _ => build_output_typed::<u16>(device, config, shared),
    }
}

fn build_output_typed<T>(
    device: &Device,
    config: &SupportedStreamConfig,
    shared: Arc<Shared>,
) -> Result<Stream, AudioStartError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels().max(1) as usize;
    let errors = shared.clone();
    device
        .build_output_stream::<T, _, _>(
            config.config(),
            move |data: &mut [T], _| {
                shared.last_output.store(shared.now(), Ordering::Relaxed);
                let frames = data.len() / channels;
                let Some(playback) = &shared.playback else {
                    data.fill(T::EQUILIBRIUM);
                    return;
                };
                let Ok(mut playback) = playback.lock() else {
                    data.fill(T::EQUILIBRIUM);
                    return;
                };
                while playback.device.len() < frames {
                    playback.render_frame(shared.reference.as_ref());
                }
                for frame in data.chunks_mut(channels) {
                    let sample = T::from_sample(playback.device.pop_front().unwrap_or(0.0));
                    frame.fill(sample);
                }
            },
            move |error| on_stream_error(&errors, error),
            None,
        )
        .map_err(|error| play_error(&error, Side::Speaker))
}

fn build_input(
    device: &Device,
    config: &SupportedStreamConfig,
    state: Capture,
    shared: Arc<Shared>,
) -> Result<Stream, AudioStartError> {
    match config.sample_format() {
        SampleFormat::F32 => build_input_typed::<f32>(device, config, state, shared),
        SampleFormat::I16 => build_input_typed::<i16>(device, config, state, shared),
        SampleFormat::I32 => build_input_typed::<i32>(device, config, state, shared),
        _ => build_input_typed::<u16>(device, config, state, shared),
    }
}

fn build_input_typed<T>(
    device: &Device,
    config: &SupportedStreamConfig,
    mut state: Capture,
    shared: Arc<Shared>,
) -> Result<Stream, AudioStartError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            config.config(),
            move |data: &[T], _| state.process(data),
            move |error| on_stream_error(&shared, error),
            None,
        )
        .map_err(|error| play_error(&error, Side::Microphone))
}

/// A device that went away restarts the audio. Other errors are not acted on: some streams carry
/// on after them (a dropout or an xrun, or a default stream cpal moved to the new default device
/// itself), and one that stopped is caught by the watcher (`STALLED`).
fn on_stream_error(shared: &Shared, error: cpal::Error) {
    if error.kind() == ErrorKind::DeviceNotAvailable {
        shared.report_change();
    }
}

#[derive(Clone, Copy)]
enum Side {
    Microphone,
    Speaker,
}

/// Why a device offered no format the audio can use.
fn start_failure(device: &Device, side: Side) -> AudioStartError {
    let error = match side {
        Side::Microphone => device.default_input_config().err(),
        Side::Speaker => device.default_output_config().err(),
    };
    match error {
        Some(error) => play_error(&error, side),
        None => AudioStartError::Failed(match side {
            Side::Microphone => "The microphone's format is not supported.".to_string(),
            Side::Speaker => "The speaker's format is not supported.".to_string(),
        }),
    }
}

/// `play_failure` for a cpal error, which also names the refusal and the missing device itself.
fn play_error(error: &cpal::Error, side: Side) -> AudioStartError {
    match (error.kind(), side) {
        (ErrorKind::PermissionDenied, Side::Microphone) => AudioStartError::PermissionDenied,
        (ErrorKind::DeviceNotAvailable, Side::Microphone) => AudioStartError::NoInput,
        _ => play_failure(&error.to_string(), side),
    }
}

/// The user's sentence for a device that did not open. Windows answers "access denied"
/// (E_ACCESSDENIED) when microphone access is off for desktop apps in Settings > Privacy &
/// security > Microphone; Linux answers EACCES when this user may not open the sound devices, and
/// ENODEV when no sound card is there (or none this user can see). The system's own words follow,
/// for support.
fn play_failure(message: &str, side: Side) -> AudioStartError {
    let lower = message.to_ascii_lowercase();
    let what = match side {
        Side::Microphone => "microphone",
        Side::Speaker => "speaker",
    };
    if lower.contains("0x80070005")
        || lower.contains("access is denied")
        || lower.contains("permission denied")
    {
        return match side {
            Side::Microphone => AudioStartError::PermissionDenied,
            Side::Speaker => {
                AudioStartError::Failed(format!("Ghostex can't use the speaker ({message})."))
            }
        };
    }
    if matches!(side, Side::Microphone) && lower.contains("no such device") {
        return AudioStartError::NoInput;
    }
    AudioStartError::Failed(format!("The {what} could not start ({message})."))
}
