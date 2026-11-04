//! Dictate's recordings: every dictation's audio, kept on this computer with what became of it
//! (transcribed text, a failure and its reason, or dismissed), so a dictation is never lost to a
//! refused key, an empty account, a dropped connection or a restart.
//!
//! A client records to a file of its own first and uploads it here in pieces once the reader
//! stops (`/api/saveDictationRecording`); only then is it transcribed (`transcribe.rs`). The
//! client keeps its copy until this store answers that the whole file arrived. Each recording is
//! `<id>.json` beside its audio (`<id>.m4a`, AAC at 32 kbit/s, about 240 KB a minute, where
//! macOS's `afconvert` can make it; otherwise the uploaded `<id>.wav`, about 2.9 MB a minute), in
//! `<gxserver state>/voice-agent/dictations/`. Recordings stay until the user deletes them.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "i want that we never lose what the user was trying to transcribe please, if dictation fails we need to say so and allow the user to transcribe what we had recorded so far (we need to keep that voice recording basically)" and "we can't fail the dictation due to any issues with api or connection. please fully separate recording the audio from sending it to the api and getting transcription, they are not related". Recording and transcribing are two steps; a cancelled or dismissed recording is kept ("even if he cancels it we need to save so user can hover over the voice button and click Transcribe previous"), and every recording is listed in Settings > Voice agent's Dictation history until the user deletes it.
//! CDXC:VoiceMode 2026-10-20 SEE-ALSO:
//! `transcribe.rs` (xAI's batch speech-to-text), `recordings_http.rs` (the API), the desktop's `apps/desktop/src/app/voice_agent/recorder.rs` and `dictation.rs`, the phone's `src/voice/dictation.ts` and `GhostexVoiceRecorder.kt`.

use std::collections::HashMap;
use std::fs;
use std::io::{Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::paths::GxserverPaths;

/// What became of a recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Status {
    /// Uploaded; nobody asked for its text yet.
    Recorded,
    Transcribing,
    Transcribed,
    Failed,
    /// Cancelled while recording or while transcribing; Recover transcribes it.
    Dismissed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Problem {
    pub(super) code: String,
    /// The sentence clients show: what went wrong and what to do, in plain words.
    pub(super) message: String,
    /// The provider whose key the transcription used (`xai`, `openai`), so a client's Check key
    /// opens that key's field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) provider: Option<String>,
    /// The provider's own words, when it gave a reason (shown on hover, never instead of `message`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Recording {
    pub(super) id: String,
    /// Milliseconds since the epoch when the reader pressed Dictate.
    pub(super) created_at: i64,
    pub(super) duration_ms: u64,
    /// `desktop` or `phone`.
    pub(super) client: String,
    /// The audio's file name in the folder.
    pub(super) audio_file: String,
    pub(super) audio_bytes: u64,
    pub(super) status: Status,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) error: Option<Problem>,
    /// The recognizer's language hint the client sent ("en"), reused by Transcribe again.
    #[serde(default)]
    pub(super) language: Option<String>,
    pub(super) updated_at: i64,
}

impl Recording {
    /// What clients get. `path` is the audio on this computer, for the desktop's Play and Download.
    pub(super) fn to_client(&self, paths: &GxserverPaths) -> Value {
        let path = folder(paths).join(&self.audio_file);
        json!({
            "id": self.id,
            "createdAt": self.created_at,
            "durationMs": self.duration_ms,
            "client": self.client,
            "status": self.status,
            "text": self.text,
            "error": self.error,
            "format": audio_format(&self.audio_file),
            "bytes": self.audio_bytes,
            "path": path.to_string_lossy(),
            "updatedAt": self.updated_at,
        })
    }

    /// Whether Transcribe previous may pick it: it has no text yet and nothing is transcribing it.
    pub(super) fn awaiting_text(&self) -> bool {
        matches!(
            self.status,
            Status::Recorded | Status::Failed | Status::Dismissed
        )
    }
}

fn audio_format(file: &str) -> &'static str {
    if file.ends_with(".m4a") {
        "m4a"
    } else {
        "wav"
    }
}

pub(super) fn folder(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("voice-agent").join("dictations")
}

/// Serializes every write to the folder (uploads, status changes, deletes).
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Bumped by a dismiss, so a transcription that finishes after the reader cancelled it is not saved
/// over the dismissal.
fn generations() -> std::sync::MutexGuard<'static, HashMap<String, u64>> {
    static GENERATIONS: std::sync::OnceLock<Mutex<HashMap<String, u64>>> =
        std::sync::OnceLock::new();
    GENERATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn generation(id: &str) -> u64 {
    generations().get(id).copied().unwrap_or(0)
}

