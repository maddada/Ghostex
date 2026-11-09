// Loopback forwarder for Easy Connect remote machines: the desktop-side
// counterpart of `server/src/ghostex_cli/tailcat_tunnel.rs`, adapted to the
// desktop crate (which does not depend on gxserver) and to this app's
// per-machine connection lifecycle.

use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::app::helpers::*;
use crate::*;
use ghostex_paths::tailcat::{
    TAILCAT_MIN_VERSION, bundled_tailcat_binary, managed_tailcat_binary, tailcat_version_supported,
};

/*
CDXC:RemotePairing 2026-09-03:
An Easy Connect machine has no reachable SSH host. Its pairing code carries a
tailcat address blob, so this module runs one loopback listener per machine
and relays every accepted connection to that machine's one tailcat client
(`tailcat forward`, CDXC:RemotePairing 2026-11-10 below). Everything else in `helpers/remote/` then dials a
plain `127.0.0.1:<port>` exactly as it would dial an SSH host: the config
builder reads the port from the registry here, so the SSH tunnel, command
runner, terminal attach, and VS Code launch all go through the same forwarder.

Lifecycle: `gpui_start_easy_connect_forward` is called by the connect flow
before the first SSH command, and `gpui_stop_easy_connect_forward` by the
same paths that terminate the machine's gxserver connection (disconnect,
watchdog, app quit) and by a failed connect. A machine whose forwarder is not
running resolves to an empty SSH host, which is deliberately not dialable.

The binary is the first of: explicit override, the helper Ghostex installs (Settings), the one the Linux packages ship beside their gxserver, PATH, `~/go/bin`, then Homebrew, that is at least `TAILCAT_MIN_VERSION` (`gpui_resolve_easy_connect_binary`).
*/

pub(crate) const GPUI_EASY_CONNECT_FORWARD_HOST: &str = "127.0.0.1";
pub(crate) const GPUI_EASY_CONNECT_DEFAULT_SSH_PORT: u16 = 22;

/// How long the accept loop sleeps between non-blocking accepts; this is also
/// the worst-case delay before the listener is released after a stop.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PIPE_BUFFER_BYTES: usize = 32 * 1024;
/// How long an accepted connection waits for the machine's tailcat client to be listening.
const FORWARD_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the supervisor looks at its tailcat client and the stop flag.
const FORWARD_POLL_INTERVAL: Duration = Duration::from_millis(100);
const FORWARD_RESTART_MIN: Duration = Duration::from_millis(500);
const FORWARD_RESTART_MAX: Duration = Duration::from_secs(10);
/// A tailcat client that ran this long was healthy: its next restart starts the backoff over.
const FORWARD_STABLE_AFTER: Duration = Duration::from_secs(30);
/// The forwarded port is SSH, whose server speaks first: a fresh connection that hears nothing
/// this long went through a tailcat client that carries nothing, which is started again.
const FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(20);

/// Both sockets of one relayed connection, so `stop` can end it.
struct LiveConnection {
    client: TcpStream,
    upstream: TcpStream,
}

type SharedConnections = Arc<Mutex<HashMap<u64, LiveConnection>>>;

/// The machine's tailcat client: the loopback port `tailcat forward` listens on while it runs,
/// and a restart a connection asked for.
#[derive(Default)]
struct TailcatClientState {
    port: Option<u16>,
    restart: bool,
}

type SharedTailcatClient = Arc<(Mutex<TailcatClientState>, Condvar)>;

struct EasyConnectForward {
    local_port: u16,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    client: SharedTailcatClient,
    accept_thread: JoinHandle<()>,
    supervisor_thread: JoinHandle<()>,
}

impl EasyConnectForward {
    fn stop(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.client.1.notify_all();
        // Unblocks both pumps of every in-flight connection so their threads can finish.
        if let Ok(mut connections) = self.connections.lock() {
            for (_, connection) in connections.drain() {
                let _ = connection.client.shutdown(Shutdown::Both);
                let _ = connection.upstream.shutdown(Shutdown::Both);
            }
        }
        /*
        The listener is owned by the accept thread and only closes when that
        thread returns (within one ACCEPT_POLL_INTERVAL of the flag). Joining
        here makes the port free by the time `stop` returns, so an immediate
        reconnect can bind the same preferred port instead of moving to a new
        one and stranding attach terminals that poll the old one. The
        supervisor kills the tailcat client within one FORWARD_POLL_INTERVAL,
        so none outlives the app (a tailcat client does not exit by itself
        while its tunnel link stays up).
        */
        let _ = self.accept_thread.join();
        let _ = self.supervisor_thread.join();
    }
}

