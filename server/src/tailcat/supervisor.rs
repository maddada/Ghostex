use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::paths::GxserverPaths;

use super::binary::*;
use super::keys::*;
use super::repository::ports_csv;
use super::types::*;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A child that stayed up this long counts as a healthy start, so the next
/// crash restarts fast instead of inheriting the previous crash loop's delay.
const STABLE_UPTIME: Duration = Duration::from_secs(30);
const STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
struct TailcatServeSpec {
    addr_file: PathBuf,
    allowed_client_keys: Vec<String>,
    binary_path: PathBuf,
    key_file: PathBuf,
    ports: Vec<u16>,
}

#[derive(Clone, Default)]
pub struct TailcatRuntime {
    inner: Arc<Mutex<TailcatRuntimeInner>>,
}

#[derive(Default)]
struct TailcatRuntimeInner {
    generation: u64,
    spec: Option<TailcatServeSpec>,
    stop_tx: Option<mpsc::Sender<mpsc::SyncSender<()>>>,
    running: bool,
    token: Option<String>,
    last_error: Option<String>,
    binary_version: Option<(PathBuf, Option<String>)>,
}

impl TailcatRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn snapshot(&self) -> TailcatRuntimeSnapshot {
        let Ok(inner) = self.inner.lock() else {
            return TailcatRuntimeSnapshot::default();
        };
        TailcatRuntimeSnapshot {
            running: inner.running,
            token: inner.token.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    /// `tailcat version` is a process spawn, so it is resolved once per binary
    /// path and then answered from memory for every later status read.
    pub(crate) fn binary_version(&self, binary_path: &std::path::Path) -> Option<String> {
        if let Ok(inner) = self.inner.lock() {
            if let Some((cached_path, version)) = inner.binary_version.as_ref() {
                if cached_path == binary_path {
                    return version.clone();
                }
            }
        }
        let version = read_tailcat_binary_version(binary_path);
        if let Ok(mut inner) = self.inner.lock() {
            inner.binary_version = Some((binary_path.to_path_buf(), version.clone()));
        }
        version
    }

    /*
    CDXC:RemotePairing 2026-09-01:
    One reconcile entry point for every trigger — enable/disable, a port or
    allow-list edit, and daemon start — so the sidecar's command line is always
    a pure function of persisted state. Restart only when the resolved spec
    actually differs; an unrelated settings save must not drop live tunnels.
    */
    pub(crate) fn apply(&self, paths: &GxserverPaths, state: &TailcatState) {
        if !state.enabled {
            self.stop();
            self.set_last_error(None);
            return;
        }
        let Some(binary_path) = resolve_tailcat_binary() else {
            self.stop();
            self.set_last_error(Some(
                "The tailcat binary was not found. Install tailcat or set GHOSTEX_TAILCAT_BIN."
                    .to_string(),
            ));
            return;
        };
        let key_file = tailcat_key_file(paths);
        if let Err(error) = ensure_tailcat_key_file(&binary_path, &key_file) {
            self.stop();
            self.set_last_error(Some(format!(
                "Could not create the tailcat server key: {error}"
            )));
            return;
        }
        let spec = TailcatServeSpec {
            addr_file: tailcat_address_file(paths),
            allowed_client_keys: state.allowed_client_keys.clone(),
            binary_path,
            key_file,
            ports: state.ports.clone(),
        };
        if self.is_current_spec(&spec) {
            return;
        }
        self.stop();
        self.start(spec);
    }

    pub fn stop(&self) {
        let stop_tx = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            inner.generation = inner.generation.wrapping_add(1);
            inner.spec = None;
            inner.running = false;
            inner.token = None;
            inner.stop_tx.take()
        };
        let Some(stop_tx) = stop_tx else {
            return;
        };
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        if stop_tx.send(reply_tx).is_ok() {
            let _ = reply_rx.recv_timeout(STOP_WAIT_TIMEOUT);
        }
    }

