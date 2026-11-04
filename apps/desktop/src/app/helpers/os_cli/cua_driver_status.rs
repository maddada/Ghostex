use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::app::helpers::*;

pub(crate) fn gpui_cua_driver_executable_path() -> Option<PathBuf> {
    if let Some(path) = gpui_which_command("cua-driver") {
        return Some(path);
    }
    #[cfg(target_os = "macos")]
    {
        let app_binary = PathBuf::from("/Applications/CuaDriver.app/Contents/MacOS/cua-driver");
        if gpui_is_file(&app_binary) {
            return Some(app_binary);
        }
    }
    None
}

/// Where an agent finds the cua-driver skill pack: `cua-driver skills install` keeps the pack in
/// `~/.cua-driver/skills/cua-driver` and links it into each detected agent's skills folder, so
/// the skill counts as installed once one of those links resolves to its SKILL.md.
pub(crate) fn gpui_cua_driver_skill_path() -> Option<PathBuf> {
    let home = gpui_home_dir();
    [".agents", ".claude"]
        .into_iter()
        .map(|root| home.join(root).join("skills").join("cua-driver"))
        .find(|path| gpui_is_file(&path.join("SKILL.md")))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiCuaDriverUpdateStatus {
    pub(crate) current_version: Option<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) update_available: Option<bool>,
}

/// `fresh` skips cua-driver's 20h update-check cache; Settings' "check again" button asks for it.
pub(crate) fn gpui_cua_driver_update_status(
    cua_driver_path: Option<&Path>,
    fresh: bool,
) -> GpuiCuaDriverUpdateStatus {
    let Some(cua_driver_path) = cua_driver_path else {
        return GpuiCuaDriverUpdateStatus::default();
    };
    let current_version = gpui_run_command_with_captured_output_timeout(
        cua_driver_path,
        &["--version"],
        Duration::from_secs(3),
        8 * 1024,
    )
    .ok()
    .filter(|output| output.success)
    .and_then(|output| gpui_cua_driver_version_from_text(output.stdout.as_str()));

    #[cfg(target_os = "macos")]
    {
        let args: &[&str] = if fresh {
            &["check-update", "--json", "--no-cache"]
        } else {
            &["check-update", "--json"]
        };
        let Ok(output) = gpui_run_command_with_captured_output_timeout(
            cua_driver_path,
            args,
            Duration::from_secs(15),
            64 * 1024,
        ) else {
            return GpuiCuaDriverUpdateStatus {
                current_version,
                ..GpuiCuaDriverUpdateStatus::default()
            };
        };
        let Some(payload) = gpui_cua_driver_update_payload(output.stdout.as_str()) else {
            return GpuiCuaDriverUpdateStatus {
                current_version,
                ..GpuiCuaDriverUpdateStatus::default()
            };
        };
        let current_version = current_version.or_else(|| {
            payload
                .get("current_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
        let latest_version = payload
            .get("latest_version")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        // The cached answer written just before `update --apply` still says
        // "update available"; the installed binary's own version is the truth.
        let update_available = payload
            .get("update_available")
            .and_then(serde_json::Value::as_bool)
            .map(|available| available && current_version != latest_version);
        GpuiCuaDriverUpdateStatus {
            current_version,
            latest_version,
            update_available,
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = fresh;
    #[cfg(not(target_os = "macos"))]
    GpuiCuaDriverUpdateStatus {
        current_version,
        ..GpuiCuaDriverUpdateStatus::default()
    }
}

pub(crate) fn gpui_cua_driver_update_payload(stdout: &str) -> Option<serde_json::Value> {
    // A fresh Cua install can print its telemetry notice before JSON even when
    // --json is requested. Isolate the object so the Plugins status still gets
    // an exact current/latest version on that first check.
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    (start <= end)
        .then(|| serde_json::from_str::<serde_json::Value>(&stdout[start..=end]).ok())
        .flatten()
}

pub(crate) fn gpui_cua_driver_version_from_text(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '+')
            })
        })
        .find(|token| {
            token
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && token.contains('.')
                && token.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
                })
        })
        .map(str::to_string)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCuaDriverPermissionStatus {
    pub(crate) accessibility: GpuiToolPermissionState,
    pub(crate) accessibility_granted: Option<bool>,
    /// The running daemon is a copy that is no longer the installed one.
    pub(crate) daemon_stale: bool,
    pub(crate) detail: String,
    pub(crate) screen_recording: GpuiToolPermissionState,
    pub(crate) screen_recording_granted: Option<bool>,
}

