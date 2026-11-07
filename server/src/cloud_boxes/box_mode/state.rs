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

/// Which private folder the last boot made (one absolute path), so every verb finds it.
fn private_dir_file() -> PathBuf {
    state_dir().join("private-dir")
}

/// The private folder of this box (tmpfs; never in a snapshot): the one the last boot made, else
/// the provider's [`box_private_dir`].
pub(crate) fn private_dir(provider: &CloudProviderId) -> PathBuf {
    std::fs::read_to_string(private_dir_file())
        .ok()
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(box_private_dir(provider)))
}

/// [`private_dir`], made again when it is gone or no longer this user's (a restart empties
/// tmpfs, and a verb may run before the next boot does).
pub(crate) fn ensure_private_dir(provider: &CloudProviderId) -> Result<PathBuf, String> {
    let current = private_dir(provider);
    if own_dir(&current, false).is_ok() {
        return Ok(current);
    }
    make_private_dir(provider)
}

/// CDXC:CloudBoxes 2026-11-07 FIX:
/// Reporter (issue #433, zxcvbinz): "Creating a Cloud Box on Instabox fails at the boot step
/// with: Can't make /run/ghostex: Permission denied (os error 13) ... Every new Instabox box
/// fails the same way, so Cloud Box can't be used with Instabox at all." Instabox runs exec as
/// its guest user (`dev`), and `/run` belongs to root.
/// Boot makes the private folder the way the package install already makes `/opt/ghostex`:
/// itself when it may (root, or a folder it already owns), else with `sudo -n` and a chown to
/// the box's user, so the folder stays the provider's tmpfs path. Without passwordless sudo it
/// takes a per-user tmpfs folder (`$XDG_RUNTIME_DIR/ghostex`, else `/tmp/ghostex-<uid>`, mode
/// 0700, refused when it is a link or someone else's), remembers it, and reports it in the boot
/// result so home and the phone write secrets there too. `~/.local/state` is not a candidate:
/// Instabox snapshots save HOME, and this folder holds logins.
/// SEE-ALSO: packages/gx-cloud/src/install.rs (the same sudo rule), server/src/cloud_boxes/boot_steps.rs (home writes into the reported folder).
pub(crate) fn make_private_dir(provider: &CloudProviderId) -> Result<PathBuf, String> {
    let shared = PathBuf::from(box_private_dir(provider));
    let mut tried = Vec::new();
    let mut chosen = None;
    match own_dir(&shared, true) {
        Ok(()) => chosen = Some(shared.clone()),
        Err(error) => tried.push(error),
    }
    if chosen.is_none() && !is_root() {
        match sudo_own_dir(&shared) {
            Ok(()) => chosen = Some(shared.clone()),
            Err(error) => tried.push(error),
        }
    }
    if chosen.is_none() {
        for candidate in per_user_dirs() {
            match own_dir(&candidate, true) {
                Ok(()) => {
                    chosen = Some(candidate);
                    break;
                }
                Err(error) => tried.push(error),
            }
        }
    }
    let Some(dir) = chosen else {
        return Err(format!(
            "Can't make the box's private folder: {}",
            tried.join("; ")
        ));
    };
    write_private(&private_dir_file(), dir.to_string_lossy().as_bytes())?;
    Ok(dir)
}

/// The per-user tmpfs folders tried when the provider's folder can't be this user's.
fn per_user_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir())
    {
        dirs.push(runtime.join("ghostex"));
    }
    dirs.push(PathBuf::from(format!("/tmp/ghostex-{}", effective_uid())));
    dirs
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn effective_uid() -> u32 {
    0
}

fn is_root() -> bool {
    effective_uid() == 0
}

/// Makes `dir` (with `create`) and checks it is a real folder this user owns (root may use any),
/// then sets mode 0700.
fn own_dir(dir: &Path, create: bool) -> Result<(), String> {
    let shown = dir.display();
    if create {
        std::fs::create_dir_all(dir).map_err(|error| format!("Can't make {shown}: {error}"))?;
    }
    let metadata =
        std::fs::symlink_metadata(dir).map_err(|error| format!("Can't read {shown}: {error}"))?;
    if !metadata.is_dir() {
        return Err(format!("{shown} is not a folder"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if !is_root() && metadata.uid() != effective_uid() {
            return Err(format!("{shown} belongs to another user"));
        }
        if metadata.mode() & 0o777 != 0o700 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("Can't make {shown} private: {error}"))?;
        }
    }
    Ok(())
}

/// Makes `dir` this user's with `sudo -n` (passwordless sudo only; never prompts).
fn sudo_own_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        // SAFETY: getegid has no preconditions and cannot fail.
        let owner = format!("{}:{}", effective_uid(), unsafe { libc::getegid() });
        let output = std::process::Command::new("sudo")
            .args([
                "-n",
                "sh",
                "-c",
                r#"mkdir -p -- "$1" && chown -- "$2" "$1" && chmod 700 -- "$1""#,
                "sh",
            ])
            .arg(dir)
            .arg(&owner)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("Can't run sudo for {}: {error}", dir.display()))?;
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "sudo can't make {}: {}",
                dir.display(),
                if reason.is_empty() {
                    output.status.to_string()
                } else {
                    reason
                }
            ));
        }
        own_dir(dir, false)
    }
    #[cfg(not(unix))]
    Err(format!("Can't make {} without sudo.", dir.display()))
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
