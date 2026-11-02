// The way back: an SSH remote forward that gives a connected computer's gxserver a port on that
// computer leading to this computer's gxserver, so the agents there can reach this one.

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::app::helpers::*;

/// How long ssh gets to log in and report the port the remote side picked.
const REVERSE_TUNNEL_PORT_WAIT: Duration = Duration::from_secs(20);

pub(crate) struct GpuiRemoteReverseTunnel {
    pub(crate) child: Child,
    /// The port on the connected computer's loopback that leads back here.
    pub(crate) remote_port: u16,
    /// This computer's way-back listener the forward lands on (gxserver's `peerPort`).
    pub(crate) peer_port: u16,
    _askpass: Option<GpuiRemoteAskpassScript>,
}

impl GpuiRemoteReverseTunnel {
    /// The ssh process still runs. A forward that died frees its port on the other computer, so
    /// it is replaced (with a new token) rather than left for the other side to keep using.
    pub(crate) fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub(crate) fn terminate(&mut self) {
        gpui_forget_remote_tunnel(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Opens the way back for `config`, or `None` when it cannot be had.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// The way back is its own `ssh -N -R 127.0.0.1:0:…` rather than a `-R` on the machine's main tunnel: the main tunnel runs with `ExitOnForwardFailure=yes`, so an sshd that refuses remote forwards (`AllowTcpForwarding local`) would have taken the whole connection down with it. Port 0 lets the other computer pick a free port, which ssh reports on stderr ("Allocated port N for remote forward"), so two computers connected to the same one never collide; the explicit 127.0.0.1 keeps it on that computer's loopback unless its sshd forces `GatewayPorts yes`. It lands on gxserver's way-back listener (`peer_port`), which answers only the agent allowlist to a token this computer gave, never the main listener and its pages. A WSL machine gets none: its gxserver runs inside WSL, where the Windows host's loopback is not reachable.
/// SEE-ALSO: server/src/connected_computers/registry.rs (the inbound registration that names this port), apps/desktop/src/app/remote_conn/connected_computers.rs.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn gpui_open_remote_reverse_tunnel(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    peer_port: u16,
) -> Option<GpuiRemoteReverseTunnel> {
    if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsWsl { .. }
    ) {
        return None;
    }
    let target_arguments = gpui_remote_ssh_target_arguments(config).ok()?;
    let askpass = gpui_remote_ssh_askpass_script(config).ok()?;
    let forward = format!("127.0.0.1:0:127.0.0.1:{peer_port}");
    let mut arguments = vec!["-N".to_string()];
    arguments.extend(gpui_remote_ssh_tunnel_options(config.has_saved_password));
    arguments.extend([
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-R".to_string(),
        forward.clone(),
    ]);
    arguments.extend(target_arguments);
    let mut command = gpui_remote_background_command(&gpui_remote_ssh_executable());
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    let mut child = command.spawn().ok()?;
    gpui_record_remote_tunnel(child.id(), forward.as_str());
    let stderr = child.stderr.take()?;
    let (sender, receiver) = mpsc::channel();
    // ssh writes only a few lines, but the reader keeps draining for the tunnel's whole life so
    // a chatty server can never fill the pipe and stall it.
    let _ = thread::Builder::new()
        .name("ghostex-remote-reverse-tunnel".into())
        .spawn(move || {
            let mut sender = Some(sender);
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(port) = allocated_remote_port(&line) {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(port);
                    }
                }
            }
        });
    match receiver.recv_timeout(REVERSE_TUNNEL_PORT_WAIT) {
        Ok(remote_port) if matches!(child.try_wait(), Ok(None)) => Some(GpuiRemoteReverseTunnel {
            child,
            remote_port,
            peer_port,
            _askpass: askpass,
        }),
        _ => {
            gpui_forget_remote_tunnel(child.id());
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_open_remote_reverse_tunnel(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _peer_port: u16,
) -> Option<GpuiRemoteReverseTunnel> {
    None
}

/// The port in ssh's "Allocated port 50123 for remote forward to 127.0.0.1:58744".
fn allocated_remote_port(line: &str) -> Option<u16> {
    let rest = line.trim().strip_prefix("Allocated port ")?;
    let (port, tail) = rest.split_once(' ')?;
    tail.starts_with("for remote forward")
        .then(|| port.parse().ok())
        .flatten()
        .filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
    use super::allocated_remote_port;

    #[test]
    fn the_port_the_other_computer_picked_is_read_from_ssh() {
        assert_eq!(
            allocated_remote_port("Allocated port 50123 for remote forward to 127.0.0.1:58744"),
            Some(50123)
        );
        assert_eq!(
            allocated_remote_port("Warning: remote port forwarding failed for listen port 0"),
            None
        );
        assert_eq!(
            allocated_remote_port("Allocated port 0 for remote forward to x"),
            None
        );
    }
}
