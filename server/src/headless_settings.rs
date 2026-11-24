//! Settings saved by gxserver itself on a computer without the desktop app (a headless Linux box
//! that runs only gxserver), for `ghostex settings set` and the other CLI writes that go through
//! `updateSettingsPatch`.
//!
//! CDXC:Settings 2026-11-07 FIX:
//! User (Q1653, asked whether `settings set` may save through gxserver where the app never saved settings): "Yes, allow it there".
//! Issue #475: `ghostex settings set` on a headless computer answered "Ghostex desktop app is not running", because every settings write goes through the running app (CDXC:Settings 2026-09-09 DECISION in ghostex_cli/settings.rs) and that computer has no app at all, so nothing could ever change a setting there. That decision stays for every computer the desktop app saves settings on: with the app closed, the write still fails. gxserver saves a write itself only when no app is connected and the settings file is one no app ever wrote: there is none yet, or it is exactly what gxserver last saved (a hash kept in `headless-settings.json` beside gxserver's state). Once anything else saves the file (the app on its first start, Settings sync through the app, a person editing it), gxserver stops and the app is the one writer again. The value is saved as given, already checked against the Settings catalog by the CLI; none of the app's own follow-ups run, since there is no app here to show it.
//! SEE-ALSO: server/src/server/zmx_http.rs (`handle_renderer_command_http`, where it is used), packages/paths/src/settings_file.rs (the read and the durable save).

use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::paths::GxserverPaths;

const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";
const MARKER_FILE_NAME: &str = "headless-settings.json";

fn settings_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_config_dir.join(SETTINGS_FILE_NAME)
}

fn marker_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join(MARKER_FILE_NAME)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The hashes `headless-settings.json` vouches for: the file as gxserver last saved it, and the
/// one before it while that save is under way (`previous`, so a crash between writing the marker
/// and the settings file leaves a file gxserver still owns). `None` when there is no marker.
fn vouched_hashes(paths: &GxserverPaths) -> Option<Vec<String>> {
    let marker = std::fs::read(marker_path(paths)).ok()?;
    let marker = serde_json::from_slice::<Value>(&marker).ok()?;
    Some(
        ["sha256", "previous"]
            .iter()
            .filter_map(|key| marker[*key].as_str().map(str::to_string))
            .collect(),
    )
}

/// Whether the settings file is one no desktop app wrote: there is none, or it holds exactly what
/// gxserver last saved. `None` when the file could not be read.
fn gxserver_owns_settings(paths: &GxserverPaths) -> Option<bool> {
    let bytes = match std::fs::read(settings_path(paths)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(true),
        Err(_) => return None,
    };
    let hash = digest(&bytes);
    Some(vouched_hashes(paths).is_some_and(|hashes| hashes.contains(&hash)))
}

/// Why gxserver left a save to the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeftToTheApp {
    /// gxserver never saved this file: the desktop app did (or a person made it).
    AppsFile,
    /// gxserver saved it before, and something else changed it since: the app on its first start,
    /// Settings sync through the app, or a person editing it by hand.
    ChangedSince,
}

/// Why [`save_settings_patch`] answered `Ok(None)`, for a caller that words the refusal itself.
pub(crate) fn left_to_the_app(paths: &GxserverPaths) -> LeftToTheApp {
    if vouched_hashes(paths).is_some() {
        LeftToTheApp::ChangedSince
    } else {
        LeftToTheApp::AppsFile
    }
}

/// The settings file gxserver and the app read and write.
pub(crate) fn settings_file_path(paths: &GxserverPaths) -> PathBuf {
    settings_path(paths)
}

