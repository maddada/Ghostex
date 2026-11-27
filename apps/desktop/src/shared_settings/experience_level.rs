//! The saved Experience level (`experienceLevel`) arrived with Power User Flags on 2026-11-27. This
//! saves Advanced, once, for someone whose Experience already matched Advanced in every area, so
//! their pages keep showing the Power User Flags.

use std::fs;

use super::*;
use serde_json::Value;

/// Saves `experienceLevel` = `advanced` in an existing user's settings file when it has no level
/// yet and every Experience area matches Advanced, once and with no UI.
///
/// CDXC:Settings 2026-11-27 WHY:
/// Same shape as `keep_old_heavy_views_once` (heavy_views_default.rs): a brand-new install has no
/// settings file at its first start (or an unfinished first-run setup) and is left alone; the
/// marker file records that this has run so it never runs twice. Runs before anything reads the
/// settings file. The user's decision is on `mark_existing_advanced` in
/// packages/settings-catalog/src/power_user_flags.rs.
pub fn mark_existing_advanced_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.experience_level_marker_file();
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
                "settings from an unfinished setup, level left unsaved"
            }
            Some(mut object) => {
                if ghostex_settings_catalog::power_user_flags::mark_existing_advanced(&mut object) {
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
                            "existing settings on Advanced, level saved"
                        }
                        Err(error) => {
                            eprintln!(
                                "Ghostex could not save the Advanced Experience level: {error}"
                            );
                            return;
                        }
                    }
                } else {
                    "existing settings with a saved level or not on Advanced"
                }
            }
            None => "no settings file, level left unsaved",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to save the Advanced Experience level: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the Experience level step ran ({}): {error}",
            marker.display()
        );
    }
}
