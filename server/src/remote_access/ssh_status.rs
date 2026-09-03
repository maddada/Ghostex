use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    process::{Command, Stdio},
    time::Duration,
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_SSH_PORT: u16 = 22;
const SSH_PROBE_TIMEOUT: Duration = Duration::from_millis(600);

/*
CDXC:RemotePairing 2026-09-03:
"SSH access is on" means a daemon answers on the loopback SSH port — the same
port Easy Connect forwards and the phone dials — so the TCP probe is the
verdict on every OS. The per-OS service query is only DETAIL for the UI; it
never overrides the probe, because a loaded launchd job or an enabled systemd
unit can still be one that is not listening.
*/
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshAccessStatus {
    pub enabled: bool,
    pub port: u16,
    pub checked_at: String,
    pub detail: Option<String>,
}

pub fn read_ssh_access_status(port: u16) -> SshAccessStatus {
    let enabled = ssh_port_accepts_connections(port);
    SshAccessStatus {
        enabled,
        port,
        checked_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        detail: read_ssh_service_detail(),
    }
}

/// The SSH port Easy Connect forwards for the phone: 22 when it is among the
/// served ports, else 22 anyway — the pairing code always names the port the
/// phone must dial, and Easy Connect's default port set carries 22.
pub fn ssh_port_for_pairing(served_ports: &[u16]) -> u16 {
    served_ports
        .iter()
        .copied()
        .find(|port| *port == DEFAULT_SSH_PORT)
        .unwrap_or(DEFAULT_SSH_PORT)
}

pub fn ssh_port_accepts_connections(port: u16) -> bool {
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&address, SSH_PROBE_TIMEOUT).is_ok()
}

fn read_ssh_service_detail() -> Option<String> {
    if cfg!(target_os = "macos") {
        let loaded = command_succeeds("launchctl", &["print", "system/com.openssh.sshd"]);
        return Some(if loaded {
            "Remote Login service is loaded".to_string()
        } else {
            "Remote Login service is not loaded".to_string()
        });
    }
    if cfg!(windows) {
        // Unverified on a real Windows machine: reads the OpenSSH Server
        // service state through PowerShell.
        let state = read_command_first_line(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Service -Name sshd -ErrorAction Stop).Status",
            ],
        );
        return Some(match state {
            Some(state) => format!("OpenSSH Server service is {}", state.to_lowercase()),
            None => "OpenSSH Server is not installed".to_string(),
        });
    }
    // Unverified on a real Linux machine: Debian/Ubuntu name the unit `ssh`,
    // Fedora/Arch name it `sshd`.
    for unit in ["ssh", "sshd"] {
        if let Some(state) = read_command_first_line("systemctl", &["is-active", unit]) {
            return Some(format!("{unit}.service is {state}"));
        }
    }
    None
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn read_command_first_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or_default().trim().to_string();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}
