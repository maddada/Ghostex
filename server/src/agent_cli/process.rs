use crate::platform::process::NoConsoleWindow;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

/// Re-probed now (and written back to the hook-status cache), so the Agents page and hook status agree
/// right after an install. Windows reads the live registry PATH through `platform::live_path`.
pub(crate) fn resolve(binary: &str, home: &Path) -> Option<String> {
    crate::agent_hooks::probing::resolve_cli_command(binary, home)
}

/// The agent's binary on PATH, otherwise in one of the folders its official installer uses. An
/// install-on-demand wrapper counts as not installed yet, because reading its version would install
/// the agent (`agent_hooks/on_demand.rs`).
pub(crate) fn resolve_agent(binary: &str, install_dirs: &[PathBuf], home: &Path) -> Option<String> {
    let path = resolve(binary, home).or_else(|| {
        crate::platform::live_path::find_in(binary, install_dirs)
            .map(|path| path.to_string_lossy().into_owned())
    })?;
    crate::agent_hooks::on_demand::probe_target(&path, binary, home)
}

/// Windows PowerShell 5.1, which every Windows has and every vendor installer is written for (Cursor's calls
/// `Get-WmiObject`, which PowerShell 7 only reaches through a compatibility session).
#[cfg(windows)]
fn windows_powershell() -> String {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
        .to_string_lossy()
        .into_owned()
}

fn command(script: &str, home: &Path, env: &BTreeMap<String, String>) -> Command {
    #[cfg(windows)]
    let shell = crate::platform::shell::command_shell_for_path(&windows_powershell());
    #[cfg(not(windows))]
    let shell = if script.contains('|') {
        crate::platform::shell::command_shell_for_path("/bin/bash")
    } else {
        crate::platform::shell::command_shell()
    };
    let mut command = Command::new(&shell.executable);
    #[cfg(unix)]
    command.process_group(0);
    command.no_console_window();
    /*
    CDXC:AgentProviders 2026-09-28 WHY:
    A fresh Windows keeps the Restricted execution policy, which refuses npm's `npm.ps1` shim and scripts an installer starts, so the job runs with a process-scoped Bypass as the installers' own instructions do. `$ErrorActionPreference = 'Stop'` is not set around the command: it leaks into `irm … | iex` vendor scripts and turned Cursor's harmless `Get-WmiObject` warning into an abort after its installer had already deleted the previous install. A terminating error (a failed download) or a non-zero exit code fails the job, and every job is re-checked afterwards. Progress bars are off (they slow downloads in 5.1), TLS 1.2 is forced for 5.1, and output is UTF-8 so installer check marks survive.
    Every stream is merged and printed as text: under -EncodedCommand with redirected output, PowerShell 5.1 serialized the installers' Write-Host lines as CLIXML, which the job output then showed as XML.
    */
    #[cfg(windows)]
    let script = format!(
        "$ProgressPreference = 'SilentlyContinue'; [Console]::OutputEncoding = [Text.Encoding]::UTF8; [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12; $global:LASTEXITCODE = 0; $global:GhostexJobFailed = $false; & {{ try {{ {script} }} catch {{ $_; $global:GhostexJobFailed = $true }} }} *>&1 | Out-String -Stream -Width 240; if ($global:GhostexJobFailed) {{ exit 1 }}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}"
    );
    #[cfg(not(windows))]
    let script = if script.contains('|') {
        // The download must fail the operation even when the installer receives an empty pipe.
        format!("set -o pipefail; {script}")
    } else {
        script.to_string()
    };
    #[cfg(windows)]
    {
        command.args(["-ExecutionPolicy", "Bypass"]);
        // A tool installed a moment ago (by this job or another program) is on the registry PATH, not on ours.
        command.env("PATH", crate::platform::live_path::value());
    }
    command.args(shell.profileless_script_args(&script));
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    command
        .current_dir(home)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

pub(crate) fn quote(value: &str) -> String {
    if cfg!(windows) {
        crate::platform::shell::powershell_single_quote(value)
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub(crate) async fn run(
    script: &str,
    home: &Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
    output: impl Fn(String) + Clone + Send + 'static,
) -> Result<(), String> {
    wait(command(script, home, env), timeout, output).await
}

/// Runs `executable args…` directly, without a shell per probe. Only a PowerShell script shim needs one.
pub(crate) async fn run_executable(
    executable: &str,
    args: &[String],
    home: &Path,
    timeout: Duration,
    output: impl Fn(String) + Clone + Send + 'static,
) -> Result<(), String> {
    if executable.to_ascii_lowercase().ends_with(".ps1") {
        let script = format!(
            "& {} {}",
            quote(executable),
            args.iter()
                .map(|arg| quote(arg))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let mut env = BTreeMap::new();
        if is_agy(executable) {
            let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
            env.insert(key.to_string(), value.to_string());
        }
        return run(&script, home, &env, timeout, output).await;
    }
    let mut command = Command::new(executable);
    #[cfg(unix)]
    command.process_group(0);
    command.no_console_window();
    #[cfg(windows)]
    command.env("PATH", crate::platform::live_path::value());
    command
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // See `AGY_BACKGROUND_AUTO_UPDATE_OFF` (issue #397): this runs the Agents Hub's version and update checks.
    if is_agy(executable) {
        let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
        command.env(key, value);
    }
    wait(command, timeout, output).await
}

fn is_agy(executable: &str) -> bool {
    Path::new(executable)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("agy"))
}

async fn wait(
    mut command: Command,
    timeout: Duration,
    output: impl Fn(String) + Clone + Send + 'static,
) -> Result<(), String> {
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or("CLI stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("CLI stderr unavailable")?;
    let stdout_task = tokio::spawn(drain(stdout, output.clone()));
    let stderr_task = tokio::spawn(drain(stderr, output));
    let result = tokio::time::timeout(timeout, child.wait()).await;
    if result.is_err() {
        #[cfg(unix)]
        if let Some(pid) = pid {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        if let Some(pid) = pid {
            let mut kill = Command::new("taskkill.exe");
            kill.no_console_window();
            let _ = kill
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output()
                .await;
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    // An installer can leave a detached child holding its pipes open.
    for mut task in [stdout_task, stderr_task] {
        if tokio::time::timeout(Duration::from_secs(2), &mut task)
            .await
            .is_err()
        {
            task.abort();
        }
    }
    match result {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("Command exited with {status}.")),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err(format!(
            "Command timed out after {} seconds.",
            timeout.as_secs()
        )),
    }
}

async fn drain(mut stream: impl tokio::io::AsyncRead + Unpin, output: impl Fn(String)) {
    let mut buffer = [0; 4096];
    while let Ok(count) = stream.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        output(String::from_utf8_lossy(&buffer[..count]).into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::is_agy;

    #[test]
    fn agy_is_recognised_by_its_file_name() {
        assert!(is_agy("/Users/me/.local/bin/agy"));
        assert!(is_agy("agy.exe"));
        assert!(!is_agy("/usr/local/bin/codex"));
    }
}
