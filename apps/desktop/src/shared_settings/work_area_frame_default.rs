//! Frame the work area (`workAreaFrame`) is on by default for a new install. This keeps it off for
//! everyone who already had a settings file.

use std::fs;

use super::*;
use serde_json::Value;

/// Writes `workAreaFrame` false into an existing settings file that does not carry the key, once.
///
/// CDXC:Workarea 2026-11-08 WHY:
/// Stored settings leave a key out until it is saved, so an existing user who never touched Frame the work area has no `workAreaFrame` in the file, and the new default (on) would frame their window without asking. A new install has no settings file at its first start, so it is left alone and gets the default; the marker file records that this has run, so a new install that saves other settings later is never mistaken for an existing one. A file that cannot be read or saved is left as it is and no marker is written, so the next start tries again. This runs before anything reads the settings file.
/// SEE-ALSO: `keep_work_area_frame_off_for_existing_settings` in packages/settings-catalog/src/lib.rs.
pub fn migrate_work_area_frame_default_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.work_area_frame_default_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(mut object) => {
                if ghostex_settings_catalog::keep_work_area_frame_off_for_existing_settings(
                    &mut object,
                ) {
                    let saved = serde_json::to_vec_pretty(&Value::Object(object))
                        .map_err(std::io::Error::other)
                        .and_then(|bytes| {
                            ghostex_paths::settings_file::save(&settings_file, &bytes)
                        });
                    match saved {
                        Ok(problems) => {
                            for problem in problems {
                                log::error!("Settings file: {problem}");
                            }
                            "existing settings, frame kept off"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not keep the work area frame off: {error}");
                            return;
                        }
                    }
                } else {
                    "existing settings already choose the frame"
                }
            }
            // No file (a new install, which starts on the default), or a damaged one with no good
            // copy that was set aside (Ghostex starts on defaults).
            None => "no settings file, frame left on by default",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to keep the work area frame off: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
