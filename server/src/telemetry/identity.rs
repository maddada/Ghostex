//! CDXC:Telemetry 2026-11-26 DECISION: Use a fresh random UUIDv4 for weekly counts after the Settings opt-out (Q34). Retire the outgoing identity on each mode change so weekly-only counts cannot join a full analytics profile. Keep Q17's rule that telemetry never reads agent credentials or account IDs for identity.

use std::{fs, path::PathBuf};

use uuid::{Uuid, Version};

use crate::paths::GxserverPaths;

use super::gate::TelemetryMode;

pub(super) const FULL_ID_FILE: &str = "analytics-install-id";
pub(super) const WEEKLY_ID_FILE: &str = "analytics-weekly-id";

pub(super) fn file_for_mode(mode: TelemetryMode) -> Option<&'static str> {
    match mode {
        TelemetryMode::Full => Some(FULL_ID_FILE),
        TelemetryMode::WeeklyCountOnly => Some(WEEKLY_ID_FILE),
        TelemetryMode::Off => None,
    }
}

fn install_id_file(paths: &GxserverPaths, filename: &str) -> PathBuf {
    paths.root_dir.join(filename)
}

pub(crate) fn read_mode_id(paths: &GxserverPaths, mode: TelemetryMode) -> Option<String> {
    read_install_id(paths, file_for_mode(mode)?)
}

pub(super) fn read_install_id(paths: &GxserverPaths, filename: &str) -> Option<String> {
    let raw = fs::read_to_string(install_id_file(paths, filename)).ok()?;
    parse_uuid_v4(&raw)
}

/// Generate a fresh random UUIDv4 string in canonical lowercase hyphenated form.
pub fn generate_install_id() -> String {
    Uuid::new_v4().hyphenated().to_string()
}

/// Persist the mode's ID with mode 0600 on Unix, best-effort.
pub(super) fn persist_install_id(paths: &GxserverPaths, filename: &str, install_id: &str) {
    if parse_uuid_v4(install_id).is_none() {
        return;
    }
    if let Err(error) = fs::create_dir_all(&paths.root_dir) {
        super::debug_log(format!("telemetry install id dir failed: {error}"));
        return;
    }
    let path = install_id_file(paths, filename);
    if let Err(error) = fs::write(&path, format!("{install_id}\n")) {
        super::debug_log(format!("telemetry install id write failed: {error}"));
        return;
    }
    set_file_mode_0600(&path);
}

pub(super) fn delete_install_id(paths: &GxserverPaths, filename: &str) {
    let path = install_id_file(paths, filename);
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
