//! Starts, supervises and stops the Collab component (`ghostex-collab serve`) while it is
//! installed.
//!
//! gxserver writes the component one hello line on its stdin: gxserver's loopback address, the
//! component's data folder and two fresh random secrets (the host token it calls
//! `/api/collabHost` with, and the sidecar token gxserver calls it with). Secrets never go
//! through argv or the environment, so other processes can't read them from the process list.
//! The component answers with its loopback port on stdout. gxserver keeps the stdin open: when
//! gxserver stops, or uninstalls Collab, the pipe closes and the component ends with it.
//!
//! A component that exits is restarted with a growing wait (2s, doubling to 60s). While it is
//! down, every share token is refused and chats show no Collab part.

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use ghostex_gx_protocol::collab_component::{
    OwnerCall, OwnerReply, SidecarEvent, SidecarHello, SidecarReady, COLLAB_PROTOCOL_VERSION,
    SIDECAR_EVENT_PATH, SIDECAR_OWNER_PATH,
};
use rand::Rng as _;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

use crate::server::AppState;

const TICK: Duration = Duration::from_secs(2);
const READY_DEADLINE: Duration = Duration::from_secs(20);
const FIRST_RETRY: Duration = Duration::from_secs(2);
const LAST_RETRY: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    NotInstalled,
    Starting,
    Running,
    /// It could not start or it stopped; the text says why. It is retried.
    Failed(String),
}

impl Status {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::NotInstalled => "notInstalled",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed(_) => "failed",
        }
    }
}

struct Sidecar {
    child: Option<Child>,
    /// Held open for the life of the process: closing it ends the component.
    stdin: Option<ChildStdin>,
    port: Option<u16>,
    host_token: String,
    sidecar_token: String,
    version: String,
    status: Status,
    retry_at: Option<Instant>,
    retry_wait: Duration,
    /// Set while install, update or uninstall changes the files: the supervisor waits.
    paused: bool,
}

static SIDECAR: Mutex<Sidecar> = Mutex::new(Sidecar {
    child: None,
    stdin: None,
    port: None,
    host_token: String::new(),
    sidecar_token: String::new(),
    version: String::new(),
    status: Status::NotInstalled,
    retry_at: None,
    retry_wait: FIRST_RETRY,
    paused: false,
});

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static STATE: std::sync::OnceLock<Arc<AppState>> = std::sync::OnceLock::new();
static RUNTIME: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();

fn sidecar() -> std::sync::MutexGuard<'static, Sidecar> {
    SIDECAR.lock().unwrap_or_else(PoisonError::into_inner)
}

/// gxserver's state, once the supervisor started (updates start a Collab update with it).
pub(crate) fn app_state() -> Option<Arc<AppState>> {
    STATE.get().cloned()
}

pub(crate) fn is_running() -> bool {
    let sidecar = sidecar();
    sidecar.status == Status::Running && sidecar.port.is_some()
}

/// Whether `token` is the host token of the component running now.
pub(crate) fn host_token_matches(token: &str) -> bool {
    let sidecar = sidecar();
    sidecar.status == Status::Running
        && !sidecar.host_token.is_empty()
        && token.len() == sidecar.host_token.len()
        && bool::from(token.as_bytes().ct_eq(sidecar.host_token.as_bytes()))
}

pub(crate) fn set_paused(paused: bool) {
    sidecar().paused = paused;
}

/// Starts the supervisor thread, which runs for the life of gxserver, and stops the component
/// the moment gxserver's stop is broadcast.
pub(crate) fn start_supervisor(state: Arc<AppState>) {
    let _ = STATE.set(state.clone());
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let _ = RUNTIME.set(handle);
    }
    let mut shutdown = state.shutdown_tx.subscribe();
    let _ = std::thread::Builder::new()
        .name("collab-shutdown".to_string())
        .spawn(move || {
            let _ = shutdown.blocking_recv();
            SHUTTING_DOWN.store(true, Ordering::SeqCst);
            stop_now();
        });
    let _ = std::thread::Builder::new()
        .name("collab-supervisor".to_string())
        .spawn(move || loop {
            if SHUTTING_DOWN.load(Ordering::SeqCst) {
                return;
            }
            let _runtime = RUNTIME.get().map(tokio::runtime::Handle::enter);
            supervise_once(&state);
            std::thread::sleep(TICK);
        });
}

