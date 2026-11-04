//! The Accessibility and Screen Recording grants of the desktop-control tools (SpaceO and Fast
//! Computer & Browser Use) for Settings > Integrations: each grant's state, the Grant button's
//! real system request from the tool's own identity, SpaceO's restart once a grant lands, and
//! the quick permission-only refresh Settings polls while it is open.

use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::app::helpers::*;

/// CDXC:ManagedTools 2026-10-14 DECISION:
/// User: "We should always show the status of each permission after it's done." One row per permission with a live status that stays visible after the grant (Granted, Not granted, Needs restart); Grant makes the tool request its own permission (so it is listed in System Settings) and opens that Privacy pane; Settings re-checks from a fresh process, since a running process never sees a new grant, until the grant lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiToolPermissionState {
    Granted,
    NotGranted,
    /// macOS has the grant, but the tool's long-running service started before it and still
    /// runs without it until it restarts.
    NeedsRestart,
    #[default]
    Unknown,
}

impl GpuiToolPermissionState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::NotGranted => "notGranted",
            Self::NeedsRestart => "needsRestart",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn from_granted(granted: Option<bool>) -> Self {
        match granted {
            Some(true) => Self::Granted,
            Some(false) => Self::NotGranted,
            None => Self::Unknown,
        }
    }
}

/// What `spaceo doctor --json`, run as SpaceO's own identity, says about its grants and daemon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSpaceoPermissionReport {
    pub(crate) daemon_running: bool,
    /// The app macOS attributes the daemon's grants to, as the daemon reports it.
    pub(crate) permission_app: Option<String>,
    pub(crate) accessibility: GpuiToolPermissionState,
    pub(crate) screen_recording: GpuiToolPermissionState,
}

impl GpuiSpaceoPermissionReport {
    pub(crate) fn detail(&self) -> String {
        use GpuiToolPermissionState::*;
        match (self.accessibility, self.screen_recording) {
            (Granted, Granted) => "SpaceO has Accessibility and Screen Recording.".to_string(),
            (NeedsRestart, _) | (_, NeedsRestart) => {
                "SpaceO needs a restart to use the permissions it was granted.".to_string()
            }
            (Unknown, Unknown) => "SpaceO's permissions could not be checked.".to_string(),
            _ => "SpaceO permissions need attention.".to_string(),
        }
    }
}

/// CDXC:ManagedTools 2026-10-14 WHY:
/// A process reads its Screen Recording grant once, so the daemon keeps reporting a grant made after it started as missing; a fresh `spaceo` process with SpaceO's identity (`capabilities`) shows what macOS has now, and the daemon's own answer (`daemon`) shows what agents get. Granted for SpaceO but not for the daemon is Needs restart.
pub(crate) fn gpui_spaceo_permission_report(path: &Path) -> GpuiSpaceoPermissionReport {
    let Some(doctor) = gpui_run_disclaimed_command_with_captured_output_timeout(
        path,
        &["doctor", "--json"],
        Duration::from_secs(10),
        256 * 1024,
    )
    .ok()
    // doctor exits 1 whenever it found something to fix; its JSON is still complete.
    .and_then(|output| gpui_spaceo_json_object(&output.stdout)) else {
        return GpuiSpaceoPermissionReport::default();
    };
    let identity = |name: &str| {
        doctor
            .get("capabilities")?
            .as_array()?
            .iter()
            .find(|item| item.get("name").and_then(serde_json::Value::as_str) == Some(name))?
            .get("available")?
            .as_bool()
    };
    let daemon = doctor.get("daemon");
    let daemon_field = |key: &str| daemon.and_then(|daemon| daemon.get(key));
    let daemon_running = daemon_field("running").and_then(serde_json::Value::as_bool) == Some(true);
    let state = |identity: Option<bool>, daemon_key: &str| match identity {
        Some(true)
            if daemon_running
                && daemon_field(daemon_key).and_then(serde_json::Value::as_bool) == Some(false) =>
        {
            GpuiToolPermissionState::NeedsRestart
        }
        granted => GpuiToolPermissionState::from_granted(granted),
    };
    GpuiSpaceoPermissionReport {
        daemon_running,
        permission_app: daemon_field("responsibleProcess")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|app| !app.is_empty())
            .map(str::to_string),
        accessibility: state(identity("accessibility"), "accessibilityGranted"),
        screen_recording: state(identity("screen-recording"), "screenRecordingGranted"),
    }
}