pub(crate) fn gpui_cua_driver_permission_status(
    cua_driver_path: Option<&Path>,
    cua_app_installed: bool,
) -> GpuiCuaDriverPermissionStatus {
    let Some(cua_driver_path) = cua_driver_path else {
        return GpuiCuaDriverPermissionStatus {
            accessibility: GpuiToolPermissionState::Unknown,
            accessibility_granted: None,
            daemon_stale: false,
            detail: if cua_app_installed {
                "Fast Computer & Browser Use is installed, but the cua-driver CLI was not found on PATH, so GPUI cannot run the read-only permission check."
                    .to_string()
            } else {
                "Fast Computer & Browser Use is not installed.".to_string()
            },
            screen_recording: GpuiToolPermissionState::Unknown,
            screen_recording_granted: None,
        };
    };

    /*
    CDXC:Extensions 2026-06-24-13:14:
    The Cua permission probe is a status refresh, not a repair action. Run only `cua-driver check_permissions {"prompt":false}` with a short timeout, parse Accessibility and Screen Recording, and discard stdout/stderr before producing user-facing copy.
    */
    let (daemon, daemon_pid) = match gpui_run_command_with_captured_output_timeout(
        cua_driver_path,
        &["check_permissions", r#"{"prompt":false}"#],
        Duration::from_secs(5),
        64 * 1024,
    ) {
        Ok(output) => {
            let combined = output.combined_text();
            (
                gpui_cua_driver_permission_status_from_output(&combined, output.success),
                gpui_cua_driver_daemon_pid(&combined),
            )
        }
        Err(_) => (
            GpuiCuaDriverPermissionStatus {
                accessibility: GpuiToolPermissionState::Unknown,
                accessibility_granted: None,
                daemon_stale: false,
                detail:
                    "Unable to check Fast Computer & Browser Use permissions without prompting."
                        .to_string(),
                screen_recording: GpuiToolPermissionState::Unknown,
                screen_recording_granted: None,
            },
            None,
        ),
    };
    gpui_cua_driver_resolve_permission_status(daemon, cua_driver_path, daemon_pid)
}

/// The daemon's process id, when it answered as CuaDriver's own identity.
fn gpui_cua_driver_daemon_pid(output: &str) -> Option<i32> {
    let payload = gpui_parse_cua_permission_payload(output)?;
    let source = payload.get("source")?;
    if source.get("attribution")?.as_str()? != "driver-daemon" {
        return None;
    }
    i32::try_from(source.get("pid")?.as_i64()?).ok()
}

pub(crate) fn gpui_cua_driver_permission_status_from_output(
    output: &str,
    command_success: bool,
) -> GpuiCuaDriverPermissionStatus {
    let payload = gpui_parse_cua_permission_payload(output);
    let accessibility_granted = gpui_parse_cua_permission(payload.as_ref(), "accessibility");
    let screen_recording_granted = gpui_parse_cua_permission(payload.as_ref(), "screen_recording");
    GpuiCuaDriverPermissionStatus {
        accessibility: GpuiToolPermissionState::from_granted(accessibility_granted),
        accessibility_granted,
        daemon_stale: false,
        detail: gpui_cua_driver_permission_detail(
            accessibility_granted,
            screen_recording_granted,
            command_success,
        ),
        screen_recording: GpuiToolPermissionState::from_granted(screen_recording_granted),
        screen_recording_granted,
    }
}

/*
CDXC:Extensions 2026-08-20-11:05:
`cua-driver check_permissions` answers with a JSON object whose `accessibility`
and `screen_recording` members are booleans, so read those members instead of
scanning for prose lines that the CLI never prints.
*/
pub(crate) fn gpui_parse_cua_permission_payload(output: &str) -> Option<serde_json::Value> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(value);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]).ok()
}

pub(crate) fn gpui_parse_cua_permission(
    payload: Option<&serde_json::Value>,
    key: &str,
) -> Option<bool> {
    payload?.get(key)?.as_bool()
}

pub(crate) fn gpui_cua_driver_permission_detail(
    accessibility_granted: Option<bool>,
    screen_recording_granted: Option<bool>,
    command_success: bool,
) -> String {
    match (accessibility_granted, screen_recording_granted) {
        (Some(true), Some(true)) => {
            "Fast Computer & Browser Use reports Accessibility and Screen Recording permissions are granted.".to_string()
        }
        (Some(false), Some(false)) => "Fast Computer & Browser Use permissions need attention.".to_string(),
        (Some(false), _) => "Fast Computer & Browser Use Accessibility permission needs attention.".to_string(),
        (_, Some(false)) => "Fast Computer & Browser Use Screen Recording permission needs attention.".to_string(),
        _ if command_success => {
            "Fast Computer & Browser Use permission check completed, but GPUI could not recognize the permission state."
                .to_string()
        }
        _ => "Unable to check Fast Computer & Browser Use permissions without prompting.".to_string(),
    }
}
