//! Enable Experimental Features (`showBetaFeatures`) is gone; this saves its replacement once.

use super::*;
use serde_json::Value;

/// Replaces Enable Experimental Features in the settings file with the switches it opened, each in
/// the state it had (`migrate_experimental_features_switch` in the settings catalog), and saves
/// the file. A file without the old key is left alone, so this runs once per settings file.
///
/// CDXC:Extensions 2026-11-09 DECISION: User (Q2043): "remove the 'Experimental features' toggle
/// itself and move toggling each of those individually here instead". Saved before anything reads
/// the settings file, so the app never writes the old key back and every later choice is the
/// person's own. Every snapshot also applies it on read (snapshot.rs), so nothing sees the old key
/// if this save fails; the next start tries again. gxserver applies it to its own reads.
pub fn migrate_experimental_features_switch_once() {
    let settings_file = shared_sidebar_settings_path();
    let mut object = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => match read.object {
            Some(object) => object,
            None => return,
        },
        Err(error) => {
            eprintln!(
                "Ghostex could not read its settings to replace Enable Experimental Features: {error}"
            );
            return;
        }
    };
    if !ghostex_settings_catalog::migrate_experimental_features_switch(&mut object) {
        return;
    }
    let saved = serde_json::to_vec_pretty(&Value::Object(object))
        .map_err(std::io::Error::other)
        .and_then(|bytes| ghostex_paths::settings_file::save(&settings_file, &bytes));
    match saved {
        Ok(problems) => {
            for problem in problems {
                log::error!("Settings file: {problem}");
            }
        }
        Err(error) => {
            eprintln!("Ghostex could not save the experimental switches: {error}");
        }
    }
}
