//! Installing, updating and uninstalling the Collab component (`/api/extensionComponents`).
//!
//! - Install downloads the component the release sealed (crate::components), checked against its
//!   sealed size and SHA-256, into `<components>/collab/<version>/<platform>/`, and the supervisor
//!   starts it. It needs the owner's signed approval (`installExtensionComponent`, target
//!   `collab`): Touch ID, Windows Hello, the Linux system prompt or the phone's biometrics, so an
//!   agent can't install it.
//! - Update downloads the newer component an app update sealed and restarts it; shares carry on.
//!   No approval: the owner already let this component in.
//! - Uninstall never asks: every share ends, the component stops, and its files and data folder
//!   are deleted.
//!
//! A development build without a sealed component can install the binary named by
//! `GHOSTEX_COLLAB_DEV_BINARY` (`cargo build` in server/collab), still with the approval.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495): "when enabled, we need to show touch id / auth so we make sure it's the user enabling this feature and installing it not an agent". Uninstalling or turning off never asks.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ghostex_gx_protocol::collab_component::{
    SidecarEvent, COLLAB_COMPONENT, COLLAB_SIDECAR_BINARY,
};
use serde_json::{json, Value};

use crate::server::AppState;

pub(crate) const TITLE: &str = "Collab";
pub(crate) const DESCRIPTION: &str =
    "Share a chat through a link that opens it in a browser. People you share with need its code; their messages wait for your approval.";

/// The installed component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) version: String,
    /// `<components>/collab/<version>/<platform>`.
    pub(crate) root: PathBuf,
    pub(crate) binary: PathBuf,
}

fn binary_name() -> String {
    if cfg!(windows) {
        format!("{COLLAB_SIDECAR_BINARY}.exe")
    } else {
        COLLAB_SIDECAR_BINARY.to_string()
    }
}

fn version_dir(version: &str) -> PathBuf {
    crate::components::component_root(COLLAB_COMPONENT)
        .join(version)
        .join(crate::components::platform_dir())
}

/// The installed component: the sealed version's when it is there, else any other version's
/// (an app update sealed a newer one, which Settings › Updates offers).
pub(crate) fn installed() -> Option<Installed> {
    let read = |version: &str| {
        let root = version_dir(version);
        let binary = root.join(binary_name());
        binary.is_file().then(|| Installed {
            version: version.to_string(),
            root,
            binary,
        })
    };
    if let Some(sealed) = crate::components::sealed(COLLAB_COMPONENT) {
        if let Some(found) = read(&sealed.component_version) {
            return Some(found);
        }
    }
    let entries = std::fs::read_dir(crate::components::component_root(COLLAB_COMPONENT)).ok()?;
    let mut versions: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.starts_with('.'))
        .collect();
    versions.sort();
    versions.iter().rev().find_map(|version| read(version))
}

