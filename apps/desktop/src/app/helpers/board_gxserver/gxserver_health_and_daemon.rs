// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds gxserver RPC/health probing, local binary resolution, and
// local daemon spawn/restart (macOS launchd, Linux, Windows) helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

/// A refused gxserver call: the sentence to show, and gxserver's error code when it sent one
/// (`accountUnavailable`, `notFound`, …), so a caller can tell a refusal that waiting cannot fix
/// from one that can.
#[derive(Clone, Debug)]
pub(crate) struct GpuiGxserverRpcError {
    pub(crate) code: Option<String>,
    pub(crate) message: String,
}

impl From<String> for GpuiGxserverRpcError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            message,
        }
    }
}

/// `gpui_gxserver_rpc_result`, keeping gxserver's error code.
pub(crate) fn gpui_gxserver_rpc_result_with_code(
    endpoint: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, GpuiGxserverRpcError> {
    let (status_code, body) = gxserver_post_typed_operation(endpoint, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(GpuiGxserverRpcError {
            code: serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value.get("error")?.as_str().map(str::to_string)),
            message: gpui_gxserver_rpc_error_message(status_code, &body),
        });
    }
    Ok(parse_gpui_gxserver_rpc_result(&body)?)
}

pub(crate) fn gpui_gxserver_rpc_result(
    endpoint: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:StatusPet 2026-06-24-11:48:
    Settings status/actions share the typed-operation transport and may expose only the validated gxserver `result` object to the modal host. Transport, status, envelope, and parse failures stay as local errors so callers can clear loading with explicit empty status payloads without logging private daemon data.
    */
    let (status_code, body) = gxserver_post_typed_operation(endpoint, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(gpui_gxserver_rpc_error_message(status_code, &body));
    }
    parse_gpui_gxserver_rpc_result(&body)
}

/// CDXC:ServerApi 2026-09-28 WHY:
/// Every refusal here surfaced as "gxserver request failed with HTTP 503." in toasts such as "Session attach unavailable", hiding the reason gxserver sent (a wmx timeout, a missing project folder, a session that could not be woken). gxserver's `{ok:false, error:<code>, message}` envelope carries that reason as plain user-facing text, so show it and keep the status line only as the fallback.
fn gpui_gxserver_rpc_error_message(status_code: u16, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("gxserver request failed with HTTP {status_code}."))
}

pub(crate) fn gpui_gxserver_server_health(timeout: Duration) -> Result<serde_json::Value, String> {
    let (status_code, body) = gxserver_get_typed_operation("/api/health/server", timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver health failed with HTTP {status_code}."));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|_| "gxserver health returned invalid JSON.".to_string())
}

/// Pid the local daemon reports on `/api/health/server`, read before asking it
/// to stop so the caller can wait for the process itself and not just its port.
pub(crate) fn gpui_local_gxserver_health_pid() -> Option<u32> {
    let health = gpui_gxserver_server_health(Duration::from_millis(1000)).ok()?;
    let pid = health.get("pid")?.as_u64()?;
    u32::try_from(pid).ok().filter(|pid| *pid > 0)
}

/*
CDXC:ServerDaemon 2026-09-16 WHY:
`/api/control/stop` closes the listener first and the process exits only after its shutdown tail (telemetry flush, tailcat stop, observer teardown), so "port unreachable" arrives seconds before "process gone".
Restart paths that spawned on the first signal raced launchd's removal of the old job and ended with no daemon at all (see gpui_spawn_local_gxserver_daemon). Callers wait for the process, bounded, before starting the next one.
*/
pub(crate) fn gpui_wait_for_local_gxserver_process_exit(
    previous_pid: Option<u32>,
    timeout: Duration,
) -> Result<(), String> {
    let pid = previous_pid.ok_or_else(|| {
        "Could not identify the previous gxserver process to verify shutdown.".to_string()
    })?;
    let started = std::time::Instant::now();
    loop {
        if !gpui_local_process_is_alive(pid)? {
            return Ok(());
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "gxserver process {pid} did not exit within {} seconds.",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(unix)]
fn gpui_local_process_is_alive(pid: u32) -> Result<bool, String> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| "Invalid gxserver process ID.".to_string())?;
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err(format!("Could not inspect gxserver process {pid}: {error}")),
    }
}

#[cfg(windows)]
fn gpui_local_process_is_alive(pid: u32) -> Result<bool, String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    // CDXC:ServerDaemon 2026-09-16 WHY:
    // A WSL daemon reports a Linux PID, which must be inspected in its selected distribution, never in the Windows process table.
    if let windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { distribution } =
        windows_terminal_backend::resolve_current()?
    {
        let script = format!("if test -d /proc/{pid}; then printf alive; else printf gone; fi");
        let output = gpui_run_command_with_captured_output_timeout(
            Path::new("wsl.exe"),
            &[
                "--distribution",
                &distribution,
                "--exec",
                "sh",
                "-c",
                &script,
            ],
            Duration::from_secs(3),
            16 * 1024,
        )?;
        return match (output.success, output.stdout.trim()) {
            (true, "alive") => Ok(true),
            (true, "gone") => Ok(false),
            _ => Err(format!(
                "Could not inspect gxserver process in WSL: {}",
                output.combined_text()
            )),
        };
    }

    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Ok(false)
        } else {
            Err(format!("Could not inspect gxserver process {pid}: {error}"))
        };
    }
    let status = unsafe { WaitForSingleObject(process, 0) };
    let error = std::io::Error::last_os_error();
    unsafe { CloseHandle(process) };
    match status {
        WAIT_OBJECT_0 => Ok(false),
        WAIT_TIMEOUT => Ok(true),
        _ => Err(format!("Could not inspect gxserver process {pid}: {error}")),
    }
}

/// Ends a local gxserver process that will not stop when asked. Native processes only: a WSL daemon is never taken over this way.
#[cfg(unix)]
pub(crate) fn gpui_terminate_local_gxserver_process(pid: u32) -> Result<(), String> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| "Invalid gxserver process ID.".to_string())?;
    if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    Err(format!(
        "Could not end gxserver process {pid}: {}",
        std::io::Error::last_os_error()
    ))
}

