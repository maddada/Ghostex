//! The usage-only accounts and their sign-ins, in one private file beside gxserver's auth token.
//!
//! CDXC:AgentProviders 2026-11-05 WHY:
//! These logins are Ghostex's own (no helper keeps them), and gxserver has no OS credential store it
//! can use on every platform, so they live like the Work Mode credentials
//! (work_mode/credentials.rs): one file in the 0700 auth folder, written 0600 through a rename. No
//! endpoint ever returns a token.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::UsageProvider;
use crate::paths::GxserverPaths;

const FILE_NAME: &str = "usage-accounts.json";

/// A sign-in's tokens. `extra` holds what a provider needs besides them (Gemini's project).
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Credential {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Milliseconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageAccount {
    pub id: String,
    pub provider: UsageProvider,
    /// The provider's own user id, which tells two sign-ins to one account apart from two accounts.
    pub identity: String,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub show_in_titlebar: bool,
    #[serde(default)]
    pub user_disabled: bool,
    pub credential: Credential,
}

#[derive(Default, Deserialize, Serialize)]
struct File {
    #[serde(default)]
    accounts: Vec<UsageAccount>,
}

fn path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(FILE_NAME)
}

/// Every reader and writer holds this across read, change and write.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The saved accounts; a missing file has none. A file that can't be read is an error, so a change
/// never writes over accounts it could not read.
pub(crate) fn read(paths: &GxserverPaths) -> std::io::Result<Vec<UsageAccount>> {
    match fs::read_to_string(path(paths)) {
        Ok(text) => serde_json::from_str::<File>(&text)
            .map(|file| file.accounts)
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

pub(crate) fn write(paths: &GxserverPaths, accounts: &[UsageAccount]) -> std::io::Result<()> {
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
            accounts: accounts.to_vec(),
        })
        .map_err(std::io::Error::other)?;
        file.write_all(&body)?;
        file.sync_all()?;
    }
    fs::rename(&temp, &target)
}

/// Changes one account under the lock and saves the file; `false` when it is no longer saved.
pub(crate) fn update(
    paths: &GxserverPaths,
    id: &str,
    change: impl FnOnce(&mut UsageAccount),
) -> std::io::Result<bool> {
    let _lock = lock();
    let mut accounts = read(paths)?;
    let Some(account) = accounts.iter_mut().find(|account| account.id == id) else {
        return Ok(false);
    };
    change(account);
    write(paths, &accounts)?;
    Ok(true)
}
