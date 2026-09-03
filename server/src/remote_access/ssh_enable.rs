use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::Stdio;

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

// macOS: Remote Login is the `com.openssh.sshd` launchd job. `systemsetup
// -setremotelogin on` is not used because, since macOS 10.15, it demands that
// the *parent process* hold Full Disk Access (Apple support article 101653),
// which gxserver never has; enabling and bootstrapping the job with
// `launchctl` is the equivalent that is not TCC-gated. The bootstrap is
// skipped when `launchctl print` already finds the job, so the command is
// idempotent. `do shell script … with administrator privileges` shows the
// standard macOS administrator prompt.
#[cfg(target_os = "macos")]
fn run_privileged_enable() -> SshEnableResult {
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
// `Start-Process -Verb RunAs -Wait -PassThru` and forwards its exit code.
// Both scripts are written to temp files and run with `-File`, which keeps
// paths and quoting out of the command line entirely. The elevated process
// cannot share stdout with its parent, so it reports through exit codes and
// a log file the parent reads afterwards.
#[cfg(windows)]
mod windows_enable {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use super::{cancelled, enabled, failed, SshEnableResult};

    // Exit codes chosen by the elevated script. 1223 is Windows'
    // ERROR_CANCELLED, which is what a declined UAC prompt raises.
    const EXIT_CAPABILITY_FAILED: i32 = 2;
    const EXIT_SERVICE_FAILED: i32 = 3;
    const EXIT_RESTART_NEEDED: i32 = 4;
    const EXIT_FIREWALL_FAILED: i32 = 5;
    const EXIT_LAUNCH_FAILED: i32 = 91;
    const EXIT_NO_PROCESS: i32 = 92;
    const EXIT_NO_EXIT_CODE: i32 = 93;
    const EXIT_UAC_CANCELLED: i32 = 1223;

    const ELEVATED_SCRIPT: &str = r#"param([string]$LogPath)
$ErrorActionPreference = 'Stop'
function Note([string]$Text) { Add-Content -LiteralPath $LogPath -Value $Text }
try {
    if ($null -eq (Get-Service -Name sshd -ErrorAction SilentlyContinue)) {
        Note 'Installing the OpenSSH Server feature.'
        $result = Add-WindowsCapability -Online -Name 'OpenSSH.Server~~~~0.0.1.0'
        if ($null -eq (Get-Service -Name sshd -ErrorAction SilentlyContinue)) {
            if ($result.RestartNeeded) { exit 4 }
            Note 'The sshd service is still missing after the feature was installed.'
            exit 2
        }
    }
} catch {
    Note ('Add-WindowsCapability failed: ' + $_.Exception.Message)
    exit 2
}
try {
    Set-Service -Name sshd -StartupType Automatic
    if ((Get-Service -Name sshd).Status -ne 'Running') { Start-Service -Name sshd }
} catch {
    Note ('Starting the sshd service failed: ' + $_.Exception.Message)
    exit 3
}
try {
    if ($null -eq (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {
        Note 'Creating the OpenSSH-Server-In-TCP firewall rule.'
        New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 | Out-Null
    }
} catch {
    Note ('Creating the firewall rule failed: ' + $_.Exception.Message)
    exit 5
}
exit 0
"#;

    pub(super) fn run() -> SshEnableResult {
        let files = match TempFiles::create() {
            Ok(files) => files,
            Err(error) => {
                return failed(format!("Could not prepare the elevation script: {error}."))
            }
        };
        let mut command = Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&files.launcher)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        super::hide_console_window(&mut command);
        let output = match command.output() {
            Ok(output) => output,
            Err(error) => {
                return failed(format!("Could not open the administrator prompt: {error}."))
            }
        };
        let log = files.read_log();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        match output.status.code() {
            Some(0) => enabled(),
            Some(EXIT_UAC_CANCELLED) => cancelled(),
            Some(EXIT_CAPABILITY_FAILED) => failed(with_log(
                "Installing the OpenSSH Server feature failed",
                &log,
            )),
            Some(EXIT_SERVICE_FAILED) => failed(with_log(
                "Starting the OpenSSH Server service failed",
                &log,
            )),
            Some(EXIT_RESTART_NEEDED) => failed(
                "The OpenSSH Server feature was installed, but Windows needs a restart before the sshd service is available. Restart your computer, then try again."
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

    fn with_log(lead: &str, detail: &str) -> String {
        let detail = detail.trim();
        if detail.is_empty() {
            format!("{lead}.")
        } else {
            format!("{lead}: {detail}.")
        }
    }

    struct TempFiles {
        launcher: PathBuf,
        script: PathBuf,
        log: PathBuf,
    }

    impl TempFiles {
        fn create() -> std::io::Result<Self> {
            let stem = format!(
                "ghostex-ssh-enable-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default()
            );
            let dir = std::env::temp_dir();
            let files = Self {
                launcher: dir.join(format!("{stem}-launch.ps1")),
                script: dir.join(format!("{stem}.ps1")),
                log: dir.join(format!("{stem}.log")),
            };
            fs::write(&files.log, "")?;
            fs::write(&files.script, ELEVATED_SCRIPT)?;
            fs::write(&files.launcher, launcher_script(&files.script, &files.log))?;
            Ok(files)
        }

        fn read_log(&self) -> String {
            let text = fs::read_to_string(&self.log).unwrap_or_default();
            let lines = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>();
            lines.join(" ").chars().take(600).collect()
        }
    }

    impl Drop for TempFiles {
        fn drop(&mut self) {
            for path in [&self.launcher, &self.script, &self.log] {
                let _ = fs::remove_file(path);
            }
        }
    }

    // The launcher waits for the elevated PowerShell and exits with its exit
    // code. When the user declines the UAC prompt, `Start-Process` throws with
    // a Win32Exception whose NativeErrorCode is 1223 (ERROR_CANCELLED)
    // somewhere in the exception chain; that number is matched instead of the
    // message, which is localized.
    fn launcher_script(script: &Path, log: &Path) -> String {
        format!(
            r#"$ErrorActionPreference = 'Stop'
try {{
    $p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', '"{script}"', '"{log}"')
}} catch {{
    $e = $_.Exception
    while ($null -ne $e) {{
        if ($e -is [System.ComponentModel.Win32Exception] -and $e.NativeErrorCode -eq {EXIT_UAC_CANCELLED}) {{ exit {EXIT_UAC_CANCELLED} }}
        $e = $e.InnerException
    }}
    [Console]::Error.WriteLine($_.Exception.Message)
    exit {EXIT_LAUNCH_FAILED}
}}
if ($null -eq $p) {{ exit {EXIT_NO_PROCESS} }}
if ($null -eq $p.ExitCode) {{ exit {EXIT_NO_EXIT_CODE} }}
exit $p.ExitCode
"#,
            script = single_quoted_literal(script),
            log = single_quoted_literal(log),
        )
    }

    // Inside a single-quoted PowerShell string the only special character is
    // the quote itself, written as two quotes.
    fn single_quoted_literal(path: &Path) -> String {
        path.to_string_lossy().replace('\'', "''")
    }
}

#[cfg(windows)]
fn run_privileged_enable() -> SshEnableResult {
    windows_enable::run()
}

/// CREATE_NO_WINDOW, so helper PowerShell processes never flash a console.
#[cfg(windows)]
pub(crate) fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
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
fn run_privileged_enable() -> SshEnableResult {
    use super::ssh_status::{probe_linux_ssh_units, LinuxSshProbe};

    let units = match probe_linux_ssh_units() {
        LinuxSshProbe::NoSystemctl => {
            return failed(
                "systemctl was not found, so Ghostex cannot start the SSH server. Start sshd with your init system."
                    .to_string(),
            )
        }
        LinuxSshProbe::NotInstalled => {
            return failed(
                "The OpenSSH server is not installed. Install it with your package manager (for example `sudo apt install openssh-server`, `sudo dnf install openssh-server`, or `sudo pacman -S openssh`), then try again."
                    .to_string(),
            )
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
    let output = Command::new("pkexec")
        .args(["--disable-internal-agent", "/bin/sh", "-c", &command])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return failed(format!(
                "pkexec (polkit) is not installed, so Ghostex cannot show an administrator prompt. Run `sudo {command}` in a terminal instead."
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
            "The administrator prompt did not authorize the change. If no password dialog appeared, this desktop session has no polkit authentication agent; run `sudo {command}` in a terminal instead."
        )),
        _ => {
            let combined = combined_output(&output.stdout, &output.stderr);
            failed(format!(
                "Turning on SSH access failed: {}.",
                trimmed_or_exit_code(&combined, &output.status)
            ))
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, unix)))]
fn run_privileged_enable() -> SshEnableResult {
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