#[cfg(windows)]
pub(crate) fn gpui_terminate_local_gxserver_process(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess,
            WaitForSingleObject,
        },
    };
    // A WSL daemon reports a Linux pid, which names some other Windows process.
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        return Err(format!(
            "gxserver process {pid} runs inside WSL and was not ended."
        ));
    }
    let process = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return Err(format!(
            "Could not end gxserver process {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe { TerminateProcess(process, 1) };
    let waited = unsafe { WaitForSingleObject(process, 10_000) };
    unsafe { CloseHandle(process) };
    if waited == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(format!(
            "gxserver process {pid} did not exit after it was ended."
        ))
    }
}

/// CDXC:ServerDaemon 2026-10-12 WHY:
/// After a restart the port can be held by a gxserver of another build: one a `gxserver start` from the SSH side started from the not yet refreshed package, which the app's package refresh then marked `superseded`. The startup loop used to wait for it to go away for its whole patience and then show the startup diagnostics while the app ran on that old build. It is asked to stop, ended if it keeps answering, and the bundled daemon is started; returns the steps for the diagnostics report.
/// SEE-ALSO: `bind_local_listener_taking_over_stale_holder` in server/src/server/stale_server_takeover.rs, which takes the port from a holder that does not answer at all.
pub(crate) fn gpui_take_over_local_gxserver_from_other_build(binary: &Path) -> Vec<String> {
    let started = std::time::Instant::now();
    let mut steps = Vec::new();
    let mut note = |text: String| {
        steps.push(format!(
            "takeover +{}ms: {text}",
            started.elapsed().as_millis()
        ))
    };
    let pid = gpui_local_gxserver_health_pid();
    let stop = gxserver_post_typed_operation(
        "/api/control/stop",
        &serde_json::json!({}),
        Duration::from_secs(5),
    );
    note(format!(
        "asked gxserver {} of another build to stop: {}",
        pid.map_or_else(|| "(pid unknown)".to_string(), |pid| pid.to_string()),
        match &stop {
            Ok((status, _)) => format!("HTTP {status}"),
            Err(error) => error.clone(),
        }
    ));
    let mut still_answering = true;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        match gpui_probe_local_gxserver_health() {
            GpuiLocalGxserverHealthState::BuildMismatch => {}
            GpuiLocalGxserverHealthState::Healthy { .. } => {
                note("the bundled build already answers".to_string());
                return steps;
            }
            GpuiLocalGxserverHealthState::Unreachable
            | GpuiLocalGxserverHealthState::Starting { .. }
            | GpuiLocalGxserverHealthState::ProtocolMismatch { .. } => {
                still_answering = false;
                break;
            }
        }
    }
    if still_answering {
        match pid {
            Some(pid) => note(match gpui_terminate_local_gxserver_process(pid) {
                Ok(()) => format!("it kept answering, so process {pid} was ended"),
                Err(error) => error,
            }),
            None => note("it kept answering and reported no pid".to_string()),
        }
    } else if let Some(pid) = pid {
        // CDXC:ServerDaemon 2026-10-17 WHY: the port goes quiet the moment the stop arrives, but the process can live on (until 2026-10-17 a held request kept it alive indefinitely, see server/src/server/shutdown_bounds.rs), and the replacement must not start beside it.
        if gpui_wait_for_local_gxserver_process_exit(Some(pid), Duration::from_secs(15)).is_err() {
            note(match gpui_terminate_local_gxserver_process(pid) {
                Ok(()) => format!("process {pid} outlived its stop, so it was ended"),
                Err(error) => error,
            });
        }
    }
    match gpui_spawn_local_gxserver_daemon(binary) {
        Ok(launch_steps) => steps.extend(launch_steps),
        Err(error) => steps.push(format!("launcher failed: {error}")),
    }
    steps
}

pub(crate) const GPUI_GXSERVER_DAEMON_TOAST_ID: &str = "toast-gxserver-daemon";

/// How long a freshly spawned gxserver may take to answer its health check before startup counts as failed.
/// A cold first launch on Windows spends most of a minute in the antivirus scan of the new executable.
pub(crate) const GPUI_GXSERVER_START_PATIENCE: Duration = Duration::from_secs(180);
pub(crate) const GPUI_MISSING_MONACO_PROMPT_EDITOR_TOAST_ID: &str =
    "toast-monaco-prompt-editor-missing";
pub(crate) const GPUI_GXSERVER_EXPECTED_PRODUCT: &str = "gxserver";

pub(crate) enum GpuiLocalGxserverHealthState {
    Healthy {
        tools_available: bool,
    },
    BuildMismatch,
    ProtocolMismatch {
        reported: Option<u64>,
    },
    /// A gxserver holds the port but is still starting (CDXC:ServerDaemon 2026-10-22 DECISION in server/src/server/startup_gate.rs).
    Starting {
        starting_for: Duration,
    },
    Unreachable,
}