fn dev_binary() -> Option<PathBuf> {
    std::env::var_os("GHOSTEX_COLLAB_DEV_BINARY")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

/// Whether this app has a component to install (a sealed one, or a development binary).
pub(crate) fn installable() -> bool {
    crate::components::sealed(COLLAB_COMPONENT).is_some() || dev_binary().is_some()
}

/// The sealed version newer than the installed one, when an app update brought it.
pub(crate) fn update_available() -> Option<String> {
    let installed = installed()?;
    let sealed = crate::components::sealed(COLLAB_COMPONENT)?;
    (installed.version != "dev" && installed.version != sealed.component_version)
        .then_some(sealed.component_version)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Downloading { done: u64, total: u64 },
    Installing,
    Removing,
    Failed(String),
}

static PHASE: Mutex<Phase> = Mutex::new(Phase::Idle);

fn phase() -> std::sync::MutexGuard<'static, Phase> {
    PHASE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Collab as Settings › Extensions, Settings › Updates and the phone show it.
pub(crate) fn item() -> Value {
    let installed = installed();
    let sealed = crate::components::sealed(COLLAB_COMPONENT);
    let (phase_name, done, total, error) = match &*phase() {
        Phase::Idle => ("idle", None, None, None),
        Phase::Downloading { done, total } => ("downloading", Some(*done), Some(*total), None),
        Phase::Installing => ("installing", None, None, None),
        Phase::Removing => ("removing", None, None, None),
        Phase::Failed(reason) => ("failed", None, None, Some(reason.clone())),
    };
    let sidecar = super::sidecar::status_value();
    json!({
        "name": COLLAB_COMPONENT,
        "title": TITLE,
        "description": DESCRIPTION,
        "installed": installed.is_some(),
        "version": installed.as_ref().map(|installed| installed.version.clone()),
        "availableVersion": sealed.as_ref().map(|sealed| sealed.component_version.clone()),
        "updateAvailable": update_available(),
        "canInstall": installable(),
        "needsApproval": true,
        "phase": phase_name,
        "doneBytes": done,
        "totalBytes": total,
        "error": error,
        "status": sidecar["status"],
        "statusError": sidecar["error"],
        "running": super::sidecar::is_running(),
        // "Don't show Share" (the not-installed Share popover): Extensions offers to show it again.
        "shareHidden": super::share_hidden(),
    })
}

fn check_staging(staging: &Path) -> Result<(), String> {
    staging
        .join(binary_name())
        .is_file()
        .then_some(())
        .ok_or_else(|| "The Collab download holds no Collab component.".to_string())
}

/// Puts the component in place (blocking): the sealed download, or the development binary.
fn fetch_into_place() -> Result<PathBuf, String> {
    if let Some(sealed) = crate::components::sealed(COLLAB_COMPONENT) {
        let destination = version_dir(&sealed.component_version);
        crate::components::download_and_install(
            &sealed,
            TITLE,
            &destination,
            &|done, total| *phase() = Phase::Downloading { done, total },
            &|| *phase() = Phase::Installing,
            &check_staging,
        )?;
        return Ok(destination);
    }
    let binary = dev_binary()
        .ok_or_else(|| "This build of Ghostex has no Collab component to install.".to_string())?;
    *phase() = Phase::Installing;
    let destination = version_dir("dev");
    std::fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    let target = destination.join(binary_name());
    std::fs::copy(&binary, &target)
        .map_err(|error| format!("Could not install Collab: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(destination)
}

/// Downloads and installs on a thread of its own; `done(ok)` runs when it finished.
pub(crate) fn start_install(
    state: Arc<AppState>,
    done: Box<dyn FnOnce(bool) + Send>,
) -> Result<(), String> {
    if !installable() {
        return Err("This build of Ghostex has no Collab component to install.".to_string());
    }
    {
        let mut phase = phase();
        if matches!(
            *phase,
            Phase::Downloading { .. } | Phase::Installing | Phase::Removing
        ) {
            return Ok(());
        }
        *phase = Phase::Downloading { done: 0, total: 0 };
    }
    let _ = std::thread::Builder::new()
        .name("collab-install".to_string())
        .spawn(move || {
            let result = fetch_into_place();
            match &result {
                Ok(destination) => {
                    crate::components::prune_other_versions(COLLAB_COMPONENT, destination);
                    *phase() = Phase::Idle;
                }
                Err(reason) => *phase() = Phase::Failed(reason.clone()),
            }
            super::sidecar::supervise_once(&state);
            done(result.is_ok());
        });
    Ok(())
}

/// Downloads the newer sealed component and restarts Collab on it (blocking until it is in
/// place; on a thread of its own).
pub(crate) fn start_update(state: Arc<AppState>) -> Result<(), String> {
    if installed().is_none() {
        return Err("Collab isn't installed.".to_string());
    }
    if update_available().is_none() {
        return Ok(());
    }
    start_install(
        state,
        Box::new(|ok| {
            if ok {
                // The old version was pruned: the supervisor starts the new one.
                super::sidecar::stop_now();
            }
        }),
    )
}

/// Ends every share, stops the component and deletes its files and data (blocking).
pub(crate) fn uninstall(state: &Arc<AppState>) -> Result<(), String> {
    *phase() = Phase::Removing;
    super::sidecar::set_paused(true);
    // The component ends its shares itself and tells gxserver (no token stays open), then stops.
    let _ = super::sidecar::send_event(&SidecarEvent::EndAll);
    super::sidecar::stop_now();
    let mut result = Ok(());
    for folder in [
        crate::components::component_root(COLLAB_COMPONENT),
        super::sidecar::data_dir(state),
    ] {
        if let Err(error) = std::fs::remove_dir_all(&folder) {
            if error.kind() != std::io::ErrorKind::NotFound {
                result = Err(format!("Could not delete {}: {error}", folder.display()));
            }
        }
    }
    super::sidecar::set_paused(false);
    super::sidecar::supervise_once(state);
    super::republish_followed(state);
    *phase() = match &result {
        Ok(()) => Phase::Idle,
        Err(reason) => Phase::Failed(reason.clone()),
    };
    result
}