struct EasyConnectForwardRegistry {
    running: HashMap<String, EasyConnectForward>,
    /// Port each machine last forwarded on, so a reconnect reuses it when it
    /// is still free and long-lived attach terminals keep a valid endpoint.
    last_ports: HashMap<String, u16>,
}

static EASY_CONNECT_FORWARDS: Mutex<Option<EasyConnectForwardRegistry>> = Mutex::new(None);

fn with_registry<T>(apply: impl FnOnce(&mut EasyConnectForwardRegistry) -> T) -> T {
    let mut guard = EASY_CONNECT_FORWARDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let registry = guard.get_or_insert_with(|| EasyConnectForwardRegistry {
        running: HashMap::new(),
        last_ports: HashMap::new(),
    });
    apply(registry)
}

/// The loopback port currently forwarding to the machine, if its forwarder is running.
pub(crate) fn gpui_easy_connect_forward_port(remote_machine_id: &str) -> Option<u16> {
    with_registry(|registry| {
        registry
            .running
            .get(remote_machine_id)
            .map(|forward| forward.local_port)
    })
}

/// Starts (or reuses) the machine's forwarder and returns the loopback port to dial.
pub(crate) fn gpui_start_easy_connect_forward(
    remote_machine_id: &str,
    address_blob: &str,
    remote_ssh_port: u16,
) -> Result<u16, String> {
    if let Some(port) = gpui_easy_connect_forward_port(remote_machine_id) {
        return Ok(port);
    }
    let address_blob = address_blob.trim();
    if address_blob.is_empty() || address_blob.chars().any(char::is_whitespace) {
        return Err("The saved Easy Connect address is invalid.".to_string());
    }
    let binary_path = match gpui_resolve_easy_connect_binary() {
        EasyConnectBinary::Ready(path) => path,
        EasyConnectBinary::TooOld(path, version) => {
            request_easy_connect_install();
            return Err(format!(
                "Easy Connect's helper on this computer ({version} at {}) is older than {TAILCAT_MIN_VERSION} and can't reach this machine. Ghostex is installing the current one (Settings → Remote → Easy Connect shows its progress); reconnect when it finishes.",
                path.display()
            ));
        }
        EasyConnectBinary::Missing => {
            request_easy_connect_install();
            return Err(
                "Easy Connect is not installed on this computer. Ghostex is installing it now (Settings → Remote → Easy Connect shows its progress); reconnect when it finishes."
                    .to_string(),
            );
        }
    };
    let preferred_port =
        with_registry(|registry| registry.last_ports.get(remote_machine_id).copied());
    let listener = gpui_bind_easy_connect_listener(preferred_port)?;
    let local_port = listener
        .local_addr()
        .map_err(|_| "Could not read the Easy Connect forwarder port.".to_string())?
        .port();
    // Non-blocking accepts are what let the loop observe the stop flag and
    // release the port instead of parking forever inside `accept`.
    listener
        .set_nonblocking(true)
        .map_err(|_| "Could not start the Easy Connect forwarder.".to_string())?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let client: SharedTailcatClient = Arc::new((Mutex::default(), Condvar::new()));
    let spec = EasyConnectConnectionSpec {
        address_blob: address_blob.to_string(),
        binary_path,
        remote_port: remote_ssh_port,
    };
    let supervisor_thread = {
        let shutdown = Arc::clone(&shutdown);
        let client = Arc::clone(&client);
        thread::spawn(move || supervise_tailcat_client(spec, shutdown, client))
    };
    let accept_thread = {
        let shutdown = Arc::clone(&shutdown);
        let connections = Arc::clone(&connections);
        let client = Arc::clone(&client);
        thread::spawn(move || accept_loop(listener, shutdown, connections, client))
    };
    let forward = EasyConnectForward {
        local_port,
        shutdown,
        connections,
        client,
        accept_thread,
        supervisor_thread,
    };
    with_registry(|registry| {
        registry
            .last_ports
            .insert(remote_machine_id.to_string(), local_port);
        registry
            .running
            .insert(remote_machine_id.to_string(), forward);
    });
    Ok(local_port)
}

