//! Listening TCP sockets on macOS and Linux, read from `lsof` or `ss`.
use std::io::Read;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

use crate::platform::shell::command_shell;

use super::listener_discovery::TcpListenerDetail;

const PORTLESS_LISTENER_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES: usize = 64 * 1024;

/// The listener command reports a missing tool (exit 127) so the caller can say
/// so instead of printing an empty list.
///
/// Tool order is per-platform, because the two tools see different things:
/// on Linux `lsof` only reports sockets whose /proc entry this user can read, so
/// a root- or Docker-owned listener on port 80 is simply missing from its
/// output, while `ss` reads the kernel's socket table and lists every listening
/// socket (dropping only the pid/command of the ones it may not inspect).
/// macOS has no `ss`, and its `lsof` does report other users' sockets, so there
/// `lsof` stays first.
pub(crate) fn build_tcp_listener_command() -> String {
    r#"
if [ "$(uname -s 2>/dev/null)" = Linux ]; then
  if [ -x /usr/sbin/ss ]; then
    exec /usr/sbin/ss -H -ltnp
  fi
  if [ -x /usr/bin/ss ]; then
    exec /usr/bin/ss -H -ltnp
  fi
  if command -v ss >/dev/null 2>&1; then
    exec ss -H -ltnp
  fi
fi
if [ -x /usr/sbin/lsof ]; then
  exec /usr/sbin/lsof -nP -iTCP -sTCP:LISTEN +c 0 -F pcn
fi
if [ -x /usr/bin/lsof ]; then
  exec /usr/bin/lsof -nP -iTCP -sTCP:LISTEN +c 0 -F pcn
fi
if command -v lsof >/dev/null 2>&1; then
  exec lsof -nP -iTCP -sTCP:LISTEN +c 0 -F pcn
fi
if [ -x /usr/sbin/ss ]; then
  exec /usr/sbin/ss -H -ltnp
fi
if [ -x /usr/bin/ss ]; then
  exec /usr/bin/ss -H -ltnp
fi
if command -v ss >/dev/null 2>&1; then
  exec ss -H -ltnp
fi
exit 127
"#
    .trim()
    .to_string()
}

pub(crate) fn read_posix_tcp_listeners() -> Result<Vec<TcpListenerDetail>> {
    let output = run_portless_listener_snapshot_command(&build_tcp_listener_command())?;
    if output.exit_code == 127 {
        bail!("Neither lsof nor ss is installed, so this machine cannot list listening TCP ports.");
    }
    if output.stdout_truncated {
        bail!("The listening TCP port list exceeded the safety limit.");
    }
    /*
    A run we had to kill produced whatever it had managed to print, which is a
    prefix of the socket table rather than the table. Reporting it as the
    complete list would tell the caller a port is not listening when the tool
    simply never got to it.
    */
    if output.exit_code == 124 {
        bail!(
            "Listing listening TCP ports timed out after {} seconds.",
            PORTLESS_LISTENER_SNAPSHOT_TIMEOUT.as_secs()
        );
    }
    let listeners = parse_tcp_listener_details(&output.stdout);
    /*
    lsof exits 1 both for "no matching sockets" and for "some sockets were
    unreadable", and it still prints the sockets it could read. Rows on stdout
    therefore mean the tool worked. The silent exit 1 — no rows, no diagnostics
    — is lsof's way of saying "you own no listening sockets", which is an empty
    list, not a failure; only a non-zero exit that also has something to
    complain about is a real failure.
    */
    if output.exit_code != 0 && listeners.is_empty() && !output.stderr.is_empty() {
        bail!(
            "Listing listening TCP ports failed with exit code {}. {}",
            output.exit_code,
            output.stderr
        );
    }
    Ok(listeners)
}

