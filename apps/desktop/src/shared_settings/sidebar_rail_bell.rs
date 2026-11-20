//! With the rail on, the notifications bell sits in the sidebar's top bar (app/native_sidebar/
//! navigation.rs). 11.12.0 pinned it on the rail by default and saved it with the pins, so this
//! takes it off the saved pins once, and a bell pinned again from ⋯ after that stays.

use std::fs;

use super::*;
use serde_json::Value;

/// Removes Notifications from `sidebarRailPins` in an existing settings file, once.
///
/// CDXC:Sidebar 2026-11-20 WHY:
/// The same shape as `migrate_sidebar_rail_default_once`, with its own marker: the marker file records that this has run, so a bell pinned again from ⋯ is never taken off. The marker and the settings file are this computer's (the state folder is in no sync category, and neither is `sidebarRailPins`), so each computer runs it once over its own pins. A file that cannot be read or saved is left as it is and no marker is written, so the next start tries again. This runs before anything reads the settings file.
/// SEE-ALSO: `unpin_bell_from_saved_rail_pins` in packages/settings-catalog/src/lib.rs.
pub fn migrate_sidebar_rail_bell_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.sidebar_rail_bell_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(mut object) => {
                if ghostex_settings_catalog::unpin_bell_from_saved_rail_pins(&mut object) {
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
                            "saved rail pins, notifications unpinned"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not unpin the bell from the rail: {error}");
                            return;
                        }
                    }
                } else {
                    "no saved rail pins with notifications"
                }
            }
            None => "no settings file",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to unpin the bell from the rail: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
