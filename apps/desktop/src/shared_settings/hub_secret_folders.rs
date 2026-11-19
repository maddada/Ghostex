// Adds the CLIProxyAPI hub key files to a saved "Folders agents can't read in your home folder"
// list once (`add_hub_secret_folders` in the settings catalog).

use super::*;
use serde_json::Value;

/// Once per computer, before the first settings read: a saved `homeSecretFolders` list gains the
/// hub entries; the marker keeps a later removal of them in place.
pub fn add_hub_secret_folders_once() {
    let marker = ghostex_storage_paths().hub_secret_folders_marker_file();
    if marker.exists() {
        return;
    }
    let settings_file = shared_sidebar_settings_path();
    let Ok(read) = ghostex_paths::settings_file::read(&settings_file) else {
        // An unreadable file is tried again at the next start.
        return;
    };
    if let Some(mut object) = read.object {
        if ghostex_settings_catalog::add_hub_secret_folders(&mut object) {
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
                    eprintln!(
                        "Ghostex could not add the hub key files to the secret folders: {error}"
                    );
                    return;
                }
            }
        }
    }
    if let Some(folder) = marker.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(&marker, b"");
}