fn bump_generation(id: &str) {
    *generations().entry(id.to_string()).or_insert(0) += 1;
}

pub(super) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Client-made ids: letters, digits, `-` and `_`, at most 64.
pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn record_path(paths: &GxserverPaths, id: &str) -> PathBuf {
    folder(paths).join(format!("{id}.json"))
}

fn part_path(paths: &GxserverPaths, id: &str) -> PathBuf {
    folder(paths).join(format!("{id}.wav.part"))
}

pub(super) fn read(paths: &GxserverPaths, id: &str) -> Option<Recording> {
    if !valid_id(id) {
        return None;
    }
    fs::read_to_string(record_path(paths, id))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

/// Writes the record through a temporary file, so a crash never leaves half a record.
fn write(paths: &GxserverPaths, recording: &Recording) -> Result<(), String> {
    let path = record_path(paths, &recording.id);
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(recording).map_err(|error| error.to_string())?;
    fs::create_dir_all(folder(paths))
        .and_then(|_| fs::write(&temp, text))
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|error| format!("Couldn't save the recording's details: {error}"))
}

/// Every recording, newest first.
pub(super) fn list(paths: &GxserverPaths) -> Vec<Recording> {
    let Ok(entries) = fs::read_dir(folder(paths)) else {
        return Vec::new();
    };
    let mut recordings: Vec<Recording> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|text| serde_json::from_str::<Recording>(&text).ok())
        .collect();
    recordings.sort_by_key(|recording| std::cmp::Reverse(recording.created_at));
    recordings
}

/// The newest recording that has no text yet: what Transcribe previous transcribes.
pub(super) fn latest_awaiting_text(paths: &GxserverPaths) -> Option<Recording> {
    list(paths).into_iter().find(Recording::awaiting_text)
}

/// Changes a recording's status (and text or error) under the folder lock.
pub(super) fn update(
    paths: &GxserverPaths,
    id: &str,
    change: impl FnOnce(&mut Recording),
) -> Result<Recording, String> {
    let _guard = lock();
    let mut recording =
        read(paths, id).ok_or_else(|| "That recording no longer exists.".to_string())?;
    change(&mut recording);
    recording.updated_at = now_ms();
    write(paths, &recording)?;
    Ok(recording)
}

/// Marks a recording dismissed and stops a transcription in flight from saving over it. Text that
/// arrived just before the reader cancelled is kept, so Recover answers at once.
pub(super) fn dismiss(paths: &GxserverPaths, id: &str) -> Result<Recording, String> {
    bump_generation(id);
    update(paths, id, |recording| recording.status = Status::Dismissed)
}

pub(super) fn delete(paths: &GxserverPaths, id: &str) -> Result<(), String> {
    let _guard = lock();
    bump_generation(id);
    let recording =
        read(paths, id).ok_or_else(|| "That recording no longer exists.".to_string())?;
    let _ = fs::remove_file(folder(paths).join(&recording.audio_file));
    fs::remove_file(record_path(paths, id))
        .map_err(|error| format!("Couldn't delete the recording: {error}"))
}

/// One piece of an upload.
pub(super) struct Piece<'a> {
    pub(super) id: &'a str,
    pub(super) offset: u64,
    pub(super) bytes: &'a [u8],
    /// The last piece: the file is complete and becomes a recording.
    pub(super) done: Option<Finished>,
}

/// What the client knows about a finished recording.
pub(super) struct Finished {
    pub(super) created_at: i64,
    pub(super) duration_ms: u64,
    pub(super) client: String,
    pub(super) dismissed: bool,
    pub(super) language: Option<String>,
}

/// What an upload step answers.
pub(super) enum Saved {
    /// The bytes so far, so a client that lost an answer resumes at the right offset.
    Partial(u64),
    Complete(Recording),
}