pub(crate) fn run_portless_listener_snapshot_command(
    script: &str,
) -> Result<PortlessSnapshotCommandOutput> {
    let shell = command_shell();
    let mut child = crate::platform::process::background_command(&shell.executable)
        .args(shell.script_args(script))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "start Portless listener snapshot")?;
    let stdout = child
        .stdout
        .take()
        .with_context(|| "open Portless listener snapshot stdout")?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| "open Portless listener snapshot stderr")?;
    let terminate = Arc::new(AtomicBool::new(false));
    let stdout_terminate = terminate.clone();
    let stderr_terminate = terminate.clone();
    let stdout_thread = thread::spawn(move || {
        read_portless_capped_output(
            stdout,
            PORTLESS_LISTENER_SNAPSHOT_STDOUT_LIMIT_BYTES,
            stdout_terminate,
        )
    });
    let stderr_thread = thread::spawn(move || {
        read_portless_capped_output(
            stderr,
            PORTLESS_LISTENER_SNAPSHOT_STDERR_LIMIT_BYTES,
            stderr_terminate,
        )
    });

    let started = Instant::now();
    let mut timed_out = false;
    let mut terminate_started: Option<Instant> = None;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| "wait for Portless listener snapshot")?
        {
            let (stdout, stdout_truncated) = stdout_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stdout reader failed."))?;
            let (stderr, stderr_truncated) = stderr_thread
                .join()
                .map_err(|_| anyhow!("Portless listener snapshot stderr reader failed."))?;
            let mut exit_code = status.code().unwrap_or(1);
            if timed_out {
                exit_code = 124;
            } else if stdout_truncated || stderr_truncated {
                exit_code = 125;
            }
            return Ok(PortlessSnapshotCommandOutput {
                exit_code,
                stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
                stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
                stdout_truncated,
            });
        }

        let should_terminate = terminate.load(Ordering::SeqCst)
            || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
        if should_terminate {
            timed_out = timed_out || started.elapsed() >= PORTLESS_LISTENER_SNAPSHOT_TIMEOUT;
            if terminate_started.is_none() {
                terminate_started = Some(Instant::now());
                signal_portless_snapshot_child(&mut child, false);
            } else if terminate_started
                .map(|instant| instant.elapsed() >= Duration::from_millis(1_000))
                .unwrap_or(false)
            {
                signal_portless_snapshot_child(&mut child, true);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_portless_capped_output<R: Read>(
    mut reader: R,
    limit: usize,
    terminate: Arc<AtomicBool>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if read > remaining {
            if remaining > 0 {
                output.extend_from_slice(&buffer[..remaining]);
            }
            truncated = true;
            terminate.store(true, Ordering::SeqCst);
            break;
        }
        output.extend_from_slice(&buffer[..read]);
    }
    (output, truncated)
}

fn signal_portless_snapshot_child(child: &mut std::process::Child, force: bool) {
    #[cfg(unix)]
    unsafe {
        libc::kill(
            child.id() as i32,
            if force { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
    #[cfg(not(unix))]
    {
        let _ = force;
        let _ = child.kill();
    }
}

pub(crate) fn parse_tcp_listener_details(listener_output: &str) -> Vec<TcpListenerDetail> {
    let mut listeners = Vec::new();
    let mut current_pid: Option<u32> = None;
    let mut current_command: Option<String> = None;
    for raw_line in listener_output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(field) = line.chars().next() else {
            continue;
        };
        if !matches!(field, 'p' | 'c' | 'n') {
            if let Some(row) = parse_portless_ss_listener_row(line) {
                listeners.push(row);
            }
            continue;
        }
        let value = &line[field.len_utf8()..];
        match field {
            'p' => {
                current_pid = parse_positive_u32(value);
                current_command = None;
            }
            'c' => {
                let command = value.trim();
                current_command = (!command.is_empty()).then(|| command.to_string());
            }
            'n' => {
                let Some((address, port)) = parse_portless_tcp_listener_endpoint(value) else {
                    continue;
                };
                listeners.push(TcpListenerDetail {
                    address,
                    command: current_command.clone(),
                    pid: current_pid,
                    port,
                });
            }
            _ => {}
        }
    }
    listeners
}

fn parse_portless_ss_listener_row(line: &str) -> Option<TcpListenerDetail> {
    /*
    `ss -H -ltn` puts the socket state in the first column, and it is always
    LISTEN for `-l`. Requiring it keeps banner lines, warnings, and any row from
    a wider `ss` invocation from being read as an endpoint, which would emit an
    address assembled out of whatever columns happened to contain a colon.
    */
    if line.split_whitespace().next() != Some("LISTEN") {
        return None;
    }
    let before_process = line.split(" users:").next().unwrap_or(line);
    let (address, port) = before_process
        .split_whitespace()
        .find_map(parse_portless_tcp_listener_endpoint)?;
    Some(TcpListenerDetail {
        address,
        command: parse_portless_ss_listener_command(line),
        pid: parse_portless_ss_listener_pid(line),
        port,
    })
}

/// `users:(("nginx",pid=123,fd=6),("nginx",pid=124,fd=6))` → `nginx`.
fn parse_portless_ss_listener_command(line: &str) -> Option<String> {
    let command = line.split("users:((\"").nth(1)?.split('"').next()?.trim();
    (!command.is_empty()).then(|| command.to_string())
}

fn parse_portless_ss_listener_pid(line: &str) -> Option<u32> {
    let value = line.split("pid=").nth(1)?;
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    parse_positive_u32(&digits)
}

/// `TCP 127.0.0.1:8000 (LISTEN)` → `("127.0.0.1", 8000)`, `[::1]:9000` →
/// `("::1", 9000)`, `*:5173` → `("*", 5173)`.
fn parse_portless_tcp_listener_endpoint(endpoint: &str) -> Option<(String, u16)> {
    let endpoint = endpoint
        .trim()
        .strip_prefix("TCP ")
        .unwrap_or_else(|| endpoint.trim())
        .split(" (")
        .next()
        .unwrap_or("")
        .trim();
    if endpoint.is_empty() {
        return None;
    }
    /*
    lsof writes a connected socket's `n` field as `local->peer`. Splitting that
    on its last colon would report the peer's port bound to an address made of
    both endpoints, so a row that is not a bare listening endpoint is skipped.
    */
    if endpoint.contains("->") {
        return None;
    }

    let (address, raw_port) = if endpoint.starts_with('[') {
        let host_end = endpoint.find("]:")?;
        (&endpoint[1..host_end], &endpoint[host_end + 2..])
    } else {
        let separator_index = endpoint.rfind(':')?;
        (
            &endpoint[..separator_index],
            &endpoint[separator_index + 1..],
        )
    };
    let port = raw_port.trim().parse::<u16>().ok()?;
    (port > 0).then(|| (address.trim().to_string(), port))
}

fn parse_positive_u32(value: &str) -> Option<u32> {
    let parsed = value.trim().parse::<u64>().ok()?;
    if parsed == 0 || parsed > u32::MAX as u64 {
        return None;
    }
    Some(parsed as u32)
}

pub(crate) struct PortlessSnapshotCommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stderr: String,
    pub(crate) stdout: String,
    pub(crate) stdout_truncated: bool,
}
