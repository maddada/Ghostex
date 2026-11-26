//! Each workspace's connector sign-ins on disk: one file per provider and workspace under
//! `<auth>/connectors/<provider>/`, sealed with the vault and written 0600.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::paths::GxserverPaths;
use crate::vault::{self, UnsealError};

/// A workspace id as a file name: ids are `personal` or `ws-<uuid>`, so anything else is
/// hex-encoded rather than trusted as a path part.
pub(super) fn file_stem(workspace_id: &str) -> String {
    let plain = !workspace_id.is_empty()
        && workspace_id.len() <= 128
        && workspace_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        workspace_id.to_string()
    } else {
        let hex: String = workspace_id
            .bytes()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("x-{hex}")
    }
}

fn sealed_path(paths: &GxserverPaths, provider: &str, workspace_id: &str) -> PathBuf {
    paths
        .auth_dir
        .join("connectors")
        .join(provider)
        .join(format!("{}.sealed", file_stem(workspace_id)))
}

/// Why a saved sign-in couldn't be read.
pub(super) enum ReadError {
    /// It doesn't open any more, so only signing in again helps.
    ///
    /// CDXC:Connectors 2026-10-22 DECISION:
    /// User, asked whether Settings should say when a saved key can't be unlocked: "sure just make the error more general please". A saved sign-in that no longer opens shows on Settings > Connectors and in `ghostex connectors status` as needing a new sign-in instead of as signed out, in general words that never guess why (no other computer, restore or vault key).
    Unreadable,
    /// Couldn't read it right now (the vault key didn't load, or the file couldn't be read).
    Failed,
}

/// The workspace's saved sign-in, unsealed; `None` when it has none.
pub(super) fn read_secret(
    paths: &GxserverPaths,
    provider: &str,
    workspace_id: &str,
) -> Result<Option<String>, ReadError> {
    match fs::read_to_string(sealed_path(paths, provider, workspace_id)) {
        Ok(sealed) => match vault::unseal(paths, &sealed) {
            Ok(plain) => Ok(Some(plain)),
            Err(UnsealError::Unreadable) => Err(ReadError::Unreadable),
            Err(UnsealError::Vault(_)) => Err(ReadError::Failed),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ReadError::Failed),
    }
}

/// Seals and saves the workspace's sign-in (a temporary file renamed over the old one).
pub(super) fn write_secret(
    paths: &GxserverPaths,
    provider: &str,
    workspace_id: &str,
    plain: &str,
) -> Result<(), String> {
    let sealed = vault::seal(paths, plain)?;
    let path = sealed_path(paths, provider, workspace_id);
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("sealed.tmp");
        let _ = fs::remove_file(&temp);
        {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(sealed.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&temp, &path)
    };
    write().map_err(|error| format!("Couldn't save the sign-in: {error}"))
}

/// Forgets the workspace's sign-in.
pub(super) fn remove_secret(paths: &GxserverPaths, provider: &str, workspace_id: &str) {
    let _ = fs::remove_file(sealed_path(paths, provider, workspace_id));
}