/// How long and in which step a gxserver said it is still starting, read from its HTTP 503 `serverStarting` answer; `None` for any other answer, so an older gxserver (which never sends it) reads as before.
pub(crate) fn gpui_gxserver_starting_answer(status: u16, body: &str) -> Option<(Duration, String)> {
    if status != 503 {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    if value.get("product").and_then(serde_json::Value::as_str)
        != Some(GPUI_GXSERVER_EXPECTED_PRODUCT)
        || value.get("error").and_then(serde_json::Value::as_str) != Some("serverStarting")
    {
        return None;
    }
    Some((
        Duration::from_millis(
            value
                .get("startingForMs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        ),
        value
            .get("step")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("starting")
            .to_string(),
    ))
}

/// CDXC:ServerDaemon 2026-10-25 WHY:
/// On 2026-10-25 a `gxserver start` run elsewhere bound the port in the ~4 s between `cargo xtask start` stopping the old gxserver and the app's launchd start, so the job's gxserver found the port taken and exited after ~20 ms, twice; the 3 s pid wait never saw either run, the launcher reported "launchd loaded the gxserver job but never spawned it", and the app showed the startup failure report while the other gxserver was healthy. A start that ends while a gxserver of this build answers (or is still starting) is not a failure: the app uses that one. Another build or protocol still counts as not started, so the startup loop takes it over or reports it.
pub(crate) fn gpui_gxserver_already_answers_for_launch(
    health: &GpuiLocalGxserverHealthState,
) -> bool {
    matches!(
        health,
        GpuiLocalGxserverHealthState::Healthy { .. }
            | GpuiLocalGxserverHealthState::Starting { .. }
    )
}

/// Whether a launcher error is the final word on startup. Only a port nobody answers on is: any other answer goes to the startup loop, which uses a healthy gxserver, waits for a starting one, takes over another build and reports a protocol mismatch.
pub(crate) fn gpui_gxserver_launch_error_is_final(health: &GpuiLocalGxserverHealthState) -> bool {
    matches!(health, GpuiLocalGxserverHealthState::Unreachable)
}

/// Mirrors the macOS GxserverClient handshake: authenticated health, product
/// check, hard protocol-version and build-identity matches, and availability
/// of the tools shipped beside this GPUI build's gxserver binary.
pub(crate) fn gpui_probe_local_gxserver_health() -> GpuiLocalGxserverHealthState {
    gpui_probe_local_gxserver_health_with_diagnostics().0
}

pub(crate) fn gpui_probe_local_gxserver_health_with_diagnostics()
-> (GpuiLocalGxserverHealthState, String) {
    let health =
        match gxserver_get_typed_operation("/api/health/server", Duration::from_millis(1000)) {
            Ok((status, body)) if (200..300).contains(&status) => {
                match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(health) => health,
                    Err(_) => {
                        return (
                            GpuiLocalGxserverHealthState::Unreachable,
                            "gxserver health returned invalid JSON.".into(),
                        );
                    }
                }
            }
            Ok((status, body)) => {
                if let Some((starting_for, step)) = gpui_gxserver_starting_answer(status, &body) {
                    let detail = format!(
                        "gxserver is still starting ({step}, {} s so far).",
                        starting_for.as_secs()
                    );
                    return (
                        GpuiLocalGxserverHealthState::Starting { starting_for },
                        detail,
                    );
                }
                return (
                    GpuiLocalGxserverHealthState::Unreachable,
                    format!("gxserver health failed with HTTP {status}."),
                );
            }
            Err(reason) => return (GpuiLocalGxserverHealthState::Unreachable, reason),
        };
    if health.get("product").and_then(serde_json::Value::as_str)
        != Some(GPUI_GXSERVER_EXPECTED_PRODUCT)
    {
        return (
            GpuiLocalGxserverHealthState::Unreachable,
            "Health endpoint returned an unexpected product.".into(),
        );
    }
    let reported_protocol = health
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64);
    if reported_protocol != Some(GPUI_GXSERVER_PROTOCOL_VERSION) {
        return (
            GpuiLocalGxserverHealthState::ProtocolMismatch {
                reported: reported_protocol,
            },
            gpui_gxserver_protocol_mismatch_message(reported_protocol),
        );
    }
    if let Some(expected_build_identity) = gpui_expected_local_gxserver_build_identity() {
        let reported_build_identity = health
            .get("buildIdentity")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if reported_build_identity != Some(expected_build_identity.as_str()) {
            return (
                GpuiLocalGxserverHealthState::BuildMismatch,
                format!(
                    "Build mismatch: expected {expected_build_identity}; reported {}.",
                    reported_build_identity.unwrap_or("missing")
                ),
            );
        }
    }
    let tools_available = gpui_gxserver_required_tools_available(&health);
    (
        GpuiLocalGxserverHealthState::Healthy { tools_available },
        format!("Health OK; required tools available: {tools_available}."),
    )
}

pub(crate) fn gpui_expected_local_gxserver_build_identity() -> Option<String> {
    #[cfg(target_os = "windows")]
    if windows_terminal_backend::current_preference()
        == windows_terminal_backend::WindowsTerminalBackendPreference::Wsl
    {
        return None;
    }
    let binary = gpui_resolve_local_gxserver_binary()?;
    let package_root = binary.parent()?.parent()?;
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(package_root.join("build-identity.json")).ok()?)
            .ok()?;
    value
        .get("buildIdentity")?
        .as_str()
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_string)
}

/// A running app can outlive replacement of its app bundle during an update.
/// Reconcile that version skew at the durable Delayed Send command boundary so
/// the user's submission is persisted by the daemon that implements the
/// bundled contract instead of being accepted by the modal and rejected by an
/// obsolete control plane.
pub(crate) fn gpui_schedule_agents_delayed_send_with_current_gxserver_build(
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if matches!(
        gpui_probe_local_gxserver_health(),
        GpuiLocalGxserverHealthState::BuildMismatch
    ) {
        gpui_restart_local_gxserver_for_current_build()?;
    }
    gpui_gxserver_rpc_result("/api/scheduleDelayedSend", params, Duration::from_secs(5))
}

pub(crate) fn gpui_restart_local_gxserver_for_current_build() -> Result<(), String> {
    let previous_pid = gpui_local_gxserver_health_pid();
    let (status_code, _) = gxserver_post_typed_operation(
        "/api/control/stop",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver stop failed with HTTP {status_code}."));
    }

    let mut stopped = false;
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(250));
        if matches!(
            gpui_probe_local_gxserver_health(),
            GpuiLocalGxserverHealthState::Unreachable
        ) {
            stopped = true;
            break;
        }
    }
    if !stopped {
        return Err("gxserver did not stop before its build update.".to_string());
    }
    gpui_wait_for_local_gxserver_process_exit(previous_pid, Duration::from_secs(15)).or_else(
        |error| match previous_pid {
            Some(pid) => gpui_terminate_local_gxserver_process(pid),
            None => Err(error),
        },
    )?;

    let binary = gpui_resolve_local_gxserver_binary()
        .ok_or_else(|| "Bundled gxserver binary is missing.".to_string())?;
    let _launch_steps = gpui_spawn_local_gxserver_daemon(&binary)?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        match gpui_probe_local_gxserver_health() {
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: true,
            } => return Ok(()),
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: false,
            } => {
                return Err(
                    "The current gxserver build is missing its required toolchain.".to_string(),
                );
            }
            GpuiLocalGxserverHealthState::ProtocolMismatch { .. } => {
                return Err("The current gxserver build has an incompatible protocol.".to_string());
            }
            GpuiLocalGxserverHealthState::BuildMismatch
            | GpuiLocalGxserverHealthState::Starting { .. }
            | GpuiLocalGxserverHealthState::Unreachable => {}
        }
    }
    Err("The current gxserver build did not become healthy in time.".to_string())
}