    fn start(&self, spec: TailcatServeSpec) {
        let (stop_tx, stop_rx) = mpsc::channel();
        let generation = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            inner.generation = inner.generation.wrapping_add(1);
            inner.spec = Some(spec.clone());
            inner.stop_tx = Some(stop_tx);
            inner.running = false;
            inner.token = None;
            inner.last_error = None;
            inner.generation
        };
        let runtime = self.clone();
        thread::spawn(move || supervise(runtime, generation, spec, stop_rx));
    }

    fn is_current_spec(&self, spec: &TailcatServeSpec) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|inner| inner.spec.as_ref() == Some(spec))
    }

    fn set_last_error(&self, error: Option<String>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_error = error;
        }
    }

    fn mark_running(&self, generation: u64, running: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.generation != generation {
                return;
            }
            inner.running = running;
            if !running {
                inner.token = None;
            }
        }
    }

    fn record_error(&self, generation: u64, error: String) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.generation != generation {
                return;
            }
            inner.running = false;
            inner.token = None;
            inner.last_error = Some(error);
        }
    }

    fn needs_token(&self, generation: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|inner| inner.generation == generation && inner.token.is_none())
    }

    fn set_token(&self, generation: u64, token: String) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.generation != generation {
                return;
            }
            inner.token = Some(token);
            inner.last_error = None;
        }
    }
}

fn supervise(
    runtime: TailcatRuntime,
    generation: u64,
    spec: TailcatServeSpec,
    stop_rx: mpsc::Receiver<mpsc::SyncSender<()>>,
) {
    let mut backoff = INITIAL_BACKOFF;
    loop {
        // The blob file is written by the child on startup; a stale one from
        // the previous run must never be reported as this run's token.
        let _ = fs::remove_file(&spec.addr_file);
        let mut child = match spawn_tailcat_serve(&spec) {
            Ok(child) => child,
            Err(error) => {
                runtime.record_error(
                    generation,
                    format!("Could not start tailcat serve: {error}"),
                );
                if wait_for_stop(&stop_rx, backoff) {
                    return;
                }
                backoff = next_backoff(backoff);
                continue;
            }
        };
        runtime.mark_running(generation, true);
        let started_at = Instant::now();
        let exit_reason = loop {
            if let Ok(reply) = stop_rx.try_recv() {
                terminate_process_group(&mut child);
                let _ = reply.send(());
                return;
            }
            if runtime.needs_token(generation) {
                if let Some(token) = read_address_blob(&spec.addr_file) {
                    runtime.set_token(generation, token);
                }
            }
            match child.try_wait() {
                Ok(Some(status)) => break format!("tailcat serve exited with {status}."),
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    terminate_process_group(&mut child);
                    break format!("Could not inspect the tailcat process: {error}");
                }
            }
        };
        let was_stable = started_at.elapsed() >= STABLE_UPTIME;
        runtime.record_error(generation, exit_reason);
        if was_stable {
            backoff = INITIAL_BACKOFF;
        }
        if wait_for_stop(&stop_rx, backoff) {
            return;
        }
        backoff = next_backoff(backoff);
    }
}

fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

/// Returns true when the supervisor was asked to stop while waiting.
fn wait_for_stop(stop_rx: &mpsc::Receiver<mpsc::SyncSender<()>>, wait: Duration) -> bool {
    match stop_rx.recv_timeout(wait) {
        Ok(reply) => {
            let _ = reply.send(());
            true
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => true,
        Err(mpsc::RecvTimeoutError::Timeout) => false,
    }
}

fn read_address_blob(addr_file: &std::path::Path) -> Option<String> {
    let text = fs::read_to_string(addr_file).ok()?;
    let token = text.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn spawn_tailcat_serve(spec: &TailcatServeSpec) -> std::io::Result<Child> {
    let mut command = Command::new(&spec.binary_path);
    command
        .arg("serve")
        // Emit a self-contained address blob with the DERP relay details
        // embedded. Without it the blob only references a DERP map region id,
        // so every phone has to fetch https://tailcat.dev/derpmap.json before
        // it can move a single tunnel byte — a hard-timeout network dependency
        // that fails on cellular links and filtered DNS.
        .arg("--full-address")
        .arg(format!("--key={}", spec.key_file.display()));
    if !spec.allowed_client_keys.is_empty() {
        command.arg(format!("--allow={}", spec.allowed_client_keys.join(",")));
    }
    command
        .arg(ports_csv(&spec.ports))
        .env("TAILCAT_ADDR_FILE", &spec.addr_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_process_group(&mut command);
    command.spawn()
}

pub(super) fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0200);
    }
}

pub(super) fn terminate_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group_id = child.id() as libc::pid_t;
        if process_group_id > 0 {
            unsafe {
                libc::kill(-process_group_id, libc::SIGKILL);
            }
        }
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.kill();
    let _ = child.wait();
}
