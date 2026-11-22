//! What the Connectors routes do with a workspace's Executor: turn it on (downloading it the first
//! time) or off, report its state without waiting on it, and manage its apps and tools. All
//! blocking: Executor may need to start first.

use std::time::Duration;

use serde_json::{json, Value};

use crate::managed_tools::jobs;
use crate::paths::GxserverPaths;

use super::apps;
use super::client::Endpoint;
use super::connections::{self, ExecutorConnectStart};
use super::live::{notify_changed, with_live};
use super::process;
use super::release;
use super::settings;
use super::tools;

/// The install job's key among Ghostex's tool installs.
const INSTALL_JOB: &str = "executor";
/// How long a failed start keeps a read from trying again on its own.
const RETRY_START_AFTER: Duration = Duration::from_secs(60);
const CATALOG_RESULTS: usize = 25;

/// The workspace's Executor as Settings shows it. Never waits on Executor: a read of an enabled,
/// installed Executor whose apps this gxserver hasn't read yet starts it in the background; once
/// read, the list stays while an idle Executor is stopped.
pub(crate) fn status(paths: &GxserverPaths, workspace: &str) -> Value {
    let saved = settings::load(paths, workspace);
    let enabled = saved
        .as_ref()
        .ok()
        .and_then(|settings| settings.as_ref())
        .is_some_and(|settings| settings.enabled);
    let installed = release::installed();
    let running = process::running(paths, workspace);
    if enabled && installed && !running {
        let retry_due = with_live(paths, workspace, |live| {
            !live.apps_read
                && !live.starting
                && live
                    .start_error
                    .as_ref()
                    .is_none_or(|(_, at)| at.elapsed() >= RETRY_START_AFTER)
        });
        if retry_due {
            start_in_background(paths, workspace);
        }
    }
    let install = jobs::get(INSTALL_JOB).map(|job| job.view());
    let (apps, starting, error) = with_live(paths, workspace, |live| {
        (
            live.apps.clone(),
            live.starting,
            live.start_error.as_ref().map(|(error, _)| error.clone()),
        )
    });
    // Where an app's own OAuth client must send the browser back (Google's, for example).
    let callback_url = saved
        .as_ref()
        .ok()
        .and_then(|settings| settings.as_ref())
        .map(|settings| format!("http://127.0.0.1:{}/api/oauth/callback", settings.port));
    let unreadable = saved.is_err() && settings::unreadable(paths, workspace);
    json!({
        "enabled": enabled,
        "unreadable": unreadable,
        "version": release::VERSION,
        "installed": installed,
        "install": install,
        "running": running,
        "starting": starting,
        "error": error.or_else(|| saved.err()).or_else(|| {
            (enabled && !installed && jobs::get(INSTALL_JOB).is_none_or(|job| !job.active()))
                .then(|| "Executor isn't installed on this computer. Turn it on again to download it.".to_string())
        }),
        "apps": if enabled { apps } else { Vec::new() },
        "oauthCallbackUrl": callback_url,
    })
}

/// Reads the workspace's app list again into the live state.
pub(super) fn refresh_apps(paths: &GxserverPaths, workspace: &str, endpoint: &Endpoint) {
    if let Ok(list) = apps::list(endpoint) {
        with_live(paths, workspace, |live| {
            live.apps = list;
            live.apps_read = true;
        });
    }
}

/// The running Executor (started first if needed), with its app list read once so Settings
/// shows it after a start that a route, not a read, caused.
fn running_endpoint(paths: &GxserverPaths, workspace: &str) -> Result<Endpoint, String> {
    let endpoint = process::ensure_running(paths, workspace)?;
    if !with_live(paths, workspace, |live| live.apps_read) {
        refresh_apps(paths, workspace, &endpoint);
        notify_changed(workspace);
    }
    Ok(endpoint)
}

/// Starts the workspace's Executor on a background thread and reads its apps.
fn start_in_background(paths: &GxserverPaths, workspace: &str) {
    let start = with_live(paths, workspace, |live| {
        !std::mem::replace(&mut live.starting, true)
    });
    if !start {
        return;
    }
    let paths = paths.clone();
    let workspace = workspace.to_string();
    std::thread::spawn(move || {
        let result = process::ensure_running(&paths, &workspace);
        if let Ok(endpoint) = &result {
            refresh_apps(&paths, &workspace, endpoint);
        }
        with_live(&paths, &workspace, |live| {
            live.starting = false;
            live.start_error = result.err().map(|error| (error, std::time::Instant::now()));
        });
        notify_changed(&workspace);
    });
}

