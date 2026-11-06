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

/// The two Easy Connect helpers: the first address, on the public relays, which every phone paired
/// so far holds; and the second, on the relay Settings picked (Ghostex's own by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TailcatLane {
    Public,
    Relay,
}

impl TailcatLane {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Relay => "relay",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TailcatServeSpec {
    lane: TailcatLane,
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
struct TailcatLaneState {
    generation: u64,
    spec: Option<TailcatServeSpec>,
    stop_tx: Option<mpsc::Sender<mpsc::SyncSender<()>>>,
    running: bool,
    token: Option<String>,
    last_error: Option<String>,
}

#[derive(Default)]
struct TailcatRuntimeInner {
    public: TailcatLaneState,
    relay: TailcatLaneState,
    binary_version: Option<(PathBuf, Option<String>)>,
    /// While set and in the future, the allow-list is lifted so a new phone can pair.
    pairing_open_until: Option<Instant>,
    /// The relay the second helper is set up for (`TailcatRelaySetting::relay_host`).
    relay_host: Option<String>,
    /// The last `/generate_204` answer of the relay named first, for that relay only.
    relay_reachable: Option<(String, bool)>,
    /// Where link records go (`link_log.rs`); none in tests.
    logger: Option<std::sync::Arc<crate::logging::GxserverLogger>>,
}

impl TailcatRuntimeInner {
    fn lane(&mut self, lane: TailcatLane) -> &mut TailcatLaneState {
        match lane {
            TailcatLane::Public => &mut self.public,
            TailcatLane::Relay => &mut self.relay,
        }
    }
}

fn lane_snapshot(lane: &TailcatLaneState) -> TailcatLaneSnapshot {
    TailcatLaneSnapshot {
        running: lane.running,
        token: lane.token.clone(),
        last_error: lane.last_error.clone(),
    }
}

impl TailcatRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// A runtime whose helpers record each Easy Connect link in gxserver's log.
    pub fn with_logger(logger: Arc<crate::logging::GxserverLogger>) -> Self {
        let runtime = Self::default();
        if let Ok(mut inner) = runtime.inner.lock() {
            inner.logger = Some(logger);
        }
        runtime
    }

    pub(crate) fn snapshot(&self) -> TailcatRuntimeSnapshot {
        let Ok(inner) = self.inner.lock() else {
            return TailcatRuntimeSnapshot::default();
        };
        let relay_reachable = match (&inner.relay_host, &inner.relay_reachable) {
            (Some(host), Some((probed, reachable))) if host == probed => Some(*reachable),
            _ => None,
        };
        TailcatRuntimeSnapshot {
            running: inner.public.running,
            token: inner.public.token.clone(),
            last_error: inner.public.last_error.clone(),
            relay: lane_snapshot(&inner.relay),
            relay_host: inner.relay_host.clone(),
            relay_reachable,
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
            self.set_last_error(TailcatLane::Public, None);
            self.set_last_error(TailcatLane::Relay, None);
            return;
        }
        let Some(binary_path) = resolve_host_tailcat_binary(paths) else {
            self.stop();
            // CDXC:Notices 2026-11-06: Settings > Remote shows these to a person (Q1410); raw errors go to the log.
            eprintln!(
                "[gxserver] tailcat binary not found (bundled, managed or GHOSTEX_TAILCAT_BIN)"
            );
            self.set_last_error(
                TailcatLane::Public,
                Some(
                    "Part of Easy Connect is missing on this computer. Reinstall Ghostex to get it back."
                        .to_string(),
                ),
            );
            return;
        };
        let key_file = tailcat_key_file(paths);
        if let Err(error) = ensure_tailcat_key_file(&binary_path, &key_file) {
            self.stop();
            eprintln!("[gxserver] could not create the tailcat server key: {error}");
            self.set_last_error(
                TailcatLane::Public,
                Some("Easy Connect couldn't set itself up. Turn it off and on again.".to_string()),
            );
            return;
        }
        if let Err(error) = record_host_tailcat_binary(paths, &binary_path) {
            self.stop();
            self.set_last_error(
                TailcatLane::Public,
                Some(format!("Could not record the Easy Connect helper: {error}")),
            );
            return;
        }
        let allowed_client_keys = if self.pairing_window_open() {
            Vec::new()
        } else {
            state.allowed_client_keys.clone()
        };
        let spec = TailcatServeSpec {
            lane: TailcatLane::Public,
            addr_file: tailcat_address_file(paths),
            allowed_client_keys: allowed_client_keys.clone(),
            binary_path,
            key_file,
            ports: tailcat_served_ports(state),
        };
        self.reconcile_lane(spec);
        self.apply_relay(paths, state, allowed_client_keys);
    }

