use std::{
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use super::ssh_status::ssh_port_accepts_connections;

const SSH_LISTEN_WAIT: Duration = Duration::from_secs(4);

/*
CDXC:RemotePairing 2026-09-03:
Turning SSH access on is the one privileged step in pairing, and it runs from
gxserver rather than the desktop app because gxserver lives in the user's GUI
session on every OS (so the admin prompt can appear) and the CLI and web
Settings page then share the same code path. A declined prompt is a distinct
outcome, not a failure: the UI answers it with the per-OS manual steps. A
`failed` message is a full sentence the Settings row shows verbatim before
"Or do it by hand:", so each one names the thing the user can act on.
*/
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SshEnableOutcome {
    Enabled,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshEnableResult {
    pub outcome: SshEnableOutcome,
    pub message: Option<String>,
}

/// `install`: on Linux, install the OpenSSH server first when it is missing (the window's Install
/// click, `ssh_install.rs`); without it a missing server fails with the command to install it.
pub fn enable_ssh_access(port: u16, install: bool) -> SshEnableResult {
    if ssh_port_accepts_connections(port) {
        return SshEnableResult {
            outcome: SshEnableOutcome::Enabled,
            message: None,
        };
    }
    let result = run_privileged_enable(install);
    if result.outcome != SshEnableOutcome::Enabled {
        return result;
    }
    // The service can take a moment to start listening after the command
    // returns; report `enabled` only once the port actually answers.
    let deadline = Instant::now() + SSH_LISTEN_WAIT;
    while Instant::now() < deadline {
        if ssh_port_accepts_connections(port) {
            return result;
        }
        thread::sleep(Duration::from_millis(200));
    }
    SshEnableResult {
        outcome: SshEnableOutcome::Failed,
        message: Some(format!(
            "SSH access was turned on, but nothing is listening on port {port} yet."
        )),
    }
}

// macOS: Remote Login is the `com.openssh.sshd` launchd job. `systemsetup
// -setremotelogin on` is not used because, since macOS 10.15, it demands that
// the *parent process* hold Full Disk Access (Apple support article 101653),
// which gxserver never has; enabling and bootstrapping the job with
// `launchctl` is the equivalent that is not TCC-gated. The bootstrap is
// skipped when `launchctl print` already finds the job, so the command is
// idempotent. `do shell script … with administrator privileges` shows the
// standard macOS administrator prompt.
#[cfg(target_os = "macos")]
fn run_privileged_enable(_install: bool) -> SshEnableResult {
    const SCRIPT: &str = "/bin/launchctl enable system/com.openssh.sshd && (/bin/launchctl print system/com.openssh.sshd >/dev/null 2>&1 || /bin/launchctl bootstrap system /System/Library/LaunchDaemons/ssh.plist)";
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(format!(
            "do shell script \"{SCRIPT}\" with administrator privileges"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return failed(format!("Could not open the administrator prompt: {error}.")),
    };
    if output.status.success() {
        return enabled();
    }
    let combined = combined_output(&output.stdout, &output.stderr);
    let normalized = combined.to_lowercase();
    // osascript reports a dismissed prompt as AppleScript error -128
    // ("User canceled.").
    if normalized.contains("user canceled")
        || normalized.contains("user cancelled")
        || normalized.contains("(-128)")
    {
        return cancelled();
    }
    failed(format!(
        "Turning on SSH access failed: {}.",
        trimmed_or_exit_code(&combined, &output.status)
    ))
}

// Windows: the steps are the ones in Microsoft's "Get started with OpenSSH
// Server for Windows" guide — install the `OpenSSH.Server~~~~0.0.1.0`
// capability, set `sshd` to start automatically, start it, and make sure the
// `OpenSSH-Server-In-TCP` inbound rule exists. They need an elevated process,
// so a non-elevated PowerShell launches an elevated one with
// `Start-Process -Verb RunAs -Wait -PassThru` and forwards its exit code
// (`windows_elevation.rs`).
/*
CDXC:RemotePairing 2026-10-03 WHY:
`Get-Service sshd` returning nothing does not mean OpenSSH Server is missing:
the inbox `sshd` service keeps the default service DACL (SYSTEM,
Administrators, INTERACTIVE, SERVICE), so a token without those (a network
or SSH logon, an unelevated child) gets access denied and Get-Service reports
"not found". A script that decides "installed?" from it can run
Add-WindowsCapability on an installed feature (a silent no-op) and then fail
with "The sshd service is still missing after the feature was installed".
Presence is read from the `Services\sshd` registry key (readable by every
user); Get-Service is only asked once the key exists, and a key Windows has
not loaded into the service manager yet means a restart is pending. The
other way to reach that message, the feature listed as Installed while its
service was deleted (typically by uninstalling a GitHub/MSI OpenSSH build,
which shares the `sshd` name), is repaired by removing and re-adding the
feature in the same elevated run; Add-WindowsCapability alone does nothing
for a feature Windows already counts as installed.
*/
#[cfg(windows)]
mod windows_enable {
    use super::super::windows_elevation::{
        run_elevated_powershell, EXIT_LAUNCH_FAILED, EXIT_NO_DESKTOP, EXIT_NO_EXIT_CODE,
        EXIT_NO_PROCESS, EXIT_UAC_CANCELLED,
    };
    use super::{cancelled, enabled, failed, SshEnableResult};

    // Exit codes chosen by the elevated script.
    const EXIT_CAPABILITY_FAILED: i32 = 2;
    const EXIT_SERVICE_FAILED: i32 = 3;
    const EXIT_RESTART_NEEDED: i32 = 4;
    const EXIT_FIREWALL_FAILED: i32 = 5;
    const EXIT_NOT_ELEVATED: i32 = 6;
    const EXIT_OTHER_SSH_SERVER: i32 = 7;

    const ELEVATED_SCRIPT: &str = r#"param([string]$LogPath)
$ErrorActionPreference = 'Stop'
function Note([string]$Text) { Add-Content -LiteralPath $LogPath -Value $Text }
$capability = 'OpenSSH.Server~~~~0.0.1.0'
$servicesKey = 'HKLM:\SYSTEM\CurrentControlSet\Services'
function Test-SshdRegistered { Test-Path -LiteralPath "$servicesKey\sshd" }
function Get-CapabilityState { [string](Get-WindowsCapability -Online -Name $capability).State }
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { exit 6 }
try {
    if (-not (Test-SshdRegistered)) {
        $other = Get-ChildItem -LiteralPath $servicesKey | Where-Object { [string]$_.GetValue('ImagePath') -match 'sshd\.exe' } | Select-Object -First 1
        if ($null -ne $other) {
            Note ("the '" + $other.PSChildName + "' service runs " + [string]$other.GetValue('ImagePath'))
            exit 7
        }
        $state = Get-CapabilityState
        if ($state -eq 'InstallPending' -or $state -eq 'UninstallPending') { exit 4 }
        if ($state -eq 'Installed') {
            $removed = Remove-WindowsCapability -Online -Name $capability
            if ($removed.RestartNeeded) { exit 4 }
        }
        $result = Add-WindowsCapability -Online -Name $capability
        if (-not (Test-SshdRegistered)) {
            $state = Get-CapabilityState
            if ($result.RestartNeeded -or $state -eq 'InstallPending') { exit 4 }
            Note ("Windows lists the feature as $state but did not create its sshd service. Remove OpenSSH Server in Settings > System > Optional features, restart, and try again")
            exit 2
        }
    }
    if ($null -eq (Get-Service -Name sshd -ErrorAction SilentlyContinue)) { exit 4 }
} catch {
    Note $_.Exception.Message
    exit 2
}
try {
    Set-Service -Name sshd -StartupType Automatic
    if ((Get-Service -Name sshd).Status -ne 'Running') { Start-Service -Name sshd }
} catch {
    Note $_.Exception.Message
    exit 3
}
try {
    if ($null -eq (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 | Out-Null
    }
} catch {
    Note $_.Exception.Message
    exit 5
}
exit 0
"#;

    pub(super) fn run() -> SshEnableResult {
        let run = match run_elevated_powershell("ghostex-ssh-enable", ELEVATED_SCRIPT, &[]) {
            Ok(run) => run,
            Err(message) => return failed(message),
        };
        let log = run.log;
        let stderr = run.stderr;
        match run.code {
            Some(0) => enabled(),
            Some(EXIT_UAC_CANCELLED) => cancelled(),
            Some(EXIT_CAPABILITY_FAILED) => failed(capability_failed_message(&log)),
            Some(EXIT_SERVICE_FAILED) => failed(with_log(
                "Starting the OpenSSH Server service failed",
                &log,
            )),
            Some(EXIT_RESTART_NEEDED) => failed(
                "Windows needs a restart to finish setting up the OpenSSH Server feature. Restart your computer, then turn SSH access on again."
                    .to_string(),
            ),
            Some(EXIT_NOT_ELEVATED) => failed(
                "The administrator prompt did not give Windows PowerShell administrator rights, so the OpenSSH Server feature could not be installed. Sign in with an administrator account, then try again."
                    .to_string(),
            ),
            Some(EXIT_OTHER_SSH_SERVER) => failed(with_log(
                "Another SSH server is installed but is not answering, so Ghostex did not install the Windows OpenSSH Server over it. Start that server or uninstall it, then try again. Details",
                &log,
            )),
            Some(EXIT_NO_DESKTOP) => failed(
                "Windows cannot show the administrator prompt because Ghostex's background service is not running in your desktop session (it was started from an SSH or remote connection). Quit Ghostex together with its background service, open Ghostex from the Start menu, then try again."
                    .to_string(),
            ),
            Some(EXIT_FIREWALL_FAILED) => failed(with_log(
                "The OpenSSH Server service is running, but the OpenSSH-Server-In-TCP firewall rule could not be created",
                &log,
            )),
            Some(EXIT_LAUNCH_FAILED) => failed(with_log(
                "Could not open the administrator prompt",
                &stderr,
            )),
            Some(EXIT_NO_PROCESS) | Some(EXIT_NO_EXIT_CODE) => failed(
                "The elevated PowerShell did not report a result.".to_string(),
            ),
            Some(code) => failed(with_log(
                &format!("Turning on SSH access failed with exit code {code}"),
                &if log.is_empty() { stderr } else { log },
            )),
            None => failed("The elevated PowerShell was terminated.".to_string()),
        }
    }

    // 0x800F0954 is what Add-WindowsCapability raises when Windows Update is
    // pointed at a WSUS server that does not carry Features on Demand, the
    // usual cause on managed computers; the error alone does not say so.
    fn capability_failed_message(log: &str) -> String {
        let message = with_log("Installing the OpenSSH Server feature failed", log);
        if log.to_ascii_lowercase().contains("0x800f0954") {
            format!(
                "{message} Windows Update on this computer is managed by WSUS, which does not offer optional features. In the Group Policy \"Specify settings for optional component installation and component repair\", turn on \"Download repair content and optional features directly from Windows Update instead of WSUS\" (or ask your administrator), then try again."
            )
        } else {
            message
        }
    }

    fn with_log(lead: &str, detail: &str) -> String {
        let detail = detail.trim();
        if detail.is_empty() {
            format!("{lead}.")
        } else {
            format!("{lead}: {detail}.")
        }
    }
}

#[cfg(windows)]
fn run_privileged_enable(_install: bool) -> SshEnableResult {
    windows_enable::run()
}

// Linux: the unit is `ssh` on Debian/Ubuntu and `sshd` on Fedora/RHEL/Arch,
// and the state is read without elevation first so a missing server never
// opens a password prompt it cannot satisfy. When the distribution has the
// socket unit enabled (Ubuntu 22.10 and later start sshd on demand from
// `ssh.socket`), starting that socket is what makes port 22 listen;
// otherwise `systemctl enable --now <unit>.service` starts the daemon and
// keeps it across reboots. `pkexec` shows the desktop's polkit prompt; the
// internal text agent is disabled so that a session without an agent fails
// immediately instead of waiting for a terminal that does not exist.
#[cfg(all(unix, not(target_os = "macos")))]
fn run_privileged_enable(install: bool) -> SshEnableResult {
    use super::ssh_install::{install_command_hint, LinuxPackageManager};
    use super::ssh_status::{probe_linux_ssh_units, LinuxSshProbe};

    let units = match probe_linux_ssh_units() {
        LinuxSshProbe::NoSystemctl => {
            return failed(
                "systemctl was not found, so Ghostex cannot start the SSH server. Start sshd with your init system."
                    .to_string(),
            )
        }
        LinuxSshProbe::NotInstalled => {
            let manager = LinuxPackageManager::detect();
            let Some(manager) = manager.filter(|_| install) else {
                return failed(format!(
                    "The OpenSSH server isn't installed on this computer. Install it with {}, then try again.",
                    install_command_hint(manager)
                ));
            };
            let manual = manager.user_command();
            return run_pkexec(
                &manager.root_install_and_start_script(),
                manual,
                |detail| {
                    format!(
                        "Installing the OpenSSH server failed: {detail}. Install it yourself with `{manual}` in a terminal, then try again."
                    )
                },
            );
        }
        LinuxSshProbe::Installed(units) => units,
    };
    let unit = units.unit;
    let command = if units
        .socket
        .as_ref()
        .is_some_and(|socket| socket.is_enabled())
    {
        format!("systemctl start {unit}.socket")
    } else {
        format!("systemctl enable --now {unit}.service")
    };
    let manual = format!("sudo {command}");
    run_pkexec(&command, &manual, |detail| {
        format!("Turning on SSH access failed: {detail}.")
    })
}

/// Runs `script` as root behind the desktop's polkit prompt. `manual` is what the user can type
/// instead; `failure` words a failed run from its output.
#[cfg(all(unix, not(target_os = "macos")))]
fn run_pkexec(script: &str, manual: &str, failure: impl Fn(&str) -> String) -> SshEnableResult {
    let output = Command::new("pkexec")
        .args(["--disable-internal-agent", "/bin/sh", "-c", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return failed(format!(
                "pkexec (polkit) is not installed, so Ghostex cannot show an administrator prompt. Run `{manual}` in a terminal instead."
            ))
        }
        Err(error) => {
            return failed(format!(
                "Could not open the administrator prompt: {error}."
            ))
        }
    };
    if output.status.success() {
        return enabled();
    }
    // pkexec(1): 126 when the user dismissed the authentication dialog, 127
    // when authorization could not be obtained (not authorized, no
    // authentication agent, or an error).
    match output.status.code() {
        Some(126) => cancelled(),
        Some(127) => failed(format!(
            "The administrator prompt did not authorize the change. If no password dialog appeared, this desktop session has no polkit authentication agent; run `{manual}` in a terminal instead."
        )),
        _ => {
            let combined = combined_output(&output.stdout, &output.stderr);
            failed(failure(&last_lines(
                &trimmed_or_exit_code(&combined, &output.status),
                3,
            )))
        }
    }
}

/// The last `count` non-empty lines of a command's output, joined: a package manager's useful
/// error is at the end of a long log.
#[cfg(all(unix, not(target_os = "macos")))]
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..]
        .join(" ")
        .trim_end_matches('.')
        .to_string()
}

#[cfg(not(any(target_os = "macos", windows, unix)))]
fn run_privileged_enable(_install: bool) -> SshEnableResult {
    failed("Ghostex cannot turn on SSH access on this platform.".to_string())
}

fn enabled() -> SshEnableResult {
    SshEnableResult {
        outcome: SshEnableOutcome::Enabled,
        message: None,
    }
}

fn cancelled() -> SshEnableResult {
    SshEnableResult {
        outcome: SshEnableOutcome::Cancelled,
        message: Some("The administrator prompt was dismissed.".to_string()),
    }
}

fn failed(message: String) -> SshEnableResult {
    SshEnableResult {
        outcome: SshEnableOutcome::Failed,
        message: Some(message),
    }
}

#[cfg(unix)]
fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .chars()
        .chain(stderr.chars())
        .take(4096)
        .collect::<String>()
}

#[cfg(unix)]
fn trimmed_or_exit_code(combined: &str, status: &std::process::ExitStatus) -> String {
    let trimmed = combined.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        format!("exit code {}", status.code().unwrap_or(-1))
    } else {
        trimmed.to_string()
    }
}