/// Saves an `updateSettingsPatch` patch when gxserver owns the settings file here (a `null`
/// value removes the key). `Ok(None)` when it does not, so the caller answers as before.
pub(crate) fn save_settings_patch(
    paths: &GxserverPaths,
    patch: &Map<String, Value>,
) -> Result<Option<Value>, String> {
    // One save at a time: each reads the file, changes it and writes it back with its hash, so two
    // at once would drop one's change or leave a hash that is not the file's.
    static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = SAVING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if gxserver_owns_settings(paths) != Some(true) {
        return Ok(None);
    }
    let path = settings_path(paths);
    let previous = std::fs::read(&path).ok().map(|bytes| digest(&bytes));
    let read = ghostex_paths::settings_file::read(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let mut settings = read.object.unwrap_or_default();
    // Saved with the experimental switches in place of Enable Experimental Features, so the old
    // key never overrides a switch saved here (CDXC:Extensions 2026-11-09 in the settings catalog).
    ghostex_settings_catalog::migrate_experimental_features_switch(&mut settings);
    ghostex_settings_catalog::migrate_retired_chat_looks(&mut settings);
    // Marked handovers is Off, Coordinators only or Every session (CDXC:MarkedHandovers 2026-11-13).
    ghostex_settings_catalog::migrate_marked_handovers(&mut settings);
    for (key, value) in patch {
        if value.is_null() {
            settings.remove(key);
        } else {
            settings.insert(key.clone(), value.clone());
        }
    }
    let bytes = serde_json::to_vec_pretty(&Value::Object(settings))
        .map_err(|error| format!("Could not encode the settings: {error}"))?;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("Could not create {}: {error}", folder.display()))?;
    }
    // The marker first, vouching for the file before and after this save, each write durable: a
    // crash at any point leaves a file the marker vouches for, so gxserver keeps saving here.
    let marker = marker_path(paths);
    let vouch = |hashes: Value| {
        ghostex_paths::durable_file::write_durably(&marker, hashes.to_string().as_bytes())
            .map_err(|error| format!("Could not save {}: {error}", marker.display()))
    };
    let new_hash = digest(&bytes);
    vouch(json!({ "sha256": new_hash, "previous": previous }))?;
    ghostex_paths::settings_file::save(&path, &bytes)
        .map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    vouch(json!({ "sha256": new_hash }))?;
    // If SQLite already holds a persisted `agents.settings.v1` row, keep its `agentAcceptAllEnabled`
    // in step with a headless write so a stale `false` there cannot override turning it on (or vice
    // versa) under `agents::resolve_global_accept_all_enabled`. When no row is persisted yet, leave
    // it absent so first-start desktop hydration still sees `isPersisted: false`.
    if let Some(value) = patch.get("agentAcceptAllEnabled") {
        let next = value.as_bool().unwrap_or(false);
        if let Ok(db) = crate::storage::open_gxserver_database(paths) {
            if matches!(
                crate::agents::read_agent_settings_metadata_value(&db),
                Ok(Some(_))
            ) {
                let mut update = Map::new();
                update.insert("agentAcceptAllEnabled".to_string(), Value::Bool(next));
                let _ = crate::agents::update_agent_settings(&db, &update);
            }
        }
    }
    Ok(Some(json!({
        "ok": true,
        "savedBy": "gxserver",
        "keys": patch.keys().collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (tempfile::TempDir, GxserverPaths) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        (temp, paths)
    }

    fn patch(value: Value) -> Map<String, Value> {
        value.as_object().expect("object").clone()
    }

    fn saved(paths: &GxserverPaths) -> Value {
        serde_json::from_slice(&std::fs::read(settings_path(paths)).expect("settings"))
            .expect("json")
    }

    #[test]
    fn a_computer_no_app_ever_saved_settings_on_takes_gxservers_saves() {
        let (_temp, paths) = fresh();
        let result = save_settings_patch(&paths, &patch(json!({"autoSleepMinutes": 30})))
            .expect("save")
            .expect("saved");
        assert_eq!(result["savedBy"], "gxserver");
        assert_eq!(saved(&paths), json!({"autoSleepMinutes": 30}));
        // Its own file again: the next save adds to it, and null removes a key.
        save_settings_patch(
            &paths,
            &patch(json!({"useAgentHooks": true, "autoSleepMinutes": null})),
        )
        .expect("save")
        .expect("saved");
        assert_eq!(saved(&paths), json!({"useAgentHooks": true}));
        // The readers every setting goes through see it.
        assert_eq!(
            crate::shared_settings_file::read_shared_settings(&paths).expect("settings")
                ["useAgentHooks"],
            true
        );
    }

    #[test]
    fn a_settings_file_anything_else_wrote_is_left_to_the_app() {
        let (_temp, paths) = fresh();
        std::fs::create_dir_all(&paths.app_config_dir).expect("dir");
        // The desktop app's file: gxserver never wrote it.
        std::fs::write(settings_path(&paths), r#"{"theme":"dark"}"#).expect("write");
        assert_eq!(
            save_settings_patch(&paths, &patch(json!({"autoSleepMinutes": 30}))).expect("save"),
            None
        );
        assert_eq!(saved(&paths), json!({"theme": "dark"}));

        // gxserver's own file, which the app then saved over on its first start.
        let (_temp, paths) = fresh();
        save_settings_patch(&paths, &patch(json!({"autoSleepMinutes": 30})))
            .expect("save")
            .expect("saved");
        std::fs::write(
            settings_path(&paths),
            r#"{"autoSleepMinutes":30,"theme":"dark"}"#,
        )
        .expect("app save");
        assert_eq!(
            save_settings_patch(&paths, &patch(json!({"autoSleepMinutes": 45}))).expect("save"),
            None
        );
    }
}
