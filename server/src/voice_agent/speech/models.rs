//! The local models on this computer, in gxserver's data folder (`<data>/gxserver/speech-models`):
//! downloading with progress, resuming, cancelling, preparing, deleting and measuring them.
//!
//! A download writes into `<id>.download/`: each file as `<name>.part`, fetched in 64 MB ranges
//! (so a stalled connection fails one range, not the whole file, and a stopped download resumes
//! where it was), checked against its SHA-256 and renamed. Then the files are prepared into what
//! the engine loads (Parakeet: telmo's pack; Whisper: 8-bit weights) and the folder is renamed to
//! `<id>/`, so a model folder exists only when it is ready.
//!
//! CDXC:Dictate 2026-11-05 DECISION:
//! User (Q1270): models live in gxserver's data folder (`…/gxserver/speech-models`), where cleaners leave them alone, with Show in Finder; each one shows its download progress, Cancel, Remove and its size on disk.

use std::collections::HashMap;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

use super::catalog::{self, Family, ModelSpec, RemoteFile};
use crate::paths::GxserverPaths;

pub(crate) fn folder(paths: &GxserverPaths) -> PathBuf {
    paths.app_data_dir.join("gxserver").join("speech-models")
}

pub(crate) fn model_dir(paths: &GxserverPaths, id: &str) -> PathBuf {
    folder(paths).join(id)
}

fn download_dir(paths: &GxserverPaths, id: &str) -> PathBuf {
    folder(paths).join(format!("{id}.download"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Downloading,
    Preparing,
    Failed,
}

/// A download that is running, or stopped with an error.
pub(crate) struct Download {
    phase: Mutex<Phase>,
    received: AtomicU64,
    cancel: AtomicBool,
    error: Mutex<Option<String>>,
}

fn downloads() -> std::sync::MutexGuard<'static, HashMap<&'static str, Arc<Download>>> {
    static DOWNLOADS: std::sync::OnceLock<Mutex<HashMap<&'static str, Arc<Download>>>> =
        std::sync::OnceLock::new();
    DOWNLOADS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the model is downloaded and prepared.
pub(crate) fn is_ready(paths: &GxserverPaths, spec: &ModelSpec) -> bool {
    let dir = model_dir(paths, spec.id);
    match spec.family {
        Family::Parakeet => dir.join(super::engine::PARAKEET_PACK).is_file(),
        Family::Whisper => {
            dir.join(super::whisper::WEIGHTS_FILE).is_file()
                && dir.join("config.json").is_file()
                && dir.join("tokenizer.json").is_file()
        }
    }
}

/// The bytes under `path` (a file or a folder, recursively).
pub(crate) fn disk_bytes(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| disk_bytes(&entry.path()))
                    .sum()
            })
            .unwrap_or(0),
        _ => 0,
    }
}

/// What clients get for a model (`SpeechModel` in the API contract).
pub(crate) fn view(paths: &GxserverPaths, spec: &ModelSpec, in_use: bool) -> Value {
    let ready = is_ready(paths, spec);
    let running = downloads().get(spec.id).cloned();
    let (state, download) = match (&running, ready) {
        (Some(download), _) => {
            let phase = *download.phase.lock().unwrap_or_else(|p| p.into_inner());
            let error = download
                .error
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            (
                match phase {
                    Phase::Downloading => "downloading",
                    Phase::Preparing => "preparing",
                    Phase::Failed => "failed",
                },
                json!({
                    "receivedBytes": download.received.load(Ordering::Relaxed),
                    "totalBytes": spec.download_bytes(),
                    "error": error,
                }),
            )
        }
        (None, true) => ("ready", Value::Null),
        (None, false) => ("notDownloaded", Value::Null),
    };
    let disk = if ready {
        disk_bytes(&model_dir(paths, spec.id))
    } else {
        spec.disk_bytes_estimate
    };
    json!({
        "id": spec.id,
        "title": spec.title,
        "shortTitle": spec.short_title,
        "languagesLabel": spec.languages_label,
        "languages": spec.languages(),
        "speedLabel": spec.speed_label,
        "downloadBytes": spec.download_bytes(),
        "diskBytes": disk,
        "state": state,
        "download": download,
        "inUse": in_use,
        "attribution": spec.attribution,
    })
}