pub(crate) fn gpui_gxserver_required_tools_available(health: &serde_json::Value) -> bool {
    let Some(tools) = health.get("tools").and_then(serde_json::Value::as_array) else {
        return true;
    };
    // zmx is the one mandatory GPUI daemon companion.
    //
    // CDXC:PromptSearch 2026-08-20: Zehn used to be gated here too,
    // because it was a separate bundled binary a build might not carry. It is
    // now a Rust crate compiled into gxserver, so every daemon that exists can
    // serve prompt-history search and there is nothing left to probe for.
    //
    // Beads is deliberately excluded. gxserver resolves `bd` from the user's
    // machine-installed Beads release (see `system_bd_tool_candidates`) and
    // never from the app bundle, so a missing `bd` reports the operator's
    // environment, not a stale daemon. Gating on it made a first launch on a
    // machine without Beads report "gxserver toolchain unavailable" and then
    // restart a perfectly healthy daemon on every later launch. Project board
    // surfaces already carry their own install guidance for that case.
    let required_tools = ["zmx"];
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        return required_tools.iter().all(|required| {
            tools.iter().any(|tool| {
                tool.get("tool").and_then(serde_json::Value::as_str) == Some(*required)
                    && tool.get("availability").and_then(serde_json::Value::as_str)
                        == Some("available")
            })
        });
    }
    required_tools.iter().all(|required| {
        tools.iter().any(|tool| {
            tool.get("tool").and_then(serde_json::Value::as_str) == Some(*required)
                && tool.get("availability").and_then(serde_json::Value::as_str) == Some("available")
        })
    })
}

/// Resolution order is explicit env selection, this app bundle's Resources,
/// then the GPUI-owned development runtime next to this crate. Only native
/// gxserver executables are launched.
pub(crate) fn gpui_resolve_local_gxserver_binary() -> Option<PathBuf> {
    for key in ["GHOSTEX_GXSERVER_CLI", "GHOSTEX_GXSERVER_BIN"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                let path = PathBuf::from(trimmed);
                if path.is_absolute() && gpui_is_executable_file(&path) {
                    return Some(path);
                }
                return None;
            }
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        #[cfg(windows)]
        if let Some(directory) = current_exe.parent() {
            candidates.push(directory.join("resources/native/gxserver.exe"));
        }
        if let Some(contents_dir) = current_exe.parent().and_then(Path::parent) {
            candidates.push(contents_dir.join("Resources/Web/gxserver/bin/gxserver"));
        }
        // CDXC:PlatformSupport 2026-07-05: the Linux app is a flat
        // CEF-conventional directory (scripts/build-linux-app.sh), so the
        // bundled gxserver package sits beside the executable instead of
        // under a macOS Contents/Resources tree.
        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join("gxserver/bin/gxserver"));
        }
    }
    if let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
    {
        candidates.push(repo_root.join("apps/desktop/runtime/macos/Web/gxserver/bin/gxserver"));
        // Linux dev runs resolve the package produced by
        // `bun server/package-remote-linux.mjs` before any packaging step.
        #[cfg(target_os = "linux")]
        {
            #[cfg(target_arch = "x86_64")]
            const GPUI_LINUX_GXSERVER_PACKAGE_ARCH: &str = "x64";
            #[cfg(target_arch = "aarch64")]
            const GPUI_LINUX_GXSERVER_PACKAGE_ARCH: &str = "arm64";
            candidates.push(repo_root.join(format!(
                "build/remote-gxserver-linux/{GPUI_LINUX_GXSERVER_PACKAGE_ARCH}/package/bin/gxserver"
            )));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| gpui_is_executable_file(candidate))
}

/// The bundled zmx binary ships beside the bundled gxserver binary in every
/// GPUI package layout (`Resources/Web/gxserver/bin`, the Linux flat app
/// directory, and the development tree), so it resolves as a sibling of the
/// resolved gxserver executable. Mirrors macOS `nativeBundledZmxExecutablePath`.
pub(crate) fn gpui_resolve_local_zmx_binary() -> Option<PathBuf> {
    let gxserver = gpui_resolve_local_gxserver_binary()?;
    let candidate = gxserver.parent()?.join("zmx");
    gpui_is_executable_file(&candidate).then_some(candidate)
}

/*
CDXC:Zmx 2026-07-06:
Terminal-content clicks should repair a zmx session that another client
resized, but a click inside an already-correct pane must not repaint the
terminal because a repaint scrolls the view to the visible bottom. Mirror
macOS `nativeRunZmxRefreshIfStaleProcess`: run bundled
`zmx refresh-if-stale <name> <rows> <cols>` through zmx IPC outside the
terminal input/output path, on a background thread, with a one-second
deadline, discarding output.
*/
pub(crate) fn gpui_gxserver_launch_log_path() -> PathBuf {
    shared_settings::ghostex_storage_paths()
        .logs_dir
        .join("gxserver")
        .join("launch.log")
}

/// CDXC:ServerDaemon 2026-10-16 WHY:
/// launchd opens StandardOutPath before it runs the program. When the log folder cannot be created or written (GitHub issue #231: a storage folder left owned by root), the job exits 78 (EX_CONFIG) without ever starting gxserver and leaves no log, and the diagnostics only said "no pid within 3 s". Checking Ghostex's storage and the log first fails the start at once, naming the folder and the command that fixes it.
pub(crate) fn gpui_prepare_gxserver_launch_log() -> Result<PathBuf, String> {
    shared_settings::ghostex_storage_paths()
        .check_writable()
        .map_err(|error| error.to_string())?;
    let launch_log = gpui_gxserver_launch_log_path();
    ghostex_paths::ensure_writable_file(&launch_log).map_err(|error| error.to_string())?;
    Ok(launch_log)
}

pub(crate) fn gpui_normalized_user_tool_path(current_path: Option<&str>) -> String {
    /*
    CDXC:OsIntegration 2026-07-24:
    Keep the normal user tool locations used by the GPUI process and its local
    daemon bootstrap in one place. Packaged macOS apps otherwise inherit a
    sparse LaunchServices PATH that cannot see Homebrew-installed Ghostex tools.
    This PATH is for the app and gxserver only; it is never exported into a
    terminal session's shell (see CDXC:Zmx).
    2026-10-31: the login shell's own PATH leads (probed once per process),
    then the inherited PATH, then the usual tool folders (Homebrew, mise,
    asdf, volta, nodenv, Nix profiles, `~/.local/bin`) as fallbacks that can
    never shadow either, so a failed probe still finds them. NixOS's
    `/run/wrappers/bin` stays ahead of the Nix profiles (GitHub PR #354).
    */
    let home = ghostex_paths::user_home_dir();
    ghostex_paths::tool_path::normalized_user_tool_path(current_path, &home)
}