/// The Grant button for SpaceO: `spaceo setup` as SpaceO's own identity asks macOS for every
/// missing grant (`CGRequestScreenCaptureAccess`, `AXIsProcessTrustedWithOptions` with the prompt),
/// which adds `spaceo` to both Privacy lists. `--no-self-test` keeps it from starting a daemon.
pub(crate) fn gpui_spaceo_request_permissions(path: &Path) -> Result<(), String> {
    gpui_run_disclaimed_command_with_captured_output_timeout(
        path,
        &["setup", "--no-self-test", "--yes", "--json"],
        Duration::from_secs(30),
        64 * 1024,
    )
    .map(|_| ())
}

static SPACEO_RESTARTING: AtomicBool = AtomicBool::new(false);

/// True while a SpaceO restart started here is waiting for agents' sessions to finish.
pub(crate) fn gpui_spaceo_restarting() -> bool {
    SPACEO_RESTARTING.load(Ordering::SeqCst)
}

/// Restarts SpaceO's daemon so it picks up new grants. `--operator` without `--now` drains: the
/// daemon refuses new sessions, waits for running ones to end, and launchd starts it again as
/// the LaunchAgent, so the restart never ends an agent's session. `None` when one is running.
pub(crate) fn gpui_spaceo_restart_daemon(path: &Path) -> Option<Result<(), String>> {
    if SPACEO_RESTARTING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = gpui_run_command_with_captured_output_timeout(
        path,
        &["daemon", "restart", "--operator"],
        Duration::from_secs(30 * 60),
        16 * 1024,
    )
    .and_then(|output| {
        if output.success {
            Ok(())
        } else {
            Err(output.combined_text().trim().to_string())
        }
    });
    SPACEO_RESTARTING.store(false, Ordering::SeqCst);
    Some(result)
}

static CUA_DRIVER_GRANT_RUNNING: AtomicBool = AtomicBool::new(false);
static CUA_DRIVER_RESTARTING: AtomicBool = AtomicBool::new(false);
static CUA_DRIVER_GRANT_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// How long a restarted daemon gets to answer before the restart counts as failed.
const CUA_DRIVER_RESTART_WAIT: Duration = Duration::from_secs(15);

/// What CuaDriver can use right now, from the daemon agents talk to, and what macOS has now,
/// from a fresh process.
///
/// CDXC:ManagedTools 2026-10-31 WHY:
/// A `cua-driver serve` daemon that was refused once keeps saying so: macOS caches the negative preflight in that process, and one that outlived a login-session restart cannot reach tccd at all (seen on macOS 27: every TCCAccessRequest failed with a bootstrap look-up error, so Screen Recording read false with the switch on). `permissions status`, `check_permissions` and `permissions grant` all ask that daemon, so Grant waited 180 seconds and failed while the row stayed Not granted. A fresh process with CuaDriver's identity tells the two apart: macOS has it but the daemon says no is Needs restart, as for SpaceO, and only a real no stays Not granted.
pub(crate) fn gpui_cua_driver_grant_state(
    daemon: Option<bool>,
    fresh: Option<bool>,
    daemon_stale: bool,
) -> GpuiToolPermissionState {
    use GpuiToolPermissionState::*;
    match (daemon, fresh) {
        (Some(_), Some(false)) | (None, Some(false)) => NotGranted,
        (Some(false), Some(true)) => NeedsRestart,
        (Some(_), _) if daemon_stale => NeedsRestart,
        (Some(granted), _) => GpuiToolPermissionState::from_granted(Some(granted)),
        (None, Some(true)) => Granted,
        (None, None) => Unknown,
    }
}