/// Downloads Executor as one of Ghostex's tool installs, then starts the workspace's.
fn start_install(paths: &GxserverPaths, workspace: &str) {
    if jobs::begin(INSTALL_JOB, "install").is_err() {
        // Already downloading; whoever started it starts their workspace, and reads start the rest.
        return;
    }
    let paths = paths.clone();
    let workspace = workspace.to_string();
    std::thread::spawn(move || {
        let _turn = jobs::INSTALL_LOCK.blocking_lock();
        jobs::set_status(INSTALL_JOB, "running");
        let log = jobs::log_for(INSTALL_JOB);
        let result = release::install(&log);
        jobs::finish(INSTALL_JOB, &result);
        if result.is_ok() {
            start_in_background(&paths, &workspace);
        }
        notify_changed(&workspace);
    });
}

/// Turns the workspace's Executor on, downloading Executor first if this computer has none.
pub(crate) fn enable(paths: &GxserverPaths, workspace: &str) -> Result<Value, String> {
    let mut settings = settings::load_or_create(paths, workspace)?;
    if !settings.enabled {
        settings.enabled = true;
        settings::save(paths, workspace, &settings)?;
    }
    with_live(paths, workspace, |live| live.start_error = None);
    if release::installed() {
        start_in_background(paths, workspace);
    } else {
        start_install(paths, workspace);
    }
    notify_changed(workspace);
    Ok(status(paths, workspace))
}

/// Turns it off: stops the process and keeps its data, so turning it on again restores its apps
/// and sign-ins.
pub(crate) fn disable(paths: &GxserverPaths, workspace: &str) -> Result<Value, String> {
    if let Some(mut settings) = settings::load(paths, workspace)? {
        settings.enabled = false;
        settings::save(paths, workspace, &settings)?;
    }
    process::stop(paths, workspace);
    with_live(paths, workspace, |live| {
        live.apps.clear();
        live.apps_read = false;
        live.start_error = None;
    });
    notify_changed(workspace);
    Ok(status(paths, workspace))
}

pub(crate) fn search_catalog(
    paths: &GxserverPaths,
    workspace: &str,
    query: &str,
) -> Result<Vec<Value>, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    apps::search_catalog(&endpoint, query, CATALOG_RESULTS)
}

fn added(paths: &GxserverPaths, workspace: &str, endpoint: &Endpoint, app: &Value) -> Value {
    refresh_apps(paths, workspace, endpoint);
    notify_changed(workspace);
    let id = app.get("id").and_then(Value::as_str).unwrap_or_default();
    let listed = with_live(paths, workspace, |live| {
        live.apps
            .iter()
            .find(|listed| listed.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    });
    listed.unwrap_or_else(|| {
        json!({
            "id": id,
            "slug": app.get("slug").cloned().unwrap_or(Value::Null),
            "name": app.get("name").cloned().unwrap_or(Value::Null),
        })
    })
}

pub(crate) fn add_catalog_app(
    paths: &GxserverPaths,
    workspace: &str,
    reference: &str,
) -> Result<Value, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    let app = apps::add_from_catalog(&endpoint, reference)?;
    Ok(added(paths, workspace, &endpoint, &app))
}

pub(crate) fn add_mcp_server(
    paths: &GxserverPaths,
    workspace: &str,
    name: &str,
    url: &str,
) -> Result<Value, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    let app = apps::add_mcp_server(&endpoint, name, url)?;
    Ok(added(paths, workspace, &endpoint, &app))
}

pub(crate) fn remove_app(
    paths: &GxserverPaths,
    workspace: &str,
    reference: &str,
) -> Result<String, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    let app = apps::find(&endpoint, reference)?;
    apps::remove(&endpoint, &app)?;
    refresh_apps(paths, workspace, &endpoint);
    notify_changed(workspace);
    Ok(apps::app_name(&app).to_string())
}

pub(crate) fn connect_app(
    paths: &GxserverPaths,
    workspace: &str,
    reference: &str,
) -> Result<ExecutorConnectStart, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    connections::start(paths, workspace, &endpoint, reference)
}

/// One app's tools, or every app's (`{ app, name, tools }` each).
pub(crate) fn list_tools(
    paths: &GxserverPaths,
    workspace: &str,
    reference: Option<&str>,
) -> Result<Value, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    let targets = match reference.map(str::trim).filter(|text| !text.is_empty()) {
        Some(reference) => vec![apps::find(&endpoint, reference)?],
        None => apps::all(&endpoint)?,
    };
    let mut listed = Vec::new();
    for app in &targets {
        let tools = match tools::list(&endpoint, app) {
            Ok(tools) => json!(tools),
            Err(error) => json!({ "error": error }),
        };
        listed.push(json!({
            "app": apps::app_id(app),
            "name": apps::app_name(app),
            "tools": tools,
        }));
    }
    Ok(Value::Array(listed))
}

pub(crate) fn call_tool(
    paths: &GxserverPaths,
    workspace: &str,
    reference: &str,
    tool: &str,
    input: &Value,
) -> Result<String, String> {
    let endpoint = running_endpoint(paths, workspace)?;
    let app = apps::find(&endpoint, reference)?;
    tools::call_read_only(&endpoint, &app, tool, input)
}
