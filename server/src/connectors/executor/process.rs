//! Running a workspace's Executor: one `executor serve` per workspace that turned it on, with its
//! own data folder, port and keys, started when something needs it and stopped when the
//! workspace turns it off or gxserver exits.

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::paths::GxserverPaths;

use super::client::{self, Endpoint};
use super::live::{lock, notify_changed, take_all_children, take_idle, with_live};
use super::release;
use super::settings::{self, WorkspaceExecutor};

const READY_WITHIN: Duration = Duration::from_secs(90);
const STOP_WITHIN: Duration = Duration::from_secs(5);
const IDLE_STOP_AFTER: Duration = Duration::from_secs(15 * 60);
const IDLE_CHECK_EVERY: Duration = Duration::from_secs(60);
const PID_FILE: &str = "ghostex-executor.pid";
const LOG_FILE: &str = "ghostex-executor.log";

/// Serializes starts, so two requests never start the same workspace's Executor twice.
static START_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn endpoint_of(settings: &WorkspaceExecutor) -> Endpoint {
    Endpoint {
        port: settings.port,
        api_key: settings.api_key.clone(),
    }
}

/// True while this gxserver's Executor for the workspace is alive.
pub(super) fn running(paths: &GxserverPaths, workspace: &str) -> bool {
    with_live(paths, workspace, |live| match live.child.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => true,
            _ => {
                live.child = None;
                false
            }
        },
        None => false,
    })
}

/// The workspace's Executor, started first when it isn't running. Blocks up to 90 seconds.
pub(super) fn ensure_running(paths: &GxserverPaths, workspace: &str) -> Result<Endpoint, String> {
    // Marked used before the running check, so the idle stop can't take it away in between.
    with_live(paths, workspace, |live| {
        live.workspace = workspace.to_string();
        live.last_used = Some(Instant::now());
    });
    let settings = settings::load(paths, workspace)?
        .filter(|settings| settings.enabled)
        .ok_or_else(|| {
            "Executor is off in this workspace. Turn it on with `ghostex connectors executor enable`."
                .to_string()
        })?;
    if !release::installed() {
        return Err(
            "Executor isn't installed yet. Turn it on with `ghostex connectors executor enable`, which downloads it."
                .to_string(),
        );
    }
    let endpoint = endpoint_of(&settings);
    if running(paths, workspace) {
        return Ok(endpoint);
    }
    let _start = lock(&START_LOCK);
    if running(paths, workspace) {
        return Ok(endpoint);
    }
    let child = start(paths, workspace, &settings, &endpoint)?;
    with_live(paths, workspace, |live| {
        live.child = Some(child);
        live.start_error = None;
    });
    Ok(endpoint)
}

/// The variables Executor reads, on top of gxserver's own environment minus anything that would
/// change where it stores data, which keys it uses or where it sends telemetry.
fn environment(command: &mut std::process::Command, settings: &WorkspaceExecutor, data_dir: &Path) {
    for (name, _) in std::env::vars_os() {
        let name_text = name.to_string_lossy();
        if name_text.starts_with("EXECUTOR_") || name_text.starts_with("OTEL_") {
            command.env_remove(&name);
        }
    }
    command
        .env("EXECUTOR_DATA_DIR", data_dir)
        .env("EXECUTOR_PORT", settings.port.to_string())
        .env("EXECUTOR_API_KEY", &settings.api_key)
        .env("EXECUTOR_ENCRYPTION_KEY", &settings.encryption_key)
        .env("DO_NOT_TRACK", "1")
        .env("EXECUTOR_DISABLE_ANALYTICS", "1")
        .env("EXECUTOR_NO_UPDATE_CHECK", "1");
}

