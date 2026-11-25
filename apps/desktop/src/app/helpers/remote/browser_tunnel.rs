//! Machine-scoped browser networking over the saved authenticated connection.
mod tcp_proxy;
use crate::app::helpers::*;
use crate::*;
use std::{
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) struct RemoteBrowserTunnel {
    /// The port of the SOCKS proxy the machine's Browser context uses (`tcp_proxy.rs`), which
    /// sends only the machine's own and private-network addresses through `transport`.
    pub(crate) port: u16,
    proxy: tcp_proxy::BrowserTcpProxy,
    transport: BrowserTransport,
}

enum BrowserTransport {
    Ssh {
        /// The tunnel's ssh process, which owns the forward; `None` when the forward lives in the
        /// user's own SSH master (GitHub #496).
        child: Mutex<Option<Child>>,
        /// The `-D` forward in the user's SSH master, taken back when the tunnel stops
        /// (GitHub #496).
        forward: Mutex<Option<GpuiRemoteSharedSshForward>>,
    },
    /// The machine's gxserver connector, which the proxy dials for each connection.
    Gxserver,
}

impl RemoteBrowserTunnel {
    pub(crate) fn stop(&self) {
        self.proxy.stop();
        self.transport.stop();
    }
    pub(crate) fn is_alive(&self) -> bool {
        self.proxy.is_alive() && self.transport.is_alive()
    }
}
impl Drop for RemoteBrowserTunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

