//! VS Code (`code`), Beads (`kanban`) and Automate (`automate`) became opt-in for new installs on
//! 2026-11-24. This keeps them enabled for everyone who already had Ghostex set up, unless they
//! explicitly turned them off.

use std::fs;

use super::*;
use serde_json::Value;

/// Writes `false` (visible) for any of `codeViewTabHidden`, `kanbanViewTabHidden` or
/// `automateViewTabHidden` missing from an existing user's settings file, once.
///
/// CDXC:Extensions 2026-11-24 WHY:
/// Same shape as `keep_old_dark_look_once` (dark_look_default.rs) and
/// `migrate_sidebar_rail_default_once` (sidebar_rail_default.rs): a brand-new install has no
/// settings file at its first start (or an unfinished first-run setup) and gets the opt-in
/// defaults; an existing user with a completed setup keeps any of the three views they had not
/// explicitly disabled. The marker file records that this has run so it never runs twice and a new
/// install that saves settings later is never mistaken for an existing one. Runs before anything
/// reads the settings file.
/// SEE-ALSO: `keep_old_heavy_views_for_existing_settings` in packages/settings-catalog/src/lib.rs.
pub fn keep_old_heavy_views_once() {
    let paths = ghostex_storage_paths();
    let marker = paths.heavy_views_default_marker_file();
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
                "settings from an unfinished setup, heavy views opt-in"
            }
            Some(mut object) => {
                if ghostex_settings_catalog::keep_old_heavy_views_for_existing_settings(&mut object)
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
                            "existing settings, heavy views kept"
                        }
                        Err(error) => {
                            eprintln!(
                                "Ghostex could not keep existing VS Code/Beads/Automate views: {error}"
                            );
                            return;
                        }
                    }
                } else {
                    "existing settings already choose every heavy view"
                }
            }
            None => "no settings file, heavy views opt-in",
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to keep existing VS Code/Beads/Automate views: {error}"
            );
            return;
        }
    };
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&marker, format!("{outcome}\n")) {
        log::error!(
            "Ghostex could not record that the heavy views step ran ({}): {error}",
            marker.display()
        );
    }
}
