//! What is left of Dictate's own local engine, retired for Hex: the notice for people who used it
//! and the speech models it downloaded, which they can delete to free the space.
//!
//! CDXC:Dictate 2026-11-08 DECISION:
//! User (Q1709, "Drop local now"): Ghostex's own local transcription engine (Parakeet and Whisper on the CPU, with model downloads) is gone, and Hex is the recommended engine. "For people who were using it please let them know we're switching to hex as the recommended dictation app that ghostex integrates with and to just install that (hex used even on the phone)." A computer whose engine was Local moves to Hex, never to a paid cloud engine, and shows a one-time notice on the Dictation page and with the next dictation that can't be transcribed; the old models folder stays until the user deletes it from the Dictation page. Supersedes CDXC:Dictate 2026-11-05 (Q1270, the models folder with its downloads) and "Our local engine stays until Hex is proven" in hex.rs.
//! SEE-ALSO: settings.rs (the move to Hex), apps/desktop/src/app/window/settings_modal/tabs/voice_dictate.rs and apps/mobile/app/src/screens/settings/VoicePage.tsx (the notice and Delete old models).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

/// The notice, said once on the Dictation page and once with a dictation that can't be transcribed.
pub(crate) const NOTICE: &str = "Ghostex no longer has its own speech model. Hex, a free dictation app that runs on your computer, is now the recommended way to dictate, and Ghostex works with it directly. Install Hex to keep dictating offline; your phone's dictation goes through this computer, so it uses Hex too.";

/// Where the retired engine kept its models (`<data>/gxserver/speech-models`).
pub(crate) fn models_folder(paths: &GxserverPaths) -> PathBuf {
    paths.app_data_dir.join("gxserver").join("speech-models")
}

/// The bytes of every file under `path` (0 when it is missing).
pub(crate) fn disk_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => disk_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}

/// `oldModels` in `/api/readDictateSettings`: the folder and its size, or null when there is
/// nothing to delete.
pub(crate) fn old_models_view(paths: &GxserverPaths) -> Value {
    let folder = models_folder(paths);
    if !folder.is_dir() {
        return Value::Null;
    }
    json!({
        "folder": folder.to_string_lossy(),
        "bytes": disk_bytes(&folder),
    })
}

/// Deletes the old models folder. Deleting a folder that is already gone succeeds.
pub(crate) fn delete_old_models(paths: &GxserverPaths) -> Result<(), String> {
    match fs::remove_dir_all(models_folder(paths)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Couldn't delete the old speech models: {error}")),
    }
}