    /*
    CDXC:RemotePairing 2026-11-06 DECISION:
    User (Q1490 B, approved plan "How Ghostex would point at it, without re-pairing"): the relay is part of an address, so moving a computer to Ghostex's relay must not replace its key: gxserver makes a second key pinned to the relay (`genkey --region=<host>`) and runs a second `tailcat serve` beside the first, on the same ports with the same allow-list. The first key, helper and address stay exactly as they were, so a phone that never updates keeps working on the public relays. The second helper uses the newest tailcat here (v0.6.0 or later, as dialing does), since no phone holds its address yet; when there is none, or its relay is down, Easy Connect simply has the first address alone.
    SEE-ALSO: server/src/tailcat/types.rs (`TailcatRelaySetting`), server/src/remote_access/easy_connect_addresses.rs.
    */
    fn apply_relay(&self, paths: &GxserverPaths, state: &TailcatState, allowed: Vec<String>) {
        let host = state.relay.relay_host();
        if let Ok(mut inner) = self.inner.lock() {
            inner.relay_host = host.clone();
        }
        let Some(host) = host else {
            self.stop_lane(TailcatLane::Relay);
            self.set_last_error(TailcatLane::Relay, None);
            return;
        };
        let Some(binary_path) = resolve_client_tailcat_binary() else {
            self.stop_lane(TailcatLane::Relay);
            self.set_last_error(
                TailcatLane::Relay,
                Some("Ghostex's relay needs a newer Easy Connect helper; install it above. The public relays still work.".to_string()),
            );
            return;
        };
        let key_file = tailcat_relay_key_file(paths, &host);
        if let Err(error) = ensure_tailcat_relay_key_file(&binary_path, &key_file, &host) {
            self.stop_lane(TailcatLane::Relay);
            eprintln!("[gxserver] could not create the tailcat relay key for {host}: {error}");
            self.set_last_error(
                TailcatLane::Relay,
                Some(
                    "Easy Connect couldn't set up the relay address. The public relays still work."
                        .to_string(),
                ),
            );
            return;
        }
        self.reconcile_lane(TailcatServeSpec {
            lane: TailcatLane::Relay,
            addr_file: tailcat_relay_address_file(paths),
            allowed_client_keys: allowed,
            binary_path,
            key_file,
            ports: tailcat_served_ports(state),
        });
    }

    fn reconcile_lane(&self, spec: TailcatServeSpec) {
        if self.is_current_spec(&spec) {
            return;
        }
        self.stop_lane(spec.lane);
        self.start(spec);
    }