impl BrowserTransport {
    fn stop(&self) {
        match self {
            BrowserTransport::Ssh { child, forward, .. } => {
                if let Ok(mut child) = child.lock()
                    && let Some(child) = child.as_mut()
                {
                    gpui_forget_remote_tunnel(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Ok(mut forward) = forward.lock() {
                    forward.take();
                }
            }
            BrowserTransport::Gxserver => {}
        }
    }
    fn is_alive(&self) -> bool {
        match self {
            BrowserTransport::Ssh { child, .. } => child.lock().ok().is_some_and(|mut child| {
                match child.as_mut() {
                    Some(child) => matches!(child.try_wait(), Ok(None)),
                    // A forward registered in the user's SSH master has no process of its own.
                    None => true,
                }
            }),
            BrowserTransport::Gxserver => true,
        }
    }
}
impl Drop for BrowserTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

/// CDXC:Browser 2026-09-05 WHY:
/// SSH dynamic forwarding preserves localhost origins, form bodies and cross-port API/WebSocket traffic without rewriting documents.
/// Each remote browser context uses its machine's tunnel; the app UI keeps its own local network context.
/// Since 2026-11-14 only the machine's own and private-network addresses take it; the proxy in
/// front of it opens every other address from this computer (CDXC:Browser 2026-11-14 in
/// browser_route.rs).
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn start_remote_browser_tunnel(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    route: RemoteBrowserRoute,
) -> Result<RemoteBrowserTunnel, String> {
    // The proxy's own port, held open until the proxy listens on it.
    let reservation = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "Could not reserve a browser tunnel port.")?;
    let port = reservation
        .local_addr()
        .map_err(|_| "Could not read the browser tunnel port.")?
        .port();
    let start_proxy = |reservation, upstream, transport| {
        let proxy = tcp_proxy::BrowserTcpProxy::start(reservation, upstream, route)
            .map_err(|_| "Could not start the remote browser connection.")?;
        Ok(RemoteBrowserTunnel {
            port,
            proxy,
            transport,
        })
    };
    if target.execution_target == GpuiRemoteExecutionTarget::WindowsPowerShell {
        if !target.capabilities.browser_tcp_websocket_v1 {
            return Err(
                "Update Ghostex on the Windows computer to enable remote browser networking."
                    .into(),
            );
        }
        return start_proxy(
            reservation,
            tcp_proxy::BrowserUpstream::Gxserver(target.clone()),
            BrowserTransport::Gxserver,
        );
    }
    // SSH's dynamic forward listens on a port of its own; only the proxy talks to it.
    let forward_port = TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map_err(|_| "Could not reserve a browser tunnel port.")?
        .port();
    let askpass = gpui_remote_ssh_askpass_script(config, GpuiAskpassServe::Handshake)?;
    let dynamic_forward = format!("127.0.0.1:{forward_port}");
    if config.reuse_ssh_connection {
        let _ =
            gpui_remote_ssh_forward_in_existing_connection(config, "-D", dynamic_forward.as_str())?;
        let transport = BrowserTransport::Ssh {
            child: Mutex::new(None),
            forward: Mutex::new(GpuiRemoteSharedSshForward::new(
                config,
                "-D",
                dynamic_forward.as_str(),
            )),
        };
        // The master reported the forward as made (`ssh -O forward` fails otherwise), so the
        // listener on the port is its own.
        gpui_wait_for_remote_browser_forward(&transport, forward_port, None)?;
        return start_proxy(
            reservation,
            tcp_proxy::BrowserUpstream::Socks(forward_port),
            transport,
        );
    }
    // `-v` makes ssh say when its own listener is up (see `gpui_wait_for_remote_browser_forward`).
    let mut args = vec!["-v".to_string()];
    args.extend(gpui_remote_ssh_tunnel_options(config));
    args.extend([
        "-N".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-D".into(),
        dynamic_forward.clone(),
    ]);
    args.extend(gpui_remote_ssh_target_arguments(config)?);
    let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start the browser SSH tunnel.")?;
    gpui_record_remote_tunnel(child.id(), dynamic_forward.as_str());
    let listening = Arc::new(AtomicBool::new(false));
    if let Some(stderr) = child.stderr.take() {
        let listening = listening.clone();
        // Drained for the tunnel's whole life, so ssh's log can never fill the pipe and stall it.
        // Without this thread the tunnel never counts as ready and is stopped.
        let _ = std::thread::Builder::new()
            .name("ghostex-remote-browser-tunnel".into())
            .spawn(move || {
                let mut log = SshForwardLog::new(forward_port);
                let mut stderr = BufReader::new(stderr);
                let mut line = Vec::new();
                // Bytes to each newline, so a line that is not UTF-8 (a `ProxyCommand` writing to
                // the same stderr) neither stops the draining nor the readiness check.
                loop {
                    line.clear();
                    match stderr.read_until(b'\n', &mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if log.line(&String::from_utf8_lossy(&line)) {
                                listening.store(true, Ordering::Release);
                            }
                        }
                        Err(error) if error.kind() == ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            });
    }
    let transport = BrowserTransport::Ssh {
        child: Mutex::new(Some(child)),
        forward: Mutex::new(None),
    };
    gpui_wait_for_remote_browser_forward(&transport, forward_port, Some(&listening))?;
    // The forward answers, so ssh has signed in and never asks again: the saved-password helper
    // stops answering now rather than at the end of its window (askpass_broker.rs).
    drop(askpass);
    start_proxy(
        reservation,
        tcp_proxy::BrowserUpstream::Socks(forward_port),
        transport,
    )
}

/// Follows the browser tunnel's ssh log (`-v`) to the point where ssh itself holds the forward's
/// port. ssh logs "Local forwarding listening on 127.0.0.1 port N." just before it binds, so that
/// line alone proves nothing; "Entering interactive session." comes after the forwards are set up,
/// and with `ExitOnForwardFailure=yes` a failed bind ends ssh before it can be printed.
struct SshForwardLog {
    listening: String,
    asked_to_listen: bool,
}

impl SshForwardLog {
    fn new(port: u16) -> Self {
        Self {
            listening: format!("Local forwarding listening on 127.0.0.1 port {port}."),
            asked_to_listen: false,
        }
    }

    /// Takes one log line; `true` once ssh holds the port.
    fn line(&mut self, line: &str) -> bool {
        let line = line.trim_end();
        if line.ends_with(self.listening.as_str()) {
            self.asked_to_listen = true;
        }
        self.asked_to_listen && line.ends_with("Entering interactive session.")
    }
}

/// Waits until SSH's dynamic forward on `port` answers SOCKS, the proof that it works. On failure
/// the caller drops `transport`, which stops it.
///
/// CDXC:RemoteMachines 2026-11-26 WHY: the port is reserved and released before ssh binds it, so
/// another local program could take it first and answer SOCKS; with `ExitOnForwardFailure` ssh
/// only exits after it has signed in, and until then that program counted as the tunnel and saw
/// the machine's browser traffic. When ssh's own process is the tunnel, `listening` says ssh's log
/// showed it holding this exact port (`SshForwardLog`); once it does, nobody else can listen there.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpui_wait_for_remote_browser_forward(
    transport: &BrowserTransport,
    port: u16,
    listening: Option<&AtomicBool>,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline && transport.is_alive() {
        if listening.is_some_and(|listening| !listening.load(Ordering::Acquire)) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if let Ok(mut stream) =
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(150))
        {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));
            let mut reply = [0; 2];
            if stream.write_all(&[5, 1, 0]).is_ok()
                && stream.read_exact(&mut reply).is_ok()
                && reply == [5, 0]
            {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("The browser tunnel could not connect. Check the machine's SSH connection and forwarding permissions.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn start_remote_browser_tunnel(
    _: &GpuiRemoteMachineConfig,
    _: &GpuiRemoteGxserverRequestTarget,
    _: RemoteBrowserRoute,
) -> Result<RemoteBrowserTunnel, String> {
    Err("Remote SSH browsing is not available on this desktop platform yet.".into())
}

#[cfg(test)]
mod tests {
    use super::SshForwardLog;

    fn ready_after(port: u16, lines: &[&str]) -> bool {
        let mut log = SshForwardLog::new(port);
        lines.iter().any(|line| log.line(line))
    }

    const LISTENING: &str = "debug1: Local forwarding listening on 127.0.0.1 port 51234.";
    const ENTERING: &str = "debug1: Entering interactive session.";

    #[test]
    fn ready_only_once_ssh_enters_its_session_after_listening_on_this_port() {
        assert!(ready_after(
            51234,
            &[LISTENING, "debug1: channel 0: new port-listener", ENTERING]
        ));
        // Windows' ssh ends its lines with \r\n.
        assert!(ready_after(
            51234,
            &[&format!("{LISTENING}\r"), &format!("{ENTERING}\r")]
        ));
        // ssh logs the listening line before bind(): alone, or after a failed bind, it is not
        // ready.
        assert!(!ready_after(51234, &[LISTENING]));
        assert!(!ready_after(
            51234,
            &[
                LISTENING,
                "bind [127.0.0.1]:51234: Address already in use",
                "Could not request local forwarding.",
            ]
        ));
        assert!(!ready_after(51234, &[ENTERING, LISTENING]));
        assert!(!ready_after(5123, &[LISTENING, ENTERING]));
        assert!(!ready_after(
            51234,
            &[
                "debug1: Local forwarding listening on ::1 port 51234.",
                ENTERING
            ]
        ));
    }
}