/// `gpui_normalized_user_tool_path` for the app's own startup PATH: it waits for the login
/// shell at most `STARTUP_PROBE_WAIT`, then uses the fallbacks alone and lets the probe finish
/// in the background for later callers (CDXC:OsIntegration 2026-10-31 in tool_path.rs).
#[cfg(target_os = "macos")]
pub(crate) fn gpui_startup_user_tool_path(current_path: Option<&str>) -> String {
    let home = ghostex_paths::user_home_dir();
    let login_shell_entries = ghostex_paths::tool_path::login_shell_path_entries_within(
        &home,
        ghostex_paths::tool_path::STARTUP_PROBE_WAIT,
    )
    .unwrap_or_default();
    ghostex_paths::tool_path::normalized_user_tool_path_with(
        login_shell_entries,
        current_path,
        &home,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_gxserver_launch_agent_label() -> String {
    format!(
        "{}gxserver",
        ghostex_paths::launchd_label_prefix(gpui_local_gxserver_api_port())
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_launchd_plist_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/*
CDXC:ServerDaemon 2026-07-10:
macOS 27 attributes app-spawned processes that outlive the app to the app's
Background Task Management identity, and the Dock then renders the dim
"Running in Background" indicator instead of the normal running dot even
while the app is open. Registered launchd jobs carry their own BTM identity
(the TeamViewer_Service and sh.portless.proxy precedents), so the persistent
gxserver daemon -- and every zmx/node process it goes on to spawn -- must
enter the user session as a launchd agent instead of a detached nohup child
of the GPUI process. launchd starts the job with a clean environment, which
also covers the color-blocker and session-identity stripping the nohup path
performed; CLICOLOR mirrors terminal_environment's color opt-in.
*/
#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<Vec<String>, String> {
    // CDXC:ServerDaemon 2026-09-16 WHY:
    // Two reloads can both inspect an empty label before either bootstraps it; serialize the complete inspect/bootout/bootstrap sequence so one cannot remove the other's new job.
    static LAUNCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LAUNCH_LOCK
        .lock()
        .map_err(|_| "gxserver launcher lock was poisoned.".to_string())?;
    let mut steps = Vec::new();
    let log_offset =
        std::fs::metadata(gpui_gxserver_launch_log_path()).map_or(0, |metadata| metadata.len());
    gpui_spawn_local_gxserver_launchd_job(binary, &mut steps)
        .map_err(|error| format!("{error}\n{}", steps.join("\n")))?;
    gpui_note_gxserver_launch(log_offset);
    Ok(steps)
}

#[cfg(target_os = "macos")]
fn gpui_spawn_local_gxserver_launchd_job(
    binary: &Path,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    const LAUNCH_FAILURE: &str = "gxserver failed to launch.";
    let Some(binary_path) = binary.to_str() else {
        return Err(LAUNCH_FAILURE.to_string());
    };
    let launch_log = gpui_prepare_gxserver_launch_log()?;
    let Some(launch_log_path) = launch_log.to_str() else {
        return Err(LAUNCH_FAILURE.to_string());
    };
    let home = env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LAUNCH_FAILURE.to_string())?;
    let agents_dir = PathBuf::from(&home).join("Library/LaunchAgents");
    std::fs::create_dir_all(&agents_dir).map_err(|_| LAUNCH_FAILURE.to_string())?;
    let label = gpui_gxserver_launch_agent_label();
    let plist_path = agents_dir.join(format!("{label}.plist"));

    let current_path = env::var("PATH").ok();
    let mut environment_entries = vec![
        (
            "PATH".to_string(),
            gpui_normalized_user_tool_path(current_path.as_deref()),
        ),
        ("CLICOLOR".to_string(), "1".to_string()),
        (
            "GHOSTEX_GXSERVER_DEV_PORT".to_string(),
            gpui_local_gxserver_api_port().to_string(),
        ),
    ];
    for variable in [
        "GHOSTEX_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        if let Some(value) = env::var(variable)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| Path::new(value).is_absolute())
        {
            environment_entries.push((variable.to_string(), value));
        }
    }
    let environment_xml = environment_entries
        .iter()
        .map(|(key, value)| {
            format!(
                "\t\t<key>{}</key>\n\t\t<string>{}</string>\n",
                gpui_launchd_plist_xml_escape(key),
                gpui_launchd_plist_xml_escape(value),
            )
        })
        .collect::<String>();
    // RunAtLoad stays false so the daemon keeps its on-demand lifecycle:
    // loading the job at login must not start gxserver; only an explicit
    // kickstart from a client that found it unreachable does.
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{binary}</string>
		<string>--foreground</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
{environment_xml}	</dict>
	<key>RunAtLoad</key>
	<false/>
	<key>KeepAlive</key>
	<false/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>{launch_log}</string>
	<key>StandardErrorPath</key>
	<string>{launch_log}</string>
</dict>
</plist>
"#,
        label = gpui_launchd_plist_xml_escape(&label),
        binary = gpui_launchd_plist_xml_escape(binary_path),
        environment_xml = environment_xml,
        launch_log = gpui_launchd_plist_xml_escape(launch_log_path),
    );
    std::fs::write(&plist_path, plist).map_err(|_| LAUNCH_FAILURE.to_string())?;

    let job_target = gpui_gxserver_launchd_job_target();
    let domain_target = format!("gui/{}", gpui_current_uid());
    let plist_argument = plist_path.to_string_lossy().to_string();
    let started = std::time::Instant::now();
    let note = |steps: &mut Vec<String>, text: String| {
        steps.push(format!(
            "launchd +{}ms: {text}",
            started.elapsed().as_millis()
        ));
    };

    /*
    CDXC:ServerDaemon 2026-09-16 WHY:
    `launchctl bootout` only signals the job and returns; launchd drops the label after the process has exited, up to 5 s later when it escalates to SIGKILL.
    On 2026-09-16 an app update stopped the previous daemon's control plane, the port closed, and this function ran bootout, bootstrap and kickstart within 30 ms while pid 83573 was still draining its shutdown. launchd's log: bootout returned at once and only sent SIGTERM; 7 ms later "Bootstrap by launchctl failed (37: Operation already in progress)"; kickstart landed on the dying registration, blocked until that process exited 380 ms later, and returned 0; launchd then removed the label. No WILL_SPAWN ever followed, so the 20 s health loop timed out with "Connection refused" on every probe and the daemon only came up when the user clicked reload.
    Every step below therefore waits for launchd's actual state (label gone, then a pid) instead of trusting exit codes, and a job that is already running this binary and answers health is left alone so two overlapping starts cannot boot each other out.
    */
    let before = gpui_inspect_launchd_job(&job_target)?;
    note(steps, format!("job before start: {}", before.describe()));
    if before.pid.is_some() && before.program.as_deref() == Some(binary_path) {
        // Same binary with a live process: either a concurrent start or a
        // daemon still shutting down. Health decides, with a moment's grace
        // for a fresh start to open its listener.
        let probe_started = std::time::Instant::now();
        loop {
            if matches!(
                gpui_probe_local_gxserver_health(),
                GpuiLocalGxserverHealthState::Healthy { .. }
            ) {
                note(
                    steps,
                    "a daemon with this binary is already running and healthy; leaving it alone"
                        .to_string(),
                );
                gpui_log_launchd_spawn_outcome(true, true, before.pid, steps);
                return Ok(());
            }
            if probe_started.elapsed() >= Duration::from_secs(20) {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        note(
            steps,
            "the running process did not answer health within 20 s; treating it as unresponsive"
                .to_string(),
        );
    }

    let current = gpui_inspect_launchd_job(&job_target)?;
    if current.loaded {
        let bootout = gpui_run_launchctl(&["bootout", &job_target]);
        note(
            steps,
            format!(
                "bootout of the {} job: {}",
                current.describe(),
                gpui_describe_launchctl_result(&bootout)
            ),
        );
        match gpui_wait_for_launchd_job_removal(&job_target, Duration::from_secs(12)) {
            Ok(elapsed) => note(
                steps,
                format!("previous job removed after {}ms", elapsed.as_millis()),
            ),
            Err(error) => {
                note(steps, error.clone());
                gpui_log_launchd_spawn_outcome(false, false, current.pid, steps);
                return Err(error);
            }
        }
    } else {
        note(steps, "no job loaded; nothing to boot out".to_string());
    }

    let mut bootstrap = gpui_run_launchctl(&["bootstrap", &domain_target, &plist_argument]);
    let mut bootstrap_attempts = 1u32;
    while !bootstrap.success
        && bootstrap_attempts < 10
        && !gpui_inspect_launchd_job(&job_target)?.loaded
    {
        std::thread::sleep(Duration::from_millis(200));
        bootstrap = gpui_run_launchctl(&["bootstrap", &domain_target, &plist_argument]);
        bootstrap_attempts += 1;
    }
    let bootstrapped = bootstrap.success;
    note(
        steps,
        format!(
            "bootstrap after {bootstrap_attempts} attempt(s): {}",
            gpui_describe_launchctl_result(&bootstrap)
        ),
    );
    // A concurrent client may have loaded the same label in between, which
    // makes our bootstrap fail although the job is there; the pid check below
    // owns the verdict either way.
    if !bootstrapped && !gpui_inspect_launchd_job(&job_target)?.loaded {
        gpui_log_launchd_spawn_outcome(false, false, None, steps);
        return Err(format!(
            "launchctl could not load the gxserver job: {}",
            gpui_launchctl_failure_text(&bootstrap)
        ));
    }

    let mut spawned_pid = None;
    let mut last_kickstart = None;
    for round in 1..=2u32 {
        let kickstart = gpui_run_launchctl(&["kickstart", &job_target]);
        note(
            steps,
            format!(
                "kickstart {round}: {}",
                gpui_describe_launchctl_result(&kickstart)
            ),
        );
        last_kickstart = Some(kickstart);
        if let Some(pid) = gpui_wait_for_launchd_job_pid(&job_target, Duration::from_secs(3))? {
            spawned_pid = Some(pid);
            break;
        }
        note(steps, "no pid within 3 s".to_string());
    }
    gpui_log_launchd_spawn_outcome(bootstrapped, spawned_pid.is_some(), spawned_pid, steps);
    if spawned_pid.is_none() {
        let (health, detail) = gpui_probe_local_gxserver_health_with_diagnostics();
        if gpui_gxserver_already_answers_for_launch(&health) {
            note(
                steps,
                format!(
                    "the job's gxserver left at once because a gxserver already answers on the port: {detail}"
                ),
            );
            return Ok(());
        }
    }
    match spawned_pid {
        Some(pid) => {
            note(steps, format!("gxserver spawned as pid {pid}"));
            Ok(())
        }
        None => Err(format!(
            "launchd loaded the gxserver job but never spawned it: {}",
            last_kickstart
                .as_ref()
                .map(gpui_launchctl_failure_text)
                .unwrap_or_default()
        )),
    }
}

/// Launches the daemon detached from the app (its own session), so it is app-independent and
/// survives quitting Ghostex, while the app keeps the process to see how it ends.
/// (Linux has no Dock background-attribution concern, so a detached child stays correct there.)
#[cfg(target_os = "linux")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<Vec<String>, String> {
    use std::os::unix::process::CommandExt as _;

    let launch_log = gpui_prepare_gxserver_launch_log()?;
    let log_offset = std::fs::metadata(&launch_log).map_or(0, |metadata| metadata.len());
    let stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&launch_log)
        .map_err(|error| format!("gxserver failed to launch: {error}"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|error| format!("gxserver failed to launch: {error}"))?;
    let mut command = std::process::Command::new(binary);
    terminal_environment::apply_color_capable_process_command(&mut command);
    terminal_environment::remove_session_identity_from_process_command(&mut command);
    // Agent sessions inherit gxserver's environment.
    crate::restore_wayland_display_for_process_command(&mut command);
    /*
    CDXC:ComputerUse 2026-10-28 WHY:
    On a Wayland desktop an agent's cua-driver used its X11 backend, because its native Wayland backend is opt-in (`CUA_DRIVER_RS_ENABLE_WAYLAND=1`, cua-driver LINUX.md "Wayland"), and XWayland's root window has no desktop pixels: every screenshot failed with an X11 GetImage error, so Computer Use could not see the screen on Hyprland. Sessions get the opt-in whenever they get the Wayland display; a value the user set is kept.
    */
    if crate::linux_inherited_wayland_display().is_some()
        && env::var_os("CUA_DRIVER_RS_ENABLE_WAYLAND").is_none()
    {
        command.env("CUA_DRIVER_RS_ENABLE_WAYLAND", "1");
    }
    let current_path = env::var("PATH").ok();
    command.env(
        "PATH",
        gpui_normalized_user_tool_path(current_path.as_deref()),
    );
    command
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr));
    // CDXC:ServerDaemon 2026-10-28 WHY:
    // Its own session, as `nohup … &` gave it before: no controlling terminal and outside the
    // app's process group, so closing the terminal Ghostex was started from or quitting Ghostex
    // never signals it. Started directly rather than through a shell so the app holds the process
    // and sees it end (gxserver_exit.rs).
    // SAFETY: `setsid` is async-signal-safe and touches nothing of the parent's state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command
        .spawn()
        .map_err(|error| format!("gxserver failed to launch: {error}"))?;
    gpui_note_gxserver_launch(log_offset);
    gpui_watch_gxserver_exit(child, launch_log, log_offset);
    Ok(vec![
        "launcher: detached process start accepted".to_string(),
    ])
}