/// Whether the model is downloading or preparing right now (not stopped with an error), and how
/// far it is (0 to 100).
pub(crate) fn progress(spec: &ModelSpec) -> Option<u64> {
    let download = downloads().get(spec.id).cloned()?;
    let phase = *download.phase.lock().unwrap_or_else(|p| p.into_inner());
    match phase {
        Phase::Failed => None,
        Phase::Preparing => Some(100),
        Phase::Downloading => {
            Some(download.received.load(Ordering::Relaxed) * 100 / spec.download_bytes().max(1))
        }
    }
}

/// Starts (or resumes) downloading `spec` on a thread of its own. Harmless while one runs.
/// `on_done` is told how it ended, for the log.
pub(crate) fn start_download(
    paths: &GxserverPaths,
    spec: &'static ModelSpec,
    on_done: impl FnOnce(Result<(), String>) + Send + 'static,
) {
    if is_ready(paths, spec) {
        return;
    }
    let download = {
        let mut downloads = downloads();
        if let Some(existing) = downloads.get(spec.id) {
            let phase = *existing.phase.lock().unwrap_or_else(|p| p.into_inner());
            if phase != Phase::Failed {
                return;
            }
        }
        let download = Arc::new(Download {
            phase: Mutex::new(Phase::Downloading),
            received: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        downloads.insert(spec.id, download.clone());
        download
    };
    let paths = paths.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("dictate-download-{}", spec.id))
        .spawn(move || {
            let result = run_download(&paths, spec, &download);
            let cancelled = download.cancel.load(Ordering::Relaxed);
            match &result {
                Ok(()) => {
                    downloads().remove(spec.id);
                }
                Err(_) if cancelled => {
                    downloads().remove(spec.id);
                    let _ = fs::remove_dir_all(download_dir(&paths, spec.id));
                }
                Err(message) => {
                    *download.error.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(message.clone());
                    *download.phase.lock().unwrap_or_else(|p| p.into_inner()) = Phase::Failed;
                }
            }
            if !cancelled {
                on_done(result);
            }
        });
    if spawned.is_err() {
        downloads().remove(spec.id);
    }
}

/// Stops a download and removes its partial files.
pub(crate) fn cancel_download(paths: &GxserverPaths, spec: &ModelSpec) {
    let running = downloads().get(spec.id).cloned();
    match running {
        Some(download) => {
            let phase = *download.phase.lock().unwrap_or_else(|p| p.into_inner());
            download.cancel.store(true, Ordering::Relaxed);
            if phase == Phase::Failed {
                downloads().remove(spec.id);
                let _ = fs::remove_dir_all(download_dir(paths, spec.id));
            }
            // A running thread removes the folder itself once it sees the flag.
        }
        None => {
            let _ = fs::remove_dir_all(download_dir(paths, spec.id));
        }
    }
}

/// Removes a downloaded model (and any download of it).
pub(crate) fn delete(paths: &GxserverPaths, spec: &ModelSpec) -> Result<(), String> {
    cancel_download(paths, spec);
    super::engine::unload(spec.id);
    match fs::remove_dir_all(model_dir(paths, spec.id)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("Couldn't remove {}: {error}", spec.title))
        }
        _ => Ok(()),
    }
}

/// Bytes fetched per request.
const RANGE_BYTES: u64 = 64 * 1024 * 1024;
/// How long one range may take (64 MB over a slow 2 Mbit/s link takes about 4.5 minutes).
const RANGE_TIMEOUT: Duration = Duration::from_secs(8 * 60);
/// Tries per range before the download stops with an error (time fixes most of these).
const RANGE_TRIES: u32 = 4;

fn run_download(
    paths: &GxserverPaths,
    spec: &ModelSpec,
    download: &Download,
) -> Result<(), String> {
    let staging = download_dir(paths, spec.id);
    fs::create_dir_all(&staging)
        .map_err(|error| format!("Couldn't create {}: {error}", staging.display()))?;
    let mut done_bytes = 0u64;
    for file in spec.files {
        let target = staging.join(file.name);
        if !target.is_file() {
            fetch_file(spec, file, &staging, done_bytes, download)?;
        }
        done_bytes += file.bytes;
        download.received.store(done_bytes, Ordering::Relaxed);
    }
    *download.phase.lock().unwrap_or_else(|p| p.into_inner()) = Phase::Preparing;
    prepare(spec, &staging)?;
    let target = model_dir(paths, spec.id);
    let _ = fs::remove_dir_all(&target);
    fs::rename(&staging, &target)
        .map_err(|error| format!("Couldn't finish {}: {error}", spec.title))
}

