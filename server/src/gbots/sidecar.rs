//! Starts, supervises and stops the GBots sidecar (the runtime's `serve`) on the computer the bots
//! run on.
//!
//! CDXC:GBots 2026-11-06 WHY:
//! The sidecar runs in its own process group so stopping it also stops the `claude` and `codex`
//! turns it started; its pid is recorded beside its data, so a gxserver that died without stopping
//! it ends the leftover before starting a new one instead of leaving two servers on one data folder.
//! A sidecar that exits is restarted with a growing wait (5s, doubling to 60s), because a bad
//! runtime that dies at once would otherwise restart in a tight loop.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::server::AppState;

/// How often the supervisor looks at the settings and the process.
const TICK: Duration = Duration::from_secs(3);
/// How long a start may take before it counts as failed (the server takes ~15s cold).
const START_DEADLINE: Duration = Duration::from_secs(120);
const FIRST_RETRY: Duration = Duration::from_secs(5);
const LAST_RETRY: Duration = Duration::from_secs(60);

/// What the sidecar is doing, as `ghostex gbots status` and the Bots space show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SidecarStatus {
    /// GBots is off here, or runs on another computer.
    Off,
    /// GBots is on but its runtime is not installed on this computer.
    NotInstalled,
    Starting,
    Running,
    /// It could not start or it stopped; the text says why. It is retried.
    Failed(String),
}

impl SidecarStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::NotInstalled => "notInstalled",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed(_) => "failed",
        }
    }
}

struct Sidecar {
    child: Option<Child>,
    port: Option<u16>,
    status: SidecarStatus,
    started_at: Option<Instant>,
    retry_at: Option<Instant>,
    retry_wait: Duration,
}

static SIDECAR: Mutex<Sidecar> = Mutex::new(Sidecar {
    child: None,
    port: None,
    status: SidecarStatus::Off,
    started_at: None,
    retry_at: None,
    retry_wait: FIRST_RETRY,
});

/// Bumped on every start and stop, so an event reader for an older process stops.
pub(crate) static GENERATION: AtomicU64 = AtomicU64::new(0);

fn sidecar() -> std::sync::MutexGuard<'static, Sidecar> {
    SIDECAR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The sidecar's port while it runs and answers.
pub(crate) fn running_port() -> Option<u16> {
    let sidecar = sidecar();
    (sidecar.status == SidecarStatus::Running)
        .then_some(sidecar.port)
        .flatten()
}

/// The status, its reason when it failed, and the port.
pub(crate) fn status_value() -> Value {
    let sidecar = sidecar();
    json!({
        "status": sidecar.status.as_str(),
        "error": match &sidecar.status { SidecarStatus::Failed(reason) => Some(reason.clone()), _ => None },
        "port": sidecar.port,
        "runtimeVersion": super::runtime::GBOTS_RUNTIME_VERSION,
        "runtimeDir": super::runtime::install_dir(),
        "dataDir": data_dir(),
    })
}

pub(crate) fn is_running() -> bool {
    running_port().is_some()
}

/// GBots' own folder in gxserver's state: the sidecar's data, its pid record and its log.
pub(crate) fn state_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("gbots")
}

pub(crate) fn data_dir() -> PathBuf {
    state_dir().join("data")
}

fn pid_file() -> PathBuf {
    state_dir().join("sidecar.json")
}

static RUNTIME: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();

/// Enters gxserver's runtime on a GBots thread, for the work that schedules on it.
pub(crate) fn enter_runtime() -> Option<tokio::runtime::EnterGuard<'static>> {
    RUNTIME.get().map(tokio::runtime::Handle::enter)
}

