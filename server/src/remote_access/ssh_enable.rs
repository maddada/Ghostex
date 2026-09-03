use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::ssh_status::ssh_port_accepts_connections;

const SSH_LISTEN_WAIT: Duration = Duration::from_secs(4);

/*
CDXC:RemotePairing 2026-09-03:
Turning SSH access on is the one privileged step in pairing, and it runs from
gxserver rather than the desktop app because gxserver lives in the user's GUI
session on every OS (so the admin prompt can appear) and the CLI and web
Settings page then share the same code path. A declined prompt is a distinct
outcome, not a failure: the UI answers it with the per-OS manual steps.
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

pub fn enable_ssh_access(port: u16) -> SshEnableResult {
    if ssh_port_accepts_connections(port) {
        return SshEnableResult {
            outcome: SshEnableOutcome::Enabled,
            message: None,
        };
    }
    let result = run_privileged_enable();
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

#[cfg(target_os = "macos")]
fn run_privileged_enable() -> SshEnableResult {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg("do shell script \"/usr/sbin/systemsetup -setremotelogin on\" with administrator privileges")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return failed(format!("Could not open the administrator prompt: {error}")),
    };
    if output.status.success() {
        return enabled();
    }
    let combined = combined_output(&output.stdout, &output.stderr);
    let normalized = combined.to_lowercase();
    // osascript reports a dismissed prompt as AppleScript error -128.
    if normalized.contains("user canceled")
        || normalized.contains("user cancelled")
        || normalized.contains("(-128)")
    {
        return cancelled();
    }
    failed(format!(
        "Turning on SSH access failed: {}",
        trimmed_or_exit_code(&combined, &output.status)
    ))
}

#[cfg(windows)]
fn run_privileged_enable() -> SshEnableResult {
    // Unverified on a real Windows machine: elevates one PowerShell that
    // installs the OpenSSH Server capability and starts the sshd service.
    const ELEVATED_SCRIPT: &str = "Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0; Set-Service -Name sshd -StartupType Automatic; Start-Service -Name sshd";
    let launcher = format!(
        "$p = Start-Process -FilePath powershell -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile','-NonInteractive','-Command','{ELEVATED_SCRIPT}'; exit $p.ExitCode"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &launcher])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return failed(format!("Could not open the administrator prompt: {error}")),
    };
    if output.status.success() {
        return enabled();
    }
    let combined = combined_output(&output.stdout, &output.stderr);
    // A declined UAC prompt makes Start-Process throw "The operation was
    // canceled by the user."
    if combined.to_lowercase().contains("canceled by the user") {
        return cancelled();
    }
    failed(format!(
        "Turning on SSH access failed: {}",
        trimmed_or_exit_code(&combined, &output.status)
    ))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn run_privileged_enable() -> SshEnableResult {
    // Unverified on a real Linux machine: pkexec shows the desktop's polkit
    // prompt; Debian/Ubuntu name the unit `ssh`, Fedora/Arch name it `sshd`.
    let output = Command::new("pkexec")
        .args([
            "sh",
            "-c",
            "systemctl enable --now ssh || systemctl enable --now sshd",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return failed(format!("Could not open the administrator prompt: {error}")),
    };
    if output.status.success() {
        return enabled();
    }
    // pkexec exits 126 when the user dismisses the prompt and 127 when the
    // user is not authorized.
    if output.status.code() == Some(126) {
        return cancelled();
    }
    let combined = combined_output(&output.stdout, &output.stderr);
    failed(format!(
        "Turning on SSH access failed: {}",
        trimmed_or_exit_code(&combined, &output.status)
    ))
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

fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .chars()
        .chain(stderr.chars())
        .take(4096)
        .collect::<String>()
}

fn trimmed_or_exit_code(combined: &str, status: &std::process::ExitStatus) -> String {
    let trimmed = combined.trim();
    if trimmed.is_empty() {
        format!("exit code {}", status.code().unwrap_or(-1))
    } else {
        trimmed.to_string()
    }
}
