use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Result};

use crate::paths::GxserverPaths;

pub fn tailcat_key_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("server.private.json")
}

pub fn tailcat_address_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("address.txt")
}

/*
CDXC:RemotePairing 2026-09-01:
The server key IS the identity every paired device dialed, so it is created
exactly once and never regenerated: overwriting it would silently orphan every
saved address blob. `--fixed-region` is what makes the derived blob stable
across restarts, which is the whole reason the key is persisted at all.
*/
pub fn ensure_tailcat_key_file(binary_path: &Path, key_file: &Path) -> Result<()> {
    if key_file.is_file() {
        return Ok(());
    }
    if let Some(parent) = key_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| "create the tailcat state directory".to_string())?;
        set_dir_mode_0700(parent)?;
    }
    let output = Command::new(binary_path)
        .arg("genkey")
        .arg(format!("--key={}", key_file.display()))
        .arg("--fixed-region")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| "run tailcat genkey")?;
    if !output.status.success() {
        bail!(
            "tailcat genkey exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !key_file.is_file() {
        bail!("tailcat genkey did not write a server key file.");
    }
    set_file_mode_0600(key_file)?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_mode_0700(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| "restrict the tailcat state directory permissions")
}

#[cfg(not(unix))]
fn set_dir_mode_0700(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_mode_0600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| "restrict the tailcat server key permissions")
}

#[cfg(not(unix))]
fn set_file_mode_0600(_path: &Path) -> Result<()> {
    Ok(())
}
