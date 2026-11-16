//! The background install, update, reinstall or uninstall job the desktop app runs for a tool it
//! installs with the tool's official script (Trycua in cua_driver_job.rs, SpaceO in
//! spaceo_job.rs), and the `ghostexCliStatus` payload its once-a-second progress refresh repeats.

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::app::helpers::*;

const OUTPUT_LIMIT: usize = 16 * 1024;
const JOB_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Clone)]
struct JobState {
    operation: &'static str,
    status: &'static str,
    output: String,
    error: Option<String>,
}

/// One tool's job: at most one runs at a time, and the last one's result stays for Settings.
pub(crate) struct GpuiInstallJob {
    /// The product name its messages use.
    product: &'static str,
    state: Mutex<Option<JobState>>,
}

/// What an Install, Update, Reinstall or Uninstall button runs: a script for the background job
/// (bash, or Windows PowerShell), and how its completion is reported.
pub(crate) struct GpuiInstallJobAction {
    pub(crate) script: String,
    /// `install`, `update`, `reinstall` or `uninstall` (the job's `operation`).
    pub(crate) operation: &'static str,
    pub(crate) running_message: &'static str,
    pub(crate) toast_title: &'static str,
}

/// The last full `ghostexCliStatus` payload, so progress updates can repeat it with the job
/// fields refreshed instead of re-running every probe each second.
static LAST_STATUS: Mutex<Option<serde_json::Value>> = Mutex::new(None);

/// Adds every install job's fields to a `ghostexCliStatus` payload and remembers it for progress
/// updates.
pub(crate) fn gpui_decorate_ghostex_cli_status(payload: &mut serde_json::Value) {
    gpui_decorate_cua_driver_status(payload);
    gpui_decorate_spaceo_status(payload);
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(payload.clone());
    }
}

/// The last status with every job's current progress, for the once-a-second refresh while one runs.
pub(crate) fn gpui_install_job_progress_status_payload() -> serde_json::Value {
    let mut payload = LAST_STATUS
        .lock()
        .ok()
        .and_then(|last| last.clone())
        .unwrap_or_else(|| serde_json::json!({ "type": "ghostexCliStatus" }));
    payload["cuaDriverJob"] = CUA_DRIVER_JOB.json();
    payload["spaceoJob"] = SPACEO_JOB.json();
    payload
}

impl GpuiInstallJob {
    pub(crate) const fn new(product: &'static str) -> Self {
        Self {
            product,
            state: Mutex::new(None),
        }
    }

    pub(crate) fn running(&self) -> bool {
        self.state
            .lock()
            .ok()
            .and_then(|job| job.as_ref().map(|job| job.status == "running"))
            .unwrap_or(false)
    }

    /// The job's status payload field (`SidebarCuaDriverJob`).
    pub(crate) fn json(&self) -> serde_json::Value {
        self.state
            .lock()
            .ok()
            .and_then(|job| job.clone())
            .map_or(serde_json::Value::Null, |job| {
                serde_json::json!({
                    "operation": job.operation,
                    "status": job.status,
                    "output": job.output,
                    "error": job.error,
                })
            })
    }

    /// Registers a running job, or refuses when one is already running.
    pub(crate) fn begin(&self, operation: &'static str) -> Result<(), String> {
        let mut job = self.state.lock().map_err(|error| error.to_string())?;
        if job.as_ref().is_some_and(|job| job.status == "running") {
            return Err(format!(
                "{} is already being installed or changed. Wait for it to finish.",
                self.product
            ));
        }
        *job = Some(JobState {
            operation,
            status: "running",
            output: String::new(),
            error: None,
        });
        Ok(())
    }

    /// Adds one line of progress to the job's output (a job run from Rust, such as SpaceO's Repair).
    pub(crate) fn append_line(&self, line: &str) {
        self.append(&format!("{line}\n"));
    }

    fn append(&self, chunk: &str) {
        if let Ok(mut job) = self.state.lock()
            && let Some(job) = job.as_mut()
        {
            job.output.push_str(chunk);
            if job.output.len() > OUTPUT_LIMIT {
                let mut start = job.output.len() - OUTPUT_LIMIT;
                while !job.output.is_char_boundary(start) {
                    start += 1;
                }
                job.output.drain(..start);
            }
        }
    }

    pub(crate) fn finish(&self, result: &Result<(), String>) {
        if let Ok(mut job) = self.state.lock()
            && let Some(job) = job.as_mut()
        {
            job.status = if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            job.error = result.as_ref().err().cloned();
        }
    }

    /// Runs `script` to completion, streaming its output into the job. Blocking.
    pub(crate) fn run_script(&'static self, script: &str) -> Result<(), String> {
        let mut child = job_command(script)
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start the {} installer: {error}", self.product))?;
        let readers = [
            child.stdout.take().map(|pipe| self.drain(pipe)),
            child.stderr.take().map(|pipe| self.drain(pipe)),
        ];
        let deadline = Instant::now() + JOB_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    #[cfg(unix)]
                    // SAFETY: signals only the process group this job started.
                    unsafe {
                        libc::kill(-(child.id() as i32), libc::SIGKILL);
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "The {} installer did not finish within 20 minutes.",
                        self.product
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(250)),
                Err(error) => return Err(error.to_string()),
            }
        };
        for reader in readers.into_iter().flatten() {
            let _ = reader.join();
        }
        if status.success() {
            Ok(())
        } else {
            let last = self
                .state
                .lock()
                .ok()
                .and_then(|job| {
                    job.as_ref().and_then(|job| {
                        job.output
                            .lines()
                            .rev()
                            .map(str::trim)
                            .find(|line| !line.is_empty())
                            .map(str::to_string)
                    })
                })
                .unwrap_or_else(|| format!("The installer exited with {status}."));
            Err(last)
        }
    }

    fn drain(&'static self, mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = pipe.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                self.append(&String::from_utf8_lossy(&buffer[..read]));
            }
        })
    }
}

/// The program and arguments that run `script` headless: bash on macOS and Linux, Windows
/// PowerShell 5.1 (every Windows has it) with the process-scoped execution-policy bypass the
/// official installers ask for.
fn job_command(script: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = gpui_background_command("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!(
                "$ProgressPreference = 'SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12; {script}"
            ),
        ]);
        command
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new("/bin/bash");
        command.args(["-c", &format!("set -o pipefail; {script}")]);
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        command
    }
}
