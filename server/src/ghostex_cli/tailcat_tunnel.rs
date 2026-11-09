use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::ghostex_cli::rpc::{CliError, CliResult, GXSERVER_LOCAL_API_HOST};

/*
CDXC:RemotePairing 2026-09-01:
The CLI's ssh transport forwards a port with `ssh -N -L`. The forwarder here
is the tailcat equivalent: one loopback listener on the plan's port, relaying
every accepted connection to the tunnel's one `tailcat forward` client
(CDXC:RemotePairing 2026-11-10 below). That keeps the rest of the RPC layer talking to a
plain `http://127.0.0.1:<port>` base URL exactly like the ssh arm does.
*/

/// How long the accept loop sleeps between non-blocking accepts; this is also
/// the worst-case delay before the listener is released after a teardown.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PIPE_BUFFER_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct TailcatForwardPlan {
    pub address_blob: String,
    pub base_url: String,
    pub local_port: u16,
    pub remote_port: u16,
    /// A key of this computer's own for that computer (`computer_link_keys.rs`); `None` dials with
    /// tailcat's `client-default`, the key it paired with.
    pub client_key: Option<PathBuf>,
}

pub fn create_cli_tailcat_forward_plan(
    profile: &Value,
    local_port: u16,
    remote_port: u16,
) -> CliResult<TailcatForwardPlan> {
    let address_blob = profile
        .get("tailcatToken")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if address_blob.is_empty() {
        let label = profile
            .get("name")
            .or_else(|| profile.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        return Err(CliError::Other(format!(
            "gxserver profile \"{label}\" is missing tailcatToken. Copy the remote's tailcat address from its Ghostex tailcat settings."
        )));
    }
    Ok(TailcatForwardPlan {
        address_blob,
        base_url: format!("http://{GXSERVER_LOCAL_API_HOST}:{local_port}"),
        local_port,
        remote_port,
        client_key: None,
    })
}

/// How long an accepted connection waits for the tunnel's tailcat client to be listening.
const CLIENT_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the supervisor looks at its tailcat client and the stop flag.
const CLIENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CLIENT_RESTART_MIN: Duration = Duration::from_millis(500);
const CLIENT_RESTART_MAX: Duration = Duration::from_secs(10);
/// A tailcat client that ran this long was healthy: its next restart starts the backoff over.
const CLIENT_STABLE_AFTER: Duration = Duration::from_secs(30);

/// Both sockets of one relayed connection, so `stop` can end it.
struct LiveConnection {
    client: TcpStream,
    upstream: TcpStream,
}

type SharedConnections = Arc<Mutex<HashMap<u64, LiveConnection>>>;

/// The tunnel's tailcat client: the loopback port `tailcat forward` listens on while it runs,
/// and its pid, so `stop` can end it at once.
#[derive(Default)]
struct TailcatClientState {
    port: Option<u16>,
    pid: Option<u32>,
}

type SharedTailcatClient = Arc<(Mutex<TailcatClientState>, Condvar)>;

/// Handle for a running loopback forwarder. Dropping it does nothing: the
/// tunnel registry in `rpc` owns the lifetime and calls `stop` from the same
/// idle-kill path that kills ssh tunnel children.
pub struct TailcatTunnel {
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    client: SharedTailcatClient,
}

impl TailcatTunnel {
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        /*
        The tailcat client is killed here, synchronously: it does not exit by
        itself while its tunnel link stays up, and when `stop` runs during
        process exit the supervisor thread never gets scheduled again to reap
        it, which would orphan a live tailcat process. The socket shutdowns
        then unblock both pumps of every in-flight connection.
        */
        {
            let (state, ready) = &*self.client;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(pid) = state.pid.take() {
                kill_child_pid(pid);
            }
            ready.notify_all();
        }
        if let Ok(mut connections) = self.connections.lock() {
            for (_, connection) in connections.drain() {
                let _ = connection.client.shutdown(Shutdown::Both);
                let _ = connection.upstream.shutdown(Shutdown::Both);
            }
        }
    }
}

