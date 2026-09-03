use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use serde_json::Value;

use crate::ghostex_cli::rpc::{CliError, CliResult, GXSERVER_LOCAL_API_HOST};

/*
CDXC:RemotePairing 2026-09-01:
The CLI's ssh transport forwards a port with `ssh -N -L`. tailcat has no
equivalent listener mode — its client is a one-connection stdio pipe
(`tailcat <address-blob> <port>`) — so the forwarder is owned here: one
loopback listener, and one tailcat child per accepted connection with its
stdio pumped to the socket. That keeps the rest of the RPC layer talking to a
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
    })
}

struct LiveConnection {
    stream: TcpStream,
    child_pid: u32,
}

type SharedConnections = Arc<Mutex<HashMap<u64, LiveConnection>>>;

/// Handle for a running loopback forwarder. Dropping it does nothing: the
/// tunnel registry in `rpc` owns the lifetime and calls `stop` from the same
/// idle-kill path that kills ssh tunnel children.
pub struct TailcatTunnel {
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
}

impl TailcatTunnel {
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        /*
        The children must be killed here, synchronously: a tailcat child does
        not exit on stdin EOF while its idle tunnel link stays up, and when
        `stop` runs during process exit the pump threads never get scheduled
        again to reap it, which orphans a live tailcat process per connection.
        The socket shutdown then unblocks both pump threads of every in-flight
        connection so the owning thread can reap the corpse.
        */
        if let Ok(mut connections) = self.connections.lock() {
            for (_, connection) in connections.drain() {
                kill_child_pid(connection.child_pid);
                let _ = connection.stream.shutdown(Shutdown::Both);
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
    let tunnel = TailcatTunnel {
        shutdown: Arc::clone(&shutdown),
        connections: Arc::clone(&connections),
    };
    let connection = TailcatConnectionSpec {
        address_blob: plan.address_blob.clone(),
        binary_path: binary_path.to_path_buf(),
        remote_port: plan.remote_port,
    };
    thread::spawn(move || accept_loop(listener, shutdown, connections, connection));
    Ok(tunnel)
}

#[derive(Clone)]
struct TailcatConnectionSpec {
    address_blob: String,
    binary_path: PathBuf,
    remote_port: u16,
}

fn accept_loop(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    connections: SharedConnections,
    connection: TailcatConnectionSpec,
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
                let connection = connection.clone();
                thread::spawn(move || {
                    serve_connection(id, stream, shutdown, connections, connection)
                });
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
    connection: TailcatConnectionSpec,
) {
    // Accepted sockets inherit the listener's non-blocking mode on macOS, so
    // the pumps must put this one back into blocking mode before reading.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let child = Command::new(&connection.binary_path)
        .arg(&connection.address_blob)
        .arg(connection.remote_port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
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
    if let Ok(mut live) = connections.lock() {
        live.insert(
            id,
            LiveConnection {
                stream: registered,
                child_pid: child.id(),
            },
        );
    }
    // `stop` sets the flag before draining the registry, so a connection that
    // registered after the drain sees the flag here and tears itself down.
    if shutdown.load(Ordering::SeqCst) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stream.shutdown(Shutdown::Both);
        if let Ok(mut live) = connections.lock() {
            live.remove(&id);
        }
        return;
    }
    pump_connection(stream, child);
    if let Ok(mut live) = connections.lock() {
        live.remove(&id);
    }
}

fn pump_connection(stream: TcpStream, mut child: std::process::Child) {
    let (Some(mut child_stdin), Some(mut child_stdout)) = (child.stdin.take(), child.stdout.take())
    else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let Ok(mut upstream) = stream.try_clone() else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let uploader = thread::spawn(move || {
        copy_stream(&mut upstream, &mut child_stdin);
        // Half close: the client is done sending, so tailcat's stdin closes
        // and the remote side sees the same end-of-request it would have seen
        // on a direct socket.
        drop(child_stdin);
    });
    let mut downstream = stream;
    copy_stream(&mut child_stdout, &mut downstream);
    let _ = downstream.shutdown(Shutdown::Write);
    /*
    Once either direction is done the connection is over, but tailcat does not
    exit on its own while its idle tunnel link stays up, so waiting for a
    natural exit here parks this thread and leaks a live child if the process
    exits first. Kill and reap. When the child already exited the kill is a
    no-op.
    */
    let _ = child.kill();
    let _ = child.wait();
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
