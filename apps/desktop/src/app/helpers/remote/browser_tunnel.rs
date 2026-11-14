//! Machine-scoped browser networking over the saved authenticated connection.
mod tcp_proxy;
use crate::app::helpers::*;
use crate::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Stdio},
    sync::Mutex,
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
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        _askpass: Option<GpuiRemoteAskpassScript>,
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
    let askpass = gpui_remote_ssh_askpass_script(config)?;
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
            _askpass: None,
        };
        gpui_wait_for_remote_browser_forward(&transport, forward_port)?;
        return start_proxy(
            reservation,
            tcp_proxy::BrowserUpstream::Socks(forward_port),
            transport,
        );
    }
    let mut args = gpui_remote_ssh_tunnel_options(config);
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
        .stderr(Stdio::null());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    let child = command
        .spawn()
        .map_err(|_| "Could not start the browser SSH tunnel.")?;
    gpui_record_remote_tunnel(child.id(), dynamic_forward.as_str());
    let transport = BrowserTransport::Ssh {
        child: Mutex::new(Some(child)),
        forward: Mutex::new(None),
        _askpass: askpass,
    };
    gpui_wait_for_remote_browser_forward(&transport, forward_port)?;
    start_proxy(
        reservation,
        tcp_proxy::BrowserUpstream::Socks(forward_port),
        transport,
    )
}

/// Waits until SSH's dynamic forward on `port` answers SOCKS, the proof that it works. On failure
/// the caller drops `transport`, which stops it.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpui_wait_for_remote_browser_forward(
    transport: &BrowserTransport,
    port: u16,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline && transport.is_alive() {
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
