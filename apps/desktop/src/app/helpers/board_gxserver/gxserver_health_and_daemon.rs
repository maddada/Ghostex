// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds gxserver RPC/health probing, local binary resolution, and
// local daemon spawn/restart (macOS launchd, Linux, Windows) helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_gxserver_rpc_result(
    endpoint: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:GPUISettingsStatusBridge 2026-06-24-11:48:
    Settings status/actions share the typed-operation transport and may expose only the validated gxserver `result` object to the modal host. Transport, status, envelope, and parse failures stay as local errors so callers can clear loading with explicit empty status payloads without logging private daemon data.
    */
    let (status_code, body) = gxserver_post_typed_operation(endpoint, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver request failed with HTTP {status_code}."));
    }
    parse_gpui_gxserver_rpc_result(&body)
}

pub(crate) fn gpui_update_portless_gxserver_state(
    update: GpuiPortlessStateUpdate,
) -> Result<serde_json::Value, String> {
    /*
    CDXC:GPUISettingsPortlessBridge 2026-06-24-11:48:
    Successful Portless state updates return canonical gxserver status and presentation metadata. Parse just enough of that response to refresh the shared app-modal `hud.portless` payload immediately, while transport/parser failures remain silent local `Result` values so unavailable gxserver cannot create fake success or roll back saved Settings.
    */
    let result = gpui_gxserver_rpc_result(
        "/api/updatePortlessState",
        &update.to_rpc_params(),
        Duration::from_secs(10),
    )?;
    gpui_sidebar_portless_state_from_update_result(&result)
        .ok_or_else(|| "gxserver returned invalid Portless state.".to_string())
}

pub(crate) fn gpui_gxserver_server_health(timeout: Duration) -> Result<serde_json::Value, String> {
    let (status_code, body) = gxserver_get_typed_operation("/api/health/server", timeout)?;
    if !(200..300).contains(&status_code) {
        return Err(format!("gxserver health failed with HTTP {status_code}."));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|_| "gxserver health returned invalid JSON.".to_string())
}

pub(crate) const GPUI_GXSERVER_DAEMON_TOAST_ID: &str = "toast-gxserver-daemon";
pub(crate) const GPUI_MISSING_MONACO_PROMPT_EDITOR_TOAST_ID: &str =
    "toast-monaco-prompt-editor-missing";
pub(crate) const GPUI_GXSERVER_EXPECTED_PRODUCT: &str = "gxserver";

pub(crate) enum GpuiLocalGxserverHealthState {
    Healthy { tools_available: bool },
    BuildMismatch,
    ProtocolMismatch { reported: Option<u64> },
    Unreachable,
}