/// Stops the machine's forwarder and its tailcat client.
pub(crate) fn gpui_stop_easy_connect_forward(remote_machine_id: &str) {
    if let Some(forward) = with_registry(|registry| registry.running.remove(remote_machine_id)) {
        forward.stop();
    }
}

pub(crate) fn gpui_stop_all_easy_connect_forwards() {
    let forwards: Vec<EasyConnectForward> = with_registry(|registry| {
        registry
            .running
            .drain()
            .map(|(_, forward)| forward)
            .collect()
    });
    for forward in forwards {
        forward.stop();
    }
}

/// Rewrites an Easy Connect machine's config so its SSH endpoint is the running
/// forwarder. SSH machines pass through untouched.
pub(crate) fn gpui_remote_machine_config_with_easy_connect_forward(
    mut config: GpuiRemoteMachineConfig,
) -> Result<GpuiRemoteMachineConfig, String> {
    if !config.uses_easy_connect() {
        return Ok(config);
    }
    let Some(address) = config.easy_connect_address.clone() else {
        return Err("The saved machine is missing its Easy Connect address.".to_string());
    };
    let local_port = gpui_start_easy_connect_forward(
        config.remote_machine_id.as_str(),
        address.as_str(),
        config.easy_connect_ssh_port,
    )?;
    config.ssh_host = GPUI_EASY_CONNECT_FORWARD_HOST.to_string();
    config.ssh_port = Some(local_port);
    Ok(config)
}

fn gpui_bind_easy_connect_listener(preferred_port: Option<u16>) -> Result<TcpListener, String> {
    if let Some(port) = preferred_port
        && let Ok(listener) = TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, port))
    {
        return Ok(listener);
    }
    TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, 0))
        .map_err(|error| format!("Could not bind the Easy Connect forwarder on loopback: {error}"))
}

// ---------------------------------------------------------------------------
// Binary resolution: a tailcat new enough to dial current servers.
// ---------------------------------------------------------------------------

/// What the lookup found: a helper to dial with, an older one that cannot complete a handshake
/// with a current server (its path and version), or none.
pub(crate) enum EasyConnectBinary {
    Ready(PathBuf),
    TooOld(PathBuf, String),
    Missing,
}

/// The first candidate whose `tailcat version` is at least `TAILCAT_MIN_VERSION`.
///
/// CDXC:RemotePairing 2026-11-02 WHY: Ghostex's own install comes first and an older helper
/// anywhere (a `go install` in `~/go/bin`, Homebrew) is skipped, never replaced: a v0.4.0 client
/// pings a v0.6.0 Cloud Box but never completes its handshake (`ghostex_paths::tailcat`).
pub(crate) fn gpui_resolve_easy_connect_binary() -> EasyConnectBinary {
    let mut too_old: Option<(PathBuf, String)> = None;
    for candidate in easy_connect_binary_candidates() {
        if !is_executable_file(&candidate) {
            continue;
        }
        match easy_connect_binary_version(&candidate) {
            Some(version) if tailcat_version_supported(&version) => {
                return EasyConnectBinary::Ready(candidate);
            }
            version => {
                too_old.get_or_insert((candidate, version.unwrap_or_else(|| "unknown".into())));
            }
        }
    }
    match too_old {
        Some((path, version)) => EasyConnectBinary::TooOld(path, version),
        None => EasyConnectBinary::Missing,
    }
}

fn easy_connect_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = env::var_os("GHOSTEX_TAILCAT_BIN")
        && !value.is_empty()
    {
        candidates.push(PathBuf::from(value));
    }
    // CDXC:RemotePairing 2026-09-05 SEE-ALSO: server/src/tailcat/binary.rs installs the Settings helper at this shared per-user location.
    candidates.push(managed_tailcat_binary(
        &ghostex_paths::GhostexPaths::resolve().data_dir,
    ));
    // CDXC:CloudBoxes 2026-11-02 WHY: the Linux packages ship tailcat in the bundled gxserver
    // package (package-remote-linux.mjs), so a Linux desktop dials boxes without installing one.
    if let Some(bundled) =
        gpui_resolve_local_gxserver_binary().and_then(|gxserver| bundled_tailcat_binary(&gxserver))
    {
        candidates.push(bundled);
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(
            env::split_paths(&path).map(|directory| directory.join(easy_connect_executable_name())),
        );
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(
            home.join("go")
                .join("bin")
                .join(easy_connect_executable_name()),
        );
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(easy_connect_executable_name()));
    candidates
}

