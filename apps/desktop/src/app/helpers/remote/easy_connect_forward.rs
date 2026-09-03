// Loopback forwarder for Easy Connect remote machines: the desktop-side
// counterpart of `server/src/ghostex_cli/tailcat_tunnel.rs`, adapted to the
// desktop crate (which does not depend on gxserver) and to this app's
// per-machine connection lifecycle.

use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

/*
CDXC:RemotePairing 2026-09-03:
An Easy Connect machine has no reachable SSH host. Its pairing code carries a
tailcat address blob, and the tailcat client is a one-connection stdio pipe
(`tailcat <address-blob> <port>`), so this module runs one loopback listener
per machine and spawns one tailcat child per accepted connection with its
stdio pumped to the socket. Everything else in `helpers/remote/` then dials a
plain `127.0.0.1:<port>` exactly as it would dial an SSH host: the config
builder reads the port from the registry here, so the SSH tunnel, command
runner, terminal attach, and VS Code launch all go through the same forwarder.

Lifecycle: `gpui_start_easy_connect_forward` is called by the connect flow
before the first SSH command, and `gpui_stop_easy_connect_forward` by the
same paths that terminate the machine's gxserver connection (disconnect,
watchdog, app quit) and by a failed connect. A machine whose forwarder is not
running resolves to an empty SSH host, which is deliberately not dialable.

The binary is never vendored or downloaded: it is located in the same order
as `server/src/tailcat/binary.rs` (explicit override, PATH, `~/go/bin`,
Homebrew).
*/

pub(crate) const GPUI_EASY_CONNECT_FORWARD_HOST: &str = "127.0.0.1";
pub(crate) const GPUI_EASY_CONNECT_DEFAULT_SSH_PORT: u16 = 22;

/// How long the accept loop sleeps between non-blocking accepts; this is also
/// the worst-case delay before the listener is released after a stop.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PIPE_BUFFER_BYTES: usize = 32 * 1024;

struct LiveConnection {
    stream: TcpStream,
    child: Child,
}

type SharedConnections = Arc<Mutex<HashMap<u64, LiveConnection>>>;

struct EasyConnectForward {
    local_port: u16,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    accept_thread: JoinHandle<()>,
}

impl EasyConnectForward {
    fn stop(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        /*
        The children are killed here, synchronously: a tailcat child does not
        exit on stdin EOF while its idle tunnel link stays up, and when this
        runs during app quit the pump threads never get scheduled again to
        reap it. The socket shutdown unblocks both pumps of every in-flight
        connection so their threads can finish.
        */
        if let Ok(mut connections) = self.connections.lock() {
            for (_, mut connection) in connections.drain() {
                let _ = connection.child.kill();
                let _ = connection.child.wait();
                let _ = connection.stream.shutdown(Shutdown::Both);
            }
        }
        /*
        The listener is owned by the accept thread and only closes when that
        thread returns (within one ACCEPT_POLL_INTERVAL of the flag). Joining
        here makes the port free by the time `stop` returns, so an immediate
        reconnect can bind the same preferred port instead of moving to a new
        one and stranding attach terminals that poll the old one.
        */
        let _ = self.accept_thread.join();
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
    let Some(binary_path) = gpui_resolve_easy_connect_binary() else {
        return Err(
            "Easy Connect is not installed on this computer. Install it, then reconnect the machine."
                .to_string(),
        );
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
    let spec = EasyConnectConnectionSpec {
        address_blob: address_blob.to_string(),
        binary_path,
        remote_port: remote_ssh_port,
    };
    let accept_thread = {
        let shutdown = Arc::clone(&shutdown);
        let connections = Arc::clone(&connections);
        thread::spawn(move || accept_loop(listener, shutdown, connections, spec))
    };
    let forward = EasyConnectForward {
        local_port,
        shutdown,
        connections,
        accept_thread,
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

/// Stops the machine's forwarder and every tailcat child it spawned.
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
    if let Some(port) = preferred_port {
        if let Ok(listener) = TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, port)) {
            return Ok(listener);
        }
    }
    TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, 0))
        .map_err(|error| format!("Could not bind the Easy Connect forwarder on loopback: {error}"))
}

// ---------------------------------------------------------------------------
// Binary resolution — same order as server/src/tailcat/binary.rs.
// ---------------------------------------------------------------------------

pub(crate) fn gpui_resolve_easy_connect_binary() -> Option<PathBuf> {
    easy_connect_binary_candidates()
        .into_iter()
        .find(|candidate| is_executable_file(candidate))
}

fn easy_connect_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = env::var_os("GHOSTEX_TAILCAT_BIN") {
        if !value.is_empty() {
            candidates.push(PathBuf::from(value));
        }
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
// Accept loop and per-connection pumps.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct EasyConnectConnectionSpec {
    address_blob: String,
    binary_path: PathBuf,
    remote_port: u16,
}

fn accept_loop(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    spec: EasyConnectConnectionSpec,
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
                let spec = spec.clone();
                thread::spawn(move || serve_connection(id, stream, shutdown, connections, spec));
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
    spec: EasyConnectConnectionSpec,
) {
    // Accepted sockets inherit the listener's non-blocking mode on macOS, so
    // the pumps must put this one back into blocking mode before reading.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let child = Command::new(&spec.binary_path)
        .arg(&spec.address_blob)
        .arg(spec.remote_port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let (Some(child_stdin), Some(child_stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let registered = match stream.try_clone() {
        Ok(registered) => registered,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stream.shutdown(Shutdown::Both);
            return;
        }
    };
    // Register even through a poisoned lock: an unregistered child would
    // outlive `stop`, which only kills what the registry knows about.
    connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            id,
            LiveConnection {
                stream: registered,
                child,
            },
        );
    // `stop` sets the flag before draining the registry, so a connection that
    // registered after the drain sees the flag here and tears itself down.
    if shutdown.load(Ordering::SeqCst) {
        reap_connection(&connections, id);
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    pump_connection(stream, child_stdin, child_stdout);
    reap_connection(&connections, id);
}

fn reap_connection(connections: &SharedConnections, id: u64) {
    /*
    Once either direction is done the connection is over, but tailcat does not
    exit on its own while its idle tunnel link stays up, so waiting for a
    natural exit would leak a live child. Kill and reap; when `stop` already
    drained this entry the child is gone and there is nothing to do.
    */
    let removed = connections
        .lock()
        .ok()
        .and_then(|mut live| live.remove(&id));
    if let Some(mut connection) = removed {
        let _ = connection.child.kill();
        let _ = connection.child.wait();
        let _ = connection.stream.shutdown(Shutdown::Both);
    }
}

fn pump_connection(
    stream: TcpStream,
    mut child_stdin: std::process::ChildStdin,
    mut child_stdout: std::process::ChildStdout,
) {
    let Ok(mut upstream) = stream.try_clone() else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let uploader = thread::spawn(move || {
        copy_stream(&mut upstream, &mut child_stdin);
        // Half close: the client is done sending, so tailcat's stdin closes
        // and the far side sees the same end-of-request it would have seen
        // on a direct socket.
        drop(child_stdin);
    });
    let mut downstream = stream;
    copy_stream(&mut child_stdout, &mut downstream);
    let _ = downstream.shutdown(Shutdown::Write);
    // Nothing more can travel in either direction; closing the read half
    // releases the uploader if it is still parked on `read`.
    let _ = downstream.shutdown(Shutdown::Both);
    let _ = uploader.join();
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
