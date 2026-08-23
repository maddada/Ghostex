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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiCuaDriverUpdateStatus {
    pub(crate) current_version: Option<String>,
    pub(crate) latest_version: Option<String>,
    pub(crate) update_available: Option<bool>,
}

pub(crate) fn gpui_cua_driver_update_status(cua_driver_path: Option<&Path>) -> GpuiCuaDriverUpdateStatus {
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
        let Ok(output) = gpui_run_command_with_captured_output_timeout(
            cua_driver_path,
            &["check-update", "--json"],
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
        return GpuiCuaDriverUpdateStatus {
            current_version: current_version.or_else(|| {
                payload
                    .get("current_version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            }),
            latest_version: payload
                .get("latest_version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            update_available: payload
                .get("update_available")
                .and_then(serde_json::Value::as_bool),
        };
    }

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
    pub(crate) accessibility_granted: Option<bool>,
    pub(crate) detail: String,
    pub(crate) screen_recording_granted: Option<bool>,
}

pub(crate) fn gpui_cua_driver_permission_status(
    cua_driver_path: Option<&Path>,
    cua_app_installed: bool,
) -> GpuiCuaDriverPermissionStatus {
    let Some(cua_driver_path) = cua_driver_path else {
        return GpuiCuaDriverPermissionStatus {
            accessibility_granted: None,
            detail: if cua_app_installed {
                "Cua Driver app is installed, but the cua-driver CLI was not found on PATH, so GPUI cannot run the read-only permission check."
                    .to_string()
            } else {
                "Cua Driver is not installed.".to_string()
            },
            screen_recording_granted: None,
        };
    };

    /*
    CDXC:GPUIDesktopControlSettings 2026-06-24-13:14:
    The Cua permission probe is a status refresh, not a repair action. Run only `cua-driver check_permissions {"prompt":false}` with a short timeout, parse Accessibility and Screen Recording, and discard stdout/stderr before producing user-facing copy.
    */
    match gpui_run_command_with_captured_output_timeout(
        cua_driver_path,
        &["check_permissions", r#"{"prompt":false}"#],
        Duration::from_secs(5),
        64 * 1024,
    ) {
        Ok(output) => {
            let combined = output.combined_text();
            gpui_cua_driver_permission_status_from_output(&combined, output.success)
        }
        Err(_) => GpuiCuaDriverPermissionStatus {
            accessibility_granted: None,
            detail: "Unable to check Cua Driver permissions without prompting.".to_string(),
            screen_recording_granted: None,
        },
    }
}

pub(crate) fn gpui_cua_driver_permission_status_from_output(
    output: &str,
    command_success: bool,
) -> GpuiCuaDriverPermissionStatus {
    let payload = gpui_parse_cua_permission_payload(output);
    let accessibility_granted = gpui_parse_cua_permission(payload.as_ref(), "accessibility");
    let screen_recording_granted = gpui_parse_cua_permission(payload.as_ref(), "screen_recording");
    GpuiCuaDriverPermissionStatus {
        accessibility_granted,
        detail: gpui_cua_driver_permission_detail(
            accessibility_granted,
            screen_recording_granted,
            command_success,
        ),
        screen_recording_granted,
    }
}

/*
CDXC:GPUIDesktopControlSettings 2026-08-20-11:05:
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

pub(crate) fn gpui_parse_cua_permission(payload: Option<&serde_json::Value>, key: &str) -> Option<bool> {
    payload?.get(key)?.as_bool()
}

pub(crate) fn gpui_cua_driver_permission_detail(
    accessibility_granted: Option<bool>,
    screen_recording_granted: Option<bool>,
    command_success: bool,
) -> String {
    match (accessibility_granted, screen_recording_granted) {
        (Some(true), Some(true)) => {
            "Cua Driver reports Accessibility and Screen Recording permissions are granted."
                .to_string()
        }
        (Some(false), Some(false)) => "Cua Driver permissions need attention.".to_string(),
        (Some(false), _) => "Cua Driver Accessibility permission needs attention.".to_string(),
        (_, Some(false)) => "Cua Driver Screen Recording permission needs attention.".to_string(),
        _ if command_success => {
            "Cua Driver permission check completed, but GPUI could not recognize the permission state."
                .to_string()
        }
        _ => "Unable to check Cua Driver permissions without prompting.".to_string(),
    }
}