#[cfg(unix)]
fn kill_child_pid(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_child_pid(_pid: u32) {}

pub fn start_tailcat_tunnel(
    binary_path: &Path,
    plan: &TailcatForwardPlan,
) -> CliResult<TailcatTunnel> {
    let listener = TcpListener::bind((GXSERVER_LOCAL_API_HOST, plan.local_port)).map_err(
        |error| {
            CliError::Connection(format!(
                "Could not bind the tailcat gxserver tunnel to {GXSERVER_LOCAL_API_HOST}:{}: {error}",
                plan.local_port
            ))
        },
    )?;
    // Non-blocking accepts are what let the loop observe the shutdown flag and
    // release the port instead of parking forever inside `accept`.
    listener.set_nonblocking(true)?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let client: SharedTailcatClient = Arc::new((Mutex::default(), Condvar::new()));
    let tunnel = TailcatTunnel {
        shutdown: Arc::clone(&shutdown),
        connections: Arc::clone(&connections),
        client: Arc::clone(&client),
    };
    let spec = TailcatConnectionSpec {
        address_blob: plan.address_blob.clone(),
        binary_path: binary_path.to_path_buf(),
        remote_port: plan.remote_port,
        client_key: plan.client_key.clone(),
    };
    {
        let shutdown = Arc::clone(&shutdown);
        let client = Arc::clone(&client);
        thread::spawn(move || supervise_tailcat_client(spec, shutdown, client));
    }
    thread::spawn(move || accept_loop(listener, shutdown, connections, client));
    Ok(tunnel)
}

/*
CDXC:RemotePairing 2026-11-10 WHY:
GitHub #526: one `tailcat <address> <port>` client per accepted connection, all dialing with tailcat's one `client-default` key, stall each other (a new one starves the older ones, whose connections never close). Each tunnel now runs one long-lived `tailcat forward` client that carries every connection; the key is unchanged, so a remote on any version accepts it as before, and a warm request no longer pays a fresh client's rendezvous.
SEE-ALSO: apps/desktop/src/app/helpers/remote/easy_connect_forward.rs (the desktop's forwarder, the same rule).
*/
#[derive(Clone)]
struct TailcatConnectionSpec {
    address_blob: String,
    binary_path: PathBuf,
    remote_port: u16,
    client_key: Option<PathBuf>,
}

/// Runs the tunnel's `tailcat forward` until the tunnel stops, starting it again (after a
/// backoff) whenever it exits.
fn supervise_tailcat_client(
    spec: TailcatConnectionSpec,
    shutdown: Arc<AtomicBool>,
    client: SharedTailcatClient,
) {
    let mut backoff = CLIENT_RESTART_MIN;
    while !shutdown.load(Ordering::SeqCst) {
        let started = Instant::now();
        run_tailcat_client_once(&spec, &shutdown, &client);
        if started.elapsed() >= CLIENT_STABLE_AFTER {
            backoff = CLIENT_RESTART_MIN;
        }
        let deadline = Instant::now() + backoff;
        while Instant::now() < deadline && !shutdown.load(Ordering::SeqCst) {
            thread::sleep(CLIENT_POLL_INTERVAL);
        }
        backoff = (backoff * 2).min(CLIENT_RESTART_MAX);
    }
}

/// One run of `tailcat forward --bind=127.0.0.1 <address> 0:<port>`; its loopback port is
/// published once it prints it.
fn run_tailcat_client_once(
    spec: &TailcatConnectionSpec,
    shutdown: &Arc<AtomicBool>,
    client: &SharedTailcatClient,
) {
    let mut command = crate::platform::process::background_command(&spec.binary_path);
    if let Some(key) = &spec.client_key {
        command.arg(format!("--key={}", key.display()));
    }
    let child = command
        .arg("forward")
        .arg(format!("--bind={GXSERVER_LOCAL_API_HOST}"))
        .arg(&spec.address_blob)
        .arg(format!("0:{}", spec.remote_port))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return;
    };
    {
        let mut state = client
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // `stop` ran while this client started: it never saw the pid, so it goes now.
        if shutdown.load(Ordering::SeqCst) {
            drop(state);
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        state.pid = Some(child.id());
    }
    // tailcat prints "forwarding 127.0.0.1:<port> -> remote …" once it listens, then its log;
    // the reader keeps draining it so the pipe never fills.
    let reader = child.stderr.take().map(|stderr| {
        let client = Arc::clone(client);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { return };
                if let Some(port) = tailcat_forward_listen_port(&line) {
                    let (state, ready) = &*client;
                    state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .port = Some(port);
                    ready.notify_all();
                }
            }
        })
    });
    loop {
        if shutdown.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            break;
        }
        thread::sleep(CLIENT_POLL_INTERVAL);
    }
    {
        let mut state = client
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.port = None;
        state.pid = None;
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

/// The tailcat client's loopback port, waiting up to `CLIENT_READY_TIMEOUT` for it to listen.
fn wait_for_tailcat_client(client: &SharedTailcatClient, shutdown: &AtomicBool) -> Option<u16> {
    let deadline = Instant::now() + CLIENT_READY_TIMEOUT;
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
    let Ok(upstream) = TcpStream::connect((GXSERVER_LOCAL_API_HOST, port)) else {
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
    if let Ok(mut live) = connections.lock() {
        live.insert(
            id,
            LiveConnection {
                client: registered_client,
                upstream: registered_upstream,
            },
        );
    }
    // `stop` sets the flag before draining the registry, so a connection that
    // registered after the drain sees the flag here and tears itself down.
    if !shutdown.load(Ordering::SeqCst) {
        pump_connection(&stream, &upstream);
    }
    let _ = stream.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
    if let Ok(mut live) = connections.lock() {
        live.remove(&id);
    }
}

/// Relays one connection both ways until either side ends.
fn pump_connection(stream: &TcpStream, upstream: &TcpStream) {
    let (Ok(mut client_reader), Ok(mut upstream_writer)) =
        (stream.try_clone(), upstream.try_clone())
    else {
        return;
    };
    let uploader = thread::spawn(move || {
        copy_stream(&mut client_reader, &mut upstream_writer);
        // Half close: the client is done sending, and the remote side sees the
        // same end-of-request it would have seen on a direct socket.
        let _ = upstream_writer.shutdown(Shutdown::Write);
    });
    let (Ok(mut upstream_reader), Ok(mut downstream)) = (upstream.try_clone(), stream.try_clone())
    else {
        let _ = stream.shutdown(Shutdown::Both);
        let _ = uploader.join();
        return;
    };
    copy_stream(&mut upstream_reader, &mut downstream);
    let _ = downstream.shutdown(Shutdown::Write);
    // Nothing more can travel in either direction; closing both read halves
    // releases the uploader if it is still parked on `read`.
    let _ = downstream.shutdown(Shutdown::Both);
    let _ = upstream_reader.shutdown(Shutdown::Both);
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
