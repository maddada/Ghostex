//! Which Beads workspace a project's folder belongs to, asked of `bd where` once and kept until
//! the files that decide it change.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::typed_operations::{
    beads_where_reports_no_workspace, is_home_beads_directory, run_process_command_with_timeout,
    ProcessCommand, TypedOperationContext,
};

use super::cache;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// A failed `bd where` is asked again after this long, not on every poll.
const FAILED_DISCOVERY_RETRY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceInfo {
    /// The canonical `.beads` path; the workspace's identity.
    pub(crate) path: String,
    pub(crate) prefix: String,
    pub(crate) database_path: String,
    pub(crate) redirected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Discovered {
    Workspace(WorkspaceInfo),
    /// bd found no workspace (or only the home folder's `~/.beads`).
    None,
    Failed(String),
}

struct DiscoveryRecord {
    signature: Vec<String>,
    result: Discovered,
    at: Instant,
}

fn records() -> &'static Mutex<HashMap<String, DiscoveryRecord>> {
    static RECORDS: OnceLock<Mutex<HashMap<String, DiscoveryRecord>>> = OnceLock::new();
    RECORDS.get_or_init(Default::default)
}

fn flights() -> &'static Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static FLIGHTS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    FLIGHTS.get_or_init(Default::default)
}

/// Forgets what bd said about `cwd` (the header's Refresh, a bd call that failed on the workspace).
pub(crate) fn forget(cwd: &str) {
    records().lock().expect("beads discovery").remove(cwd);
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// `bd where` opens the workspace's storage, so asking it on a timer kept an idle view spawning bd. The answer is kept until its signature changes, read from the files bd decides by: a `.beads` folder in each folder from the project up to the one bd found, each such folder's entries (so a `.beads` that an agent initializes in place, where `metadata.json`, `config.yaml` and the database folder appear, invalidates a cached "no workspace"), the redirect file's content and the target's entries. Only one `bd where` runs per folder at a time, under the workspace's own lock when one is known, and it gives up after 10 seconds.
pub(crate) async fn discover(
    cwd: &str,
    bd_executable_path: &str,
    context: &TypedOperationContext,
) -> Discovered {
    if let Some(result) = current_record(cwd) {
        return result;
    }
    let flight = {
        let mut flights = flights().lock().expect("beads discovery flights");
        flights.entry(cwd.to_string()).or_default().clone()
    };
    let _flight = flight.lock().await;
    if let Some(result) = current_record(cwd) {
        return result;
    }
    let previous = records()
        .lock()
        .expect("beads discovery")
        .get(cwd)
        .and_then(|record| match &record.result {
            Discovered::Workspace(info) => Some(info.path.clone()),
            _ => None,
        });
    let held = match previous.as_deref().and_then(cache::existing_entry) {
        Some(entry) => Some(entry.lock_owned().await),
        None => None,
    };
    let result = run_bd_where(cwd, bd_executable_path, context).await;
    drop(held);
    let found = match &result {
        Discovered::Workspace(info) => Some(PathBuf::from(&info.path)),
        _ => None,
    };
    let signature = discovery_signature(Path::new(cwd), found.as_deref());
    records().lock().expect("beads discovery").insert(
        cwd.to_string(),
        DiscoveryRecord {
            signature,
            result: result.clone(),
            at: Instant::now(),
        },
    );
    result
}

fn current_record(cwd: &str) -> Option<Discovered> {
    let records = records().lock().expect("beads discovery");
    let record = records.get(cwd)?;
    if matches!(record.result, Discovered::Failed(_))
        && record.at.elapsed() >= FAILED_DISCOVERY_RETRY
    {
        return None;
    }
    let found = match &record.result {
        Discovered::Workspace(info) => Some(PathBuf::from(&info.path)),
        _ => None,
    };
    (discovery_signature(Path::new(cwd), found.as_deref()) == record.signature)
        .then(|| record.result.clone())
}

async fn run_bd_where(
    cwd: &str,
    bd_executable_path: &str,
    context: &TypedOperationContext,
) -> Discovered {
    let command = ProcessCommand::new(bd_executable_path, vec!["where", "--json"], cwd);
    super::log::bd_call("discovery", "where");
    let output = match run_process_command_with_timeout(&command, context, DISCOVERY_TIMEOUT).await
    {
        Ok(output) => output,
        Err(error) => return Discovered::Failed(error.message),
    };
    if output.exit_code != 0 {
        if beads_where_reports_no_workspace(&output.stdout)
            || beads_where_reports_no_workspace(&output.stderr)
        {
            return Discovered::None;
        }
        let message = if output.stderr.trim().is_empty() {
            output
                .error
                .as_ref()
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("bd where failed.")
                .to_string()
        } else {
            output.stderr.clone()
        };
        return Discovered::Failed(message);
    }
    let Ok(parsed) = serde_json::from_str::<Value>(output.stdout.trim()) else {
        return Discovered::Failed("bd where printed something unreadable.".to_string());
    };
    let parsed = parsed.get("data").cloned().unwrap_or(parsed);
    let text = |key: &str| {
        parsed
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let path = text("path");
    if path.is_empty() || !Path::new(&path).is_absolute() {
        return Discovered::Failed("bd where named no workspace folder.".to_string());
    }
    if is_home_beads_directory(&path) {
        return Discovered::None;
    }
    let canonical = fs::canonicalize(&path)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or(path);
    Discovered::Workspace(WorkspaceInfo {
        path: canonical,
        prefix: text("prefix"),
        database_path: text("database_path"),
        redirected: !text("redirected_from").is_empty(),
    })
}

/// Entries of a `.beads` folder that say what it is. Lock, pid, socket and log files come and go
/// while bd runs, so they are left out.
fn beads_folder_entries(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return vec![format!("unreadable:{}", folder.display())];
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| {
            !(name.ends_with(".lock")
                || name.ends_with(".pid")
                || name.ends_with(".sock")
                || name.ends_with(".log")
                || name.ends_with(".tmp")
                || name == "LOCK")
        })
        .collect();
    names.sort();
    names
}

fn discovery_signature(cwd: &Path, found: Option<&Path>) -> Vec<String> {
    let mut signature = Vec::new();
    let mut folder = Some(cwd);
    while let Some(current) = folder {
        let beads = current.join(".beads");
        if beads.is_dir() {
            signature.push(format!("beads:{}", beads.display()));
            signature.extend(beads_folder_entries(&beads));
            let redirect = fs::read_to_string(beads.join("redirect")).unwrap_or_default();
            let redirect = redirect.trim().to_string();
            if !redirect.is_empty() {
                signature.push(format!("redirect:{redirect}"));
                let target = if Path::new(&redirect).is_absolute() {
                    PathBuf::from(&redirect)
                } else {
                    beads.join(&redirect)
                };
                signature.extend(beads_folder_entries(&target));
            }
            break;
        }
        signature.push(format!("none:{}", current.display()));
        folder = current.parent();
    }
    if let Some(found) = found {
        signature.push(format!("found:{}:{}", found.display(), found.is_dir()));
        signature.extend(beads_folder_entries(found));
    }
    signature
}
