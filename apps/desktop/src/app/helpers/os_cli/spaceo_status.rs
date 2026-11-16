//! SpaceO (github.com/ParthJadhav/SpaceO) for Settings > Integrations: whether this Mac can run
//! it, where `spaceo` is, its installed and latest versions, and its Accessibility and Screen
//! Recording grants (tool_permissions.rs). Every probe is read-only.

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::app::helpers::*;

const SPACEO_LATEST_RELEASE_URL: &str = "https://github.com/ParthJadhav/SpaceO/releases/latest";
/// How long a successful latest-release answer is reused, like cua-driver's own update cache.
const LATEST_VERSION_TTL: Duration = Duration::from_secs(20 * 60 * 60);

static LATEST_VERSION: Mutex<Option<(Instant, String)>> = Mutex::new(None);

/// SpaceO ships only for Apple Silicon Macs on macOS 14 or later; Settings shows it nowhere else.
pub(crate) fn gpui_spaceo_supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            // `hw.optional.arm64` stays 1 under Rosetta, where the build's own arch would say x86_64.
            let apple_silicon = gpui_sysctl_int("hw.optional.arm64") == Some(1);
            let major = gpui_sysctl_text("kern.osproductversion")
                .and_then(|version| version.split('.').next()?.parse::<u32>().ok());
            apple_silicon && major.is_some_and(|major| major >= 14)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    })
}

#[cfg(target_os = "macos")]
fn gpui_sysctl_text(name: &str) -> Option<String> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut size: libc::size_t = 0;
    // SAFETY: a null buffer asks sysctl for the value's size only.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let mut buffer = vec![0u8; size];
    // SAFETY: `buffer` holds `size` writable bytes, and sysctl writes at most that many.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    buffer.truncate(size);
    let text = String::from_utf8_lossy(&buffer);
    Some(text.trim_end_matches('\0').trim().to_string())
}

#[cfg(target_os = "macos")]
fn gpui_sysctl_int(name: &str) -> Option<i32> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut value: i32 = 0;
    let mut size = std::mem::size_of::<i32>() as libc::size_t;
    // SAFETY: `value` is a writable i32 and `size` is its exact size.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut i32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0).then_some(value)
}

/// `spaceo` on PATH, else where SpaceO's installer puts it (the app's PATH may not list it).
pub(crate) fn gpui_spaceo_executable_path() -> Option<PathBuf> {
    if !gpui_spaceo_supported() {
        return None;
    }
    if let Some(path) = gpui_which_command("spaceo") {
        return Some(path);
    }
    let installed = gpui_home_dir().join(".local/bin/spaceo");
    gpui_is_executable_file(&installed).then_some(installed)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSpaceoStatus {
    pub(crate) path: Option<PathBuf>,
    pub(crate) version: Option<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) update_available: Option<bool>,
    pub(crate) permissions: GpuiSpaceoPermissionReport,
    /// Whether SpaceO can give agents screens (`spaceoHealth`, os_integration/spaceo_heal.rs).
    pub(crate) health: Option<serde_json::Value>,
}

/// `fresh` skips the cached latest-release answer; Settings' "check again" button asks for it.
pub(crate) fn gpui_spaceo_status(fresh: bool) -> GpuiSpaceoStatus {
    let Some(path) = gpui_spaceo_executable_path() else {
        return GpuiSpaceoStatus::default();
    };
    let version = gpui_spaceo_version(&path);
    let latest_version = gpui_spaceo_latest_version(fresh);
    let update_available = match (version.as_deref(), latest_version.as_deref()) {
        (Some(current), Some(latest)) => {
            Some(gpui_compare_release_versions(latest, current) == std::cmp::Ordering::Greater)
        }
        _ => None,
    };
    let permissions = gpui_spaceo_permission_report(&path);
    GpuiSpaceoStatus {
        path: Some(path),
        version,
        latest_version,
        update_available,
        permissions,
        health: crate::app::os_integration::spaceo_heal::gpui_spaceo_health_json(),
    }
}

fn gpui_spaceo_version(path: &Path) -> Option<String> {
    let output = gpui_run_command_with_captured_output_timeout(
        path,
        &["version", "--json"],
        Duration::from_secs(3),
        8 * 1024,
    )
    .ok()
    .filter(|output| output.success)?;
    gpui_spaceo_json_object(&output.stdout)
        .and_then(|payload| {
            payload
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| gpui_cua_driver_version_from_text(&output.stdout))
}

/// The tag GitHub's latest-release page redirects to, the way SpaceO's own installer finds it.
fn gpui_spaceo_latest_version(fresh: bool) -> Option<String> {
    if !fresh
        && let Some((checked_at, version)) = LATEST_VERSION.lock().ok()?.clone()
        && checked_at.elapsed() < LATEST_VERSION_TTL
    {
        return Some(version);
    }
    let output = gpui_run_command_with_captured_output_timeout(
        Path::new("/usr/bin/curl"),
        &[
            "-fsSL",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            "10",
            "--max-time",
            "15",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            SPACEO_LATEST_RELEASE_URL,
        ],
        Duration::from_secs(20),
        4 * 1024,
    )
    .ok()
    .filter(|output| output.success)?;
    let tag = output.stdout.trim().rsplit('/').next()?.strip_prefix('v')?;
    let valid = tag.split('.').count() == 3
        && tag
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if !valid {
        return None;
    }
    if let Ok(mut latest) = LATEST_VERSION.lock() {
        *latest = Some((Instant::now(), tag.to_string()));
    }
    Some(tag.to_string())
}

/// Numeric `MAJOR.MINOR.PATCH` comparison; a part that is not a number counts as 0.
fn gpui_compare_release_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split(['.', '-', '+'])
            .take(3)
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    parts(left).cmp(&parts(right))
}

pub(crate) fn gpui_spaceo_json_object(stdout: &str) -> Option<serde_json::Value> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    (start <= end)
        .then(|| serde_json::from_str::<serde_json::Value>(&stdout[start..=end]).ok())
        .flatten()
}

/// The SpaceO fields of the `ghostexCliStatus` payload (`SidebarGhostexCliStatusMessage`).
pub(crate) fn gpui_spaceo_status_fields(
    status: &GpuiSpaceoStatus,
    skill_path: Option<&str>,
) -> serde_json::Value {
    if !gpui_spaceo_supported() {
        return serde_json::json!({ "spaceoSupported": false });
    }
    serde_json::json!({
        "spaceoSupported": true,
        "spaceoInstalled": status.path.is_some(),
        "spaceoPath": status.path.as_ref().map(|path| gpui_path_string(path)),
        "spaceoVersion": status.version,
        "spaceoLatestVersion": status.latest_version,
        "spaceoUpdateAvailable": status.update_available,
        "spaceoInstallCommand": GPUI_SPACEO_INSTALL_COMMAND,
        "spaceoDaemonRunning": status.permissions.daemon_running,
        "spaceoPermissionApp": status.permissions.permission_app,
        "spaceoAccessibilityPermission": status.permissions.accessibility.as_str(),
        "spaceoScreenRecordingPermission": status.permissions.screen_recording.as_str(),
        "spaceoRestarting": gpui_spaceo_restarting(),
        "spaceoHealth": status.health,
        "spaceoSkillInstalled": skill_path.is_some(),
        "spaceoSkillPath": skill_path,
    })
}
