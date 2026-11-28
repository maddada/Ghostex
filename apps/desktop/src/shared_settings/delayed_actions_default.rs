//! Delayed Send and Close After Done became Staff pick extensions, off by default, on 2026-11-28
//! (packages/settings-catalog/src/data/defaults.rs). This turns both off, once, for an existing
//! user too, and leaves them free to turn either back on.

use std::fs;

use super::*;
use serde_json::Value;

const SWITCH_KEYS: [&str; 2] = ["delayedSendEnabled", "closeAfterDoneEnabled"];

/// Saves `delayedSendEnabled` and `closeAfterDoneEnabled` as `false` in an existing user's
/// settings file, once.
///
/// CDXC:DelayedSend 2026-11-28 DECISION:
/// User: "make delayed actions (delayed send and close after done) both staff pick extensions
/// disabled by default for new users pls. disable it even for existing users". Both keys are new,
/// so a file from before reads the default (off) already; saving `false` explicitly also turns off
/// a `true` some build or a synced computer saved before this step ran. The marker file means it
/// never runs twice, so someone who turns one back on afterwards keeps it on. Delayed sends already
/// scheduled and sessions already armed are left alone: they still run (gxserver owns them). Same
/// shape as `keep_cloud_boxes_once` (cloud_boxes_default.rs), except that a settings file from an
/// unfinished first-run setup is turned off too (saving `false` there equals the default); only a
/// brand-new install with no settings file is left alone. It runs before anything reads the
/// settings file.
pub fn turn_off_delayed_actions_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.delayed_actions_default_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(mut object) => {
                let changed = SWITCH_KEYS
                    .iter()
                    .filter(|key| object.get(**key) != Some(&Value::Bool(false)))
                    .count()
                    > 0;
                if changed {
                    for key in SWITCH_KEYS {
                        object.insert(key.to_string(), Value::Bool(false));
                    }
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
                            eprintln!(
                                "Ghostex could not turn off Delayed Send and Close After Done: {error}"
                            );
                            return;
                        }
                    }
                }
                "Delayed Send and Close After Done turned off"
            }
            None => "no settings file, left to the default",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to turn off Delayed Send and Close After Done: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the Delayed Send and Close After Done step ran ({}): {error}",
            marker.display()
        );
    }
}
