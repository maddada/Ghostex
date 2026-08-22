use std::{fs, io};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ids, paths::GxserverPaths};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GxserverIdentityFile {
    pub created_at: String,
    pub server_id: String,
}

/*
CDXC:GxserverIdentity 2026-06-14-20:37:
identity.json carries the stable serverId across Rust daemon restarts. Runtime metadata stays separate so stale pid/port files can be removed without changing server-scoped refs.

CDXC:GxserverIdentity 2026-06-22-04:38:
Existing identity files only need a valid serverId to remain reusable. If createdAt is missing or not a string, preserve the serverId and report the Unix epoch timestamp exactly like TypeScript.
*/
pub fn ensure_gxserver_identity(paths: &GxserverPaths) -> Result<GxserverIdentityFile> {
    if let Some(existing) = read_gxserver_identity(paths)? {
        return Ok(existing);
    }
    fs::create_dir_all(&paths.root_dir).with_context(|| "create gxserver root")?;
    let identity = GxserverIdentityFile {
        created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        server_id: ids::create_server_id(),
    };
    fs::write(
        &paths.identity_file,
        format!("{}\n", serde_json::to_string_pretty(&identity)?),
    )
    .with_context(|| "write gxserver identity")?;
    set_file_mode_0600(&paths.identity_file)?;
    Ok(identity)
}

fn read_gxserver_identity(paths: &GxserverPaths) -> Result<Option<GxserverIdentityFile>> {
    match fs::read_to_string(&paths.identity_file) {
        Ok(text) => {
            let parsed: Value =
                serde_json::from_str(&text).with_context(|| "parse gxserver identity")?;
            let server_id = parsed
                .get("serverId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let Some(server_id) =
                server_id.filter(|server_id| ids::is_gxserver_server_id(server_id))
            else {
                bail!(
                    "Invalid gxserver identity file at {}. Expected serverId like S7k.",
                    paths.identity_file.display()
                );
            };
            let created_at = parsed
                .get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or("1970-01-01T00:00:00.000Z")
                .to_string();
            Ok(Some(GxserverIdentityFile {
                created_at,
                server_id,
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| "read gxserver identity"),
    }
}

#[cfg(unix)]
fn set_file_mode_0600(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_mode_0600(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

    #[test]
    fn existing_identity_reuses_server_id_without_created_at() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.root_dir).expect("root");
        fs::write(&paths.identity_file, r#"{"serverId":"S7k"}"#).expect("identity");

        let identity = ensure_gxserver_identity(&paths).expect("identity");

        assert_eq!(identity.server_id, "S7k");
        assert_eq!(identity.created_at, "1970-01-01T00:00:00.000Z");
    }
}