/// CDXC:PlatformSupport 2026-09-28 WHY:
/// gxserver gets its own windowless console (CREATE_NO_WINDOW), never DETACHED_PROCESS. Windows ignores CREATE_NO_WINDOW next to DETACHED_PROCESS, so gxserver ran with no console at all and every console program it started without its own flag (tailcat, gh, git probes) got a fresh console that Windows 11 opens in a Windows Terminal window. Children now inherit the hidden console.
/// SEE-ALSO: `spawn_server` in server/src/platform/process.rs, gxserver's own relaunch, must use the same flags.
#[cfg(target_os = "windows")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<Vec<String>, String> {
    use crate::app::helpers::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    use std::os::windows::process::CommandExt;

    let launch_log = gpui_prepare_gxserver_launch_log()?;
    let log_offset = std::fs::metadata(&launch_log).map_or(0, |metadata| metadata.len());
    let stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&launch_log)
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    let stderr = stdout
        .try_clone()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    let mut command = std::process::Command::new(binary);
    terminal_environment::apply_color_capable_process_command(&mut command);
    terminal_environment::remove_session_identity_from_process_command(&mut command);
    let child = command
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    gpui_watch_gxserver_exit(child, launch_log, log_offset);
    Ok(vec![
        "launcher: detached process start accepted".to_string(),
    ])
}

