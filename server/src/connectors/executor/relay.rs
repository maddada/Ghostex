//! The one `executor mcp` this gxserver runs for its agents: the CLI's stdio relay to the signed-in
//! host's MCP endpoint, which renews the sign-in itself. Ghostex speaks JSON-RPC to it over its
//! stdin and stdout, several requests at once, and stops it after 15 minutes unused, when the
//! sign-in changes and when gxserver exits.
//!
//! CDXC:Connectors 2026-11-27 DECISION:
//! User (Executor CLI page, PLACEHOLDER-2): agents reach the Executor CLI only through Ghostex, as the `executor_*` tools of `ghostex-connectors`, so Ghostex's cards and Ask / Allow / Never rules keep answering Executor's approvals; agents are not given `executor mcp` themselves. The relay is started with the signed-in host named (`--host`), because without a sign-in `executor mcp` falls back to a local background Executor, and the user chose that agents get nothing until the CLI is signed in (PLACEHOLDER-5).

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::cli;

const IDLE_STOP_AFTER: Duration = Duration::from_secs(15 * 60);
const IDLE_CHECK_EVERY: Duration = Duration::from_secs(60);
const INITIALIZE_WITHIN: Duration = Duration::from_secs(60);
const PROTOCOL_VERSION: &str = "2025-06-18";
/// The relay's last stderr lines kept for an error message.
const STDERR_LINES: usize = 3;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Relay {
    host: String,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    last_used: Mutex<Instant>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

static RELAY: Mutex<Option<Arc<Relay>>> = Mutex::new(None);
static START: Mutex<()> = Mutex::new(());

impl Relay {
    fn write(&self, message: &Value) -> Result<(), String> {
        let mut stdin = lock(&self.stdin);
        writeln!(stdin, "{message}")
            .and_then(|_| stdin.flush())
            .map_err(|_| self.gone())
    }

    fn gone(&self) -> String {
        let tail: Vec<String> = lock(&self.stderr).iter().cloned().collect();
        if tail.is_empty() {
            "The Executor CLI's MCP relay stopped.".to_string()
        } else {
            format!("The Executor CLI's MCP relay stopped: {}", tail.join(" | "))
        }
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        *lock(&self.last_used) = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, answer) = mpsc::channel();
        lock(&self.pending).insert(id, sender);
        let sent =
            self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let reply = sent.and_then(|_| {
            answer.recv_timeout(timeout).map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => format!(
                    "Executor didn't answer within {} seconds.",
                    timeout.as_secs()
                ),
                mpsc::RecvTimeoutError::Disconnected => self.gone(),
            })
        });
        lock(&self.pending).remove(&id);
        *lock(&self.last_used) = Instant::now();
        let reply = reply?;
        if let Some(error) = reply.get("error") {
            return Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Executor refused the request.")
                .to_string());
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn stop(&self) {
        self.alive.store(false, Ordering::Relaxed);
        let mut child = lock(&self.child);
        #[cfg(unix)]
        // SAFETY: signals the relay's own process group, which `start` created for it.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(unix)]
        // SAFETY: as above.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Answers a request the relay sends Ghostex: `ping`, and a refusal for anything else (Ghostex
/// offers no roots, sampling or elicitation; in model mode approvals come back as results).
fn answer_server_request(relay_stdin: &Mutex<ChildStdin>, message: &Value) {
    let Some(id) = message.get("id").cloned() else {
        return;
    };
    let reply = if message.get("method").and_then(Value::as_str) == Some("ping") {
        json!({ "jsonrpc": "2.0", "id": id, "result": {} })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Not supported by Ghostex" } })
    };
    let mut stdin = lock(relay_stdin);
    let _ = writeln!(stdin, "{reply}").and_then(|_| stdin.flush());
}

fn start(executable: &Path, host: &str) -> Result<Arc<Relay>, String> {
    let mut command = cli::command(executable, Some(host));
    command
        .args(["mcp", "--host", host])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Couldn't start the Executor CLI's MCP relay: {error}"))?;
    let stdin = child.stdin.take().ok_or("The MCP relay has no input.")?;
    let stdout = child.stdout.take().ok_or("The MCP relay has no output.")?;
    let stderr = child.stderr.take();
    let relay = Arc::new(Relay {
        host: host.to_string(),
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        pending: Arc::new(Mutex::new(HashMap::new())),
        next_id: AtomicU64::new(1),
        alive: Arc::new(AtomicBool::new(true)),
        last_used: Mutex::new(Instant::now()),
        stderr: Arc::new(Mutex::new(VecDeque::new())),
    });
    if let Some(stderr) = stderr {
        let tail = relay.stderr.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let mut tail = lock(&tail);
                tail.push_back(line);
                while tail.len() > STDERR_LINES {
                    tail.pop_front();
                }
            }
        });
    }
    {
        let weak = Arc::downgrade(&relay);
        let pending = relay.pending.clone();
        let alive = relay.alive.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let is_reply = message.get("result").is_some() || message.get("error").is_some();
                if is_reply {
                    if let Some(id) = message.get("id").and_then(Value::as_u64) {
                        if let Some(sender) = lock(&pending).remove(&id) {
                            let _ = sender.send(message);
                        }
                    }
                } else if message.get("method").is_some() && message.get("id").is_some() {
                    if let Some(relay) = weak.upgrade() {
                        answer_server_request(&relay.stdin, &message);
                    }
                }
            }
            alive.store(false, Ordering::Relaxed);
            // Waiting requests learn it stopped.
            lock(&pending).clear();
        });
    }
    let initialized = relay.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "Ghostex", "version": env!("CARGO_PKG_VERSION") },
        }),
        INITIALIZE_WITHIN,
    );
    if let Err(error) = initialized {
        relay.stop();
        return Err(error);
    }
    relay.write(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
    start_idle_stop();
    Ok(relay)
}

/// The running relay for `host`, started when there is none (or it stopped, or the host changed).
fn ensure(executable: &Path, host: &str) -> Result<Arc<Relay>, String> {
    let current = lock(&RELAY).clone();
    if let Some(relay) =
        current.filter(|relay| relay.alive.load(Ordering::Relaxed) && relay.host == host)
    {
        return Ok(relay);
    }
    let _start = lock(&START);
    let current = lock(&RELAY).clone();
    if let Some(relay) = current {
        if relay.alive.load(Ordering::Relaxed) && relay.host == host {
            return Ok(relay);
        }
        relay.stop();
    }
    let relay = start(executable, host)?;
    *lock(&RELAY) = Some(relay.clone());
    Ok(relay)
}

/// One JSON-RPC request through the relay; a relay that stopped is started again once.
pub(super) fn request(
    executable: &Path,
    host: &str,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let relay = ensure(executable, host)?;
    match relay.request(method, params.clone(), timeout) {
        Err(_) if !relay.alive.load(Ordering::Relaxed) => {
            ensure(executable, host)?.request(method, params, timeout)
        }
        result => result,
    }
}

/// Stops the relay (a sign-in or sign-out changed what it would reach, or gxserver exits).
pub(crate) fn stop() {
    let relay = lock(&RELAY).take();
    if let Some(relay) = relay {
        relay.stop();
    }
}

/// Stops the relay once nothing used it for 15 minutes; the next call starts it again.
fn start_idle_stop() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(IDLE_CHECK_EVERY);
            let idle = lock(&RELAY).as_ref().is_some_and(|relay| {
                lock(&relay.pending).is_empty()
                    && lock(&relay.last_used).elapsed() >= IDLE_STOP_AFTER
            });
            if idle {
                stop();
            }
        });
    });
}
