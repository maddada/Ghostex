//! Dictate's first step: the microphone written to a WAV file as it is captured, so nothing said is
//! lost to a crash, a refused key, an empty account, no network or a gxserver restart. Then the
//! upload of a finished file to this computer's gxserver, which keeps it (`/api/saveDictationRecording`,
//! server/src/voice_agent/recordings.rs). This app's copy is deleted only once gxserver answers
//! that the whole file arrived; a recording it never confirmed is sent again on the next Dictate.
//!
//! Files: `<Ghostex state>/dictation-recordings/<id>.wav` (24 kHz mono PCM16, about 2.9 MB a
//! minute) and `<id>.json` (when it started, whether it was dismissed).
//!
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO: the user's decision on recording first and transcribing second is in `server/src/voice_agent/recordings.rs`; the phone's twin is `GhostexVoiceRecorder.kt` in the phone app.

use std::fs;
use std::io::{BufWriter, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};

use super::audio::{AudioStartError, MICROPHONE_KEPT_STOPPING, VoiceAudio};
use super::conversation::MAX_AUDIO_RESTARTS;

const SAMPLE_RATE: u32 = 24_000;
/// Raw bytes per upload piece: under the 1 MiB JSON limit once base64 grows it by a third.
const PIECE_BYTES: usize = 512 * 1024;
/// How often the file's header is brought up to date while recording.
const HEADER_EVERY: Duration = Duration::from_secs(2);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// gxserver restarting (an update, `start-server`) closes its port for about two seconds.
const RESTART_WAIT: Duration = Duration::from_secs(8);

/// Recordings still being made, which the unsent sweep must leave alone.
fn active_ids() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    static ACTIVE: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    ACTIVE
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Tells Read aloud whether Dictate's microphone is recording (`setDictating`), so a reply is not
/// read aloud into the recording.
fn report_microphone() {
    let recording = !active_ids().is_empty();
    crate::app::gx_chat::read_aloud_dictating(recording);
}

/// This app's own recordings folder.
fn folder() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("dictation-recordings")
}

/// A finished recording on this computer, waiting for gxserver to take it.
#[derive(Clone, Debug)]
pub(crate) struct LocalRecording {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) created_at: i64,
    pub(crate) duration_ms: u64,
    pub(crate) dismissed: bool,
}

impl LocalRecording {
    fn sidecar(&self) -> PathBuf {
        self.path.with_extension("json")
    }

    fn write_sidecar(&self) {
        let _ = fs::write(
            self.sidecar(),
            json!({"createdAt": self.created_at, "dismissed": self.dismissed}).to_string(),
        );
    }

    /// Marks it dismissed (the reader cancelled), so an upload after a crash keeps that.
    pub(crate) fn dismiss(&mut self) {
        self.dismissed = true;
        self.write_sidecar();
    }

    fn forget(&self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.sidecar());
    }
}

/// Why the microphone did not start.
pub(crate) type StartError = AudioStartError;

/// A recording in progress. [`ActiveRecording::finish`] closes the file; dropping it without that
/// still leaves a valid file behind (the next upload sweep repairs and sends it).
pub(crate) struct ActiveRecording {
    recording: LocalRecording,
    started: Instant,
    stopped: Arc<AtomicBool>,
    running: Arc<Mutex<Option<VoiceAudio>>>,
    writer: Option<std::thread::JoinHandle<u64>>,
    /// The microphone failed after it started (the error, once).
    failure: Arc<Mutex<Option<AudioStartError>>>,
    /// The microphone gives only silence (a warning; the recording goes on).
    silent: Arc<AtomicBool>,
}

