//! What box mode keeps on the box: `~/.ghostex-box/` on disk (which box this is, what box mode
//! placed where, the idle countdown) and the private folder (tmpfs: logins, secrets, and the
//! identity files `prepare-snapshot` moves out of the way).

use std::path::{Path, PathBuf};

use ghostex_gx_protocol::cloud_box::{box_private_dir, BoxBootTicket, CloudProviderId};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// `~/.ghostex-box/`.
pub(crate) fn state_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"))
        .join(".ghostex-box")
}

/// The last boot ticket. Its presence is what makes this machine a box.
pub(crate) fn ticket_file() -> PathBuf {
    state_dir().join("box.json")
}

pub(crate) fn identity_file() -> PathBuf {
    state_dir().join("identity.json")
}

fn placed_file() -> PathBuf {
    state_dir().join("placed.json")
}

pub(crate) fn idle_file() -> PathBuf {
    state_dir().join("idle.json")
}

pub(crate) fn idle_watch_pid_file() -> PathBuf {
    state_dir().join("idle-watch.pid")
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Writes `value` through a temporary file so a reader never sees half a file.
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_private(
        path,
        &serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
}

/// Writes a file only its owner can read (mode 0600), creating its folder (mode 0700).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("Can't write {}: {error}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
        set_mode(parent, 0o700);
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes).map_err(failed)?;
    set_mode(&temporary, 0o600);
    std::fs::rename(&temporary, path).map_err(failed)
}

pub(crate) fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

pub(crate) fn read_ticket() -> Option<BoxBootTicket> {
    read_json(&ticket_file())
}

/// The box's ticket, or the error every box verb gives on a machine that was never booted.
pub(crate) fn require_ticket() -> Result<BoxBootTicket, String> {
    read_ticket().ok_or_else(|| {
        "This machine is not a Cloud Box (ghostex box boot never ran here).".to_string()
    })
}

/// The private folder of this box's provider (tmpfs; never in a snapshot).
pub(crate) fn private_dir(provider: &CloudProviderId) -> PathBuf {
    PathBuf::from(box_private_dir(provider))
}

/// What box mode put into the box's ordinary disk, so `prepare-snapshot` can take it out and
/// `work-status` knows which checkouts to look at.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Placed {
    /// Folders sessions run in (a checkout or a worktree on the box).
    #[serde(default)]
    pub(crate) checkouts: Vec<String>,
    /// Ticked secret files copied into a checkout: absolute path and the project-relative path
    /// they came from in `secrets.json`.
    #[serde(default)]
    pub(crate) secret_files: Vec<PlacedSecretFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacedSecretFile {
    pub(crate) path: String,
    pub(crate) source: String,
}

pub(crate) fn read_placed() -> Placed {
    read_json(&placed_file()).unwrap_or_default()
}

pub(crate) fn write_placed(placed: &Placed) -> Result<(), String> {
    write_json(&placed_file(), placed)
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
