//! Marks an existing desktop install once after updating so the one-time Native mode announcement
//! modal opens on first launch (`Q4773`/`Q4774`).

use std::fs;

use super::*;
use serde_json::Value;

/// Writes `nativeModeIntro: "pending"` into an existing settings file once, behind a per-machine
/// state marker. New installs (no settings file yet, or setup in progress) get no modal.
///
/// CDXC:AgentProviders 2026-11-24 WHY:
/// Same one-time marker shape as `migrate_sidebar_rail_default_once` (sidebar_rail_default.rs):
/// runs before anything reads the settings file so the app opens the announcement once after
/// updating, and never again after. Unreadable or unwriteable files skip writing the marker so
/// the next start retries cleanly.
pub fn migrate_native_mode_intro_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.native_mode_intro_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(_)
                if super::dark_look_default::setup_in_progress(
                    &paths.state_dir,
                    &paths.gxserver_state_dir().join("state.db"),
                ) =>
            {
                "settings from an unfinished setup, skipping native mode intro"
            }
            Some(mut object) => {
                if ghostex_settings_catalog::mark_existing_settings_for_native_mode_intro(
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
                            "existing settings, native mode intro queued"
                        }
                        Err(error) => {
                            eprintln!(
                                "Ghostex could not queue the Native mode announcement: {error}"
                            );
                            return;
                        }
                    }
                } else {
                    "native mode intro already handled"
                }
            }
            None => "no settings file, skipping native mode intro",
        },
        Err(error) => {
            eprintln!("Ghostex could not read its settings for Native mode intro: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