/// Starts `executor serve` and waits until its API answers with the workspace's key.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// `executor serve` prints a one-use dashboard sign-in link on stdout, so stdout goes nowhere and only stderr (its startup log) is kept, in the workspace's data folder. The keys are passed in the environment, which Executor never writes to disk or the keychain.
fn start(
    paths: &GxserverPaths,
    workspace: &str,
    settings: &WorkspaceExecutor,
    endpoint: &Endpoint,
) -> Result<Child, String> {
    let data_dir = settings::data_dir(paths, workspace);
    create_private_dir(&data_dir)?;
    stop_leftover(&data_dir);
    discard_previous_traces(&data_dir);
    if TcpListener::bind(("127.0.0.1", settings.port)).is_err() {
        return Err(format!(
            "Port {} is taken by another program, and this workspace's Executor needs it (its app sign-ins come back there). Close that program and try again.",
            settings.port
        ));
    }
    let log = fs::File::create(data_dir.join(LOG_FILE))
        .map_err(|error| format!("Couldn't write Executor's log: {error}"))?;
    let mut command = crate::platform::process::background_command(release::executable());
    command
        .arg("serve")
        .current_dir(&data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    environment(&mut command, settings, &data_dir);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Couldn't start Executor: {error}"))?;
    let _ = fs::write(data_dir.join(PID_FILE), child.id().to_string());
    let deadline = Instant::now() + READY_WITHIN;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let _ = fs::remove_file(data_dir.join(PID_FILE));
            return Err(format!(
                "Executor stopped while starting ({status}). {}",
                log_tail(&data_dir)
            ));
        }
        if client::ready(endpoint) {
            return Ok(child);
        }
        if Instant::now() >= deadline {
            terminate(&mut child);
            let _ = fs::remove_file(data_dir.join(PID_FILE));
            return Err(format!(
                "Executor didn't answer within {} seconds. {}",
                READY_WITHIN.as_secs(),
                log_tail(&data_dir)
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: delete Executor's trace database and stop it from piling up. Executor (2.0.0-beta.22) always records its own traces and logs into `diagnostics/motel`, with no setting to turn that off and no size limit (657 MB after five days), and Ghostex never reads it. Each start clears the last run's database while Executor is stopped, so only the current run's traces exist.
fn discard_previous_traces(data_dir: &Path) {
    let _ = fs::remove_dir_all(data_dir.join("diagnostics").join("motel"));
}

fn create_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Couldn't create {}: {error}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// The last startup messages Executor logged (its log lines are JSON with a `message`).
fn log_tail(data_dir: &Path) -> String {
    let text = fs::read_to_string(data_dir.join(LOG_FILE)).unwrap_or_default();
    let messages = text
        .lines()
        .rev()
        .take(5)
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| line.to_string())
        })
        .filter(|message| !message.trim().is_empty())
        .collect::<Vec<_>>();
    if messages.is_empty() {
        return String::new();
    }
    let mut messages = messages;
    messages.reverse();
    format!("Its log ends with: {}", messages.join(" | "))
}

/// Stops an Executor a previous gxserver left running for this folder (one that exited without
/// stopping it), so the folder and port are free again.
fn stop_leftover(data_dir: &Path) {
    let pid_file = data_dir.join(PID_FILE);
    let Some(pid) = fs::read_to_string(&pid_file)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return;
    };
    #[cfg(not(unix))]
    let _ = pid;
    #[cfg(unix)]
    if is_our_executor(pid) {
        let pid = pid as libc::pid_t;
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let deadline = Instant::now() + STOP_WITHIN;
        while Instant::now() < deadline && unsafe { libc::kill(pid, 0) } == 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        if unsafe { libc::kill(pid, 0) } == 0 {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
    let _ = fs::remove_file(pid_file);
}

/// True when `pid` is alive and runs Ghostex's Executor program.
#[cfg(unix)]
fn is_our_executor(pid: u32) -> bool {
    if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .output()
    else {
        return false;
    };
    let command = String::from_utf8_lossy(&output.stdout);
    command.contains(&*release::executable().to_string_lossy())
}

/// Asks the process to stop (it stops its own helpers), then kills it after 5 seconds.
fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let deadline = Instant::now() + STOP_WITHIN;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Stops the workspace's Executor if it runs.
pub(super) fn stop(paths: &GxserverPaths, workspace: &str) {
    let child = with_live(paths, workspace, |live| live.child.take());
    if let Some(mut child) = child {
        terminate(&mut child);
    }
    let _ = fs::remove_file(settings::data_dir(paths, workspace).join(PID_FILE));
}

/// Stops each workspace's Executor once nothing used it for 15 minutes; the next use starts it
/// again. Runs for the gxserver's lifetime.
///
/// CDXC:Connectors 2026-10-15 DECISION:
/// User: stop a workspace's Executor after 15 minutes unused and start it again on the next use, since each running one uses about 580 MB; the first call after a pause waits a few seconds while it starts.
pub(crate) fn start_idle_stop() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(IDLE_CHECK_EVERY);
            for (data_dir, workspace, mut child) in take_idle(IDLE_STOP_AFTER) {
                terminate(&mut child);
                let _ = fs::remove_file(Path::new(&data_dir).join(PID_FILE));
                if !workspace.is_empty() {
                    notify_changed(&workspace);
                }
            }
        });
    });
}

/// Stops every Executor this gxserver started (when it exits), all at once.
pub(crate) fn stop_all() {
    let mut children = take_all_children();
    #[cfg(unix)]
    for child in &children {
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline
        && children
            .iter_mut()
            .any(|child| matches!(child.try_wait(), Ok(None)))
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    for child in &mut children {
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
