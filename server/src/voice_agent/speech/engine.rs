//! The local engine: reads a recording as 16 kHz mono, loads the chosen model (kept loaded
//! between dictations, unloaded after 15 idle minutes) and runs it, Parakeet with telmo and
//! Whisper with candle, both on the CPU.
//!
//! CDXC:Dictate 2026-11-05 WHY:
//! Parakeet runs on telmo, a pure-Rust CPU engine, instead of ONNX Runtime: ONNX Runtime's prebuilt libraries can't link into gxserver on Windows (dynamic C runtime against gxserver's static one) or Linux (no musl build), and telmo also brings the decoder biasing the dictionary needs. Both engines use every core and no GPU, so one build works on macOS, Windows and Linux, x64 and arm64.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::catalog::{self, Family, ModelSpec};
use super::whisper::Whisper;

/// What the recognizer is given besides the audio.
pub(crate) struct Hints<'a> {
    /// ISO 639-1, or `None` to detect it.
    pub(crate) language: Option<&'a str>,
    /// Dictionary words and project names to favour.
    pub(crate) words: &'a [String],
    /// (word, alias) pairs: Parakeet also favours the alias and writes the word for it.
    pub(crate) aliases: &'a [(String, String)],
}

enum Loaded {
    Parakeet(Box<telmo::Model>),
    Whisper(Box<Whisper>),
}

struct Slot {
    id: &'static str,
    model: Loaded,
    used: Instant,
}

/// The model in memory, at most one: the lock also lets one dictation run at a time.
fn slot() -> std::sync::MutexGuard<'static, Option<Slot>> {
    static SLOT: Mutex<Option<Slot>> = Mutex::new(None);
    SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// How long a loaded model stays in memory after its last dictation.
const IDLE_UNLOAD: Duration = Duration::from_secs(15 * 60);

/// Frees the model after [`IDLE_UNLOAD`] without a dictation (a background thread, started once).
fn watch_idle() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("dictate-unload".to_string())
            .spawn(|| loop {
                std::thread::sleep(Duration::from_secs(60));
                let mut slot = slot();
                if slot
                    .as_ref()
                    .is_some_and(|loaded| loaded.used.elapsed() > IDLE_UNLOAD)
                {
                    *slot = None;
                }
            });
    });
}

/// Drops the model `id` from memory if it is loaded (it is being deleted).
pub(crate) fn unload(id: &str) {
    let mut slot = slot();
    if slot.as_ref().is_some_and(|loaded| loaded.id == id) {
        *slot = None;
    }
}

/// Transcribes the recording at `audio` with the ready model `spec` in `dir`. Blocking: run it
/// off the async runtime.
pub(crate) fn transcribe(
    spec: &'static ModelSpec,
    dir: &Path,
    audio: &Path,
    hints: &Hints<'_>,
) -> Result<String, String> {
    let samples = read_audio(audio)?;
    if samples.len() < catalog_min_samples() {
        return Ok(String::new());
    }
    watch_idle();
    let mut slot = slot();
    if slot.as_ref().is_none_or(|loaded| loaded.id != spec.id) {
        // Free the old one first: two models at once may not fit in memory.
        *slot = None;
        let model = match spec.family {
            Family::Parakeet => Loaded::Parakeet(Box::new(
                telmo::Model::load(&dir.join(PARAKEET_PACK), &telmo::Options::default())
                    .map_err(|error| format!("Couldn't load {}: {error}", spec.title))?,
            )),
            Family::Whisper => Loaded::Whisper(Box::new(Whisper::load(dir)?)),
        };
        *slot = Some(Slot {
            id: spec.id,
            model,
            used: Instant::now(),
        });
    }
    let loaded = slot.as_mut().expect("loaded above");
    loaded.used = Instant::now();
    let text = match &mut loaded.model {
        Loaded::Parakeet(model) => {
            let words = special_words(model, hints);
            model
                .transcribe_biased(&samples, &words)
                .map(|transcript| transcript.text)
                .map_err(|error| format!("{} failed: {error}", spec.title))?
        }
        Loaded::Whisper(model) => {
            // Whisper copies its prompt's style, so the words go in a punctuated sentence
            // (a bare list made turbo write lowercase with no full stop).
            let prompt = (!hints.words.is_empty())
                .then(|| format!("Names you may hear: {}.", hints.words.join(", ")));
            model.transcribe(&samples, hints.language, prompt.as_deref())?
        }
    };
    loaded.used = Instant::now();
    Ok(text.trim().to_string())
}

/// A quarter of a second: shorter recordings hold no words.
fn catalog_min_samples() -> usize {
    16_000 / 4
}

pub(crate) const PARAKEET_PACK: &str = "model.pack";

/// The dictionary as telmo's special words; words it can't take (too short, a script without
/// spaces) are left out rather than failing the dictation.
fn special_words(model: &telmo::Model, hints: &Hints<'_>) -> telmo::SpecialWords {
    let mut words = telmo::SpecialWords::new();
    for word in hints.words {
        let _ = words.add(word);
    }
    for (word, alias) in hints.aliases {
        let _ = words.add_spoken(word, alias);
    }
    let unusable: Vec<String> = model
        .unusable_special_words(&words)
        .into_iter()
        .map(|(word, _)| word)
        .collect();
    if unusable.is_empty() {
        return words;
    }
    let mut usable = telmo::SpecialWords::new();
    for word in hints.words.iter().filter(|word| !unusable.contains(word)) {
        let _ = usable.add(word);
    }
    for (word, alias) in hints
        .aliases
        .iter()
        .filter(|(word, alias)| !unusable.contains(word) && !unusable.contains(alias))
    {
        let _ = usable.add_spoken(word, alias);
    }
    usable
}

/// The recording as 16 kHz mono samples. An `.m4a` (macOS keeps recordings as AAC) is decoded
/// to a WAV beside it first with macOS's `afconvert`, which made it.
pub(crate) fn read_audio(path: &Path) -> Result<Vec<f32>, String> {
    let is_m4a = path.extension().is_some_and(|ext| ext == "m4a");
    let decoded: Option<PathBuf> = if is_m4a {
        Some(decode_m4a(path)?)
    } else {
        None
    };
    let wav = decoded.as_deref().unwrap_or(path);
    let audio =
        telmo::wav::read(wav).map_err(|error| format!("Couldn't read the recording: {error}"));
    if let Some(decoded) = &decoded {
        let _ = std::fs::remove_file(decoded);
    }
    let audio = audio?;
    Ok(if audio.sample_rate == 16_000 {
        audio.samples
    } else {
        telmo::resample::resample(&audio.samples, audio.sample_rate, 16_000)
    })
}

fn decode_m4a(path: &Path) -> Result<PathBuf, String> {
    let afconvert = Path::new("/usr/bin/afconvert");
    if !afconvert.is_file() {
        return Err("This recording is AAC, which only macOS can decode here.".to_string());
    }
    let wav = path.with_extension(format!("{}.16k.wav", uuid::Uuid::new_v4().simple()));
    let status = std::process::Command::new(afconvert)
        .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
        .arg(path)
        .arg(&wav)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| format!("Couldn't decode the recording: {error}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&wav);
        return Err("Couldn't decode the recording.".to_string());
    }
    Ok(wav)
}

/// The `catalog` model that `id` names, for callers outside this folder.
pub(crate) fn spec(id: &str) -> Option<&'static ModelSpec> {
    catalog::model(id)
}