/*
CDXC:OsIntegration 2026-08-28:
Full teardown behind the "Quit Ghostex & BG Service" menu item. Plain Quit
leaves gxserver, the zmx session daemons, and the GhostexEditor daemon alive
on purpose; this path is the explicit opposite: gxserver kills every tracked
zmx session and stops itself (`/api/control/stopAll`), then every
launchd job in the selected instance's namespace is booted out (covering gxserver's own
LaunchAgent plus any zmx job the server lost track of), and the standalone
editor daemon is asked to exit over its socket. Every step is best-effort and
bounded so quitting can never hang on a wedged daemon.
*/
pub(crate) fn gpui_stop_all_ghostex_background_services() {
    if !matches!(
        gpui_probe_local_gxserver_health(),
        GpuiLocalGxserverHealthState::Unreachable
    ) {
        let _ = gxserver_post_typed_operation(
            "/api/control/stopAll",
            &serde_json::json!({}),
            Duration::from_secs(30),
        );
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if matches!(
                gpui_probe_local_gxserver_health(),
                GpuiLocalGxserverHealthState::Unreachable
            ) {
                break;
            }
        }
    }
    #[cfg(target_os = "macos")]
    gpui_bootout_all_ghostex_launchd_jobs();
    #[cfg(any(unix, windows))]
    {
        let _ = gpui_ghostex_editor_daemon_request(&serde_json::json!({
            "v": GHOSTEX_EDITOR_PROTOCOL_VERSION,
            "type": "shutdown",
        }));
    }
}

