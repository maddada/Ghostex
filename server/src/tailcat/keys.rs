use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{bail, Context, Result};

use crate::paths::GxserverPaths;

pub fn tailcat_key_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("server.private.json")
}

/// The path of the tailcat that hosts Easy Connect with the key above (`resolve_host_tailcat_binary`).
pub fn tailcat_host_binary_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("host-binary.txt")
}

pub fn tailcat_address_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("address.txt")
}

/// The second Easy Connect key, on the relay `host` (`TailcatRelaySetting`). One file per relay,
/// kept when Settings moves to another, so moving back gives the same address again.
pub fn tailcat_relay_key_file(paths: &GxserverPaths, host: &str) -> PathBuf {
    paths
        .tailcat_dir
        .join("relays")
        .join(format!("{host}.private.json"))
}

/// Where the second helper writes its address (the first one's is [`tailcat_address_file`]).
pub fn tailcat_relay_address_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("relay-address.txt")
}

/*
CDXC:RemotePairing 2026-09-01:
The server key IS the identity every paired device dialed, so it is created
exactly once and never regenerated: overwriting it would silently orphan every
saved address blob. `--fixed-region` is what makes the derived blob stable
across restarts, which is the whole reason the key is persisted at all.
*/
pub fn ensure_tailcat_key_file(binary_path: &Path, key_file: &Path) -> Result<()> {
    run_genkey(binary_path, key_file, "--fixed-region").map(|_| ())
}

/// The second key, pinned to `host` (`genkey --region=<host>`, which needs no network): its
/// address carries that relay and nothing else, so a phone dialing it never touches the public
/// relays. Created once per relay, like the first key.
pub fn ensure_tailcat_relay_key_file(
    binary_path: &Path,
    key_file: &Path,
    host: &str,
) -> Result<()> {
    run_genkey(binary_path, key_file, &format!("--region={host}")).map(|_| ())
}

/// Makes a key file like the two above (pinned to the relay `host`, or on the public relays) and
/// returns the address tailcat prints for it; `None` when the file was there already.
pub(crate) fn make_tailcat_key_file(
    binary_path: &Path,
    key_file: &Path,
    host: Option<&str>,
) -> Result<Option<String>> {
    match host {
        Some(host) => run_genkey(binary_path, key_file, &format!("--region={host}")),
        None => run_genkey(binary_path, key_file, "--fixed-region"),
    }
}

/// `tailcat genkey`: the address it prints for the new key, or `None` when the file exists.
fn run_genkey(binary_path: &Path, key_file: &Path, region_arg: &str) -> Result<Option<String>> {
    if key_file.is_file() {
        return Ok(None);
    }
    if let Some(parent) = key_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| "create the tailcat state directory".to_string())?;
        set_dir_mode_0700(parent)?;
    }
    let output = crate::platform::process::background_command(binary_path)
        .arg("genkey")
        .arg(format!("--key={}", key_file.display()))
        .arg(region_arg)
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
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("tc"))
        .map(str::to_string))
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
