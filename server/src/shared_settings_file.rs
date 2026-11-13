//! gxserver's and the CLI's read of the desktop's shared settings file. They only read it; the
//! desktop's settings service is its one writer, through the same `ghostex_paths::settings_file`,
//! except on a computer no desktop app ever saved settings on (headless_settings.rs).
//!
//! CDXC:Settings 2026-10-31 WHY:
//! After a power loss gxserver can read the damaged file before the app has started, and reading it as "nothing configured" would act on defaults: telemetry for a person who opted out, the wrong auto-sleep. So every reader goes through the repair, which brings the last good copy back. The one exception is the diagnostic gate in logging/logger.rs: it reads with its own cache mutex held and this module logs through that logger, so it treats a damaged file as "diagnostics off" until another reader repairs it.
//! SEE-ALSO: packages/paths/src/settings_file.rs, apps/desktop/src/shared_settings/service.rs.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use serde_json::Value;

use crate::{
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    paths::{get_gxserver_paths, GxserverPaths},
};

const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";

/// Where the settings file is (`server/shared_settings_watch.rs` follows its size and time).
pub(crate) fn shared_settings_path(paths: &GxserverPaths) -> std::path::PathBuf {
    paths.app_config_dir.join(SETTINGS_FILE_NAME)
}

/// The settings as a JSON object. `None` means nothing is configured, which callers read as their
/// shipped defaults: no settings file, or the system refused the read (logged once per process).
pub(crate) fn read_shared_settings(paths: &GxserverPaths) -> Option<Value> {
    read(&paths.app_config_dir, || paths.clone())
}

/// [`read_shared_settings`] for a caller that holds only the config folder.
pub(crate) fn read_shared_settings_in(app_config_dir: &Path) -> Option<Value> {
    read(app_config_dir, || get_gxserver_paths(None))
}

fn read(app_config_dir: &Path, log_paths: impl FnOnce() -> GxserverPaths) -> Option<Value> {
    read_checked(app_config_dir, log_paths).unwrap_or(None)
}

/// [`read_shared_settings`] that tells a refused read (`Err`) from a missing file (`Ok(None)`),
/// for a reader that must keep what it had rather than fall back to the defaults.
pub(crate) fn read_shared_settings_checked(
    paths: &GxserverPaths,
) -> Result<Option<Value>, std::io::Error> {
    read_checked(&paths.app_config_dir, || paths.clone())
}

fn read_checked(
    app_config_dir: &Path,
    log_paths: impl FnOnce() -> GxserverPaths,
) -> Result<Option<Value>, std::io::Error> {
    match ghostex_paths::settings_file::read(&app_config_dir.join(SETTINGS_FILE_NAME)) {
        Ok(read) => {
            if !read.problems.is_empty() {
                log_problems(&log_paths(), &read.problems);
            }
            Ok(read.object.map(|mut object| {
                // Enable Experimental Features is one switch each now; until the app saves that
                // (or a gxserver-owned file is saved), every read sees the switches it had
                // (CDXC:Extensions 2026-11-09 in the settings catalog).
                ghostex_settings_catalog::migrate_experimental_features_switch(&mut object);
                // Agent work and Tool call cards are retired (CDXC:SessionChat 2026-11-13).
                ghostex_settings_catalog::migrate_retired_chat_looks(&mut object);
                // Marked handovers is Off, Coordinators only or Every session (CDXC:MarkedHandovers 2026-11-13).
                ghostex_settings_catalog::migrate_marked_handovers(&mut object);
                Value::Object(object)
            }))
        }
        Err(error) => {
            log_refused_read(log_paths, &error);
            Err(error)
        }
    }
}

pub(crate) fn log_problems(paths: &GxserverPaths, problems: &[String]) {
    for problem in problems {
        log_error(paths, problem.clone());
    }
}

fn log_refused_read(log_paths: impl FnOnce() -> GxserverPaths, error: &std::io::Error) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        log_error(
            &log_paths(),
            format!("could not read the settings file: {error}"),
        );
    }
}

fn log_error(paths: &GxserverPaths, message: String) {
    let _ = GxserverLogger::new(paths.clone()).log(GxserverLogInput {
        level: LogLevel::Error,
        event: "settings.fileDamaged".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message),
        details: None,
    });
}
