//! The one running `ghostex-screen` helper: where it is, starting it on a loopback port with a
//! password only gxserver knows, and stopping it.

use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rand::TryRng;
use subtle::ConstantTimeEq;

use crate::paths::GxserverPaths;

/// The helper's file name beside gxserver.
const HELPER_STEM: &str = "ghostex-screen";
/// Overrides where the helper is, for a test build.
const HELPER_PATH_VARIABLE: &str = "GHOSTEX_SCREEN_HELPER";
/// How long a starting helper has to answer its health check.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// What a viewer needs to reach the running helper through gxserver.
#[derive(Clone, Debug)]
pub(crate) struct ScreenSessionInfo {
    pub(crate) ticket: String,
    pub(crate) view_only: bool,
    pub(crate) started_at_ms: u64,
    pub(crate) display: Option<String>,
    pub(crate) max_pixels: Option<u64>,
}

/// Where the proxy sends a request that carried the right ticket.
#[derive(Clone, Debug)]
pub(crate) struct HelperTarget {
    pub(crate) port: u16,
    pub(crate) password: String,
}

struct RunningHelper {
    child: Child,
    target: HelperTarget,
    info: ScreenSessionInfo,
}

static RUNNING: Mutex<Option<RunningHelper>> = Mutex::new(None);
/// One start at a time, so two viewers asking together share one helper.
static STARTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug)]
pub(crate) enum StartError {
    HelperMissing,
    Failed(String),
}

pub(crate) struct StartOptions {
    pub(crate) view_only: bool,
    pub(crate) display: Option<String>,
    /// Stream a smaller copy of a display with more pixels than this (a phone asks for about
    /// 1080p); `None` streams the display's own size.
    pub(crate) max_pixels: Option<u64>,
}

fn running() -> std::sync::MutexGuard<'static, Option<RunningHelper>> {
    RUNNING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The helper gxserver starts: `GHOSTEX_SCREEN_HELPER`, else `ghostex-screen` beside gxserver.
pub(crate) fn helper_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(HELPER_PATH_VARIABLE) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let beside = std::env::current_exe()
        .ok()?
        .parent()?
        .join(format!("{HELPER_STEM}{}", std::env::consts::EXE_SUFFIX));
    beside.is_file().then_some(beside)
}

/// The running session, after dropping one whose helper has exited.
pub(crate) fn current() -> Option<ScreenSessionInfo> {
    let mut guard = running();
    if let Some(helper) = guard.as_mut() {
        if !matches!(helper.child.try_wait(), Ok(None)) {
            *guard = None;
        }
    }
    guard.as_ref().map(|helper| helper.info.clone())
}

/// The helper behind `ticket`, compared in constant time.
pub(crate) fn target_for_ticket(ticket: &str) -> Option<HelperTarget> {
    let mut guard = running();
    let helper = guard.as_mut()?;
    if !matches!(helper.child.try_wait(), Ok(None)) {
        *guard = None;
        return None;
    }
    let expected = helper.info.ticket.as_bytes();
    let matches = ticket.len() == expected.len() && bool::from(ticket.as_bytes().ct_eq(expected));
    matches.then(|| helper.target.clone())
}

/// Starts the helper, or returns the session already running.
pub(crate) async fn start(
    paths: &GxserverPaths,
    options: StartOptions,
) -> Result<ScreenSessionInfo, StartError> {
    let _starting = STARTING.lock().await;
    if let Some(info) = current() {
        // CDXC:RemoteScreen 2026-11-26 WHY: one helper streams one size, so a viewer asking for
        // another (the phone's ~1080p after the desktop's full size, or view-only) restarts it with
        // a new ticket and the earlier viewer's stream ends. Two viewers at once at different sizes
        // is later work (a second helper, or voxl streaming two sizes).
        if info.view_only == options.view_only
            && info.display == options.display
            && info.max_pixels == options.max_pixels
        {
            return Ok(info);
        }
        stop();
    }
    let binary = helper_binary().ok_or(StartError::HelperMissing)?;
    let port = free_loopback_port().map_err(|error| StartError::Failed(error.to_string()))?;
    let password = random_hex(32);
    let ticket = random_hex(32);
    let log_path = paths.logs_dir.join("remote-screen-helper.log");
    let _ = std::fs::create_dir_all(&paths.logs_dir);
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| StartError::Failed(format!("Could not open the helper log: {error}")))?;
    let mut command = crate::platform::process::background_command(&binary);
    command
        .arg(format!("--port={port}"))
        .arg("--framerate=60")
        .arg("--enable-resize=false")
        .arg("--file-transfers=none")
        .arg(format!("--read-only={}", options.view_only))
        .env("VOXL_PASSWORD", &password)
        .env("GHOSTEX_SCREEN_EXIT_ON_STDIN_EOF", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    if let Some(display) = options.display.as_deref() {
        command.arg(format!("--display={display}"));
    }
    if let Some(max_pixels) = options.max_pixels {
        command.arg(format!("--max-capture-pixels={max_pixels}"));
    }
    let mut child = command.spawn().map_err(|error| {
        StartError::Failed(format!("Could not start {}: {error}", binary.display()))
    })?;
    crate::platform::process::end_with_server(child.id());
    let ready = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err((child, format!("The screen helper stopped ({status}).")));
            }
            if health_ok(port) {
                return Ok(child);
            }
            if Instant::now() >= deadline {
                return Err((
                    child,
                    "The screen helper did not answer in time.".to_string(),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })
    .await
    .map_err(|error| StartError::Failed(error.to_string()))?;
    let child = match ready {
        Ok(child) => child,
        Err((mut child, message)) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(StartError::Failed(format!(
                "{message} {}",
                log_tail(&log_path)
            )));
        }
    };
    let info = ScreenSessionInfo {
        ticket,
        view_only: options.view_only,
        started_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0),
        display: options.display,
        max_pixels: options.max_pixels,
    };
    *running() = Some(RunningHelper {
        child,
        target: HelperTarget { port, password },
        info: info.clone(),
    });
    Ok(info)
}

/// Stops the helper; every viewer's stream ends with it. Whether one was running.
pub(crate) fn stop() -> bool {
    let Some(mut helper) = running().take() else {
        return false;
    };
    let _ = helper.child.kill();
    let _ = helper.child.wait();
    true
}

fn free_loopback_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

/// `/healthz` answers without the password.
fn health_ok(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(300)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    if stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut answer = [0u8; 16];
    matches!(stream.read(&mut answer), Ok(count) if count >= 12 && &answer[9..12] == b"200")
}

fn log_tail(path: &std::path::Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().rev().take(5).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::rngs::SysRng
        .try_fill_bytes(&mut buffer)
        .expect("OS RNG failed");
    hex::encode(buffer)
}
