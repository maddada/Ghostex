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
unit can still be one that is not listening. The same query does double duty
for the enable path, which needs to know "installed but stopped" from "not
installed" before it decides whether an administrator prompt can help at all.
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
        detail: Some(read_ssh_server_detail()),
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

// `launchctl print` on the system domain works without root and fails with a
// non-zero status when the job is not bootstrapped, which is exactly the
// Remote Login off state.
#[cfg(target_os = "macos")]
fn read_ssh_server_detail() -> String {
    let loaded = command_succeeds("/bin/launchctl", &["print", "system/com.openssh.sshd"]);
    if loaded {
        "SSH access is on: the Remote Login service is loaded.".to_string()
    } else {
        "SSH access is off: the Remote Login service is not loaded.".to_string()
    }
}

// The OpenSSH Server optional feature registers the `sshd` service, so the
// service's presence is the install check that needs no elevation
// (`Get-WindowsCapability -Online` is a DISM call and does). `Status` and
// `StartType` are both on ServiceController in Windows PowerShell 5.1, the
// floor Microsoft's OpenSSH guide sets.
#[cfg(windows)]
fn read_ssh_server_detail() -> String {
    const QUERY: &str = "$s = Get-Service -Name sshd -ErrorAction SilentlyContinue; if ($null -eq $s) { 'absent' } else { '{0} {1}' -f $s.Status, $s.StartType }";
    let Some(line) = read_command_first_line(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", QUERY],
    ) else {
        return "The OpenSSH Server service state could not be read.".to_string();
    };
    let mut parts = line.split_whitespace();
    let status = parts.next().unwrap_or_default();
    let start_type = parts.next().unwrap_or_default();
    match status {
        "absent" => "The OpenSSH Server feature is not installed.".to_string(),
        "Running" => "SSH access is on: the OpenSSH Server service is running.".to_string(),
        "Stopped" if start_type == "Disabled" => {
            "The OpenSSH Server feature is installed but its service is disabled.".to_string()
        }
        "Stopped" => {
            "The OpenSSH Server feature is installed but the service is stopped.".to_string()
        }
        other => format!("The OpenSSH Server service is {other}."),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_ssh_server_detail() -> String {
    match probe_linux_ssh_units() {
        LinuxSshProbe::NoSystemctl => {
            "systemctl is not available, so the SSH service state could not be read.".to_string()
        }
        LinuxSshProbe::NotInstalled => {
            "The OpenSSH server is not installed: no ssh or sshd systemd unit was found."
                .to_string()
        }
        LinuxSshProbe::Installed(units) => {
            let unit = units.unit;
            if units
                .socket
                .as_ref()
                .is_some_and(|socket| socket.is_active())
            {
                format!("SSH access is on: {unit}.socket starts the server on demand.")
            } else if units.service.is_active() {
                format!("SSH access is on: {unit}.service is running.")
            } else if units.service.unit_file_state == "masked" {
                format!("The OpenSSH server is installed but {unit}.service is masked.")
            } else {
                format!("The OpenSSH server is installed but {unit}.service is stopped.")
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, unix)))]
fn read_ssh_server_detail() -> String {
    "The SSH service state could not be read on this platform.".to_string()
}

/// One systemd unit as reported by `systemctl show`, which exits 0 even for a
/// unit that does not exist (then `LoadState=not-found`).
#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinuxUnitState {
    pub load_state: String,
    pub active_state: String,
    pub unit_file_state: String,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl LinuxUnitState {
    pub(crate) fn is_loaded(&self) -> bool {
        self.load_state == "loaded"
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active_state == "active"
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.unit_file_state == "enabled"
    }
}

/// The SSH server units on this computer. Debian and Ubuntu name them
/// `ssh.service` / `ssh.socket`; Fedora, RHEL, and Arch name them
/// `sshd.service` / `sshd.socket`. Ubuntu 22.10 and later enable the socket
/// and let it start sshd on demand, so the socket is tracked separately.
#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinuxSshUnits {
    pub unit: &'static str,
    pub service: LinuxUnitState,
    pub socket: Option<LinuxUnitState>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinuxSshProbe {
    NoSystemctl,
    NotInstalled,
    Installed(LinuxSshUnits),
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn probe_linux_ssh_units() -> LinuxSshProbe {
    let mut saw_systemctl = false;
    for unit in ["ssh", "sshd"] {
        let Some(service) = systemctl_show(&format!("{unit}.service")) else {
            continue;
        };
        saw_systemctl = true;
        if !service.is_loaded() {
            continue;
        }
        let socket = systemctl_show(&format!("{unit}.socket")).filter(LinuxUnitState::is_loaded);
        return LinuxSshProbe::Installed(LinuxSshUnits {
            unit,
            service,
            socket,
        });
    }
    if saw_systemctl {
        LinuxSshProbe::NotInstalled
    } else {
        LinuxSshProbe::NoSystemctl
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn systemctl_show(unit: &str) -> Option<LinuxUnitState> {
    let output = Command::new("systemctl")
        .args([
            "show",
            "--property=LoadState,ActiveState,UnitFileState",
            unit,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut state = LinuxUnitState {
        load_state: String::new(),
        active_state: String::new(),
        unit_file_state: String::new(),
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "LoadState" => state.load_state = value,
            "ActiveState" => state.active_state = value,
            "UnitFileState" => state.unit_file_state = value,
            _ => {}
        }
    }
    Some(state)
}

#[cfg(target_os = "macos")]
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

#[cfg(windows)]
fn read_command_first_line(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    super::ssh_enable::hide_console_window(&mut command);
    let output = command.output().ok()?;
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
