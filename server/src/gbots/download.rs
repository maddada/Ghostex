//! Downloads the GBots runtime the app sealed (runtime.rs) onto the computer GBots runs on, with
//! progress the GBots card shows, and installs it.
//!
//! CDXC:GBots 2026-11-07 DECISION:
//! User: the runtime is "only downloaded if extension is enabled by the user", like CEF and the Code editor. The gxserver of the GBots computer downloads it (not the desktop that turned GBots on), because "everything is ran there only" (Q1507): that computer may be another one than the desktop's. The archive is checked against the size and SHA-256 the release sealed before anything is unpacked, unpacked into a staging folder, checked to hold a runnable runtime, and only then moved into place, so a broken download never leaves a half-installed runtime. A failed download is retried on its own (30s, doubling to 10 minutes); the card's Retry starts it at once.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

use super::runtime::{self, SealedComponent};

const FIRST_RETRY: Duration = Duration::from_secs(30);
const LAST_RETRY: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Downloading { done: u64, total: u64 },
    Installing,
    Failed(String),
}

struct Download {
    phase: Phase,
    retry_at: Option<Instant>,
    retry_wait: Duration,
}

static DOWNLOAD: Mutex<Download> = Mutex::new(Download {
    phase: Phase::Idle,
    retry_at: None,
    retry_wait: FIRST_RETRY,
});

fn download() -> std::sync::MutexGuard<'static, Download> {
    DOWNLOAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The download's state for the GBots card: `idle`, `downloading` (with bytes), `installing`
/// or `failed` (with the reason).
pub(crate) fn status_value() -> Value {
    match &download().phase {
        Phase::Idle => json!({ "phase": "idle" }),
        Phase::Downloading { done, total } => {
            json!({ "phase": "downloading", "doneBytes": done, "totalBytes": total })
        }
        Phase::Installing => json!({ "phase": "installing" }),
        Phase::Failed(reason) => json!({ "phase": "failed", "error": reason }),
    }
}

/// Whether a download or install is under way.
pub(crate) fn busy() -> bool {
    matches!(
        download().phase,
        Phase::Downloading { .. } | Phase::Installing
    )
}

/// The card's Retry: the next look starts the download at once.
pub(crate) fn retry_now() {
    let mut download = download();
    if matches!(download.phase, Phase::Failed(_)) {
        download.phase = Phase::Idle;
    }
    download.retry_at = None;
}

/// Starts the download when the runtime is missing here, nothing runs yet and no retry wait is
/// pending. `false` when the app has no sealed GBots component to download.
pub(crate) fn ensure_downloading() -> bool {
    let Some(component) = runtime::sealed_component() else {
        return false;
    };
    {
        let mut download = download();
        match download.phase {
            Phase::Downloading { .. } | Phase::Installing => return true,
            Phase::Failed(_) if download.retry_at.is_some_and(|at| Instant::now() < at) => {
                return true
            }
            _ => {}
        }
        download.phase = Phase::Downloading {
            done: 0,
            total: component.size_bytes,
        };
    }
    let _ = std::thread::Builder::new()
        .name("gbots-download".to_string())
        .spawn(move || {
            let result = fetch_and_install(&component);
            let mut download = download();
            match result {
                Ok(()) => {
                    download.phase = Phase::Idle;
                    download.retry_at = None;
                    download.retry_wait = FIRST_RETRY;
                }
                Err(reason) => {
                    download.phase = Phase::Failed(reason);
                    download.retry_at = Some(Instant::now() + download.retry_wait);
                    download.retry_wait = (download.retry_wait * 2).min(LAST_RETRY);
                }
            }
        });
    true
}

fn fetch_and_install(component: &SealedComponent) -> Result<(), String> {
    let root = runtime::component_root();
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Could not make {}: {error}", root.display()))?;
    let stamp = format!("{}-{}", std::process::id(), unique());
    let archive = root.join(format!(".download-{stamp}.tar.gz"));
    let staging = root.join(format!(".install-{stamp}"));
    let result = fetch(component, &archive).and_then(|()| {
        download().phase = Phase::Installing;
        install(&archive, &staging)
    });
    let _ = std::fs::remove_file(&archive);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default()
}

/// Streams the archive to `path`, hashing as it goes, and checks its size and SHA-256.
fn fetch(component: &SealedComponent, path: &Path) -> Result<(), String> {
    let response = crate::http_agent::agent()
        .get(&component.url)
        .config()
        .timeout_global(None)
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|error| {
            if crate::http_agent::never_connected(&error) {
                "Could not reach GitHub to download GBots. It tries again on its own.".to_string()
            } else {
                format!("The GBots download failed: {error}")
            }
        })?;
    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(path)
        .map_err(|error| format!("Could not write the GBots download: {error}"))?;
    let mut hasher = Sha256::new();
    let mut done = 0_u64;
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("The GBots download stopped: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not write the GBots download: {error}"))?;
        done += read as u64;
        if done > component.size_bytes {
            return Err("The GBots download is larger than this release expects.".to_string());
        }
        download().phase = Phase::Downloading {
            done,
            total: component.size_bytes,
        };
    }
    file.flush()
        .map_err(|error| format!("Could not write the GBots download: {error}"))?;
    if done != component.size_bytes {
        return Err(format!(
            "The GBots download ended early ({done} of {} bytes).",
            component.size_bytes
        ));
    }
    let digest = hex::encode(hasher.finalize());
    if digest != component.sha256 {
        return Err("The GBots download does not match this release's checksum.".to_string());
    }
    Ok(())
}

/// Unpacks into `staging`, checks it holds a runtime, and moves it into place.
fn install(archive: &Path, staging: &Path) -> Result<(), String> {
    std::fs::create_dir_all(staging).map_err(|error| error.to_string())?;
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(staging)
        .output()
        .map_err(|error| format!("Could not unpack GBots: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not unpack GBots: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if runtime::read_runtime(staging).is_none() {
        return Err("The GBots download holds no runnable runtime.".to_string());
    }
    let destination = runtime::install_dir();
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
    }
    std::fs::rename(staging, &destination)
        .map_err(|error| format!("Could not install GBots: {error}"))?;
    prune_other_versions(&destination);
    Ok(())
}

/// Removes runtimes of other component versions once a new one is in place (never `dev`).
fn prune_other_versions(installed: &Path) {
    let keep: Option<PathBuf> = installed.parent().map(Path::to_path_buf);
    let Ok(entries) = std::fs::read_dir(runtime::component_root()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_dir()
            || name.starts_with('.')
            || name == "dev"
            || keep.as_deref() == Some(path.as_path())
        {
            continue;
        }
        let _ = std::fs::remove_dir_all(&path);
    }
}