/// `cua-driver --cua-internal-permission-probe` is the check CuaDriver's own permissions gate
/// polls: a short-lived copy of the signed executable that prints its two grants, never prompts
/// and never talks to or starts a daemon. Spawned disclaimed, so the grants are CuaDriver's and
/// not Ghostex's. `None` when this version doesn't answer it.
fn gpui_cua_driver_fresh_permissions(path: &Path) -> Option<(bool, bool)> {
    let executable = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let output = gpui_run_disclaimed_command_with_captured_output_timeout(
        &executable,
        &["--cua-internal-permission-probe"],
        Duration::from_secs(5),
        16 * 1024,
    )
    .ok()
    .filter(|output| output.success)?;
    let payload = gpui_parse_cua_permission_payload(&output.stdout);
    Some((
        gpui_parse_cua_permission(payload.as_ref(), "accessibility")?,
        gpui_parse_cua_permission(payload.as_ref(), "screen_recording")?,
    ))
}

/// A running daemon whose executable is gone or isn't the installed one (replaced by a reinstall
/// or an update) keeps answering for CuaDriver until something restarts it.
fn gpui_cua_driver_daemon_copy_is_stale(running: Option<&Path>, installed: &Path) -> bool {
    running.is_none_or(|running| {
        let canonical = |path: &Path| std::fs::canonicalize(path).ok();
        canonical(running).is_none() || canonical(running) != canonical(installed)
    })
}

#[cfg(target_os = "macos")]
fn gpui_cua_driver_daemon_is_stale(pid: i32, installed: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: signal 0 only checks that the process exists.
    if unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    let mut buffer = vec![0u8; 4096];
    // SAFETY: a writable buffer of the size given.
    let length =
        unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    let running = (length > 0)
        .then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&buffer[..length as usize])));
    gpui_cua_driver_daemon_copy_is_stale(running.as_deref(), installed)
}

#[cfg(not(target_os = "macos"))]
fn gpui_cua_driver_daemon_is_stale(_pid: i32, _installed: &Path) -> bool {
    false
}

/// Adds macOS's current answer to the daemon's. The fresh check only runs when the daemon's
/// answer alone could be wrong, since Settings asks for this every few seconds during a Grant.
pub(crate) fn gpui_cua_driver_resolve_permission_status(
    mut daemon: GpuiCuaDriverPermissionStatus,
    path: &Path,
    daemon_pid: Option<i32>,
) -> GpuiCuaDriverPermissionStatus {
    daemon.daemon_stale = daemon_pid.is_some_and(|pid| gpui_cua_driver_daemon_is_stale(pid, path));
    let daemon_sure = daemon.accessibility_granted == Some(true)
        && daemon.screen_recording_granted == Some(true)
        && !daemon.daemon_stale;
    let fresh = (!daemon_sure)
        .then(|| gpui_cua_driver_fresh_permissions(path))
        .flatten();
    let fresh_accessibility = fresh.map(|fresh| fresh.0);
    let fresh_screen_recording = fresh.map(|fresh| fresh.1);
    daemon.accessibility = gpui_cua_driver_grant_state(
        daemon.accessibility_granted,
        fresh_accessibility,
        daemon.daemon_stale,
    );
    daemon.screen_recording = gpui_cua_driver_grant_state(
        daemon.screen_recording_granted,
        fresh_screen_recording,
        daemon.daemon_stale,
    );
    // With no daemon running, macOS's answer is what the next daemon starts with.
    daemon.accessibility_granted = daemon.accessibility_granted.or(fresh_accessibility);
    daemon.screen_recording_granted = daemon.screen_recording_granted.or(fresh_screen_recording);
    if [daemon.accessibility, daemon.screen_recording]
        .contains(&GpuiToolPermissionState::NeedsRestart)
    {
        daemon.detail =
            "Fast Computer & Browser Use needs a restart to use its permissions.".to_string();
    } else if fresh.is_some() {
        daemon.detail = gpui_cua_driver_permission_detail(
            daemon.accessibility_granted,
            daemon.screen_recording_granted,
            true,
        );
    }
    daemon
}

