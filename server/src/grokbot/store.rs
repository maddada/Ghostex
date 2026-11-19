//! What gxserver keeps for the Grok Bot connection, in `<app state>/grokbot/` (a 0700 folder):
//! `connection.json` (the webhook address and what happened lately), `webhook-key` (the key alone,
//! 0600, read only by delivery.rs), `queue.json` (notes not delivered yet) and `setup.json` (how
//! far Settings > Grok Bot's checklist got).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

const DIR: &str = "grokbot";
const CONNECTION_FILE: &str = "connection.json";
const KEY_FILE: &str = "webhook-key";
const QUEUE_FILE: &str = "queue.json";
const SETUP_FILE: &str = "setup.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Connection {
    pub(crate) url: String,
    pub(crate) connected_at: i64,
    #[serde(default)]
    pub(crate) last_sent_at: Option<i64>,
    #[serde(default)]
    pub(crate) last_error: Option<String>,
    #[serde(default)]
    pub(crate) last_error_at: Option<i64>,
    #[serde(default)]
    pub(crate) sent: u64,
}

/// One note waiting to go out. `from` is what the CLI read about the agent that sent it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueuedNote {
    pub(crate) at: i64,
    pub(crate) kind: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
    /// The sender's global reference, which `ghostex agents send <ref>` answers.
    #[serde(default, rename = "ref")]
    pub(crate) reply_to: Option<String>,
}

/// What Settings > Grok Bot's checklist has seen happen: the bot's first `ghostex grokbot brief`
/// here, the user saying the desktop app is installed (where Ghostex can't look for it), and their
/// answer to "Did the test note show up?".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Setup {
    #[serde(default)]
    pub(crate) brief_seen_at: Option<i64>,
    #[serde(default)]
    pub(crate) app_confirmed_at: Option<i64>,
    /// `true` when the user saw the test note, `false` when they didn't; `None` until they answer.
    #[serde(default)]
    pub(crate) test_arrived: Option<bool>,
}

pub(crate) fn dir(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join(DIR)
}

fn private_dir(paths: &GxserverPaths) -> std::io::Result<PathBuf> {
    let dir = dir(paths);
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// Replaces `<dir>/<name>` with `bytes` through a fresh 0600 file, never writing through an
/// existing path.
fn write_private(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let file = dir.join(name);
    let partial = dir.join(format!("{name}.partial"));
    let _ = fs::remove_file(&partial);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| {
        let mut handle = options.open(&partial)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
        fs::rename(&partial, &file)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

pub(crate) fn read_connection(paths: &GxserverPaths) -> Option<Connection> {
    let text = fs::read_to_string(dir(paths).join(CONNECTION_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn write_connection(
    paths: &GxserverPaths,
    connection: &Connection,
) -> std::io::Result<()> {
    let dir = private_dir(paths)?;
    let bytes = serde_json::to_vec_pretty(connection).map_err(std::io::Error::other)?;
    write_private(&dir, CONNECTION_FILE, &bytes)
}

pub(crate) fn read_key(paths: &GxserverPaths) -> Option<String> {
    let key = fs::read_to_string(dir(paths).join(KEY_FILE)).ok()?;
    let key = key.trim().to_string();
    (!key.is_empty()).then_some(key)
}

pub(crate) fn write_key(paths: &GxserverPaths, key: &str) -> std::io::Result<()> {
    let dir = private_dir(paths)?;
    write_private(&dir, KEY_FILE, key.trim().as_bytes())
}

pub(crate) fn read_queue(paths: &GxserverPaths) -> Vec<QueuedNote> {
    fs::read_to_string(dir(paths).join(QUEUE_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn write_queue(paths: &GxserverPaths, queue: &[QueuedNote]) -> std::io::Result<()> {
    let dir = private_dir(paths)?;
    if queue.is_empty() {
        return match fs::remove_file(dir.join(QUEUE_FILE)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let bytes = serde_json::to_vec(queue).map_err(std::io::Error::other)?;
    write_private(&dir, QUEUE_FILE, &bytes)
}

pub(crate) fn read_setup(paths: &GxserverPaths) -> Setup {
    fs::read_to_string(dir(paths).join(SETUP_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn write_setup(paths: &GxserverPaths, setup: &Setup) -> std::io::Result<()> {
    let dir = private_dir(paths)?;
    let bytes = serde_json::to_vec_pretty(setup).map_err(std::io::Error::other)?;
    write_private(&dir, SETUP_FILE, &bytes)
}

/// Forgets the connection: the key, the address and every note still waiting.
pub(crate) fn remove_all(paths: &GxserverPaths) -> std::io::Result<()> {
    let dir = dir(paths);
    for name in [KEY_FILE, CONNECTION_FILE, QUEUE_FILE] {
        match fs::remove_file(dir.join(name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}
