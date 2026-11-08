//! Downloads the GBots runtime the app sealed (runtime.rs) onto the computer GBots runs on, with
//! progress the GBots card shows, and installs it.
//!
//! CDXC:GBots 2026-11-07 DECISION:
//! User: the runtime is "only downloaded if extension is enabled by the user", like CEF and the Code editor. The gxserver of the GBots computer downloads it (not the desktop that turned GBots on), because "everything is ran there only" (Q1507): that computer may be another one than the desktop's. The archive is checked against the size and SHA-256 the release sealed before anything is unpacked, unpacked into a staging folder, checked to hold a runnable runtime, and only then moved into place, so a broken download never leaves a half-installed runtime. A failed download is retried on its own (30s, doubling to 10 minutes); the card's Retry starts it at once.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

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
    let destination = runtime::install_dir();
    crate::components::download_and_install(
        component,
        "GBots",
        &destination,
        &|done, total| download().phase = Phase::Downloading { done, total },
        &|| download().phase = Phase::Installing,
        &|staging| {
            runtime::read_runtime(staging)
                .map(|_| ())
                .ok_or_else(|| "The GBots download holds no runnable runtime.".to_string())
        },
    )?;
    crate::components::prune_other_versions(runtime::GBOTS_COMPONENT, &destination);
    Ok(())
}
