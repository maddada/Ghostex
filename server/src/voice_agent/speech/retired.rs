//! What is left of Dictate's own local engine, retired for Hex: the speech models it downloaded,
//! which gxserver deletes on its own.
//!
//! CDXC:Dictate 2026-11-10 DECISION:
//! User (Q2108): the "Old speech models" row and the one-time notice about the retired engine (on Settings › Dictation and with a failed dictation) are gone. The models Ghostex's own old engine downloaded are deleted automatically instead: once per gxserver start, in the background, only that engine's own folder (`<Ghostex data>/gxserver/speech-models`, never Hex's models, which Hex keeps in its own app folder). A failure is logged and tried again at the next start, never shown. Supersedes the 2026-11-08 decision (Q1709) that kept the folder until the user deleted it from the Dictation page and said the notice once; a computer whose engine was Local still moves to Hex, never to a paid engine (`settings.rs`).
//! SEE-ALSO: server/src/server/foreground.rs (the start), http.rs (`/api/deleteOldSpeechModels`, kept for older clients).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::paths::GxserverPaths;

/// Where the retired engine kept its models (`<data>/gxserver/speech-models`).
pub(crate) fn models_folder(paths: &GxserverPaths) -> PathBuf {
    paths.app_data_dir.join("gxserver").join("speech-models")
}

/// Deletes the old models folder. Deleting a folder that is already gone succeeds.
pub(crate) fn delete_old_models(paths: &GxserverPaths) -> Result<(), String> {
    match fs::remove_dir_all(models_folder(paths)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Couldn't delete the old speech models: {error}")),
    }
}

/// The start's pass: deletes the old models folder on its own thread, logging what happened.
pub(crate) fn spawn_startup_delete(paths: GxserverPaths, logger: Arc<GxserverLogger>) {
    let folder = models_folder(&paths);
    if !folder.exists() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("dictate-old-models".to_string())
        .spawn(move || {
            let (level, event, error) = match delete_old_models(&paths) {
                Ok(()) => (LogLevel::Info, "dictateOldSpeechModelsDeleted", None),
                Err(message) => (
                    LogLevel::Warn,
                    "dictateOldSpeechModelsDeleteFailed",
                    Some(message),
                ),
            };
            let _ = logger.log(GxserverLogInput {
                level,
                event: event.to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error,
                details: Some(json!({ "folder": folder.to_string_lossy() })),
            });
        });
}
