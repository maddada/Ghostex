//! What the old built-in Executor left on this computer: the pinned Executor build Ghostex
//! downloaded into its tools folder, and each workspace's Executor data (its apps, their sign-ins
//! and keys). The build goes at once; the data is listed once in Settings by its apps' names, then
//! deleted 30 days later or when the user presses Remove.
//!
//! CDXC:Connectors 2026-11-27 DECISION:
//! User (Executor CLI page, PLACEHOLDER-4): list the old apps once, then clean up. Executor documents no migration and each app's sign-ins are encrypted inside its own server, so nothing can move to the CLI's Executor; the user adds the apps again there. The names come from each app's own source (`package.json` in its Git repository under the data folder), so the old Executor never has to run again.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::cli::MANAGEMENT_SLUG;

const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MARKER: &str = "executor-old.json";

fn connectors_dir(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("connectors")
}

/// `<gxserver state>/connectors/executor/<workspace>`: each workspace's old Executor data.
fn data_dir(paths: &GxserverPaths) -> PathBuf {
    connectors_dir(paths).join("executor")
}

/// `<auth>/connectors/executor/<workspace>.sealed`: each workspace's old port and keys.
fn keys_dir(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join("connectors").join("executor")
}

/// `<tools>/executor/<version>`: the pinned build.
fn build_dir() -> PathBuf {
    crate::managed_tools::paths::tools_root().join("executor")
}

fn marker(paths: &GxserverPaths) -> PathBuf {
    connectors_dir(paths).join(MARKER)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn workspace_dirs(paths: &GxserverPaths) -> Vec<PathBuf> {
    fs::read_dir(data_dir(paths))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// An app's name from its `package.json`: `executor.title`, else its package name without the
/// scope, first letter capitalised (`@team/linear` gives `Linear`).
fn app_name(package: &Value) -> Option<String> {
    if let Some(title) = package.pointer("/executor/title").and_then(Value::as_str) {
        return Some(title.to_string());
    }
    let name = package.get("name").and_then(Value::as_str)?;
    let bare = name.rsplit('/').next().unwrap_or(name);
    if bare.is_empty() || bare == MANAGEMENT_SLUG {
        return None;
    }
    let mut letters = bare.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

fn read_package(repository: &Path) -> Option<Value> {
    let mut command = crate::platform::process::background_command("git");
    command
        .arg("--git-dir")
        .arg(repository)
        .args(["show", "HEAD:package.json"])
        .env("PATH", crate::platform::live_path::value());
    let output = crate::managed_tools::run::output_within(command, Duration::from_secs(5)).ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_str(&output.stdout).ok()
}

/// Every old workspace's app names, without Executor's own app.
fn read_app_names(paths: &GxserverPaths) -> Vec<String> {
    let mut names: Vec<String> = workspace_dirs(paths)
        .iter()
        .flat_map(|workspace| {
            fs::read_dir(workspace.join("repositories"))
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| entry.path())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .filter_map(|repository| read_package(&repository).as_ref().and_then(app_name))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

/// Stops an old Executor a previous gxserver left running (its pid file in a workspace's data
/// folder), only when that process still runs the old build.
fn stop_leftovers(paths: &GxserverPaths) {
    for workspace in workspace_dirs(paths) {
        let pid_file = workspace.join("ghostex-executor.pid");
        let Some(pid) = fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
        else {
            continue;
        };
        #[cfg(unix)]
        if runs_old_build(pid) {
            // SAFETY: a signal to a process checked above to run the old build.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
        #[cfg(not(unix))]
        let _ = pid;
        let _ = fs::remove_file(pid_file);
    }
}

#[cfg(unix)]
fn runs_old_build(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).contains(&*build_dir().to_string_lossy())
}

fn remove_data(paths: &GxserverPaths) {
    stop_leftovers(paths);
    let _ = fs::remove_dir_all(data_dir(paths));
    let _ = fs::remove_dir_all(keys_dir(paths));
    if let Ok(entries) = fs::read_dir(connectors_dir(paths)) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("executor-mcp-tools-") && name.ends_with(".json") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_file(marker(paths));
}

fn write_marker(paths: &GxserverPaths, found_at: u64, apps: &[String]) {
    let text = json!({ "foundAt": found_at, "apps": apps }).to_string();
    let path = marker(paths);
    if fs::write(&path, text).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

/// Checks the old Executor's leftovers once per read: removes the old build, lists the old apps
/// the first time, and deletes the data once 30 days passed. Returns the apps and when they go,
/// or `null` when nothing is left.
pub(super) fn status(paths: &GxserverPaths) -> Value {
    if build_dir().exists() {
        stop_leftovers(paths);
        let _ = fs::remove_dir_all(build_dir());
    }
    if !data_dir(paths).exists() {
        let _ = fs::remove_file(marker(paths));
        return Value::Null;
    }
    let saved: Option<Value> = fs::read_to_string(marker(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let (found_at, apps) = match saved {
        Some(saved) => (
            saved
                .get("foundAt")
                .and_then(Value::as_u64)
                .unwrap_or_else(now),
            saved
                .get("apps")
                .and_then(Value::as_array)
                .map(|apps| {
                    apps.iter()
                        .filter_map(|app| app.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        ),
        None => {
            let apps = read_app_names(paths);
            let found_at = now();
            write_marker(paths, found_at, &apps);
            (found_at, apps)
        }
    };
    let delete_at = found_at + KEEP_FOR.as_secs();
    if apps.is_empty() || now() >= delete_at {
        remove_data(paths);
        return Value::Null;
    }
    json!({ "apps": apps, "deleteAt": delete_at })
}

/// Remove: deletes the old data now.
pub(crate) fn remove(paths: &GxserverPaths) {
    remove_data(paths);
    super::status::notify_changed();
}
