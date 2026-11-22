//! The Google Cloud OAuth client a workspace's `gws` signs in with: the workspace's own when one
//! was given, else a copy of the user's usual one (`~/.config/gws/client_secret.json`).
//!
//! CDXC:Connectors 2026-10-18 DECISION:
//! User (Q26): each workspace can use its own Google OAuth client for gws. An Internal (organization-only) client blocked a personal Gmail sign-in in Personal with "Error 403: org_internal", while it is the right one for a Work workspace. A workspace's own client stays when it signs out; choosing or clearing a client signs the workspace out of gws, since its sign-in belongs to the old client.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::tools::{config_dir, CliTool};

const CLIENT_FILE: &str = "client_secret.json";
/// Marks `client_secret.json` as the workspace's own choice, not a copy of the usual one.
const OWN_MARKER: &str = "ghostex-own-client";

fn usual_client(paths: &GxserverPaths) -> PathBuf {
    std::env::var_os("GOOGLE_WORKSPACE_CLI_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.home_dir.join(".config").join("gws"))
        .join(CLIENT_FILE)
}

/// The client's Google Cloud project, read from a `client_secret.json`.
fn project_of(file: &Path) -> Option<String> {
    let value: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    let client = value.get("installed").or_else(|| value.get("web"))?;
    client
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn own(dir: &Path) -> bool {
    dir.join(OWN_MARKER).is_file()
}

/// The client as Settings shows it: whose it is and its project.
pub(crate) fn client_status(paths: &GxserverPaths, workspace: &str) -> Value {
    let dir = config_dir(paths, workspace, CliTool::Gws);
    if own(&dir) {
        return json!({ "own": true, "project": project_of(&dir.join(CLIENT_FILE)) });
    }
    json!({ "own": false, "project": project_of(&usual_client(paths)) })
}

/// Puts the usual client in the workspace's folder when it has none yet.
pub(crate) fn ensure_client(paths: &GxserverPaths, dir: &Path) -> Result<(), String> {
    let target = dir.join(CLIENT_FILE);
    if target.is_file() || std::env::var_os("GOOGLE_WORKSPACE_CLI_CLIENT_ID").is_some() {
        return Ok(());
    }
    let source = Some(usual_client(paths))
        .filter(|file| file.is_file())
        .ok_or_else(|| {
            "gws needs a Google Cloud OAuth client first: choose one for this workspace, run `gws auth setup` once (it uses gcloud), or put its client_secret.json in ~/.config/gws, then sign in again.".to_string()
        })?;
    fs::copy(&source, &target)
        .map(|_| ())
        .map_err(|error| format!("Couldn't copy your gws OAuth client: {error}"))
}

/// What the workspace's own client must be kept as across a sign-out.
pub(crate) fn kept_client(dir: &Path) -> Option<String> {
    own(dir)
        .then(|| fs::read_to_string(dir.join(CLIENT_FILE)).ok())
        .flatten()
}

/// Writes the workspace's own client back after its folder was emptied.
pub(crate) fn restore_client(dir: &Path, client: &str) {
    if fs::create_dir_all(dir).is_ok() {
        let _ = fs::write(dir.join(CLIENT_FILE), client);
        let _ = fs::write(dir.join(OWN_MARKER), "");
    }
}

/// Gives the workspace its own client from a `client_secret.json` (`Some`), or goes back to the
/// usual one (`None`). Either way the workspace is signed out of gws.
pub(crate) fn set_client(
    paths: &GxserverPaths,
    workspace: &str,
    file: Option<&Path>,
) -> Result<Value, String> {
    let contents = match file {
        Some(file) => {
            let text = fs::read_to_string(file)
                .map_err(|error| format!("Couldn't read {}: {error}", file.display()))?;
            let value: Value = serde_json::from_str(&text).map_err(|_| {
                "That file isn't a Google OAuth client (client_secret.json).".to_string()
            })?;
            let client = value.get("installed").or_else(|| value.get("web"));
            if client
                .and_then(|client| client.get("client_id"))
                .and_then(Value::as_str)
                .is_none()
            {
                return Err(
                    "That file isn't a Google OAuth client: download the client's JSON from Google Cloud Console › Google Auth Platform › Clients (a Desktop app client)."
                        .to_string(),
                );
            }
            Some(text)
        }
        None => None,
    };
    let dir = config_dir(paths, workspace, CliTool::Gws);
    super::signin::sign_out(paths, workspace, CliTool::Gws);
    if let Some(text) = contents {
        restore_client(&dir, &text);
    } else {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(client_status(paths, workspace))
}
