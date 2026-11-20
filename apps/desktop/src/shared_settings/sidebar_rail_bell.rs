//! With the rail on, the notifications bell sits in the sidebar's top bar (app/native_sidebar/
//! navigation.rs). 11.12.0 pinned it on the rail by default and saved it with the pins, so this
//! takes it off the saved pins once. Since 2026-11-21 the side panel views and Notifications are
//! not rail items at all, and a second step takes them off the saved pins once.

use std::fs;

use super::*;
use serde_json::Value;

/// Removes Notifications from `sidebarRailPins` in an existing settings file, once.
///
/// CDXC:Sidebar 2026-11-20 WHY:
/// The same shape as `migrate_sidebar_rail_default_once`, with its own marker: the marker file records that this has run, so a bell pinned again from ⋯ is never taken off. The marker and the settings file are this computer's (the state folder is in no sync category, and neither is `sidebarRailPins`), so each computer runs it once over its own pins. A file that cannot be read or saved is left as it is and no marker is written, so the next start tries again. This runs before anything reads the settings file.
/// SEE-ALSO: `unpin_bell_from_saved_rail_pins` in packages/settings-catalog/src/lib.rs.
pub fn migrate_sidebar_rail_bell_once() {
    edit_rail_pins_once(
        ghostex_storage_paths().sidebar_rail_bell_marker_file(),
        ghostex_settings_catalog::unpin_bell_from_saved_rail_pins,
        ("saved rail pins, notifications unpinned", "no saved rail pins with notifications"),
        "unpin the bell from the rail",
    );
}

/// Removes the side panel views and Notifications from `sidebarRailPins` in an existing settings
/// file, once, when they left the rail (CDXC:Sidebar 2026-11-21 DECISION on
/// `drop_retired_rail_pins` in packages/settings-catalog/src/lib.rs). The same shape as the bell's
/// step, with its own marker, `sidebar-rail-views-unpinned`.
pub fn migrate_sidebar_rail_views_once() {
    edit_rail_pins_once(
        ghostex_storage_paths().sidebar_rail_views_marker_file(),
        ghostex_settings_catalog::drop_retired_rail_pins,
        ("saved rail pins, views unpinned", "no saved rail pins with views"),
        "unpin the views from the rail",
    );
}

/// Runs `edit` over the settings file once, behind `marker`, writing the file when it changed
/// something; `notes` are the marker's text when it did and when it did not, `what` the error's.
fn edit_rail_pins_once(
    marker: std::path::PathBuf,
    edit: fn(&mut serde_json::Map<String, Value>) -> bool,
    notes: (&str, &str),
    what: &str,
) {
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(mut object) => {
                if edit(&mut object) {
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
                            notes.0
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not {what}: {error}");
                            return;
                        }
                    }
                } else {
                    notes.1
                }
            }
            None => "no settings file",
        },
        Err(error) => {
            eprintln!("Ghostex could not read its settings to {what}: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
