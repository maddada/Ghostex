//! gxserver's and the CLI's read of the desktop's shared settings file. They only read it; the
//! desktop's settings service is its one writer, through the same `ghostex_paths::settings_file`.
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
    match ghostex_paths::settings_file::read(&app_config_dir.join(SETTINGS_FILE_NAME)) {
        Ok(read) => {
            if !read.problems.is_empty() {
                log_problems(&log_paths(), &read.problems);
            }
            read.object.map(Value::Object)
        }
        Err(error) => {
            log_refused_read(log_paths, &error);
            None
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