fn fetch_file(
    spec: &ModelSpec,
    file: &RemoteFile,
    staging: &Path,
    done_before: u64,
    download: &Download,
) -> Result<(), String> {
    let part = staging.join(format!("{}.part", file.name));
    let url = spec.url(file);
    let agent: ureq::Agent = crate::http_agent::config()
        .timeout_global(Some(RANGE_TIMEOUT))
        .build()
        .into();
    loop {
        let have = fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
        download
            .received
            .store(done_before + have, Ordering::Relaxed);
        if have >= file.bytes {
            break;
        }
        let end = (have + RANGE_BYTES).min(file.bytes) - 1;
        let mut tries = 0;
        loop {
            if download.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            match fetch_range(&agent, &url, &part, have, end, done_before, download) {
                Ok(()) => break,
                Err(message) if message == "cancelled" => return Err(message),
                Err(message) => {
                    tries += 1;
                    if tries >= RANGE_TRIES {
                        return Err(message);
                    }
                    std::thread::sleep(Duration::from_secs(2 * u64::from(tries)));
                }
            }
        }
    }
    let digest = sha256_file(&part)?;
    if digest != file.sha256 {
        let _ = fs::remove_file(&part);
        return Err(format!(
            "{} didn't download correctly (its checksum doesn't match). Download again.",
            spec.title
        ));
    }
    fs::rename(&part, staging.join(file.name)).map_err(|error| error.to_string())
}

fn fetch_range(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    start: u64,
    end: u64,
    done_before: u64,
    download: &Download,
) -> Result<(), String> {
    let offline = |error: &dyn std::fmt::Display| {
        format!("Couldn't reach huggingface.co ({error}). Check this computer's internet connection, then download again.")
    };
    let response = agent
        .get(url)
        .header("range", format!("bytes={start}-{end}"))
        .call()
        .map_err(|error| offline(&error))?;
    let status = response.status().as_u16();
    if status != 206 && !(status == 200 && start == 0) {
        return Err(format!(
            "huggingface.co answered HTTP {status}. Download again later."
        ));
    }
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part)
        .map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = start;
    let limit = end + 1;
    loop {
        if download.cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let read = reader.read(&mut buffer).map_err(|error| offline(&error))?;
        if read == 0 {
            break;
        }
        let keep = read.min((limit - written) as usize);
        out.write_all(&buffer[..keep])
            .map_err(|error| error.to_string())?;
        written += keep as u64;
        download
            .received
            .store(done_before + written, Ordering::Relaxed);
        if written >= limit {
            break;
        }
    }
    out.flush().map_err(|error| error.to_string())?;
    if written < limit {
        return Err(offline(&"the connection closed early"));
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Turns the downloaded files in `staging` into what the engine loads.
pub(crate) fn prepare(spec: &ModelSpec, staging: &Path) -> Result<(), String> {
    match spec.family {
        Family::Parakeet => {
            let source = staging.join(spec.files[0].name);
            let pack = staging.join(super::engine::PARAKEET_PACK);
            let model = telmo::model::ModelType::Parakeet;
            let precision = model.recommended_precision();
            let written = (|| {
                let metadata = telmo::pack::metadata_for(&source, precision);
                let mut sink = telmo::pack::FileSink::create(&pack, model.pack_model(), metadata)?;
                model.convert_path(&source, precision, &mut sink)?;
                sink.finish()
            })();
            written.map_err(|error| format!("Couldn't prepare {}: {error}", spec.title))?;
            for file in spec.files {
                let _ = fs::remove_file(staging.join(file.name));
            }
            Ok(())
        }
        Family::Whisper => super::whisper::prepare(staging),
    }
}

/// Every model's view, in catalog order.
pub(crate) fn list(paths: &GxserverPaths, in_use: &str) -> Vec<Value> {
    catalog::MODELS
        .iter()
        .map(|spec| view(paths, spec, spec.id == in_use))
        .collect()
}