/// Versions already read, by path, with the file's size and change time when they were read.
type VersionCache = HashMap<PathBuf, (u64, Option<std::time::SystemTime>, Option<String>)>;
static VERSIONS: Mutex<Option<VersionCache>> = Mutex::new(None);
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// `tailcat version`'s first line, read once per file (and again when the file changes).
fn easy_connect_binary_version(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let stamp = (metadata.len(), metadata.modified().ok());
    if let Some((len, modified, version)) = VERSIONS
        .lock()
        .ok()
        .and_then(|cache| cache.as_ref()?.get(path).cloned())
        && (len, modified) == stamp
    {
        return version;
    }
    let version = read_easy_connect_binary_version(path);
    if let Ok(mut cache) = VERSIONS.lock() {
        cache
            .get_or_insert_with(HashMap::new)
            .insert(path.to_path_buf(), (stamp.0, stamp.1, version.clone()));
    }
    version
}

fn read_easy_connect_binary_version(path: &Path) -> Option<String> {
    let mut child = gpui_background_command(path)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    while child.try_wait().ok()?.is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().ok()?;
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    (!version.is_empty()).then_some(version)
}

/// Asks this computer's gxserver, once per app run, to install the current helper (the job
/// Settings › Remote › Easy Connect › Install runs), so a missing or old helper heals itself.
fn request_easy_connect_install() {
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    if REQUESTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        if gpui_gxserver_rpc_result(
            "/api/installTailcat",
            &serde_json::json!({}),
            Duration::from_secs(30),
        )
        .is_err()
        {
            // Asked again on the next connect.
            REQUESTED.store(false, Ordering::SeqCst);
        }
    });
}

fn easy_connect_executable_name() -> &'static str {
    if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

// ---------------------------------------------------------------------------
// The machine's one tailcat client, and the relay from each accepted connection.
// ---------------------------------------------------------------------------

/*
CDXC:RemotePairing 2026-11-10 WHY:
GitHub #526: a MacBook's Easy Connect remote machine kept dropping and its terminals reattaching. This forwarder spawned one `tailcat <address> <port>` client per accepted connection (the gxserver tunnel, every terminal attach, every command), and since computer pairing (11.1.0) every one of them dials with tailcat's one `client-default` key. Two tailcat clients with one key to one server stall each other: when a new one starts the older one stops receiving and its connection is never closed (#526's reproduction, and measured again on 2026-11-10 on Ghostex's relay). Each machine now has one long-lived `tailcat forward` client on a loopback port of its own that carries every connection, so a machine's connections never compete for its key. The key stays `client-default`, the one every computer already has on its allow-list, so a remote on any version accepts it exactly as before.
SEE-ALSO: server/src/ghostex_cli/tailcat_tunnel.rs (gxserver's own tunnels to connected computers follow the same rule).
*/
#[derive(Clone)]
struct EasyConnectConnectionSpec {
    address_blob: String,
    binary_path: PathBuf,
    remote_port: u16,
}

/// Runs the machine's `tailcat forward` until the forwarder stops, starting it again (after a
/// backoff) whenever it exits or a connection found it carrying nothing.
fn supervise_tailcat_client(
    spec: EasyConnectConnectionSpec,
    shutdown: Arc<AtomicBool>,
    client: SharedTailcatClient,
) {
    let mut backoff = FORWARD_RESTART_MIN;
    while !shutdown.load(Ordering::SeqCst) {
        let started = Instant::now();
        run_tailcat_client_once(&spec, &shutdown, &client);
        set_tailcat_client_port(&client, None);
        if started.elapsed() >= FORWARD_STABLE_AFTER {
            backoff = FORWARD_RESTART_MIN;
        }
        let deadline = Instant::now() + backoff;
        while Instant::now() < deadline && !shutdown.load(Ordering::SeqCst) {
            thread::sleep(FORWARD_POLL_INTERVAL);
        }
        backoff = (backoff * 2).min(FORWARD_RESTART_MAX);
    }
}