/// One look: start or restart the component while it is installed.
pub(crate) fn supervise_once(state: &Arc<AppState>) {
    if sidecar().paused {
        return;
    }
    let Some(installed) = super::install::installed() else {
        if sidecar().child.is_some() {
            stop_now();
        }
        sidecar().status = Status::NotInstalled;
        return;
    };
    let mut start_now = false;
    let mut stopped = false;
    {
        let mut guard = sidecar();
        let sidecar = &mut *guard;
        if let Some(child) = sidecar.child.as_mut() {
            if let Ok(Some(exit)) = child.try_wait() {
                sidecar.child = None;
                sidecar.stdin = None;
                sidecar.port = None;
                sidecar.host_token.clear();
                sidecar.status = Status::Failed(format!("Collab stopped ({exit}). Restarting."));
                sidecar.retry_at = Some(Instant::now() + sidecar.retry_wait);
                sidecar.retry_wait = (sidecar.retry_wait * 2).min(LAST_RETRY);
                stopped = true;
            }
        }
        if sidecar.child.is_none() {
            start_now = sidecar.retry_at.is_none_or(|at| Instant::now() >= at);
        }
    }
    if stopped {
        forget_component_state(state);
    }
    if !start_now {
        return;
    }
    sidecar().status = Status::Starting;
    match spawn(state, &installed) {
        Ok(()) => {
            let mut sidecar = sidecar();
            sidecar.status = Status::Running;
            sidecar.retry_at = None;
            sidecar.retry_wait = FIRST_RETRY;
        }
        Err(reason) => {
            eprintln!("[collab] {reason}");
            let mut sidecar = sidecar();
            sidecar.status = Status::Failed(reason);
            sidecar.retry_at = Some(Instant::now() + sidecar.retry_wait);
            sidecar.retry_wait = (sidecar.retry_wait * 2).min(LAST_RETRY);
        }
    }
    // Chats gain (or lose) their Collab part with the component.
    super::republish_followed(state);
}

fn secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The component's own folder in gxserver's state: its choices, shares, held messages and log.
/// Removed on uninstall.
pub(crate) fn data_dir(state: &AppState) -> std::path::PathBuf {
    state.paths.app_state_dir.join("collab-component")
}

fn spawn(state: &AppState, installed: &super::install::Installed) -> Result<(), String> {
    let host_token = secret();
    let sidecar_token = secret();
    let hello = SidecarHello {
        protocol: COLLAB_PROTOCOL_VERSION,
        host_url: format!("http://127.0.0.1:{}", state.config.listeners.local.port),
        gxserver_protocol_version: crate::constants::GXSERVER_PROTOCOL_VERSION as u32,
        data_dir: data_dir(state).to_string_lossy().to_string(),
        host_token: host_token.clone(),
        sidecar_token: sidecar_token.clone(),
    };
    let mut command = Command::new(&installed.binary);
    command
        .arg("serve")
        .current_dir(&installed.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Collab couldn't start: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("Collab has no stdin.")?;
    let stdout = child.stdout.take().ok_or("Collab has no stdout.")?;
    let line = format!("{}\n", serde_json::to_string(&hello).unwrap_or_default());
    if let Err(error) = stdin
        .write_all(line.as_bytes())
        .and_then(|()| stdin.flush())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("Collab couldn't be started: {error}"));
    }
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("collab-ready".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let _ = ready_tx.send(line);
            // Keep reading (and dropping) whatever else it prints, so it never blocks on a full pipe.
            let mut rest = String::new();
            while reader.read_line(&mut rest).is_ok_and(|read| read > 0) {
                rest.clear();
            }
        });
    let ready = ready_rx
        .recv_timeout(READY_DEADLINE)
        .ok()
        .and_then(|line| serde_json::from_str::<SidecarReady>(line.trim()).ok());
    let Some(ready) = ready.filter(|ready| ready.protocol == COLLAB_PROTOCOL_VERSION) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(
            "Collab didn't start. If Settings > Updates lists a Collab update, install it."
                .to_string(),
        );
    };
    let mut sidecar = sidecar();
    sidecar.child = Some(child);
    sidecar.stdin = Some(stdin);
    sidecar.port = Some(ready.port);
    sidecar.host_token = host_token;
    sidecar.sidecar_token = sidecar_token;
    sidecar.version = ready.version;
    Ok(())
}

