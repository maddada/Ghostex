/*
CDXC:Telemetry 2026-11-24 (privacy overhaul, Q17):
`distinct_id` is strictly a random per-install UUIDv4, stored in
`analytics-install-id` inside the gxserver state directory.

Earlier revisions hashed account ids from `~/.codex/auth.json` and
`~/.claude.json` to deduplicate one person across machines. User decision Q17
removed all third-party account file reads from telemetry: analytics must never
read agent credentials or auth files and must rely only on a random per-install
UUIDv4.

The file is created lazily only when analytics are enabled and ready to flush,
and deleted when the user opts out, so an opted-out or remote-helper install
leaves no analytics identifier on disk.
*/

use std::{fs, path::PathBuf};

use uuid::{Uuid, Version};

use crate::paths::GxserverPaths;

pub const INSTALL_ID_FILE_NAME: &str = "analytics-install-id";

pub fn install_id_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join(INSTALL_ID_FILE_NAME)
}

/// Read an existing valid UUIDv4 install id from disk without creating one.
pub fn read_existing_install_id(paths: &GxserverPaths) -> Option<String> {
    let raw = fs::read_to_string(install_id_file(paths)).ok()?;
    parse_uuid_v4(&raw)
}

/// Generate a fresh random UUIDv4 string in canonical lowercase hyphenated form.
pub fn generate_install_id() -> String {
    Uuid::new_v4().hyphenated().to_string()
}

/// Load the durable per-install UUIDv4, or generate and persist a fresh one.
/// Persistence is best-effort so a read-only filesystem never breaks the daemon.
pub fn load_or_create_install_id(paths: &GxserverPaths) -> String {
    if let Some(existing) = read_existing_install_id(paths) {
        return existing;
    }
    let fresh = generate_install_id();
    persist_install_id(paths, &fresh);
    fresh
}

/// Persist `install_id` to `analytics-install-id` (mode 0600 on Unix), best-effort.
pub fn persist_install_id(paths: &GxserverPaths, install_id: &str) {
    if parse_uuid_v4(install_id).is_none() {
        return;
    }
    if let Err(error) = fs::create_dir_all(&paths.root_dir) {
        super::debug_log(format!("telemetry install id dir failed: {error}"));
        return;
    }
    let path = install_id_file(paths);
    if let Err(error) = fs::write(&path, format!("{install_id}\n")) {
        super::debug_log(format!("telemetry install id write failed: {error}"));
        return;
    }
    set_file_mode_0600(&path);
}

/// Delete the persisted `analytics-install-id` file (called on opt-out wipe).
pub fn delete_install_id(paths: &GxserverPaths) {
    let path = install_id_file(paths);
    if let Err(error) = fs::remove_file(&path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            super::debug_log(format!("telemetry install id delete failed: {error}"));
        }
    }
}

fn parse_uuid_v4(raw: &str) -> Option<String> {
    let parsed = Uuid::parse_str(raw.trim()).ok()?;
    (parsed.get_version() == Some(Version::Random)).then(|| parsed.hyphenated().to_string())
}

#[cfg(unix)]
fn set_file_mode_0600(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_file_mode_0600(_path: &std::path::Path) {}
