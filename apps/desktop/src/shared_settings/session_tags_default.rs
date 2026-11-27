//! Session Tags became a built-in extension, off by default, on 2026-11-27
//! (packages/settings-catalog/src/data/defaults.rs). This turns it on, once, for someone who
//! already uses tags, and leaves everyone else on the default.

use std::fs;
use std::path::Path;

use super::*;
use ghostex_settings_catalog::ToJson;
use serde_json::{Map, Value};

const SWITCH_KEY: &str = "sessionTagsEnabled";
const TAG_LIST_KEY: &str = "sidebarSessionTagListItems";
const TAG_PROMPT_KEY: &str = "showTagMenuWhenParking";

/// Saves `sessionTagsEnabled` = `true` in an existing user's settings file when they already use
/// tags. A saved value of the switch is the user's own and is kept.
///
/// CDXC:SessionTags 2026-11-27 DECISION:
/// The coordinator, for the user's "make tagging sessions in general an advanced feature ...
/// disabled by default": "Existing users who actually use tags (any session has a user tag, or
/// they changed tag settings ...) keep it on. Everyone else, new or existing, starts with it off."
/// Uses tags means a session on this computer's gxserver has a tag other than Favorite (Favorite
/// keeps working while tags are off), the custom tag list has a tag, the tag list in Settings
/// differs from the default, or the tag prompt when parking is on. A database that exists but
/// cannot be opened counts as using tags. Same shape as `keep_cloud_boxes_once`
/// (cloud_boxes_default.rs): a brand-new install or an unfinished first-run setup is left alone,
/// the marker file means it never runs twice, and it runs before anything reads the settings file.
pub fn keep_session_tags_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.session_tags_default_marker_file();
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
            Some(object) if object.contains_key(SWITCH_KEY) => "switch already saved, kept",
            Some(mut object) => {
                if tag_settings_changed(&object) || gxserver_has_tags(&database) {
                    object.insert(SWITCH_KEY.to_string(), Value::Bool(true));
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
                            "tags in use, Session Tags kept on"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not save the Session Tags switch: {error}");
                            return;
                        }
                    }
                } else {
                    "tags not in use, left to the default"
                }
            }
            None => "no settings file, left to the default",
        },
        Err(error) => {
            eprintln!("Ghostex could not read its settings to keep Session Tags on: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the Session Tags step ran ({}): {error}",
            marker.display()
        );
    }
}

/// The tag prompt when parking is on, or the saved tag list is not the default one.
fn tag_settings_changed(settings: &Map<String, Value>) -> bool {
    if settings.get(TAG_PROMPT_KEY) == Some(&Value::Bool(true)) {
        return true;
    }
    settings.get(TAG_LIST_KEY).is_some_and(|saved| {
        ghostex_settings_catalog::default_value(TAG_LIST_KEY)
            .is_none_or(|default| *saved != default.to_json().to_value())
    })
}

/// Whether gxserver's database has a session tagged with something other than Favorite, or a
/// custom tag. No database reads as none; one that cannot be opened reads as tags in use. A
/// missing table or column reads as none.
fn gxserver_has_tags(database: &Path) -> bool {
    if !database.is_file() {
        return false;
    }
    let Ok(connection) =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return true;
    };
    [
        "SELECT EXISTS(SELECT 1 FROM sessions
            WHERE sessionTag IS NOT NULL AND sessionTag <> '' AND sessionTag <> 'favorite')",
        "SELECT EXISTS(SELECT 1 FROM metadata
            WHERE key = 'customSessionTags' AND json_array_length(value, '$.order') > 0)",
    ]
    .iter()
    .any(|query| {
        connection
            .query_row(query, [], |row| row.get::<_, bool>(0))
            .unwrap_or(false)
    })
}
