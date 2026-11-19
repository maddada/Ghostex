//! The CLIProxyAPI hubs the user added, in one private file beside gxserver's auth token. Both keys
//! are sealed with gxserver's vault (crate::vault) before they are written, and only unsealed to
//! call the hub or to start a session on it: no answer, log or settings file ever carries them.
//!
//! CDXC:AgentProviders 2026-11-18 WHY:
//! A hub's management key can download every token the hub holds (`/v0/management/auth-files/download`), so both keys are treated like a Connectors sign-in: sealed in the vault, kept out of the shared (synced) settings file, and the file itself is written 0600 through a rename.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

const FILE_NAME: &str = "cliproxy-hubs.json";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Hub {
    /// `hub-<hex>`, made when the hub is added; names its profile folders too.
    pub id: String,
    /// What the user called it; empty shows the hub's host.
    #[serde(default)]
    pub label: String,
    /// The hub's address without a trailing slash (`http://127.0.0.1:8317`).
    pub url: String,
    /// The client key (one of the hub's `api-keys`), sealed.
    pub client_key: String,
    /// The management key, sealed; empty when the user gave none (no accounts or usage then).
    #[serde(default)]
    pub management_key: String,
    /// The hub accounts (`auth_index`) the user took out of the sidebar's usage meters; every
    /// other account shows there.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_in_sidebar: BTreeSet<String>,
}

#[derive(Default, Deserialize, Serialize)]
struct File {
    #[serde(default)]
    hubs: Vec<Hub>,
}

fn path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(FILE_NAME)
}

/// Every reader and writer holds this across read, change and write.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The saved hubs; a missing file has none. A file that can't be read is an error, so a change
/// never writes over hubs it could not read.
pub(crate) fn read(paths: &GxserverPaths) -> std::io::Result<Vec<Hub>> {
    match fs::read_to_string(path(paths)) {
        Ok(text) => serde_json::from_str::<File>(&text)
            .map(|file| file.hubs)
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{FILE_NAME} can't be read ({error})"),
                )
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

pub(crate) fn write(paths: &GxserverPaths, hubs: &[Hub]) -> std::io::Result<()> {
    fs::create_dir_all(&paths.auth_dir)?;
    let target = path(paths);
    let temp = target.with_extension("json.tmp");
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
        let body = serde_json::to_vec(&File {
            hubs: hubs.to_vec(),
        })
        .map_err(std::io::Error::other)?;
        file.write_all(&body)?;
        file.sync_all()?;
    }
    fs::rename(&temp, &target)
}

/// One hub by id.
pub(crate) fn find(paths: &GxserverPaths, id: &str) -> Option<Hub> {
    read(paths).ok()?.into_iter().find(|hub| hub.id == id)
}

/// A sealed key opened, or the sentence to show when it can't be.
pub(crate) fn open_key(paths: &GxserverPaths, sealed: &str) -> Result<String, String> {
    crate::vault::unseal(paths, sealed).map_err(|error| match error {
        crate::vault::UnsealError::Unreadable => {
            "This hub's saved key can't be read on this computer. Edit the hub and enter its keys again."
                .to_string()
        }
        crate::vault::UnsealError::Vault(message) => message,
    })
}