    /*
    CDXC:RemotePairing 2026-10-25 DECISION:
    User (security audit, Q4 "per-device tokens and a tunnel allow-list"): with "Only paired devices can connect" on, Easy Connect admits only the client keys on the allow-list, so a removed phone can no longer even reach this computer. A new phone has no listed key yet and must reach `/api/pairDevice` through the tunnel, so while the Show QR dialog is open the desktop asks to hold pairing open and the allow-list is lifted; it returns 60 seconds after the last ask, by which time the new phone's key is on the list. Lifting and restoring the list restarts the helper, which drops paired phones' tunnels once each way; they reconnect by themselves.
    SEE-ALSO: server/src/remote_access/http.rs (`/api/remotePairingCode` `openTunnel`), apps/desktop/src/app/window/settings_modal/tabs/remote/paired_devices.rs (the switch).
    */
    pub(crate) fn hold_pairing_window_open(
        &self,
        paths: &GxserverPaths,
        state: &TailcatState,
        hold: Duration,
    ) {
        if !state.enabled || state.allowed_client_keys.is_empty() {
            return;
        }
        let was_open = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let now = Instant::now();
            let was_open = inner.pairing_open_until.is_some_and(|until| until > now);
            inner.pairing_open_until = Some(now + hold);
            was_open
        };
        if was_open {
            return;
        }
        self.apply(paths, state);
        let runtime = self.clone();
        let paths = paths.clone();
        thread::spawn(move || runtime.close_pairing_window_when_due(&paths));
    }

    fn pairing_window_open(&self) -> bool {
        self.inner.lock().ok().is_some_and(|inner| {
            inner
                .pairing_open_until
                .is_some_and(|until| until > Instant::now())
        })
    }

    /// Sleeps until the pairing window's latest deadline, then restores the allow-list.
    fn close_pairing_window_when_due(&self, paths: &GxserverPaths) {
        loop {
            let until = {
                let Ok(mut inner) = self.inner.lock() else {
                    return;
                };
                let Some(until) = inner.pairing_open_until else {
                    return;
                };
                if until <= Instant::now() {
                    inner.pairing_open_until = None;
                    None
                } else {
                    Some(until)
                }
            };
            match until {
                Some(until) => thread::sleep(until.saturating_duration_since(Instant::now())),
                None => break,
            }
        }
        super::status::start_tailcat_from_persisted_state(paths, self);
    }

    /// Stops both helpers.
    pub fn stop(&self) {
        self.stop_lane(TailcatLane::Public);
        self.stop_lane(TailcatLane::Relay);
    }

    fn stop_lane(&self, lane: TailcatLane) {
        let stop_tx = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let state = inner.lane(lane);
            state.generation = state.generation.wrapping_add(1);
            state.spec = None;
            state.running = false;
            state.token = None;
            state.stop_tx.take()
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
        let (generation, logger) = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let logger = inner.logger.clone();
            let state = inner.lane(spec.lane);
            state.generation = state.generation.wrapping_add(1);
            state.spec = Some(spec.clone());
            state.stop_tx = Some(stop_tx);
            state.running = false;
            state.token = None;
            state.last_error = None;
            (state.generation, logger)
        };
        if spec.lane == TailcatLane::Relay {
            let runtime = self.clone();
            let key_file = spec.key_file.clone();
            thread::spawn(move || watch_relay_reachability(runtime, generation, key_file));
        }
        let runtime = self.clone();
        thread::spawn(move || supervise(runtime, generation, spec, stop_rx, logger));
    }

    fn is_current_spec(&self, spec: &TailcatServeSpec) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|mut inner| inner.lane(spec.lane).spec.as_ref() == Some(spec))
    }

    fn is_current_generation(&self, lane: TailcatLane, generation: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|mut inner| inner.lane(lane).generation == generation)
    }

    fn set_last_error(&self, lane: TailcatLane, error: Option<String>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.lane(lane).last_error = error;
        }
    }

    fn set_relay_reachable(&self, generation: u64, host: String, reachable: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.relay.generation != generation {
                return;
            }
            inner.relay_reachable = Some((host, reachable));
        }
    }

    fn mark_running(&self, lane: TailcatLane, generation: u64, running: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            let state = inner.lane(lane);
            if state.generation != generation {
                return;
            }
            state.running = running;
            if !running {
                state.token = None;
            }
        }
    }

    fn record_error(&self, lane: TailcatLane, generation: u64, error: String) {
        if let Ok(mut inner) = self.inner.lock() {
            let state = inner.lane(lane);
            if state.generation != generation {
                return;
            }
            state.running = false;
            state.token = None;
            state.last_error = Some(error);
        }
    }

    fn needs_token(&self, lane: TailcatLane, generation: u64) -> bool {
        self.inner.lock().ok().is_some_and(|mut inner| {
            let state = inner.lane(lane);
            state.generation == generation && state.token.is_none()
        })
    }

    fn set_token(&self, lane: TailcatLane, generation: u64, token: String) {
        if let Ok(mut inner) = self.inner.lock() {
            let state = inner.lane(lane);
            if state.generation != generation {
                return;
            }
            state.token = Some(token);
            state.last_error = None;
        }
    }
}