/// Writes one piece at its offset (pieces may be resent) and, after the last one, turns the file
/// into a recording. A recording that already exists answers itself, so a resent last piece after
/// a lost answer is harmless.
pub(super) fn save_piece(paths: &GxserverPaths, piece: Piece<'_>) -> Result<Saved, String> {
    let _guard = lock();
    if let Some(existing) = read(paths, piece.id) {
        return Ok(Saved::Complete(existing));
    }
    let part = part_path(paths, piece.id);
    let failed = |error: std::io::Error| format!("Couldn't save the recording: {error}");
    fs::create_dir_all(folder(paths)).map_err(failed)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&part)
        .map_err(failed)?;
    let length = file.metadata().map_err(failed)?.len();
    if piece.offset > length {
        // A piece went missing: the client resends from what is here.
        return Ok(Saved::Partial(length));
    }
    file.seek(SeekFrom::Start(piece.offset)).map_err(failed)?;
    file.write_all(piece.bytes).map_err(failed)?;
    let end = piece.offset + piece.bytes.len() as u64;
    file.set_len(end).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    drop(file);
    let Some(finished) = piece.done else {
        return Ok(Saved::Partial(end));
    };
    repair_wav_header(&part).map_err(failed)?;
    let wav = folder(paths).join(format!("{}.wav", piece.id));
    fs::rename(&part, &wav).map_err(failed)?;
    let audio = compress(&wav);
    let audio_bytes = fs::metadata(&audio).map(|meta| meta.len()).unwrap_or(0);
    let now = now_ms();
    let recording = Recording {
        id: piece.id.to_string(),
        created_at: if finished.created_at > 0 {
            finished.created_at
        } else {
            now
        },
        duration_ms: finished.duration_ms.max(wav_duration_ms(end)),
        client: finished.client,
        audio_file: audio
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        audio_bytes,
        status: if finished.dismissed {
            Status::Dismissed
        } else {
            Status::Recorded
        },
        text: None,
        error: None,
        language: finished.language,
        updated_at: now,
    };
    write(paths, &recording)?;
    Ok(Saved::Complete(recording))
}

/// 24 kHz mono PCM16 after a 44-byte header.
fn wav_duration_ms(file_bytes: u64) -> u64 {
    file_bytes.saturating_sub(44) / 48
}

/// Writes the RIFF and data sizes from the file's real length: a client that crashed mid-recording
/// never got to write them.
fn repair_wav_header(path: &Path) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let length = file.metadata()?.len();
    if length < 44 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the recording is empty",
        ));
    }
    let data = (length - 44).min(u32::MAX as u64) as u32;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&(data + 36).to_le_bytes())?;
    file.seek(SeekFrom::Start(40))?;
    file.write_all(&data.to_le_bytes())?;
    file.sync_all()
}

/// The WAV as AAC in an `.m4a` (a twelfth of the size) where macOS's `afconvert` exists; the WAV
/// itself elsewhere, which xAI takes as well.
fn compress(wav: &Path) -> PathBuf {
    let afconvert = Path::new("/usr/bin/afconvert");
    if !afconvert.is_file() {
        return wav.to_path_buf();
    }
    let m4a = wav.with_extension("m4a");
    let converted = std::process::Command::new(afconvert)
        .args(["-f", "m4af", "-d", "aac", "-b", "32000"])
        .arg(wav)
        .arg(&m4a)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        && fs::metadata(&m4a).is_ok_and(|meta| meta.len() > 0);
    if converted {
        let _ = fs::remove_file(wav);
        m4a
    } else {
        let _ = fs::remove_file(&m4a);
        wav.to_path_buf()
    }
}

/// Runs [`recover_after_restart`] once per gxserver run, before its first recordings request.
pub(super) fn ensure_recovered(paths: &GxserverPaths) {
    static RECOVERED: std::sync::Once = std::sync::Once::new();
    RECOVERED.call_once(|| recover_after_restart(paths));
}

/// After a gxserver start: a transcription the restart interrupted is marked failed so it can be
/// tried again, and uploads left half-done by a client that never came back are dropped after a
/// week.
fn recover_after_restart(paths: &GxserverPaths) {
    for recording in list(paths) {
        if recording.status == Status::Transcribing {
            let _ = update(paths, &recording.id, |recording| {
                recording.status = Status::Failed;
                recording.error = Some(Problem {
                    code: "interrupted".to_string(),
                    message:
                        "Ghostex restarted while this was being transcribed. Transcribe again."
                            .to_string(),
                    provider: None,
                    detail: None,
                });
            });
        }
    }
    let Ok(entries) = fs::read_dir(folder(paths)) else {
        return;
    };
    let week = std::time::Duration::from_secs(7 * 24 * 3600);
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let stale = path.to_string_lossy().ends_with(".wav.part")
            && fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > week);
        if stale {
            let _ = fs::remove_file(path);
        }
    }
}
