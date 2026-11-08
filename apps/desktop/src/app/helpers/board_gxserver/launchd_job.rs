// launchd job inspection and sequencing for the local gxserver LaunchAgent
// (macOS only). `gpui_spawn_local_gxserver_daemon` in
// gxserver_health_and_daemon.rs drives these; they exist so every launchd step
// is checked against launchd's own state instead of launchctl's exit code.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::app::helpers::*;
use crate::*;

/// What `launchctl print gui/<uid>/<label>` reports for the gxserver job.
#[derive(Clone, Debug, Default)]
pub(crate) struct GpuiLaunchdJobSnapshot {
    /// The label is registered in the user's domain, running or not.
    pub(crate) loaded: bool,
    pub(crate) pid: Option<u32>,
    pub(crate) state: Option<String>,
    pub(crate) program: Option<String>,
}

impl GpuiLaunchdJobSnapshot {
    pub(crate) fn describe(&self) -> String {
        if !self.loaded {
            return "not loaded".to_string();
        }
        format!(
            "loaded, state {}, pid {}",
            self.state.as_deref().unwrap_or("unknown"),
            self.pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "none".to_string())
        )
    }
}

pub(crate) fn gpui_current_uid() -> u32 {
    unsafe { libc::getuid() }
}

/// The gxserver job in use: the app's bundled SMAppService agent once the app moved to it (the
/// legacy plist is gone and the agent is loaded), else the legacy `…gxserver` LaunchAgent.
pub(crate) fn gpui_gxserver_launchd_job_target() -> String {
    use ghostex_paths::macos_launch_agent as agents;
    let legacy = gpui_gxserver_launch_agent_label();
    let label = if legacy == agents::LEGACY_GXSERVER
        && !agents::legacy_plist_path(&legacy).exists()
        && agents::gui_job_loaded(agents::BUNDLED_GXSERVER)
    {
        agents::BUNDLED_GXSERVER.to_string()
    } else {
        legacy
    };
    format!("gui/{}/{label}", gpui_current_uid())
}

pub(crate) fn gpui_run_launchctl(arguments: &[&str]) -> GpuiCapturedCommandOutput {
    match gpui_run_command_with_captured_output_timeout(
        Path::new("/bin/launchctl"),
        arguments,
        Duration::from_secs(10),
        16 * 1024,
    ) {
        Ok(output) => output,
        Err(error) => GpuiCapturedCommandOutput {
            stderr: error,
            stdout: String::new(),
            success: false,
        },
    }
}

/// `launchctl print` nests `state = active` lines under the job's resource
/// limits, so only the first `pid`, `state`, and `program` lines count: those
/// are the job's own.
pub(crate) fn gpui_inspect_launchd_job(job_target: &str) -> Result<GpuiLaunchdJobSnapshot, String> {
    let output = gpui_run_launchctl(&["print", job_target]);
    if !output.success {
        let missing = format!(
            "Could not find service \"{}\"",
            job_target.rsplit('/').next().unwrap_or(job_target)
        );
        if output.stderr.contains(&missing) {
            return Ok(GpuiLaunchdJobSnapshot::default());
        }
        return Err(format!(
            "Could not inspect launchd job {job_target}: {}",
            gpui_launchctl_failure_text(&output)
        ));
    }
    let mut snapshot = GpuiLaunchdJobSnapshot {
        loaded: true,
        ..GpuiLaunchdJobSnapshot::default()
    };
    for line in output.stdout.lines() {
        let Some((key, value)) = line.trim().split_once(" = ") else {
            continue;
        };
        let value = value.trim();
        match key {
            "pid" if snapshot.pid.is_none() => snapshot.pid = value.parse().ok(),
            "state" if snapshot.state.is_none() => snapshot.state = Some(value.to_string()),
            "program" if snapshot.program.is_none() => snapshot.program = Some(value.to_string()),
            _ => {}
        }
    }
    Ok(snapshot)
}

/// Blocks until launchd has dropped the label, which happens only after the
/// job's process has exited; `launchctl bootout` itself returns as soon as the
/// signal is sent. Returns how long the removal took, or an error if the job is
/// still there at the deadline or inspection fails.
pub(crate) fn gpui_wait_for_launchd_job_removal(
    job_target: &str,
    timeout: Duration,
) -> Result<Duration, String> {
    let started = Instant::now();
    loop {
        let snapshot = gpui_inspect_launchd_job(job_target)?;
        if !snapshot.loaded {
            return Ok(started.elapsed());
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "launchd did not remove the previous gxserver job within {} seconds ({}).",
                timeout.as_secs(),
                snapshot.describe()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Blocks until the job reports a pid, i.e. launchd actually spawned it.
pub(crate) fn gpui_wait_for_launchd_job_pid(
    job_target: &str,
    timeout: Duration,
) -> Result<Option<u32>, String> {
    let started = Instant::now();
    loop {
        if let Some(pid) = gpui_inspect_launchd_job(job_target)?.pid {
            return Ok(Some(pid));
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn gpui_describe_launchctl_result(output: &GpuiCapturedCommandOutput) -> String {
    let text = gpui_launchctl_output_text(output);
    match (output.success, text.is_empty()) {
        (true, true) => "ok".to_string(),
        (true, false) => format!("ok ({text})"),
        (false, true) => "failed (no output)".to_string(),
        (false, false) => format!("failed ({text})"),
    }
}

pub(crate) fn gpui_launchctl_failure_text(output: &GpuiCapturedCommandOutput) -> String {
    let text = gpui_launchctl_output_text(output);
    if text.is_empty() {
        "launchctl gave no output".to_string()
    } else {
        text
    }
}

fn gpui_launchctl_output_text(output: &GpuiCapturedCommandOutput) -> String {
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    output.stdout.trim().to_string()
}

pub(crate) fn gpui_log_launchd_spawn_outcome(
    bootstrapped: bool,
    kickstarted: bool,
    pid: Option<u32>,
    steps: &[String],
) {
    support_logs::append(
        support_logs::GpuiSupportLog::HostLifecycle,
        "gpui.gxserverBootstrap.launchdAgent",
        serde_json::json!({
            "bootstrapped": bootstrapped,
            "kickstarted": kickstarted,
            "pid": pid,
            "steps": steps,
        }),
    );
}
