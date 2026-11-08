//! `gxserver/auth/security.json`: the owner keys, approver keys and session token hashes that
//! must survive a gxserver restart (zmx sessions outlive it and keep their token).
//!
//! Written whole, through a temporary file and a rename, mode 0600 in the 0700 auth folder.
//! Approval requests and owner sessions are never written: they live minutes, or only in the
//! desktop app's memory.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use ghostex_gx_protocol::approval::ApproverKey;
use serde::{Deserialize, Serialize};

use super::SessionRole;

pub const SECURITY_FILE_NAME: &str = "security.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSecurity {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub owner_keys: Vec<ApproverKey>,
    #[serde(default)]
    pub approver_keys: Vec<ApproverKey>,
    #[serde(default)]
    pub session_tokens: Vec<PersistedSessionToken>,
    /// The desktop's owner sessions, as hashes only: the token itself lives in the app's memory.
    #[serde(default)]
    pub owner_sessions: Vec<PersistedOwnerSession>,
    /// Connected computers the owner approved, by server id, with when (ms).
    #[serde(default)]
    pub approved_computers: std::collections::BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedOwnerSession {
    pub hash: String,
    pub device_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSessionToken {
    /// The session's `gxs_` token (see `session_tokens.rs` for why it is kept whole).
    pub token: String,
    pub project_id: String,
    pub session_id: String,
    pub role: SessionRole,
}

pub fn security_file(auth_dir: &Path) -> PathBuf {
    auth_dir.join(SECURITY_FILE_NAME)
}

/// The saved state, or an empty one when the file is missing. A file that can't be parsed is
/// an error: starting empty would silently drop every enrolled key and session token.
pub fn load(path: &Path) -> io::Result<PersistedSecurity> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(PersistedSecurity::default()),
        Err(error) => Err(error),
    }
}

pub fn save(path: &Path, state: &PersistedSecurity) -> io::Result<()> {
    let mut state = state.clone();
    state.version = 1;
    let text = serde_json::to_string_pretty(&state).map_err(io::Error::other)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    write_private(&temp, text.as_bytes())?;
    fs::rename(&temp, path)
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}