/// Forgets everything the component reported (it stopped).
fn forget_component_state(state: &AppState) {
    super::registry::clear();
    let chats = super::clear_chat_extras();
    super::republish(state, &chats);
}

/// Stops the component now (closing its stdin, then killing it if it lingers).
pub(crate) fn stop_now() {
    let child = {
        let mut sidecar = sidecar();
        sidecar.stdin = None;
        sidecar.port = None;
        sidecar.host_token.clear();
        sidecar.sidecar_token.clear();
        if !matches!(sidecar.status, Status::NotInstalled) {
            sidecar.status = Status::NotInstalled;
        }
        sidecar.child.take()
    };
    if let Some(mut child) = child {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Ok(None) = child.try_wait() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    if let Some(state) = STATE.get() {
        forget_component_state(state);
    } else {
        super::registry::clear();
        super::clear_chat_extras();
    }
}

fn endpoint(path: &str) -> Option<(String, String)> {
    let sidecar = sidecar();
    let port = sidecar.port.filter(|_| sidecar.status == Status::Running)?;
    Some((
        format!("http://127.0.0.1:{port}{path}"),
        sidecar.sidecar_token.clone(),
    ))
}

fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        crate::http_agent::config()
            .max_redirects(0)
            .http_status_as_error(false)
            // A share checks the relay's guest page first, which downloads it.
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into()
    })
}

/// One owner call to the component (blocking).
pub(crate) fn call_owner(call: &OwnerCall) -> Result<OwnerReply, String> {
    let (url, token) = endpoint(SIDECAR_OWNER_PATH).ok_or_else(not_running)?;
    let response = agent()
        .post(url.as_str())
        .header("authorization", format!("Bearer {token}"))
        .send_json(call)
        .map_err(|error| format!("Collab couldn't be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Collab answered {}.", response.status()));
    }
    serde_json::from_reader(response.into_body().into_reader())
        .map_err(|error| format!("Collab answered something unreadable: {error}"))
}

fn not_running() -> String {
    "Collab isn't running. Install it in Settings > Extensions.".to_string()
}

/// One event to the component (blocking); nothing when it isn't running.
pub(crate) fn send_event(event: &SidecarEvent) -> Result<(), String> {
    let (url, token) = endpoint(SIDECAR_EVENT_PATH).ok_or_else(not_running)?;
    agent()
        .post(url.as_str())
        .header("authorization", format!("Bearer {token}"))
        .send_json(event)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// [`send_event`] on a thread of its own, for callers that must not wait.
pub(crate) fn send_event_later(event: SidecarEvent) {
    let _ = std::thread::Builder::new()
        .name("collab-event".to_string())
        .spawn(move || {
            let _ = send_event(&event);
        });
}

/// The component's status for Settings and the CLI.
pub(crate) fn status_value() -> Value {
    let sidecar = sidecar();
    json!({
        "status": sidecar.status.as_str(),
        "error": match &sidecar.status { Status::Failed(reason) => Some(reason.clone()), _ => None },
        "version": (sidecar.status == Status::Running).then(|| sidecar.version.clone()),
    })
}
