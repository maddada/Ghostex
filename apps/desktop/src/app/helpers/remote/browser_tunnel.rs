//! SSH-backed browser networking. The tunnel belongs to the saved machine connection.
use crate::app::helpers::*;
use crate::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

pub(crate) struct RemoteBrowserTunnel {
    pub(crate) port: u16,
    child: Mutex<Child>,
    #[cfg(target_os = "macos")]
    _askpass: Option<GpuiRemoteAskpassScript>,
}

impl RemoteBrowserTunnel {
    pub(crate) fn stop(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub(crate) fn is_alive(&self) -> bool {
        self.child
            .lock()
            .ok()
            .is_some_and(|mut child| matches!(child.try_wait(), Ok(None)))
    }
}
impl Drop for RemoteBrowserTunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

/// CDXC:Browser 2026-09-05 WHY:
/// SSH dynamic forwarding preserves localhost origins, form bodies and cross-port API/WebSocket traffic without rewriting documents.
/// Each remote browser context uses its machine's tunnel; the app UI keeps its own local network context.
#[cfg(target_os = "macos")]
pub(crate) fn start_remote_browser_tunnel(
    config: &GpuiRemoteMachineConfig,
) -> Result<RemoteBrowserTunnel, String> {
    let reservation = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "Could not reserve a browser tunnel port.")?;
    let port = reservation
        .local_addr()
        .map_err(|_| "Could not read the browser tunnel port.")?
        .port();
    let askpass = gpui_remote_ssh_askpass_script(config)?;
    let mut args = gpui_remote_ssh_client_options(config.has_saved_password);
    args.extend([
        "-N".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-D".into(),
        format!("127.0.0.1:{port}"),
    ]);
    args.extend(gpui_remote_ssh_target_arguments(config)?);
    let mut command = Command::new("/usr/bin/ssh");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    drop(reservation);
    let child = command
        .spawn()
        .map_err(|_| "Could not start the browser SSH tunnel.")?;
    let tunnel = RemoteBrowserTunnel {
        port,
        child: Mutex::new(child),
        _askpass: askpass,
    };
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline && tunnel.is_alive() {
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
                return Ok(tunnel);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("The browser tunnel could not connect. Check the machine's SSH connection and forwarding permissions.".into())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn start_remote_browser_tunnel(
    _: &GpuiRemoteMachineConfig,
) -> Result<RemoteBrowserTunnel, String> {
    Err("Remote SSH browsing is not available on this desktop platform yet.".into())
}
