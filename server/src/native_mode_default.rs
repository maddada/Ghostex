//! Fresh-install Native mode default (`agentSessionMode: { antigravity, claude, codex: "non-cli" }`)
//! for installs whose preferred view is Chat (not Terminal).
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! New installs with preferred view Chat default `agentSessionMode` to `"non-cli"` (Native) for
//! Antigravity, Claude Code and Codex. Existing installs (existing settings with finished setup, or
//! any existing projects/sessions in `state.db`) and installs whose preferred view is Terminal keep
//! what they have. Both `gxserver` and the desktop app share the same marker file
//! (`native-mode-fresh-default-v1` in `app_state_dir` / `state_dir`), so whichever starts first runs
//! once and the other becomes a no-op.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::paths::GxserverPaths;

const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";
const MARKER_FILE_NAME: &str = "native-mode-fresh-default-v1";

fn onboarding_state(state_dir: &Path) -> Option<bool> {
    fs::read_to_string(state_dir.join("gpui-first-run-onboarding-state.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|state| {
            state
                .get("firstLaunchSetupComplete")
                .or_else(|| state.get("windowsFirstLaunchSetupComplete"))
                .and_then(Value::as_bool)
        })
}

fn db_has_projects_or_sessions(paths: &GxserverPaths) -> bool {
    let db_path = paths.root_dir.join("state.db");
    if !db_path.is_file() {
        return false;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        // Conservative on unreadable DB: treat as existing so we never overwrite an existing user.
        return true;
    };
    let has_project = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects
                WHERE COALESCE(isRecentProject, 0) <> 1
                  AND COALESCE(visibility, 'visible') <> 'hidden'
                  AND (systemKind IS NULL OR systemKind <> 'remoteAttachCarrier'))",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(true);
    if has_project {
        return true;
    }
    db.query_row("SELECT EXISTS(SELECT 1 FROM sessions)", [], |row| {
        row.get::<_, bool>(0)
    })
    .unwrap_or(true)
}

/// Seeds `agentSessionMode` to `"non-cli"` (Native) for Antigravity, Claude Code and Codex once on
/// a fresh install whose preferred view is Chat.
pub(crate) fn seed_fresh_install_native_mode_once(paths: &GxserverPaths) {
    let marker = paths.app_state_dir.join(MARKER_FILE_NAME);
    if marker.exists() {
        return;
    }
    let settings_file = paths.app_config_dir.join(SETTINGS_FILE_NAME);
    let read = match ghostex_paths::settings_file::read(&settings_file) {
        Ok(read) => read,
        Err(error) => {
            eprintln!("gxserver: could not read settings for fresh Native defaults: {error}");
            return;
        }
    };
    let onboarding = onboarding_state(&paths.app_state_dir);
    let existing_install = db_has_projects_or_sessions(paths)
        || onboarding == Some(true)
        || (read.object.is_some() && onboarding != Some(false));

    let outcome = if existing_install {
        "existing install, session mode unchanged"
    } else {
        let existed = read.object.is_some();
        let mut settings = read.object.unwrap_or_default();
        if !ghostex_settings_catalog::preferred_interface_is_chat(&settings) {
            "fresh install, preferred view terminal, session mode unchanged"
        } else if ghostex_settings_catalog::apply_fresh_install_native_defaults(&mut settings) {
            let saved = if !existed {
                // Save through headless_settings so a headless box / throwaway server keeps owning
                // the settings file for `ghostex settings set`, while desktop can still adopt it.
                let mut patch = Map::new();
                if let Some(modes) = settings.get("agentSessionMode").cloned() {
                    patch.insert("agentSessionMode".to_string(), modes);
                }
                crate::headless_settings::save_settings_patch(paths, &patch)
                    .map(|result| result.is_some())
            } else {
                serde_json::to_vec_pretty(&Value::Object(settings))
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        ghostex_paths::settings_file::save(&settings_file, &bytes)
                            .map(|_| true)
                            .map_err(|error| error.to_string())
                    })
            };
            match saved {
                Ok(true) => "fresh install, preferred view chat, native defaults applied",
                Ok(false) => "fresh install, settings owned elsewhere, skipping headless write",
                Err(error) => {
                    eprintln!("gxserver: could not save fresh Native defaults: {error}");
                    return;
                }
            }
        } else {
            "fresh install, session mode already set"
        }
    };

    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{outcome}\n"));
}