/// Set once gxserver starts stopping: the sidecar is stopped and never started again.
static SHUTTING_DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts the supervisor thread, which runs for the life of gxserver, and stops the sidecar (and
/// the agent turns it started) the moment gxserver's stop is broadcast, whichever way it stops.
pub(crate) fn start_gbots_supervisor(state: Arc<AppState>) {
    end_leftover_sidecar();
    // Publishing a changed row schedules work on gxserver's runtime, so GBots' own threads enter it.
    let _ = RUNTIME.set(tokio::runtime::Handle::current());
    let mut shutdown = state.shutdown_tx.subscribe();
    let _ = std::thread::Builder::new()
        .name("gbots-shutdown".to_string())
        .spawn(move || {
            let _ = shutdown.blocking_recv();
            SHUTTING_DOWN.store(true, Ordering::SeqCst);
            stop_sidecar_now();
        });
    let _ = std::thread::Builder::new()
        .name("gbots-supervisor".to_string())
        .spawn(move || loop {
            let _runtime = enter_runtime();
            if SHUTTING_DOWN.load(Ordering::SeqCst) {
                return;
            }
            let changed = supervise_once(&state);
            if changed {
                super::projects::on_sidecar_status_changed(&state);
            }
            std::thread::sleep(TICK);
        });
}

/// One look: start, restart or stop as the settings and the process say. True when the status
/// changed.
fn supervise_once(state: &AppState) -> bool {
    let wanted = super::settings::runs_here(&state.paths, &state.metadata.server_id);
    let before = sidecar().status.clone();
    if !wanted {
        stop_sidecar_now();
        sidecar().status = SidecarStatus::Off;
        return sidecar().status != before;
    }
    if super::runtime::installed_runtime().is_none() {
        stop_sidecar_now();
        sidecar().status = SidecarStatus::NotInstalled;
        return sidecar().status != before;
    }
    let mut start_now = false;
    {
        let mut guard = sidecar();
        let sidecar = &mut *guard;
        if let Some(child) = sidecar.child.as_mut() {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    sidecar.child = None;
                    sidecar.port = None;
                    sidecar.status =
                        SidecarStatus::Failed(format!("GBots stopped ({exit}). Restarting."));
                    sidecar.retry_at = Some(Instant::now() + sidecar.retry_wait);
                    sidecar.retry_wait = (sidecar.retry_wait * 2).min(LAST_RETRY);
                    GENERATION.fetch_add(1, Ordering::SeqCst);
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
        if sidecar.child.is_none() {
            start_now = sidecar.retry_at.is_none_or(|at| Instant::now() >= at);
        }
    }
    if start_now {
        match spawn_sidecar(state) {
            Ok((child, port)) => {
                let mut sidecar = sidecar();
                sidecar.child = Some(child);
                sidecar.port = Some(port);
                sidecar.status = SidecarStatus::Starting;
                sidecar.started_at = Some(Instant::now());
                sidecar.retry_at = None;
            }
            Err(reason) => {
                let mut sidecar = sidecar();
                sidecar.status = SidecarStatus::Failed(reason);
                sidecar.retry_at = Some(Instant::now() + sidecar.retry_wait);
                sidecar.retry_wait = (sidecar.retry_wait * 2).min(LAST_RETRY);
            }
        }
    }
    let starting = {
        let sidecar = sidecar();
        (sidecar.status == SidecarStatus::Starting).then(|| (sidecar.port, sidecar.started_at))
    };
    if let Some((Some(port), started_at)) = starting {
        if super::client::healthy(port) {
            let generation = {
                let mut sidecar = sidecar();
                sidecar.status = SidecarStatus::Running;
                sidecar.retry_wait = FIRST_RETRY;
                GENERATION.fetch_add(1, Ordering::SeqCst) + 1
            };
            super::events::start_event_reader(state, port, generation);
        } else if started_at.is_some_and(|at| at.elapsed() > START_DEADLINE) {
            stop_sidecar_now();
            let mut sidecar = sidecar();
            sidecar.status = SidecarStatus::Failed(format!(
                "GBots did not start within {}s; see {}.",
                START_DEADLINE.as_secs(),
                state_dir().join("sidecar.log").display()
            ));
            sidecar.retry_at = Some(Instant::now() + sidecar.retry_wait);
            sidecar.retry_wait = (sidecar.retry_wait * 2).min(LAST_RETRY);
        }
    }
    sidecar().status != before
}

/// A free loopback port whose next port is free too (the runtime also opens a webhook receiver on
/// `port + 1`).
fn free_port_pair() -> Result<u16, String> {
    for _ in 0..20 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| format!("No free port for GBots: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        if port < u16::MAX && std::net::TcpListener::bind(("127.0.0.1", port + 1)).is_ok() {
            return Ok(port);
        }
    }
    Err("No free port pair for GBots.".to_string())
}

/// The `ghostex` CLI shipped beside this gxserver.
fn cli_dir() -> Option<PathBuf> {
    let gxserver = std::env::current_exe().ok()?;
    let gxserver = std::fs::canonicalize(&gxserver).unwrap_or(gxserver);
    let dir = crate::ghostex_cli::launchers::package_bin_dir(&gxserver)
        .unwrap_or_else(|| gxserver.parent().map(PathBuf::from).unwrap_or_default());
    dir.join(if cfg!(windows) {
        "ghostex.exe"
    } else {
        "ghostex"
    })
    .is_file()
    .then_some(dir)
}

fn spawn_sidecar(state: &AppState) -> Result<(Child, u16), String> {
    let runtime = super::runtime::installed_runtime()
        .ok_or_else(|| "The GBots runtime is not installed.".to_string())?;
    let node = super::runtime::resolve_node()?;
    let port = free_port_pair()?;
    let data = data_dir();
    std::fs::create_dir_all(&data).map_err(|error| error.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state_dir().join("sidecar.log"))
        .map_err(|error| error.to_string())?;
    let log_err = log.try_clone().map_err(|error| error.to_string())?;
    let cli_dir = cli_dir();
    let mut command = Command::new(&node);
    command
        .arg(&runtime.entry)
        .args(["serve", "--no-pair", "--label", "GBots", "--port"])
        .arg(port.to_string())
        .arg("--data-dir")
        .arg(&data)
        .current_dir(&runtime.root)
        .env("OMB_DATA_DIR", &data)
        .env("OMB_LOOPBACK_TRUST", "owner")
        .env("PATH", super::runtime::sidecar_path(cli_dir.as_slice()))
        // The bots' agents reach this gxserver with the `ghostex` CLI, as a session's agent does.
        .env(
            "GHOSTEX_GXSERVER_BASE_URL",
            format!(
                "http://{}:{}",
                state.config.listeners.local.host, state.config.listeners.local.port
            ),
        )
        .env(
            "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
            &state.paths.auth_token_file,
        )
        .env("GHOSTEX_GBOTS", "1")
        .env_remove("GHOSTEX_SESSION_ID")
        .env_remove("GHOSTEX_GLOBAL_SESSION_REF")
        .env_remove("GHOSTEX_NATIVE_SESSION_ID")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    if let Some(dir) = &cli_dir {
        command.env(
            "GHOSTEX_CLI_EXECUTABLE",
            dir.join(if cfg!(windows) {
                "ghostex.exe"
            } else {
                "ghostex"
            }),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Could not start GBots: {error}"))?;
    let _ = std::fs::write(
        pid_file(),
        json!({ "pid": child.id(), "port": port, "runtimeVersion": runtime.version }).to_string(),
    );
    Ok((child, port))
}

/// Stops the sidecar and every process it started.
pub(crate) fn stop_sidecar_now() {
    let child = {
        let mut sidecar = sidecar();
        sidecar.port = None;
        sidecar.started_at = None;
        sidecar.child.take()
    };
    if let Some(mut child) = child {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        terminate_group(child.id());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(pid_file());
    }
}

#[cfg(unix)]
fn terminate_group(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: signalling a process group this gxserver created; no memory is touched.
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn terminate_group(_pid: u32) {}

/// Ends a sidecar a previous gxserver left running.
fn end_leftover_sidecar() {
    let Some(pid) = std::fs::read_to_string(pid_file())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|record| record.get("pid").and_then(Value::as_u64))
        .and_then(|pid| u32::try_from(pid).ok())
    else {
        return;
    };
    if crate::runtime::is_process_running(pid) {
        terminate_group(pid);
        std::thread::sleep(Duration::from_secs(2));
        #[cfg(unix)]
        if crate::runtime::is_process_running(pid) {
            // SAFETY: as above.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    let _ = std::fs::remove_file(pid_file());
}
