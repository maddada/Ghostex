//! A workspace's Executor: whether it is on, its fixed port and the two keys Ghostex made for it,
//! sealed in the connectors store; and the folder its data lives in.

use std::net::TcpListener;
use std::path::PathBuf;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

use super::super::store;

const PROVIDER: &str = "executor";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorkspaceExecutor {
    pub(super) enabled: bool,
    /// Picked once: Executor's OAuth sign-ins come back to this port, so it never changes.
    pub(super) port: u16,
    /// `EXECUTOR_API_KEY`: the bearer key for its API.
    pub(super) api_key: String,
    /// `EXECUTOR_ENCRYPTION_KEY`: encrypts the app sign-ins in its database.
    pub(super) encryption_key: String,
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("Couldn't find a free port for Executor: {error}"))
}

/// What Settings and the CLI say when the saved keys don't open any more.
const UNREADABLE: &str =
    "This workspace's saved Executor keys can't be read anymore. Turn Executor on again to give it new ones.";

/// The workspace's saved Executor, or none when it was never turned on.
pub(super) fn load(
    paths: &GxserverPaths,
    workspace: &str,
) -> Result<Option<WorkspaceExecutor>, String> {
    let plain = match store::read_secret(paths, PROVIDER, workspace) {
        Ok(Some(plain)) => plain,
        Ok(None) => return Ok(None),
        Err(store::ReadError::Unreadable) => return Err(UNREADABLE.to_string()),
        Err(store::ReadError::Failed(message)) => return Err(message),
    };
    serde_json::from_str(&plain)
        .map(Some)
        .map_err(|error| format!("The saved Executor settings are unreadable: {error}"))
}

/// The saved keys are there but don't open any more (`unreadable` in the Executor status).
pub(super) fn unreadable(paths: &GxserverPaths, workspace: &str) -> bool {
    matches!(
        store::read_secret(paths, PROVIDER, workspace),
        Err(store::ReadError::Unreadable)
    )
}

pub(super) fn save(
    paths: &GxserverPaths,
    workspace: &str,
    settings: &WorkspaceExecutor,
) -> Result<(), String> {
    let plain = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    store::write_secret(paths, PROVIDER, workspace, &plain)
}

/// The saved settings, or new ones (a free port and fresh keys) saved first.
pub(super) fn load_or_create(
    paths: &GxserverPaths,
    workspace: &str,
) -> Result<WorkspaceExecutor, String> {
    match load(paths, workspace) {
        Ok(Some(settings)) => return Ok(settings),
        Ok(None) => {}
        // Keys that can't be read never will be, so turning Executor on again replaces them.
        // Executor 2.0.0-beta.22 opens its existing data folder with new keys (tried 2026-10-22).
        Err(_) if unreadable(paths, workspace) => {}
        Err(error) => return Err(error),
    }
    let settings = WorkspaceExecutor {
        enabled: false,
        port: free_port()?,
        api_key: random_hex(24),
        encryption_key: random_hex(32),
    };
    save(paths, workspace, &settings)?;
    Ok(settings)
}

/// `<gxserver state>/connectors/executor/<workspace>`: everything Executor stores for the
/// workspace (its database, accounts and app builds).
pub(super) fn data_dir(paths: &GxserverPaths, workspace: &str) -> PathBuf {
    paths
        .root_dir
        .join("connectors")
        .join("executor")
        .join(store::file_stem(workspace))
}
