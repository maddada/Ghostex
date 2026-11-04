//! `POST /api/managedTools`: the tools Ghostex installs for the user, their state and their jobs.

use serde_json::{json, Map, Value};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Duration, Instant},
};

use super::{
    binaries, desktop_control,
    flight::Flights,
    homebrew, jobs, node, paths, powershell, system_tools,
    tools::{self, Status, ToolId},
    uv,
};
use crate::{domain::DomainStateError, paths::GxserverPaths};

fn error(message: impl ToString) -> DomainStateError {
    DomainStateError::bad_request(message.to_string())
}

pub(crate) async fn dispatch(
    gx_paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let home = gx_paths.agent_config_home_dir().to_path_buf();
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("read");
    let fresh = params.get("fresh").and_then(Value::as_bool) == Some(true);
    if action == "list" {
        // Fast Computer & Browser Use and SpaceO only for a client that draws them (desktop_control.rs).
        let desktop_control =
            params.get("includeDesktopControl").and_then(Value::as_bool) == Some(true);
        let tools = tokio::task::spawn_blocking(move || {
            // Every tool is checked at once, and the list answers without one that is stuck.
            let checks: Vec<_> = ToolId::ALL
                .into_iter()
                .filter(|tool| desktop_control || !desktop_control::is_desktop_control(*tool))
                .map(|tool| (tool, checked_state(tool, &home, fresh)))
                .collect();
            let deadline = Instant::now() + STATE_WAIT;
            checks
                .into_iter()
                .map(|(tool, check)| {
                    check
                        .wait_until(deadline)
                        .unwrap_or_else(|| still_checking(tool))
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(error)?;
        return Ok(json!({ "tools": tools }));
    }
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::parse)
        .ok_or_else(|| error("Unknown tool."))?;
    match action {
        "read" => read(tool, &home, fresh).await,
        "start" => {
            let operation = params
                .get("operation")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Choose install, update, reinstall or uninstall."))?
                .to_string();
            start(tool, &operation, &home, gx_paths.clone()).await
        }
        _ => Err(error("Unknown tool action.")),
    }
}

async fn read(tool: ToolId, home: &Path, fresh: bool) -> Result<Value, DomainStateError> {
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        checked_state(tool, &home, fresh)
            .wait_until(Instant::now() + STATE_WAIT)
            .unwrap_or_else(|| still_checking(tool))
    })
    .await
    .map_err(error)
}

/// How long a list or read waits for one tool's check. Its version check stops at
/// `run::PROBE_TIMEOUT`; the rest is the latest-release lookup, which can be slow on a bad network.
const STATE_WAIT: Duration = Duration::from_secs(12);

static CHECKS: LazyLock<Flights<ToolId, Value>> = LazyLock::new(Flights::new);

/// The check of `tool` running now, or a new one: a tool is never checked twice at once.
fn checked_state(
    tool: ToolId,
    home: &Path,
    fresh: bool,
) -> std::sync::Arc<super::flight::Flight<Value>> {
    let home = home.to_path_buf();
    CHECKS.join(tool, move || state(tool, &home, fresh))
}

/// A tool whose check did not finish in `STATE_WAIT`: shown as not answering, with its job, while
/// the check goes on (a Retry joins it).
///
/// CDXC:ManagedTools 2026-10-27 WHY:
/// GitHub #336: one tool's endless version check kept the whole Tools list on "Tool status is being
/// checked." The list shows every tool that answered and marks this one, so the rest of the page
/// works and Retry asks again.
pub(super) fn still_checking(tool: ToolId) -> Value {
    let mut value = json!({
        "id": tool.id(),
        "label": tool.label(),
        "description": tool.description(),
        "supported": true,
        "installed": false,
        "installPlan": "",
        "needsPassword": false,
        "actions": [],
        "notAnswering": format!(
            "Checking {} took longer than {} seconds.",
            tool.label(),
            STATE_WAIT.as_secs()
        ),
    });
    if let Some(job) = jobs::get(tool.id()) {
        value["job"] = job.view();
    }
    value
}

/// The status, and for an installed desktop-control tool the grants it still lacks.
fn status_and_grants(tool: ToolId, home: &Path) -> (Status, Option<desktop_control::Grants>) {
    let status = match tool {
        ToolId::Trycua | ToolId::Spaceo => return desktop_control::status(tool, home),
        ToolId::Node => node::status(home),
        ToolId::Uv => uv::status(home),
        ToolId::Homebrew => homebrew::homebrew_status(home),
        ToolId::SystemTools => system_tools::status(home),
        ToolId::PowerShell => powershell::status(),
        ToolId::Beads | ToolId::Gh | ToolId::Glab => binaries::status(tool, home),
    };
    (status, None)
}

/// The wire view of one tool (`ManagedToolState` in packages/shared/managed-tools.ts (deleted 2026-10-01)).
pub(crate) fn state(tool: ToolId, home: &Path, fresh: bool) -> Value {
    let job = jobs::get(tool.id());
    let active = job.as_ref().is_some_and(jobs::Job::active);
    let (status, grants) = status_and_grants(tool, home);
    let mut value = json!({
        "id": tool.id(),
        "label": tool.label(),
        "description": tool.description(),
        "supported": status.supported.is_ok(),
        "installed": status.executable.is_some() && !status.off_path,
        "installPlan": status.plan,
        "needsPassword": status.needs_password,
        "actions": [],
    });
    if let Err(reason) = &status.supported {
        value["unsupportedReason"] = json!(reason);
    }
    if let Some(path) = &status.executable {
        value["executablePath"] = json!(path);
    }
    if let Some(source) = status.source {
        value["source"] = json!(source.id());
    }
    if let Some(version) = &status.version {
        value["version"] = json!(version);
    }
    if let Some(detail) = &status.detail {
        value["detail"] = json!(detail);
    }
    if let Some(reason) = &status.not_answering {
        value["notAnswering"] = json!(reason);
    }
    if let Some(reason) = &status.install_blocker {
        value["unavailableReason"] = json!(reason);
    }
    if let Some(url) = &status.download_url {
        value["downloadUrl"] = json!(url);
    }
    if let Some(command) = &status.terminal_command {
        value["terminalCommand"] = json!(command);
    }
    if let Some(note) = grants.as_ref().and_then(desktop_control::permission_note) {
        value["permissionNote"] = json!(note);
    }
    let actions: Vec<&str> = status
        .operations
        .iter()
        .copied()
        .filter(|operation| *operation != "install" || status.install_blocker.is_none())
        .collect();
    value["actions"] = json!(actions);
    if !active && actions.contains(&"update") {
        match tools::latest_version(tool, fresh) {
            Some(Ok(latest)) => {
                if let Some(current) = &status.version {
                    value["updateAvailable"] = json!(tools::newer(&latest, current));
                }
                value["latestVersion"] = json!(latest);
            }
            Some(Err(reason)) => value["checkError"] = json!(reason),
            None => {}
        }
    }
    if let Some(job) = job {
        value["job"] = job.view();
    }
    value
}

async fn start(
    tool: ToolId,
    operation: &str,
    home: &Path,
    gx_paths: GxserverPaths,
) -> Result<Value, DomainStateError> {
    let current = read(tool, home, false).await?;
    let allowed = current["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == operation));
    if !allowed {
        return Err(error(current["unavailableReason"].as_str().unwrap_or(
            "That isn't available for this tool right now. Refresh and try again.",
        )));
    }
    if operation == "install" {
        if let Some(command) = current["terminalCommand"].as_str() {
            return Err(error(format!("Run this in a terminal: {command}")));
        }
    }
    jobs::begin(tool.id(), operation).map_err(error)?;
    let owned_home = home.to_path_buf();
    let operation = operation.to_string();
    tokio::spawn(async move {
        let _turn = jobs::INSTALL_LOCK.lock().await;
        jobs::set_status(tool.id(), "running");
        let log = jobs::log_for(tool.id());
        let job_log = log.clone();
        let uninstall = operation == "uninstall";
        let mut result = tokio::task::spawn_blocking(move || {
            run_operation(tool, &operation, &owned_home, &job_log)
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        // A desktop-control tool brings its Ghostex skill, as this computer's own rows do.
        if result.is_ok() && !uninstall && desktop_control::is_desktop_control(tool) {
            let skill = desktop_control::ghostex_skill(tool);
            result = crate::machine_integrations::install_bundled_skill(&gx_paths, skill)
                .await
                .map(|()| log.line(&format!("Installed the {skill} skill.")))
                .map_err(|error| {
                    format!("Installed, but the {skill} skill could not be installed: {error}")
                });
        }
        jobs::finish(tool.id(), &result);
    });
    read(tool, home, false).await
}

fn run_operation(tool: ToolId, operation: &str, home: &Path, log: &Log) -> Result<(), String> {
    clean_scratch();
    let installing = operation != "uninstall";
    match (tool, installing) {
        (ToolId::Node, true) => {
            node::install(log)?;
            add_to_path(&paths::node_bin_dir(), home, log);
        }
        (ToolId::Node, false) => node::uninstall(log)?,
        (ToolId::Uv, true) => {
            uv::install(log)?;
            add_to_path(&paths::uv_dir(), home, log);
        }
        (ToolId::Uv, false) => uv::uninstall(log)?,
        (ToolId::Beads | ToolId::Gh | ToolId::Glab, true) => {
            binaries::install(tool, log)?;
            if let Some(message) = binaries::expose(tool, home) {
                log.line(&message);
            }
        }
        (ToolId::Beads | ToolId::Gh | ToolId::Glab, false) => binaries::uninstall(tool, home, log)?,
        (ToolId::Homebrew, true) if operation == "update" => homebrew::update_homebrew(home, log)?,
        (ToolId::Homebrew, true) => {
            homebrew::install_homebrew(home, log)?;
        }
        (ToolId::Homebrew, false) => return Err("Ghostex does not uninstall Homebrew.".into()),
        (ToolId::SystemTools, true) => system_tools::install(home, log)?,
        (ToolId::SystemTools, false) => return Err("Ghostex does not remove system tools.".into()),
        (ToolId::PowerShell, true) => powershell::install(log)?,
        (ToolId::PowerShell, false) => return Err("Ghostex does not remove PowerShell 7.".into()),
        (ToolId::Trycua | ToolId::Spaceo, _) => {
            desktop_control::run_operation(tool, operation, home, log)?
        }
    }
    crate::agent_hooks::probing::refresh_cli_environment(home);
    Ok(())
}

use super::jobs::Log;

fn add_to_path(dir: &Path, home: &Path, log: &Log) {
    match crate::agent_cli::path_setup::ensure_on_path_end(dir, home) {
        Ok(Some(message)) | Err(message) => log.line(&message),
        Ok(None) => {}
    }
}

/// Leftover download folders from a job that was interrupted (a crash or a restart mid-download,
/// which for Homebrew may include its password helper). Jobs run one at a time, so none is in use.
fn clean_scratch() {
    let Ok(entries) = std::fs::read_dir(paths::tools_root()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".download-")
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// CDXC:ManagedTools 2026-09-29 DECISION:
/// User (2A): an npm install uses the user's own npm (nvm, Homebrew, mise…) when there is one; Ghostex installs its own Node.js only when none is found. Returns the folders to put first on the job's PATH.
pub(crate) async fn ensure_npm(home: &Path, log: &Log) -> Result<Vec<PathBuf>, String> {
    let owned_home = home.to_path_buf();
    let log = log.clone();
    tokio::task::spawn_blocking(move || {
        if let Some((npm, _)) = tools::locate("npm", &owned_home) {
            return Ok(npm.parent().map(Path::to_path_buf).into_iter().collect());
        }
        log.line("npm isn't installed, so Ghostex installs Node.js first.");
        clean_scratch();
        node::install(&log)?;
        add_to_path(&paths::node_bin_dir(), &owned_home, &log);
        Ok(vec![paths::node_bin_dir()])
    })
    .await
    .map_err(|error| error.to_string())?
}

/// uv to run: the user's own when installed, otherwise Ghostex's (installed now if needed).
pub(crate) async fn ensure_uv(home: &Path, log: &Log) -> Result<PathBuf, String> {
    let owned_home = home.to_path_buf();
    let log = log.clone();
    tokio::task::spawn_blocking(move || {
        if let Some((path, _)) = tools::locate("uv", &owned_home) {
            return Ok(path);
        }
        if let Some(path) = uv::managed_executable() {
            return Ok(path);
        }
        log.line("uv isn't installed, so Ghostex installs it first.");
        clean_scratch();
        uv::install(&log)?;
        add_to_path(&paths::uv_dir(), &owned_home, &log);
        uv::managed_executable().ok_or_else(|| "uv did not install.".to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// mise to run: the user's own when installed, otherwise Ghostex's (installed now if needed). Returns
/// the folder to put first on the job's PATH.
pub(crate) async fn ensure_mise(home: &Path, log: &Log) -> Result<Vec<PathBuf>, String> {
    let owned_home = home.to_path_buf();
    let log = log.clone();
    tokio::task::spawn_blocking(move || {
        let path = match tools::locate("mise", &owned_home)
            .map(|(path, _)| path)
            .or_else(super::mise::managed_executable)
        {
            Some(path) => path,
            None => {
                log.line("mise isn't installed, so Ghostex installs it first.");
                clean_scratch();
                super::mise::install(&log)?
            }
        };
        Ok(path.parent().map(Path::to_path_buf).into_iter().collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(crate) async fn ensure_homebrew(home: &Path, log: &Log) -> Result<PathBuf, String> {
    let owned_home = home.to_path_buf();
    let log = log.clone();
    tokio::task::spawn_blocking(move || homebrew::install_homebrew(&owned_home, &log))
        .await
        .map_err(|error| error.to_string())?
}

pub(crate) async fn ensure_system_tools(
    home: &Path,
    wanted: &'static [&'static str],
    log: &Log,
) -> Result<(), String> {
    let owned_home = home.to_path_buf();
    let log = log.clone();
    tokio::task::spawn_blocking(move || {
        if system_tools::missing(&owned_home, wanted).is_empty() {
            return Ok(());
        }
        system_tools::install(&owned_home, &log)
    })
    .await
    .map_err(|error| error.to_string())?
}
