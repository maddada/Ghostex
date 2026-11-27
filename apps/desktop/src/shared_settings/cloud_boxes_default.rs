//! Cloud Boxes became off by default on 2026-11-27 (packages/settings-catalog/src/data/defaults.rs).
//! This keeps it on, once, for someone who already set up a Cloud Box or provider, and lets
//! everyone else follow the new default.

use std::fs;
use std::path::Path;

use super::*;
use serde_json::Value;

const SWITCH_KEY: &str = "cloudBoxesEnabled";

/// Saves `cloudBoxesEnabled` = `true` in an existing user's settings file when they already set
/// up Cloud Boxes, and drops the saved `true` (the old default every settings file stored) when
/// they did not, so the catalog default decides. A saved `false` is the user's own and is kept.
///
/// CDXC:CloudBoxes 2026-11-27 DECISION:
/// User (Q5275): "Off for new users (keep it on for anyone who already set up a Cloud Box or
/// provider)". Set up means gxserver has a Cloud Box provider row (Instabox or Boat connected) or a
/// box row, a session ran in an agentbox box or a Cloud Box, or agentbox keeps files in
/// `~/.agentbox` (a More providers login, an SSH server or an agent's sign-in for boxes). A
/// database that exists but cannot be opened counts as set up, so nobody loses boxes they have.
/// Dropping the key rather than saving `false` keeps the default a one-line change in defaults.rs.
/// Same shape as `keep_old_heavy_views_once` (heavy_views_default.rs): a brand-new install (no
/// settings file, or an unfinished first-run setup) is left alone, and the marker file means it
/// never runs twice. Runs before anything reads the settings file.
pub fn keep_cloud_boxes_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.cloud_boxes_default_marker_file();
    if marker.exists() {
        return;
    }
    let database = paths.gxserver_state_dir().join("state.db");
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(_) if super::dark_look_default::setup_in_progress(&paths.state_dir, &database) => {
                "settings from an unfinished setup, left to the default"
            }
            Some(mut object) => {
                let (outcome, changed) = if object.get(SWITCH_KEY) == Some(&Value::Bool(false)) {
                    ("turned off by the user, kept off", false)
                } else if cloud_boxes_set_up(&database, &paths.home_dir) {
                    let changed = object.get(SWITCH_KEY) != Some(&Value::Bool(true));
                    object.insert(SWITCH_KEY.to_string(), Value::Bool(true));
                    ("Cloud Boxes set up, kept on", changed)
                } else {
                    let changed = object.remove(SWITCH_KEY).is_some();
                    ("Cloud Boxes not set up, left to the default", changed)
                };
                if changed {
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
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not save the Cloud Boxes switch: {error}");
                            return;
                        }
                    }
                }
                outcome
            }
            None => "no settings file, left to the default",
        },
        Err(error) => {
            eprintln!("Ghostex could not read its settings to keep Cloud Boxes on: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the Cloud Boxes step ran ({}): {error}",
            marker.display()
        );
    }
}

/// Whether this computer already has a Cloud Box or provider set up (see `keep_cloud_boxes_once`).
fn cloud_boxes_set_up(database: &Path, home: &Path) -> bool {
    let agentbox_used =
        fs::read_dir(home.join(".agentbox")).is_ok_and(|mut entries| entries.next().is_some());
    agentbox_used || gxserver_has_cloud_boxes(database)
}

/// Whether gxserver's database holds a Cloud Box provider, a box, or a session that ran in a box.
/// No database reads as none; one that cannot be opened reads as set up. A missing table (a
/// database from before Cloud Boxes) reads as none.
fn gxserver_has_cloud_boxes(database: &Path) -> bool {
    if !database.is_file() {
        return false;
    }
    let Ok(connection) =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return true;
    };
    [
        "SELECT EXISTS(SELECT 1 FROM cloud_providers)",
        "SELECT EXISTS(SELECT 1 FROM cloud_boxes)",
        "SELECT EXISTS(SELECT 1 FROM sessions
            WHERE json_extract(runtimeSettingsJson, '$.agentbox.boxName') IS NOT NULL
               OR json_extract(runtimeSettingsJson, '$.cloudBox') IS NOT NULL)",
    ]
    .iter()
    .any(|query| {
        connection
            .query_row(query, [], |row| row.get::<_, bool>(0))
            .unwrap_or(false)
    })
}