/// One run of `tailcat forward --bind=127.0.0.1 <address> 0:<port>`: its loopback port is
/// published once it prints it, and the run ends when it exits, the forwarder stops, or a
/// connection asks for a restart.
/// How long the forwarder waits for gxserver to name its key for a machine: the first ask registers
/// the keys there, which takes a tunnel's rendezvous and an answer, for each port it tries.
const LINK_KEY_TIMEOUT: Duration = Duration::from_secs(40);

/// This computer's own Easy Connect key for the machine at `address`, or `None` for `client-default`.
///
/// CDXC:RemotePairing 2026-11-10 WHY: the forwarder and gxserver's own tunnels to one machine,
/// and the forwarders to two machines on one relay, stalled each other while all dialed with
/// `client-default` (GitHub #526). gxserver, which holds the pairing device key, registers keys of
/// this computer's own with each machine (server/src/remote_access/computer_link_keys.rs) and names
/// the app's here; a machine whose Ghostex can't take them, or a gxserver that doesn't answer,
/// leaves the forwarder on `client-default` as before.
fn easy_connect_link_key(address: &str) -> Option<PathBuf> {
    let params = serde_json::json!({ "address": address, "purpose": "app" });
    let (status, body) =
        gxserver_post_typed_operation("/api/easyConnectLinkKey", &params, LINK_KEY_TIMEOUT).ok()?;
    if status != 200 {
        return None;
    }
    let answer: serde_json::Value = serde_json::from_str(&body).ok()?;
    let path = PathBuf::from(answer.pointer("/result/keyPath")?.as_str()?);
    path.is_file().then_some(path)
}

fn run_tailcat_client_once(
    spec: &EasyConnectConnectionSpec,
    shutdown: &Arc<AtomicBool>,
    client: &SharedTailcatClient,
) {
    let mut command = gpui_background_command(&spec.binary_path);
    if let Some(key) = easy_connect_link_key(&spec.address_blob) {
        command.arg(format!("--key={}", key.display()));
    }
    let child = command
        .arg("forward")
        .arg(format!("--bind={GPUI_EASY_CONNECT_FORWARD_HOST}"))
        .arg(&spec.address_blob)
        .arg(format!("0:{}", spec.remote_port))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return;
    };
    // tailcat prints "forwarding 127.0.0.1:<port> -> remote …" once it listens, then its log;
    // the reader keeps draining it so the pipe never fills.
    let reader = child.stderr.take().map(|stderr| {
        let client = Arc::clone(client);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { return };
                if let Some(port) = tailcat_forward_listen_port(&line) {
                    set_tailcat_client_port(&client, Some(port));
                }
            }
        })
    });
    loop {
        if shutdown.load(Ordering::SeqCst) || take_tailcat_client_restart(client) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            break;
        }
        thread::sleep(FORWARD_POLL_INTERVAL);
    }
    if let Some(reader) = reader {
        let _ = reader.join();
    }
}

/// The loopback port in tailcat forward's "forwarding 127.0.0.1:<port> -> …" line.
fn tailcat_forward_listen_port(line: &str) -> Option<u16> {
    let rest = line.split("forwarding ").nth(1)?;
    let address = rest.split_whitespace().next()?;
    address.rsplit(':').next()?.parse().ok()
}

fn set_tailcat_client_port(client: &SharedTailcatClient, port: Option<u16>) {
    let (state, ready) = &**client;
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.port = port;
    ready.notify_all();
}

fn take_tailcat_client_restart(client: &SharedTailcatClient) -> bool {
    let mut state = client
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut state.restart)
}

fn request_tailcat_client_restart(client: &SharedTailcatClient) {
    let mut state = client
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.restart = true;
}

/// The tailcat client's loopback port, waiting up to `FORWARD_READY_TIMEOUT` for it to listen.
fn wait_for_tailcat_client(client: &SharedTailcatClient, shutdown: &AtomicBool) -> Option<u16> {
    let deadline = Instant::now() + FORWARD_READY_TIMEOUT;
    let (state, ready) = &**client;
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return None;
        }
        if let Some(port) = state.port {
            return Some(port);
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let wait = (deadline - now).min(Duration::from_millis(200));
        state = ready
            .wait_timeout(state, wait)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
    }
}