impl ActiveRecording {
    /// Opens the microphone and starts writing. Blocks while macOS asks for microphone access the
    /// first time, so call it off the main thread.
    pub(crate) fn start() -> Result<Self, StartError> {
        let created_at = chrono::Utc::now().timestamp_millis();
        let id = format!(
            "d{created_at}-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        let dir = folder();
        fs::create_dir_all(&dir).map_err(|error| {
            AudioStartError::Failed(format!("Couldn't create the recordings folder: {error}"))
        })?;
        let path = dir.join(format!("{id}.wav"));
        let mut file = fs::File::create(&path).map_err(|error| {
            AudioStartError::Failed(format!("Couldn't create the recording file: {error}"))
        })?;
        file.write_all(&wav_header(0)).map_err(|error| {
            AudioStartError::Failed(format!("Couldn't write the recording file: {error}"))
        })?;
        let recording = LocalRecording {
            id,
            path,
            created_at,
            duration_ms: 0,
            dismissed: false,
        };
        recording.write_sidecar();
        active_ids().insert(recording.id.clone());
        report_microphone();

        let (sender, receiver) = std::sync::mpsc::channel::<Vec<i16>>();
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = {
            let stopped = stopped.clone();
            std::thread::Builder::new()
                .name("ghostex-dictation-writer".into())
                .spawn(move || write_samples(file, receiver, stopped))
                .ok()
        };
        let running: Arc<Mutex<Option<VoiceAudio>>> = Arc::new(Mutex::new(None));
        let failure: Arc<Mutex<Option<AudioStartError>>> = Arc::new(Mutex::new(None));
        let silent = Arc::new(AtomicBool::new(false));
        // The first start is waited for, so a refused microphone is reported at once; restarts
        // (Bluetooth earbuds switching profile stop the engine) happen on the same thread.
        let (first, first_result) = std::sync::mpsc::channel::<Result<(), AudioStartError>>();
        {
            let running = running.clone();
            let stopped = stopped.clone();
            let failure = failure.clone();
            let silent = silent.clone();
            let _ = std::thread::Builder::new()
                .name("ghostex-dictation-microphone".into())
                .spawn(move || {
                    let (restart, restart_requested) = std::sync::mpsc::channel::<()>();
                    for attempt in 0..=MAX_AUDIO_RESTARTS {
                        let capture_sender = sender.clone();
                        let restart = restart.clone();
                        let silent = silent.clone();
                        // Settings › Voice's microphone, read at each (re)start.
                        let started = VoiceAudio::start_with_microphone(
                            false,
                            false,
                            super::microphones::chosen(),
                            Box::new(move |samples| {
                                let _ = capture_sender.send(samples.to_vec());
                            }),
                            Box::new(move || {
                                let _ = restart.send(());
                            }),
                            Box::new(move || silent.store(true, Ordering::SeqCst)),
                        );
                        match started {
                            Ok(audio) => {
                                let Ok(mut slot) = running.lock() else {
                                    return;
                                };
                                if stopped.load(Ordering::SeqCst) {
                                    return;
                                }
                                *slot = Some(audio);
                                drop(slot);
                                if attempt == 0 {
                                    let _ = first.send(Ok(()));
                                }
                            }
                            Err(error) => {
                                if attempt == 0 {
                                    let _ = first.send(Err(error));
                                } else if let Ok(mut failure) = failure.lock() {
                                    *failure = Some(error);
                                }
                                return;
                            }
                        }
                        loop {
                            match restart_requested.recv_timeout(Duration::from_millis(200)) {
                                Ok(()) => break,
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                                    if !stopped.load(Ordering::SeqCst) => {}
                                Err(_) => return,
                            }
                        }
                        if let Ok(mut slot) = running.lock() {
                            slot.take();
                        }
                        if stopped.load(Ordering::SeqCst) {
                            return;
                        }
                    }
                    // Restarted as often as it may be and stopped again: the recording ends here,
                    // and what it has is transcribed.
                    if let Ok(mut failure) = failure.lock() {
                        *failure = Some(AudioStartError::Failed(
                            MICROPHONE_KEPT_STOPPING.to_string(),
                        ));
                    }
                });
        }
        let active = Self {
            recording,
            started: Instant::now(),
            stopped,
            running,
            writer,
            failure,
            silent,
        };
        match first_result.recv() {
            Ok(Ok(())) => Ok(active),
            Ok(Err(error)) => {
                // Nothing was recorded: leave no empty file behind.
                let recording = active.recording.clone();
                drop(active.finish());
                recording.forget();
                Err(error)
            }
            Err(_) => Err(AudioStartError::Failed(
                "The microphone could not start.".to_string(),
            )),
        }
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Whether the microphone gives only silence, once (the recording goes on).
    pub(crate) fn take_silent(&self) -> bool {
        self.silent.swap(false, Ordering::SeqCst)
    }

    /// A microphone failure after the start (the device went away), once.
    pub(crate) fn take_failure(&self) -> Option<AudioStartError> {
        self.failure
            .lock()
            .ok()
            .and_then(|mut failure| failure.take())
    }

    /// Stops the microphone and closes the file. Blocks briefly while the writer drains.
    pub(crate) fn finish(mut self) -> LocalRecording {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.running.lock() {
            slot.take();
        }
        let data_bytes = self
            .writer
            .take()
            .and_then(|writer| writer.join().ok())
            .unwrap_or(0);
        self.recording.duration_ms = data_bytes / 48;
        active_ids().remove(&self.recording.id);
        report_microphone();
        self.recording.clone()
    }
}

impl Drop for ActiveRecording {
    fn drop(&mut self) {
        active_ids().remove(&self.recording.id);
        report_microphone();
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.running.lock() {
            slot.take();
        }
    }
}

/// The 44-byte header of a 24 kHz mono PCM16 WAV holding `data_bytes`.
fn wav_header(data_bytes: u32) -> [u8; 44] {
    let mut header = [0u8; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&1u16.to_le_bytes());
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    header[32..34].copy_from_slice(&2u16.to_le_bytes());
    header[34..36].copy_from_slice(&16u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    header
}

/// The writer thread: appends samples, keeps the header current every couple of seconds, and
/// answers how many audio bytes were written once the microphone stops.
fn write_samples(
    file: fs::File,
    receiver: std::sync::mpsc::Receiver<Vec<i16>>,
    stopped: Arc<AtomicBool>,
) -> u64 {
    let mut out = BufWriter::new(file);
    let mut written: u64 = 0;
    let mut last_header = Instant::now();
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(samples) => {
                let bytes: Vec<u8> = samples
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect();
                if out.write_all(&bytes).is_ok() {
                    written += bytes.len() as u64;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if stopped.load(Ordering::SeqCst) {
            // What the audio thread already queued still belongs to the recording.
            while let Ok(samples) = receiver.try_recv() {
                let bytes: Vec<u8> = samples
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect();
                if out.write_all(&bytes).is_ok() {
                    written += bytes.len() as u64;
                }
            }
            break;
        }
        if last_header.elapsed() >= HEADER_EVERY {
            last_header = Instant::now();
            let _ = rewrite_header(&mut out, written);
        }
    }
    let _ = rewrite_header(&mut out, written);
    written
}

fn rewrite_header(out: &mut BufWriter<fs::File>, written: u64) -> std::io::Result<()> {
    out.flush()?;
    let file = out.get_mut();
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&wav_header(written.min(u32::MAX as u64 - 36) as u32))?;
    file.seek(SeekFrom::End(0))?;
    file.sync_data()
}

/// What gxserver answered for a recording it took.
pub(crate) type Dictation = Value;

/// Calls this computer's gxserver (`/api/<method>`), waiting out a restart.
pub(crate) fn rpc(method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
    let (status, body) = crate::app::helpers::gxserver_post_typed_operation_across_restart(
        &format!("/api/{method}"),
        &params,
        timeout,
        RESTART_WAIT,
    )?;
    let mut envelope: Value = serde_json::from_str(&body)
        .map_err(|_| "Ghostex's service returned an answer it couldn't read.".to_string())?;
    if envelope["ok"] == false || !(200..300).contains(&status) {
        return Err(envelope["message"]
            .as_str()
            .or(envelope["error"]["message"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Ghostex's service answered HTTP {status}.")));
    }
    Ok(envelope["result"].take())
}

/// Sends a finished recording to gxserver in pieces and deletes this app's copy once gxserver has
/// it. Answers the recording as gxserver keeps it. Blocking.
pub(crate) fn upload(
    recording: &LocalRecording,
    language: Option<&str>,
) -> Result<Dictation, String> {
    let bytes = fs::read(&recording.path)
        .map_err(|error| format!("Couldn't read the recording: {error}"))?;
    let mut offset = 0usize;
    loop {
        let end = (offset + PIECE_BYTES).min(bytes.len());
        let done = end == bytes.len();
        let answer = rpc(
            "saveDictationRecording",
            json!({
                "id": recording.id,
                "offset": offset,
                "chunk": base64::engine::general_purpose::STANDARD.encode(&bytes[offset..end]),
                "done": done,
                "createdAt": recording.created_at,
                "durationMs": recording.duration_ms,
                "client": "desktop",
                "dismissed": recording.dismissed,
                "language": language,
            }),
            UPLOAD_TIMEOUT,
        )?;
        if answer["dictation"].is_object() {
            recording.forget();
            return Ok(answer["dictation"].clone());
        }
        let received = answer["received"].as_u64().unwrap_or(end as u64) as usize;
        if done && received >= bytes.len() {
            return Err("Ghostex's service didn't confirm the recording.".to_string());
        }
        offset = received.min(bytes.len());
    }
}

/// Recordings this app made that gxserver never confirmed (it was down, or the app quit while
/// uploading), oldest first. A file a crash left without a header size is still valid: gxserver
/// sizes it from its length.
pub(crate) fn unsent() -> Vec<LocalRecording> {
    let Ok(entries) = fs::read_dir(folder()) else {
        return Vec::new();
    };
    let mut recordings: Vec<LocalRecording> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
        .filter(|path| {
            path.file_stem()
                .is_none_or(|id| !active_ids().contains(id.to_string_lossy().as_ref()))
        })
        .filter_map(|path| local_recording(&path))
        .collect();
    recordings.sort_by_key(|recording| recording.created_at);
    recordings
}

fn local_recording(path: &Path) -> Option<LocalRecording> {
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let sidecar: Value = fs::read_to_string(path.with_extension("json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let length = fs::metadata(path).ok()?.len();
    if length <= 44 {
        // The microphone never delivered anything.
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("json"));
        return None;
    }
    Some(LocalRecording {
        id,
        path: path.to_path_buf(),
        created_at: sidecar["createdAt"].as_i64().unwrap_or(0),
        duration_ms: (length - 44) / 48,
        // A recording that was never stopped (the app quit) has nobody waiting for its text.
        dismissed: sidecar["dismissed"].as_bool().unwrap_or(true),
    })
}

/// Sends every unconfirmed recording (blocking). Answers how many gxserver took.
pub(crate) fn send_unsent() -> usize {
    unsent()
        .iter()
        .filter(|recording| upload(recording, None).is_ok())
        .count()
}
