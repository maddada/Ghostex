use std::{
    fs,
    fs::File,
    io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;

use super::*;
#[cfg(target_os = "macos")]
use std::process::Command;

pub(crate) fn unpack_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| {
        format!(
            "Could not open verified component archive {}: {error}",
            archive_path.display()
        )
    })?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(destination).map_err(|error| {
        format!(
            "Could not unpack verified component archive {}: {error}",
            archive_path.display()
        )
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn remove_macos_quarantine(path: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(path)
        .status()
        .map_err(|error| {
            format!(
                "Could not strip macOS quarantine from verified component {}: {error}",
                path.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "Could not strip macOS quarantine from verified component {}: xattr exited with {status}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn remove_macos_quarantine(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(crate) fn write_install_marker(
    path: &Path,
    name: &str,
    version: &str,
    platform: &str,
    sha256: &str,
) -> Result<(), String> {
    let marker = serde_json::json!({
        "name": name,
        "version": version,
        "platform": platform,
        "sha256": sha256,
    });
    fs::write(
        path.join(".ghostex-component.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&marker).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| format!("Could not write component install marker: {error}"))
}

/// What an install's `.ghostex-component.json` says it holds.
pub(crate) struct InstallMarker {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) platform: String,
    pub(crate) sha256: String,
}

/// The install marker in `path`, when it is there and well formed.
pub(crate) fn read_install_marker(path: &Path) -> Option<InstallMarker> {
    let data = fs::read_to_string(path.join(".ghostex-component.json")).ok()?;
    let marker = serde_json::from_str::<serde_json::Value>(&data).ok()?;
    let field = |key: &str| {
        marker
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let sha256 = field("sha256").filter(|sha256| valid_sha256(sha256))?;
    Some(InstallMarker {
        name: field("name")?,
        version: field("version")?,
        platform: field("platform")?,
        sha256,
    })
}

pub(crate) fn installed_marker_matches(
    path: &Path,
    name: &str,
    version: &str,
    platform: &str,
    expected_sha256: Option<&str>,
) -> Result<bool, String> {
    if !path.is_dir() {
        return Ok(false);
    }
    let marker_path = path.join(".ghostex-component.json");
    let data = match fs::read_to_string(&marker_path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Could not read component marker {}: {error}",
                marker_path.display()
            ));
        }
    };
    let marker = serde_json::from_str::<serde_json::Value>(&data).map_err(|error| {
        format!(
            "Malformed component marker {}: {error}",
            marker_path.display()
        )
    })?;
    Ok(
        marker.get("name").and_then(serde_json::Value::as_str) == Some(name)
            && marker.get("version").and_then(serde_json::Value::as_str) == Some(version)
            && marker.get("platform").and_then(serde_json::Value::as_str) == Some(platform)
            && marker
                .get("sha256")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|sha256| {
                    valid_sha256(sha256)
                        && expected_sha256.is_none_or(|expected| sha256 == expected)
                }),
    )
}

pub(crate) fn directory_size(path: &Path) -> Result<u64, String> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)
        .map_err(|error| format!("Could not measure {}: {error}", path.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Could not measure {}: {error}", path.display()))?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(|error| format!("Could not measure {}: {error}", entry.path().display()))?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

pub(crate) fn prune_temporary_install_artifacts(version_root: &Path) {
    let Ok(entries) = fs::read_dir(version_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".download-")
            && !name.starts_with(".install-")
            && !name.starts_with(".previous-")
        {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                let _ = fs::remove_dir_all(path);
            }
            Ok(_) => {
                let _ = fs::remove_file(path);
            }
            Err(_) => {}
        }
    }
}

/// Removes every version folder under `component_root` but `retained_version`, as far as it can.
///
/// CDXC:Components 2026-11-13 WHY:
/// GitHub #581 (Q2787): on Windows an old web runtime's files can still be open (a CEF helper still running, an antivirus scan, the search indexer), so removing its folder failed with "Access is denied" and that failure stopped the whole ensure: web pages and the in-app feedback form said "The web runtime needs another try" although the current version was installed and fine. Pruning is housekeeping, so a folder it can't remove is logged and left for the next ensure, which runs at every launch and tries again; the other old folders are still removed.
pub(crate) fn prune_other_versions(component_root: &Path, retained_version: &str) {
    let entries = match fs::read_dir(component_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            log::warn!(
                "Could not prune component versions under {}: {error}",
                component_root.display()
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy() == retained_version
            || !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
        {
            continue;
        }
        if let Err(error) = fs::remove_dir_all(entry.path()) {
            log::warn!(
                "Could not prune old component version {} (trying again next time): {error}",
                entry.path().display()
            );
        }
    }
}

/// CDXC:CodeEditor 2026-10-20 WHY: code-server component versions are 84 characters, and under `%LOCALAPPDATA%\GhostexData\components\code-server\<version>\windows-native-x64\` the native editor's deepest files reached about 295 characters, past the 260 Windows still enforces on PowerShell and DLL loads. On Windows its version folder is a 16-character digest of the version instead (about 225 then); other components and platforms keep the plain version.
/// SEE-ALSO: the installed-editor scans in server/src/ghostex_cli/editors/prompt.rs and apps/desktop/src/app/helpers/remote/windows_code.rs, which match any folder name.
pub(crate) fn component_version_dir_name(name: &str, version: &str) -> String {
    if cfg!(windows) && name == CODE_SERVER_COMPONENT_NAME {
        use sha2::{Digest as _, Sha256};
        let digest = Sha256::digest(version.as_bytes());
        return digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
    }
    version.to_string()
}

pub(crate) fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