fn accept_loop(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    client: SharedTailcatClient,
) {
    let mut next_id: u64 = 0;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let id = next_id;
                next_id = next_id.wrapping_add(1);
                let shutdown = Arc::clone(&shutdown);
                let connections = Arc::clone(&connections);
                let client = Arc::clone(&client);
                thread::spawn(move || serve_connection(id, stream, shutdown, connections, client));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn serve_connection(
    id: u64,
    stream: TcpStream,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    client: SharedTailcatClient,
) {
    // Accepted sockets inherit the listener's non-blocking mode on macOS, so
    // the pumps must put this one back into blocking mode before reading.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let Some(port) = wait_for_tailcat_client(&client, &shutdown) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let Ok(upstream) = TcpStream::connect((GPUI_EASY_CONNECT_FORWARD_HOST, port)) else {
        // The client went away between printing its port and this connect; the supervisor starts
        // it again.
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let (Ok(registered_client), Ok(registered_upstream)) =
        (stream.try_clone(), upstream.try_clone())
    else {
        let _ = stream.shutdown(Shutdown::Both);
        let _ = upstream.shutdown(Shutdown::Both);
        return;
    };
    connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            id,
            LiveConnection {
                client: registered_client,
                upstream: registered_upstream,
            },
        );
    // `stop` sets the flag before draining the registry, so a connection that
    // registered after the drain sees the flag here and tears itself down.
    if shutdown.load(Ordering::SeqCst) {
        reap_connection(&connections, id);
        return;
    }
    if !pump_connection(stream, upstream) {
        request_tailcat_client_restart(&client);
    }
    reap_connection(&connections, id);
}

fn reap_connection(connections: &SharedConnections, id: u64) {
    let removed = connections
        .lock()
        .ok()
        .and_then(|mut live| live.remove(&id));
    if let Some(connection) = removed {
        let _ = connection.client.shutdown(Shutdown::Both);
        let _ = connection.upstream.shutdown(Shutdown::Both);
    }
}

/// Relays one connection both ways until either side ends. Returns false when the remote said
/// nothing within `FIRST_BYTE_TIMEOUT` of a fresh connection (a tailcat client carrying nothing).
fn pump_connection(stream: TcpStream, upstream: TcpStream) -> bool {
    let (Ok(mut client_reader), Ok(mut upstream_writer)) =
        (stream.try_clone(), upstream.try_clone())
    else {
        let _ = stream.shutdown(Shutdown::Both);
        let _ = upstream.shutdown(Shutdown::Both);
        return true;
    };
    let uploader = thread::spawn(move || {
        copy_stream(&mut client_reader, &mut upstream_writer);
        // Half close: the client is done sending, and the far side sees the
        // same end-of-request it would have seen on a direct socket.
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });
    let mut upstream_reader = upstream;
    let mut downstream = stream;
    let carried = relay_first_bytes(&mut upstream_reader, &mut downstream);
    if carried {
        copy_stream(&mut upstream_reader, &mut downstream);
    }
    let _ = downstream.shutdown(Shutdown::Write);
    // Nothing more can travel in either direction; closing both read halves
    // releases the uploader if it is still parked on `read`.
    let _ = downstream.shutdown(Shutdown::Both);
    let _ = upstream_reader.shutdown(Shutdown::Both);
    let _ = uploader.join();
    carried
}

/// Waits up to `FIRST_BYTE_TIMEOUT` for the remote's first bytes and relays them. False when
/// none came in time; true otherwise, including a remote that closed at once (what the tailcat
/// client does for a machine it cannot reach, which is no reason to start it again).
fn relay_first_bytes(upstream: &mut TcpStream, downstream: &mut TcpStream) -> bool {
    if upstream.set_read_timeout(Some(FIRST_BYTE_TIMEOUT)).is_err() {
        return true;
    }
    let mut buffer = vec![0u8; PIPE_BUFFER_BYTES];
    let carried = loop {
        match upstream.read(&mut buffer) {
            Ok(0) => break true,
            Ok(count) => {
                let _ = downstream.write_all(&buffer[..count]);
                break true;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break false;
            }
            Err(_) => break true,
        }
    };
    let _ = upstream.set_read_timeout(None);
    carried
}

fn copy_stream(reader: &mut impl Read, writer: &mut impl Write) {
    let mut buffer = vec![0u8; PIPE_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                if writer.write_all(&buffer[..count]).is_err() {
                    return;
                }
                if writer.flush().is_err() {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}