/// What a failed `cua-driver permissions grant` said, for the row. `None` when it only gave up
/// waiting for the switch: Settings keeps watching macOS itself, so that is no failure.
pub(crate) fn gpui_cua_driver_grant_failure(output: &str) -> Option<String> {
    if output.contains("Timed out waiting on") {
        return None;
    }
    let lines = || {
        output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    let line = lines()
        .find(|line| line.starts_with('\u{274c}') || line.starts_with("Didn't detect"))
        .or_else(|| lines().last())?;
    Some(
        line.trim_start_matches(|character: char| !character.is_ascii_alphanumeric())
            .to_string(),
    )
}

/// True while a Grant click's `cua-driver permissions grant` is still running.
pub(crate) fn gpui_cua_driver_grant_running() -> bool {
    CUA_DRIVER_GRANT_RUNNING.load(Ordering::SeqCst)
}

/// True while Ghostex is restarting CuaDriver's daemon.
pub(crate) fn gpui_cua_driver_restarting() -> bool {
    CUA_DRIVER_RESTARTING.load(Ordering::SeqCst)
}

fn gpui_cua_driver_stop_daemon(path: &Path) -> Result<(), String> {
    let output = gpui_run_command_with_captured_output_timeout(
        path,
        &["stop"],
        Duration::from_secs(10),
        16 * 1024,
    )?;
    let text = output.combined_text();
    if output.success || text.contains("not running") {
        Ok(())
    } else {
        Err(text.trim().to_string())
    }
}

/// The Grant button for Fast Computer & Browser Use: `cua-driver permissions grant` opens
/// CuaDriver.app through LaunchServices, so its requests and the System Settings entries are
/// CuaDriver's (`com.trycua.driver`), never Ghostex's. It verifies through the daemon, so a daemon
/// that is no longer the installed copy is stopped first and the grant starts a current one.
/// `None` when a Grant is already running: it waits up to minutes, and every extra click would
/// start another copy.
pub(crate) fn gpui_cua_driver_request_permissions(path: &Path) -> Option<Result<(), String>> {
    if CUA_DRIVER_GRANT_RUNNING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = (|| {
        if gpui_cua_driver_permission_status(Some(path), true).daemon_stale {
            gpui_cua_driver_stop_daemon(path)?;
        }
        let output = gpui_run_command_with_captured_output_timeout(
            path,
            &["permissions", "grant"],
            Duration::from_secs(10 * 60),
            64 * 1024,
        )?;
        match output.success {
            true => Ok(()),
            false => gpui_cua_driver_grant_failure(&output.combined_text()).map_or(Ok(()), Err),
        }
    })();
    if let Ok(mut error) = CUA_DRIVER_GRANT_ERROR.lock() {
        *error = result.as_ref().err().cloned();
    }
    CUA_DRIVER_GRANT_RUNNING.store(false, Ordering::SeqCst);
    Some(result)
}

/// The `.app` a CuaDriver executable belongs to: restarting has to go through it for macOS to
/// count the daemon as CuaDriver.
fn gpui_cua_driver_app_bundle(executable: &Path) -> Option<PathBuf> {
    let bundle = executable.parent()?.parent()?.parent()?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Restarts CuaDriver's daemon so it picks up the grants macOS has now: `cua-driver stop`, then
/// the launch `cua-driver` itself uses (`open -n -g <app> --args serve`), then waits until it
/// answers. Unlike SpaceO's restart nothing waits for agents, since CuaDriver has no draining
/// restart. `None` when one is already running.
pub(crate) fn gpui_cua_driver_restart_daemon(path: &Path) -> Option<Result<(), String>> {
    if CUA_DRIVER_RESTARTING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = (|| {
        let executable = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
        let bundle = gpui_cua_driver_app_bundle(&executable)
            .ok_or_else(|| format!("{} is not inside an app.", executable.display()))?;
        gpui_cua_driver_stop_daemon(path)?;
        let bundle = bundle.to_string_lossy();
        let opened = gpui_run_command_with_captured_output_timeout(
            Path::new("/usr/bin/open"),
            &["-n", "-g", bundle.as_ref(), "--args", "serve"],
            Duration::from_secs(10),
            16 * 1024,
        )?;
        if !opened.success {
            return Err(opened.combined_text().trim().to_string());
        }
        let deadline = Instant::now() + CUA_DRIVER_RESTART_WAIT;
        while Instant::now() < deadline {
            let answered = gpui_run_command_with_captured_output_timeout(
                path,
                &["status"],
                Duration::from_secs(3),
                16 * 1024,
            )
            .is_ok_and(|output| output.success && output.stdout.contains("daemon is running"));
            if answered {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Err("It did not start again in time.".to_string())
    })();
    CUA_DRIVER_RESTARTING.store(false, Ordering::SeqCst);
    Some(result)
}

/// The per-grant fields both the full status and the quick re-check publish. While the daemon is
/// down for a restart a granted row still reads as restarting, so it can't flash Granted early.
pub(crate) fn gpui_cua_driver_add_permission_fields(
    payload: &mut serde_json::Value,
    status: &GpuiCuaDriverPermissionStatus,
) {
    let restarting = gpui_cua_driver_restarting();
    let shown = |state: GpuiToolPermissionState| {
        if restarting && state == GpuiToolPermissionState::Granted {
            GpuiToolPermissionState::NeedsRestart.as_str()
        } else {
            state.as_str()
        }
    };
    payload["cuaDriverAccessibilityPermission"] = serde_json::json!(shown(status.accessibility));
    payload["cuaDriverScreenRecordingPermission"] =
        serde_json::json!(shown(status.screen_recording));
    payload["cuaDriverDaemonStale"] = serde_json::json!(status.daemon_stale);
    payload["cuaDriverRestarting"] = serde_json::json!(restarting);
    payload["cuaDriverGrantRunning"] = serde_json::json!(gpui_cua_driver_grant_running());
    payload["cuaDriverGrantError"] = serde_json::json!(
        CUA_DRIVER_GRANT_ERROR
            .lock()
            .ok()
            .and_then(|error| error.clone())
    );
}

/// A `ghostexCliStatus` update with only the grant fields, cheap enough for Settings to ask for
/// every few seconds while a grant is pending; the Settings store merges it (`partial`).
pub(crate) fn gpui_tool_permission_status_message() -> serde_json::Value {
    let mut payload = serde_json::json!({ "type": "ghostexCliStatus", "partial": true });
    if let Some(path) = gpui_spaceo_executable_path() {
        let report = gpui_spaceo_permission_report(&path);
        payload["spaceoDaemonRunning"] = serde_json::json!(report.daemon_running);
        payload["spaceoPermissionApp"] = serde_json::json!(report.permission_app);
        payload["spaceoAccessibilityPermission"] = serde_json::json!(report.accessibility.as_str());
        payload["spaceoScreenRecordingPermission"] =
            serde_json::json!(report.screen_recording.as_str());
        payload["spaceoRestarting"] = serde_json::json!(gpui_spaceo_restarting());
    }
    let cua_driver_path = gpui_cua_driver_executable_path();
    if cua_driver_path.is_some() {
        let cua_app_installed = gpui_is_dir(Path::new("/Applications/CuaDriver.app"));
        let cua = gpui_cua_driver_permission_status(cua_driver_path.as_deref(), cua_app_installed);
        payload["cuaDriverAccessibilityPermissionGranted"] =
            serde_json::json!(cua.accessibility_granted);
        payload["cuaDriverScreenRecordingPermissionGranted"] =
            serde_json::json!(cua.screen_recording_granted);
        payload["cuaDriverPermissionDetail"] = serde_json::json!(cua.detail);
        gpui_cua_driver_add_permission_fields(&mut payload, &cua);
    }
    payload
}

/// How many sessions are running in SpaceO right now (`session list --operator`, read-only);
/// `None` when no daemon answers. SpaceO restarts by itself only once this reaches 0.
pub(crate) fn gpui_spaceo_live_sessions(path: &Path) -> Option<usize> {
    let output = gpui_run_command_with_captured_output_timeout(
        path,
        &["session", "list", "--operator", "--json"],
        Duration::from_secs(5),
        256 * 1024,
    )
    .ok()
    .filter(|output| output.success)?;
    gpui_spaceo_json_object(&output.stdout)?
        .get("sessions")?
        .as_array()
        .map(Vec::len)
}

#[cfg(test)]
mod cua_driver_grant_tests {
    use super::GpuiToolPermissionState::*;
    use super::*;

    #[test]
    fn a_daemon_that_says_no_while_macos_says_yes_needs_a_restart() {
        assert_eq!(
            gpui_cua_driver_grant_state(Some(false), Some(true), false),
            NeedsRestart
        );
        assert_eq!(
            gpui_cua_driver_grant_state(Some(false), Some(true), true),
            NeedsRestart
        );
    }

    #[test]
    fn a_real_no_from_macos_stays_not_granted_so_grant_is_offered() {
        for daemon in [Some(false), Some(true), None] {
            for stale in [false, true] {
                assert_eq!(
                    gpui_cua_driver_grant_state(daemon, Some(false), stale),
                    NotGranted
                );
            }
        }
    }

    #[test]
    fn a_replaced_daemon_needs_a_restart_even_when_it_says_yes() {
        assert_eq!(
            gpui_cua_driver_grant_state(Some(true), Some(true), true),
            NeedsRestart
        );
        assert_eq!(
            gpui_cua_driver_grant_state(Some(false), None, true),
            NeedsRestart
        );
    }

    #[test]
    fn no_daemon_uses_macos_and_no_probe_keeps_the_daemons_word() {
        assert_eq!(
            gpui_cua_driver_grant_state(None, Some(true), false),
            Granted
        );
        assert_eq!(gpui_cua_driver_grant_state(None, None, false), Unknown);
        assert_eq!(
            gpui_cua_driver_grant_state(Some(true), None, false),
            Granted
        );
        assert_eq!(
            gpui_cua_driver_grant_state(Some(false), None, false),
            NotGranted
        );
    }

    #[test]
    fn a_daemon_whose_binary_is_gone_or_elsewhere_is_stale() {
        let installed = std::env::current_exe().unwrap();
        assert!(gpui_cua_driver_daemon_copy_is_stale(None, &installed));
        assert!(gpui_cua_driver_daemon_copy_is_stale(
            Some(Path::new(
                "/nonexistent/CuaDriver.app.install-backup.1/cua-driver"
            )),
            &installed
        ));
        assert!(gpui_cua_driver_daemon_copy_is_stale(
            Some(Path::new("/bin/ls")),
            &installed
        ));
        assert!(!gpui_cua_driver_daemon_copy_is_stale(
            Some(&installed),
            &installed
        ));
    }

    #[test]
    fn waiting_for_the_switch_is_no_failure_and_real_errors_lose_their_emoji() {
        assert_eq!(
            gpui_cua_driver_grant_failure(
                "checking\n\n\u{26a0}\u{fe0f}  Timed out waiting on: Screen Recording.\nApprove it"
            ),
            None
        );
        assert_eq!(
            gpui_cua_driver_grant_failure(
                "Accessibility and Screen Recording are granted.\n\u{274c} Cua Driver still cannot use direct ScreenCaptureKit capture.\nIn System Settings"
            )
            .as_deref(),
            Some("Cua Driver still cannot use direct ScreenCaptureKit capture.")
        );
        assert_eq!(
            gpui_cua_driver_grant_failure("\nDidn't detect the Cua Driver daemon: timeout\nIf you")
                .as_deref(),
            Some("Didn't detect the Cua Driver daemon: timeout")
        );
        assert_eq!(gpui_cua_driver_grant_failure("  \n"), None);
    }

    #[test]
    fn the_app_bundle_comes_from_the_executable_inside_it() {
        assert_eq!(
            gpui_cua_driver_app_bundle(Path::new(
                "/Applications/CuaDriver.app/Contents/MacOS/cua-driver"
            )),
            Some(PathBuf::from("/Applications/CuaDriver.app"))
        );
        assert_eq!(
            gpui_cua_driver_app_bundle(Path::new("/usr/local/lib/x/cua-driver")),
            None
        );
    }
}