#[cfg(target_os = "macos")]
fn gpui_bootout_all_ghostex_launchd_jobs() {
    let Ok(uid_output) = std::process::Command::new("/usr/bin/id").arg("-u").output() else {
        return;
    };
    let uid = String::from_utf8_lossy(&uid_output.stdout)
        .trim()
        .to_string();
    if uid.is_empty() {
        return;
    }
    // The gxserver label is included unconditionally: the job stays loaded
    // even after `/api/control/stopAll` exits the process, and it must also
    // go when the server was already stopped and `launchctl list` still
    // reports it without a PID.
    let prefix = ghostex_paths::launchd_label_prefix(gpui_local_gxserver_api_port());
    let mut labels = vec![gpui_gxserver_launch_agent_label()];
    if let Ok(list_output) = std::process::Command::new("/bin/launchctl")
        .arg("list")
        .output()
    {
        for line in String::from_utf8_lossy(&list_output.stdout).lines() {
            let Some(label) = line.split_whitespace().nth(2) else {
                continue;
            };
            if label.starts_with(&prefix) && !labels.iter().any(|known| known == label) {
                labels.push(label.to_string());
            }
        }
    }
    for label in labels {
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["bootout", &format!("gui/{uid}/{label}")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Last few launch-log lines so a startup-timeout toast can say why, matching
/// the macOS client's recentGxserverLaunchOutput.
pub(crate) fn gpui_recent_gxserver_launch_output() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(gpui_gxserver_launch_log_path()).ok()?;
    let length = file.metadata().ok()?.len();
    let offset = length.saturating_sub(16 * 1024);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.take(16 * 1024).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let lines = text
        .lines()
        .skip(usize::from(offset > 0))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(6);
    Some(lines[start..].join("\n"))
}

/// CDXC:ServerDaemon 2026-09-05 DECISION:
/// User: add a button to the startup failure toast to copy the relevant failure lines so users can send them back for diagnosis.
/// Capture the failed attempt before a later Load Sessions retry replaces the evidence.
pub(crate) fn gpui_gxserver_startup_failure_report(probes: &[String]) -> String {
    let captured_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let log_path = gpui_gxserver_launch_log_path();
    let log_modified = fs::metadata(&log_path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|| "unavailable".into());
    let output = gpui_recent_gxserver_launch_output()
        .unwrap_or_else(|| "No readable, non-empty launcher output.".into());
    let mut report = format!(
        "Ghostex gxserver startup diagnostics\nApp: {}\nPlatform: {} / {}\nCaptured at (Unix seconds): {captured_at}\nHealth endpoint: 127.0.0.1:{}/api/health/server\nExpected protocol: {}\n\n{}\n\nLauncher log tail (may predate this attempt; modified Unix seconds: {log_modified}):\n{output}",
        GPUI_APP_MARKETING_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        gpui_local_gxserver_api_port(),
        GPUI_GXSERVER_PROTOCOL_VERSION,
        probes.join("\n"),
    );
    #[cfg(target_os = "macos")]
    {
        let version = gpui_run_command_with_captured_output_timeout(
            Path::new("/usr/bin/sw_vers"),
            &["-productVersion"],
            Duration::from_secs(1),
            256,
        );
        if let Ok(version) = version
            && version.success
        {
            report.push_str(&format!("\n\nmacOS version: {}", version.stdout.trim()));
        }
        let uid = gpui_run_command_with_captured_output_timeout(
            Path::new("/usr/bin/id"),
            &["-u"],
            Duration::from_secs(1),
            64,
        );
        if let Ok(uid) = uid
            && uid.success
            && uid.stdout.trim().parse::<u32>().is_ok()
        {
            let target = format!(
                "gui/{}/{}",
                uid.stdout.trim(),
                gpui_gxserver_launch_agent_label()
            );
            report.push_str("\n\nlaunchd job at failure:\n");
            match gpui_run_command_with_captured_output_timeout(
                Path::new("/bin/launchctl"),
                &["print", &target],
                Duration::from_secs(2),
                32 * 1024,
            ) {
                Ok(job) if job.success => {
                    for line in job.stdout.lines() {
                        let Some((key, value)) = line.trim().split_once(" = ") else {
                            continue;
                        };
                        if [
                            "state",
                            "pid",
                            "runs",
                            "last exit code",
                            "last terminating signal",
                            "program",
                            "job state",
                        ]
                        .contains(&key)
                        {
                            report.push_str(&format!("{key}: {value}\n"));
                        }
                    }
                }
                Ok(_) => {
                    report.push_str("launchctl print failed or timed out; job may not be loaded.\n")
                }
                Err(error) => report.push_str(&format!("launchctl inspection failed: {error}\n")),
            }
        }
    }
    #[cfg(windows)]
    if windows_terminal_backend::current_preference()
        == windows_terminal_backend::WindowsTerminalBackendPreference::PowerShell
    {
        report.push_str(&gpui_windows_gxserver_runtime_record_report());
        report.push_str(&gpui_windows_gxserver_crash_evidence_report());
    }
    if let Ok(token) = read_gpui_gxserver_auth_token() {
        report = report.replace(&token, "[redacted token]");
    }
    if let Ok(home) = env::var("HOME")
        && !home.is_empty()
    {
        report = report.replace(&home, "$HOME");
    }
    report
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "bearer ",
                "authtoken",
                "authorization:",
                "password",
                "secret",
                "token=",
            ]
            .iter()
            .any(|key| lower.contains(key))
            {
                "[credential-bearing line omitted]".to_string()
            } else {
                line.chars().take(1024).collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// CDXC:ServerDaemon 2026-10-22 WHY:
/// A Windows user's `gxserver status` said running while the app could not read a single answer, and the startup report could not show which process the app was waiting on. The report now names the daemon gxserver last recorded (pid, port, build, start time), whether that process still runs and which executable it is, so a stuck daemon, a stale record and a foreign program on the port read differently.
#[cfg(windows)]
fn gpui_windows_gxserver_runtime_record_report() -> String {
    let path = shared_settings::ghostex_storage_paths()
        .gxserver_state_dir()
        .join("runtime")
        .join("server.json");
    let record = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => return format!("\n\ngxserver runtime record: none ({error})"),
    };
    let Ok(record) = serde_json::from_str::<serde_json::Value>(&record) else {
        return "\n\ngxserver runtime record: unreadable JSON".to_string();
    };
    let field = |key: &str| {
        record
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_string)
            })
            .unwrap_or_else(|| "missing".to_string())
    };
    let pid = record
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    let process = match pid {
        None => "no pid recorded".to_string(),
        Some(pid) => match gpui_local_process_is_alive(pid) {
            Ok(true) => format!(
                "running as {}",
                gpui_windows_process_image(pid)
                    .unwrap_or_else(|| "an executable Ghostex cannot read".to_string())
            ),
            Ok(false) => "not running (the record is stale)".to_string(),
            Err(error) => error,
        },
    };
    format!(
        "\n\ngxserver runtime record: pid {}, port {}, build {}, started {}\nRecorded process: {process}",
        field("pid"),
        field("port"),
        field("buildIdentity"),
        field("startedAt"),
    )
}

#[cfg(windows)]
fn gpui_windows_process_image(pid: u32) -> Option<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
    };
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: the handle is only queried and then closed; the buffer outlives the call.
    let read = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let read = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        read
    };
    (read != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

pub(crate) fn gpui_gxserver_protocol_mismatch_message(reported: Option<u64>) -> String {
    format!(
        "gxserver protocol mismatch. Expected protocol {GPUI_GXSERVER_PROTOCOL_VERSION}, got {}. Update Ghostex and gxserver so their protocol versions match.",
        reported.map_or_else(|| "none".to_string(), |version| version.to_string()),
    )
}

#[cfg(test)]
mod launch_outcome_tests {
    use super::*;

    fn every_health_state() -> Vec<GpuiLocalGxserverHealthState> {
        vec![
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: true,
            },
            GpuiLocalGxserverHealthState::Healthy {
                tools_available: false,
            },
            GpuiLocalGxserverHealthState::Starting {
                starting_for: Duration::from_secs(3),
            },
            GpuiLocalGxserverHealthState::BuildMismatch,
            GpuiLocalGxserverHealthState::ProtocolMismatch { reported: Some(0) },
            GpuiLocalGxserverHealthState::Unreachable,
        ]
    }

    #[test]
    fn a_launch_that_ends_beside_a_gxserver_of_this_build_is_not_a_failure() {
        let answers: Vec<bool> = every_health_state()
            .iter()
            .map(gpui_gxserver_already_answers_for_launch)
            .collect();
        assert_eq!(answers, [true, true, true, false, false, false]);
    }

    #[test]
    fn only_a_silent_port_makes_a_launcher_error_final() {
        let finals: Vec<bool> = every_health_state()
            .iter()
            .map(gpui_gxserver_launch_error_is_final)
            .collect();
        assert_eq!(finals, [false, false, false, false, false, true]);
    }
}
