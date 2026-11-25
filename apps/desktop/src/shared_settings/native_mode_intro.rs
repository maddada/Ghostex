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

/// Seeds `agentSessionMode` to `"non-cli"` (Native) for Antigravity, Claude Code and Codex once on
/// a fresh install whose preferred view is Chat. Existing installs and installs whose preferred
/// view is Terminal keep what they have. Shares its marker file with gxserver so whichever starts
/// first runs once and the other is a no-op.
pub fn seed_fresh_install_native_mode_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.native_mode_fresh_default_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => {
            let db_path = paths.gxserver_state_dir().join("state.db");
            let onboarding =
                fs::read_to_string(paths.state_dir.join("gpui-first-run-onboarding-state.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|state| {
                        state
                            .get("firstLaunchSetupComplete")
                            .or_else(|| state.get("windowsFirstLaunchSetupComplete"))
                            .and_then(Value::as_bool)
                    });
            let in_progress =
                super::dark_look_default::setup_in_progress(&paths.state_dir, &db_path);
            let existing_install =
                !in_progress || (read.object.is_some() && onboarding != Some(false));
            if existing_install {
                "existing install, session mode unchanged"
            } else {
                let mut object = read.object.unwrap_or_default();
                if !ghostex_settings_catalog::preferred_interface_is_chat(&object) {
                    "fresh install, preferred view terminal, session mode unchanged"
                } else if ghostex_settings_catalog::apply_fresh_install_native_defaults(&mut object)
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
                            "fresh install, preferred view chat, native defaults applied"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not save fresh Native defaults: {error}");
                            return;
                        }
                    }
                } else {
                    "fresh install, session mode already set"
                }
            }
        }
        Err(error) => {
            eprintln!("Ghostex could not read its settings for fresh Native defaults: {error}");
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