/// Mirrors the macOS GxserverClient handshake: authenticated health, product
/// check, hard protocol-version and build-identity matches, and availability
/// of the tools shipped beside this GPUI build's gxserver binary.
pub(crate) fn gpui_probe_local_gxserver_health() -> GpuiLocalGxserverHealthState {
    let Ok(health) = gpui_gxserver_server_health(Duration::from_millis(1000)) else {
        return GpuiLocalGxserverHealthState::Unreachable;
    };
    if health.get("product").and_then(serde_json::Value::as_str)
        != Some(GPUI_GXSERVER_EXPECTED_PRODUCT)
    {
        return GpuiLocalGxserverHealthState::Unreachable;
    }
    let reported_protocol = health
        .get("protocolVersion")
        .and_then(serde_json::Value::as_u64);
    if reported_protocol != Some(GPUI_GXSERVER_PROTOCOL_VERSION) {
        return GpuiLocalGxserverHealthState::ProtocolMismatch {
            reported: reported_protocol,
        };
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(expected_build_identity) = gpui_expected_local_gxserver_build_identity() {
        let reported_build_identity = health
            .get("buildIdentity")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if reported_build_identity != Some(expected_build_identity.as_str()) {
            return GpuiLocalGxserverHealthState::BuildMismatch;
        }
    }
    GpuiLocalGxserverHealthState::Healthy {
        tools_available: gpui_gxserver_required_tools_available(&health),
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn gpui_expected_local_gxserver_build_identity() -> Option<String> {
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

    let binary = gpui_resolve_local_gxserver_binary()
        .ok_or_else(|| "Bundled gxserver binary is missing.".to_string())?;
    gpui_spawn_local_gxserver_daemon(&binary)?;
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
    // CDXC:AgentHistorySearch 2026-08-20: Zehn used to be gated here too,
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
    let required_tools = vec!["zmx"];
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
        if let Some(contents_dir) = current_exe.parent().and_then(Path::parent) {
            candidates.push(contents_dir.join("Resources/Web/gxserver/bin/gxserver"));
        }
        // CDXC:GPUILinuxX11Backend 2026-07-05: the Linux app is a flat
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
CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
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
        .join("macos-launch.log")
}

pub(crate) fn gpui_normalized_user_tool_path(current_path: Option<&str>) -> String {
    /*
    CDXC:GPUIUserToolPathEntries 2026-07-24:
    Keep the normal user tool locations used by the GPUI process and its local
    daemon bootstrap in one place. Packaged macOS apps otherwise inherit a
    sparse LaunchServices PATH that cannot see Homebrew-installed Ghostex tools.
    */
    let home = env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mut entries = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
    ];
    if let Some(home) = home.as_deref() {
        entries.extend([
            format!("{home}/.volta/bin"),
            format!("{home}/.local/share/mise/shims"),
            format!("{home}/.local/bin"),
            format!("{home}/.asdf/shims"),
            format!("{home}/.nodenv/shims"),
        ]);
    }
    entries.extend([
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ]);
    if let Some(current_path) = current_path {
        entries.extend(current_path.split(':').map(str::to_string));
    }

    let mut seen = HashSet::new();
    entries
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty() && seen.insert(entry.clone()))
        .collect::<Vec<_>>()
        .join(":")
}

/// The gxserver launchd agent label is client-agnostic on purpose: one
/// per-user gxserver serves the Swift app, GPUI, and the CLI, so whichever
/// client bootstraps it owns the same job definition.
#[cfg(target_os = "macos")]
pub(crate) const GPUI_GXSERVER_LAUNCH_AGENT_LABEL: &str = "com.madda.ghostex.gxserver";

#[cfg(target_os = "macos")]
pub(crate) fn gpui_launchd_plist_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/*
CDXC:GPUIGxserverLaunchdJob 2026-07-10:
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
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    const LAUNCH_FAILURE: &str = "gxserver failed to launch.";
    let Some(binary_path) = binary.to_str() else {
        return Err(LAUNCH_FAILURE.to_string());
    };
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
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
    let plist_path = agents_dir.join(format!("{GPUI_GXSERVER_LAUNCH_AGENT_LABEL}.plist"));

    let current_path = env::var("PATH").ok();
    let mut environment_entries = vec![
        (
            "PATH".to_string(),
            gpui_normalized_user_tool_path(current_path.as_deref()),
        ),
        ("CLICOLOR".to_string(), "1".to_string()),
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
        label = gpui_launchd_plist_xml_escape(GPUI_GXSERVER_LAUNCH_AGENT_LABEL),
        binary = gpui_launchd_plist_xml_escape(binary_path),
        environment_xml = environment_xml,
        launch_log = gpui_launchd_plist_xml_escape(launch_log_path),
    );
    std::fs::write(&plist_path, plist).map_err(|_| LAUNCH_FAILURE.to_string())?;

    let uid_output = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| LAUNCH_FAILURE.to_string())?;
    let uid = String::from_utf8_lossy(&uid_output.stdout)
        .trim()
        .to_string();
    if uid.is_empty() {
        return Err(LAUNCH_FAILURE.to_string());
    }
    let job_target = format!("gui/{uid}/{GPUI_GXSERVER_LAUNCH_AGENT_LABEL}");
    let domain_target = format!("gui/{uid}");

    let run_launchctl = |arguments: &[&str]| -> bool {
        std::process::Command::new("/bin/launchctl")
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };
    // This path only runs when the daemon health probe reported unreachable,
    // so removing a stale or hung job definition before loading the fresh
    // plist (whose binary path changes across app versions) is safe.
    let _ = run_launchctl(&["bootout", &job_target]);
    let bootstrapped = run_launchctl(&["bootstrap", &domain_target, &plist_path.to_string_lossy()]);
    // A concurrent client may have loaded the same label between the bootout
    // and bootstrap (making bootstrap fail) or already started the job
    // (making kickstart report already-active), so either success signal
    // means the job is loaded; the caller's health-probe loop owns the
    // final running/not-running verdict.
    let kickstarted = run_launchctl(&["kickstart", &job_target]);
    support_logs::append(
        support_logs::GpuiSupportLog::HostLifecycle,
        "gpui.gxserverBootstrap.launchdAgent",
        serde_json::json!({
            "bootstrapped": bootstrapped,
            "kickstarted": kickstarted,
        }),
    );
    if !bootstrapped && !kickstarted {
        return Err(LAUNCH_FAILURE.to_string());
    }
    Ok(())
}

/// Launches the daemon exactly like the macOS client used to: a
/// shell-detached `nohup <gxserver> --foreground` so the process is
/// app-independent and survives quitting Ghostex. The app never retains the
/// child as ownership. (Linux has no Dock background-attribution concern, so
/// the detached-child launch remains correct there.)
#[cfg(target_os = "linux")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    let Some(binary_path) = binary.to_str() else {
        return Err("gxserver failed to launch.".to_string());
    };
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Some(launch_log_path) = launch_log.to_str() else {
        return Err("gxserver failed to launch.".to_string());
    };
    let command = format!(
        "nohup {} --foreground >>{} 2>&1 </dev/null &",
        gpui_shell_single_quote(binary_path),
        gpui_shell_single_quote(launch_log_path),
    );
    let mut shell = std::process::Command::new("/bin/sh");
    terminal_environment::apply_color_capable_process_command(&mut shell);
    terminal_environment::remove_session_identity_from_process_command(&mut shell);
    let current_path = env::var("PATH").ok();
    shell.env(
        "PATH",
        gpui_normalized_user_tool_path(current_path.as_deref()),
    );
    let status = shell
        .arg("-c")
        .arg(&command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    if !status.success() {
        return Err("gxserver failed to launch.".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_spawn_local_gxserver_daemon(binary: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let launch_log = gpui_gxserver_launch_log_path();
    if let Some(parent) = launch_log.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "gxserver failed to launch.".to_string())?;
    }
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
    command
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| "gxserver failed to launch.".to_string())?;
    Ok(())
}

/// Last few launch-log lines so a startup-timeout toast can say why, matching
/// the macOS client's recentGxserverLaunchOutput.
pub(crate) fn gpui_recent_gxserver_launch_output() -> Option<String> {
    let text = std::fs::read_to_string(gpui_gxserver_launch_log_path()).ok()?;
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(6);
    Some(lines[start..].join(" "))
}

pub(crate) fn gpui_gxserver_protocol_mismatch_message(reported: Option<u64>) -> String {
    format!(
        "gxserver protocol mismatch. Expected protocol {GPUI_GXSERVER_PROTOCOL_VERSION}, got {}. Update Ghostex and gxserver so their protocol versions match.",
        reported.map_or_else(|| "none".to_string(), |version| version.to_string()),
    )
}
