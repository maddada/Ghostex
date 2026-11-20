//! The rail (`sidebarLayout` "rail") is the default for a new install. This keeps Classic for
//! everyone who already had a settings file, and queues their one-time "Meet the rail" intro.

use std::fs;

use super::*;
use serde_json::Value;

/// Writes `sidebarLayout` "classic" and `sidebarRailIntro` "pending" into an existing settings file
/// that does not choose a layout, once.
///
/// CDXC:Sidebar 2026-11-20 WHY:
/// The same shape as `migrate_work_area_frame_default_once`: a new install has no settings file at its first start, so it is left alone and starts on Rail; the marker file records that this has run, so a new install that saves other settings later is never mistaken for an existing one. A file that cannot be read or saved is left as it is and no marker is written, so the next start tries again. This runs before anything reads the settings file.
/// SEE-ALSO: `keep_classic_sidebar_for_existing_settings` in packages/settings-catalog/src/lib.rs, app/sidebar_rail/intro.rs (the intro).
pub fn migrate_sidebar_rail_default_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.sidebar_rail_default_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            // Settings written by a setup that never finished belong to someone new: they start on
            // Rail, as the dark look's step treats them (dark_look_default.rs).
            Some(_)
                if super::dark_look_default::setup_in_progress(
                    &paths.state_dir,
                    &paths.gxserver_state_dir().join("state.db"),
                ) =>
            {
                "settings from an unfinished setup, rail by default"
            }
            Some(mut object) => {
                if ghostex_settings_catalog::keep_classic_sidebar_for_existing_settings(&mut object)
                {
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
                            "existing settings, classic sidebar kept, intro pending"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not keep the classic sidebar: {error}");
                            return;
                        }
                    }
                } else {
                    "existing settings already choose a sidebar layout"
                }
            }
            None => "no settings file, rail by default",
        },
        Err(error) => {
            eprintln!("Ghostex could not read its settings to keep the classic sidebar: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
