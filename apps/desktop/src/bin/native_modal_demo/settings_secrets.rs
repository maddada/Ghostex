//! Preview fixtures for the Secrets Settings page: a scripted `/api/readSecretProviders` with one
//! 1Password account signed in and one the CLI does not know, and `/api/checkSecretReference`
//! that reads only `op://Private/Example/credential`.
//!
//! States: `secrets` (the page), `secrets-empty` (no account yet, and no `op` installed).
use serde_json::{Value, json};

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("secrets")
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    owns(state).then(|| json!({ "initialTab": "secrets" }))
}

/// The scripted daemon for these states; `None` for paths it does not know.
pub(super) fn rpc(state: &str, path: &str, params: &Value) -> Option<Result<Value, String>> {
    Some(Ok(match path {
        "/api/readSecretProviders" if state == "secrets-empty" => {
            json!({ "providers": [], "onePasswordCliInstalled": false })
        }
        "/api/readSecretProviders" => json!({
            "onePasswordCliInstalled": true,
            "providers": [
                { "id": "personal", "kind": "onePassword", "label": "Personal", "account": "my.1password.com", "status": "ready" },
                { "id": "work", "kind": "onePassword", "label": "Work", "account": "team.1password.com", "status": "accountMissing" }
            ]
        }),
        "/api/checkSecretReference" => {
            if params["reference"] == "op://Private/Example/credential" {
                json!({ "readable": true })
            } else {
                json!({ "readable": false, "message": "1Password could not read that reference." })
            }
        }
        "/api/saveSecretProvider" | "/api/removeSecretProvider" => json!({}),
        _ => return None,
    }))
}
