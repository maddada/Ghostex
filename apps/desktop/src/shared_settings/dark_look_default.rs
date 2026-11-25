//! The user's own dark look became the default on 2026-11-20 (Black, 88 tints, a 4 point blur,
//! Kanagawa Dragon, the dark theme colours, Ink, pane gap 6, menu blur 17), and their Windows look
//! replaced it on 2026-11-21 (a Custom #46506a, Inkstone's theme colours, Live Ink glass). This
//! keeps the look they had for everyone who already had Ghostex set up.

use std::{fs, path::Path};

use super::*;
use serde_json::Value;

/// Runs each keep-old-look step of `DARK_LOOK_STEPS`, oldest first, once: writes the old default of
/// each look setting whose default changed that day into an existing settings file that does not
/// carry it.
///
/// CDXC:Theming 2026-11-20 WHY:
/// User (Q4290): long-time users keep their current look. Stored settings leave a key out until it is saved, so an existing user who never touched one of these settings would otherwise get the new value on update. Existing means the settings file is there at the first start of this version and setup is finished (the onboarding state says it completed, or gxserver has a project, the same test that decides whether setup opens again). A new install has no settings file at that start and gets the new defaults; a settings file left by a setup that never finished (onboarding in progress) counts as new too, so setup goes on in the new look. The marker file records that this has run, whatever it decided, so it never runs twice and a new install that saves settings later is never taken for an existing one. A file that cannot be read or saved is left as it is and no marker is written, so the next start tries again. This runs before anything reads the settings file.
/// SEE-ALSO: `keep_old_look_for_existing_settings` and `DARK_LOOK_STEPS` in packages/settings-catalog/src/lib.rs, sidebar_rail_default.rs (the same pattern).
pub fn keep_old_dark_look_once() {
    for (day, old_defaults) in ghostex_settings_catalog::DARK_LOOK_STEPS {
        // A step that could not finish leaves the later ones for the next start, so they never
        // run over a file the earlier one has not filled yet.
        if !keep_old_look_once(day, old_defaults) {
            return;
        }
    }
}

/// Updates an existing settings file still using the 2026-11-22 default theme accent (`#5581a9`
/// dark / `#8da101` light) to `#95d7f6` / `#0c74a6`, once.
///
/// CDXC:Theming 2026-11-24 WHY: Existing users who applied or already saved the default theme have
/// `themeRoles.dark.accent = "#5581a9"` persisted in `sidebar-settings.json`, so changing the
/// catalog default alone would leave them on the old darker blue. Behind a one-time marker so any
/// later manual colour pick is respected.
pub fn migrate_default_theme_accent_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.default_theme_accent_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(mut object) => {
                if ghostex_settings_catalog::migrate_default_theme_accent(&mut object) {
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
                            "existing default theme accent updated"
                        }
                        Err(error) => {
                            eprintln!(
                                "Ghostex could not save the updated default theme accent: {error}"
                            );
                            return;
                        }
                    }
                } else {
                    "settings do not hold the old default theme accent"
                }
            }
            None => "no settings file, new default theme accent",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to update the default theme accent: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}

/// One step of `keep_old_dark_look_once`. Returns whether it has run (now or before).
fn keep_old_look_once(day: &str, old_defaults: &[(&str, ghostex_settings_catalog::J)]) -> bool {
    let paths = ghostex_storage_paths();
    let marker = paths.dark_look_default_marker_file(day);
    if marker.exists() {
        return true;
    }
    let settings_file = shared_sidebar_settings_path();
    let outcome = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(_)
                if setup_in_progress(
                    &paths.state_dir,
                    &paths.gxserver_state_dir().join("state.db"),
                ) =>
            {
                "settings from an unfinished setup, new dark look"
            }
            Some(mut object) => {
                if ghostex_settings_catalog::keep_old_look_for_existing_settings(
                    &mut object,
                    old_defaults,
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
                            "existing settings, old dark look kept"
                        }
                        Err(error) => {
                            eprintln!("Ghostex could not keep the old dark look ({day}): {error}");
                            return false;
                        }
                    }
                } else {
                    "existing settings already choose every look setting"
                }
            }
            // No file (a new install), or a damaged one with no good copy that was set aside
            // (Ghostex starts on defaults).
            None => "no settings file, new dark look",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to keep the old dark look ({day}): {error}"
            );
            return false;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // A marker that is not written lets the next start decide again, and by then a new install
    // may have a settings file from a finished setup and be given the old look.
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the old dark look step ran ({}): {error}",
            marker.display()
        );
        return false;
    }
    true
}

/// Whether first-run setup would open again: it never completed and gxserver has no project
/// (`needs_first_launch_setup` in app/os_integration/first_run_onboarding.rs, without the
/// revision check, which also reopens setup for long-time users). Reads the onboarding state file
/// itself (`load_gpui_first_run_onboarding_state`), since this module is built into binaries
/// without the app.
pub(super) fn setup_in_progress(state_dir: &Path, gxserver_database: &Path) -> bool {
    let complete = fs::read_to_string(state_dir.join("gpui-first-run-onboarding-state.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|state| {
            state
                .get("firstLaunchSetupComplete")
                // Written under the Windows-only name until 2026-08-19.
                .or_else(|| state.get("windowsFirstLaunchSetupComplete"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    !complete && !gxserver_has_projects(gxserver_database)
}

/// Whether gxserver's database holds a project the sidebar shows: not parked in Recent Projects,
/// not hidden and not a Remote Attach carrier (`should_include_presentation_project` in
/// server/src/presentation/snapshot.rs, which the app's setup check reads). No database reads as
/// none; a database that cannot be read counts as having one, so a long-time user is never taken
/// for a new one.
fn gxserver_has_projects(database: &Path) -> bool {
    if !database.is_file() {
        return false;
    }
    rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|connection| {
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects
                    WHERE COALESCE(isRecentProject, 0) <> 1
                      AND COALESCE(visibility, 'visible') <> 'hidden'
                      AND (systemKind IS NULL OR systemKind <> 'remoteAttachCarrier'))",
                [],
                |row| row.get::<_, bool>(0),
            )
        })
        .unwrap_or(true)
}
