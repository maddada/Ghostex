//! `/api/computerIcon` and `/api/setComputerIcon`: the icon this computer wears in every client's
//! sidebar and Settings, kept here so the desktop app on this computer, another computer connected
//! to it and a phone all show the same one (CDXC:RemoteMachines 2026-11-12 in gx-core
//! computer_icons.rs).
//!
//! CDXC:RemoteMachines 2026-11-12 WHY:
//! A computer's name lives in each client's own list (`remoteMachines` on a desktop, the phone's saved computers), so an icon stored there would differ per device. gxserver keeps it in its state folder instead (`computer-icon.json`), not in the settings file, which only the app writes: a computer that runs only gxserver can still be given an icon from another computer. The answer also says what kind of computer this is (`laptop` when it has a battery, else `desktop`; unknown on Windows), which the unpicked icon follows.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};

pub(crate) const COMPUTER_ICON_ENDPOINTS: &[&str] = &["/api/computerIcon", "/api/setComputerIcon"];

const ICON_FILE_NAME: &str = "computer-icon.json";

fn icon_file(state_dir: &Path) -> PathBuf {
    state_dir.join(ICON_FILE_NAME)
}

/// The picked icon's id, when one is saved and still offered.
fn read_picked_icon(state_dir: &Path) -> Option<&'static str> {
    let text = std::fs::read_to_string(icon_file(state_dir)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    ghostex_gx_core::computer_icons::computer_icon(value.get("icon")?.as_str()?).map(|icon| icon.id)
}

/// Saves `icon` (an id from the picker's list), or clears the pick when it is empty.
fn write_picked_icon(state_dir: &Path, icon: &str) -> Result<(), String> {
    let path = icon_file(state_dir);
    if icon.trim().is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("Could not clear this computer's icon: {error}"))
            }
            _ => Ok(()),
        };
    }
    let icon = ghostex_gx_core::computer_icons::computer_icon(icon)
        .ok_or_else(|| "That icon isn't one Ghostex offers.".to_string())?;
    std::fs::create_dir_all(state_dir)
        .map_err(|error| format!("Could not save this computer's icon: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json!({ "icon": icon.id }).to_string())
        .and_then(|()| std::fs::rename(&temporary, &path))
        .map_err(|error| format!("Could not save this computer's icon: {error}"))
}

/// `laptop` when this computer has a battery, `desktop` when it has none, `None` when it can't be
/// told (Windows).
fn computer_kind() -> Option<&'static str> {
    static KIND: OnceLock<Option<&'static str>> = OnceLock::new();
    *KIND.get_or_init(|| {
        let laptop = has_battery()?;
        Some(if laptop {
            ghostex_gx_core::computer_icons::COMPUTER_KIND_LAPTOP
        } else {
            ghostex_gx_core::computer_icons::COMPUTER_KIND_DESKTOP
        })
    })
}

#[cfg(target_os = "macos")]
fn has_battery() -> Option<bool> {
    let output = std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).contains("InternalBattery"))
}

#[cfg(target_os = "linux")]
fn has_battery() -> Option<bool> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    Some(entries.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("type"))
            .is_ok_and(|kind| kind.trim().eq_ignore_ascii_case("battery"))
    }))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn has_battery() -> Option<bool> {
    None
}

fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// What every caller gets: the picked icon (`null` when none), the computer's kind and its system.
fn answer(state_dir: &Path) -> Value {
    json!({
        "icon": read_picked_icon(state_dir),
        "kind": computer_kind(),
        "platform": platform(),
    })
}

pub(crate) async fn handle_computer_icon_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", error.message, Some(request_id)),
            );
        }
    };
    let state_dir = state.paths.app_state_dir.clone();
    let path = endpoint_path.clone();
    let result = tokio::task::spawn_blocking(move || match path.as_str() {
        "/api/setComputerIcon" => {
            let icon = params
                .get("icon")
                .and_then(Value::as_str)
                .unwrap_or_default();
            write_picked_icon(&state_dir, icon).map(|()| answer(&state_dir))
        }
        _ => Ok(answer(&state_dir)),
    })
    .await
    .unwrap_or_else(|error| Err(format!("Could not read this computer's icon: {error}")));
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(message) => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error("badRequest", message, Some(request_id)),
        ),
    }
}