/// How often a running relay helper's relay is checked (`relay_probe.rs`).
const RELAY_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// Checks the second helper's relay while that helper's generation is current.
fn watch_relay_reachability(runtime: TailcatRuntime, generation: u64, key_file: PathBuf) {
    let Some(node) = super::relay_probe::read_key_relay_node(&key_file) else {
        return;
    };
    let mut last: Option<bool> = None;
    while runtime.is_current_generation(TailcatLane::Relay, generation) {
        let reachable = super::relay_probe::probe_relay(&node);
        if last != Some(reachable) {
            eprintln!(
                "[gxserver] Easy Connect relay {} is {}",
                node.host,
                if reachable {
                    "reachable"
                } else {
                    "not reachable"
                }
            );
            last = Some(reachable);
        }
        runtime.set_relay_reachable(generation, node.host.clone(), reachable);
        let deadline = Instant::now() + RELAY_PROBE_INTERVAL;
        while Instant::now() < deadline {
            if !runtime.is_current_generation(TailcatLane::Relay, generation) {
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
}

fn supervise(
    runtime: TailcatRuntime,
    generation: u64,
    spec: TailcatServeSpec,
    stop_rx: mpsc::Receiver<mpsc::SyncSender<()>>,
    logger: Option<std::sync::Arc<crate::logging::GxserverLogger>>,
) {
    let lane = spec.lane;
    let link_context = super::link_log::LinkLogContext {
        lane: lane.as_str(),
        relay: super::relay_probe::key_relay_label(&spec.key_file),
        logger,
    };
    let mut backoff = INITIAL_BACKOFF;
    loop {
        // The blob file is written by the child on startup; a stale one from
        // the previous run must never be reported as this run's token.
        let _ = fs::remove_file(&spec.addr_file);
        let mut child = match spawn_tailcat_serve(&spec) {
            Ok(child) => child,
            Err(error) => {
                eprintln!(
                    "[gxserver] could not start tailcat serve ({}): {error}",
                    lane.as_str()
                );
                runtime.record_error(
                    lane,
                    generation,
                    "Easy Connect didn't start. Ghostex keeps trying; turn it off and on again if this stays.".to_string(),
                );
                if wait_for_stop(&stop_rx, backoff) {
                    return;
                }
                backoff = next_backoff(backoff);
                continue;
            }
        };
        if let Some(stderr) = child.stderr.take() {
            let context = link_context.clone();
            thread::spawn(move || super::link_log::follow_tailcat_log(stderr, context));
        }
        runtime.mark_running(lane, generation, true);
        let started_at = Instant::now();
        let exit_reason = loop {
            if let Ok(reply) = stop_rx.try_recv() {
                terminate_process_group(&mut child);
                let _ = reply.send(());
                return;
            }
            if runtime.needs_token(lane, generation) {
                if let Some(token) = read_address_blob(&spec.addr_file) {
                    runtime.set_token(lane, generation, token);
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
        runtime.record_error(lane, generation, exit_reason);
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
        // Only the verbose log says when a link starts and when it goes direct (`link_log.rs`).
        .arg("--verbose")
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
        .stderr(Stdio::piped());
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
        use crate::platform::process::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
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
        let _ = crate::platform::process::background_command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.kill();
    let _ = child.wait();
}
