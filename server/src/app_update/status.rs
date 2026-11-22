//! Asking this computer's Ghostex app about its updater, and the status every client reads.
use serde_json::{Map, Value, json};

use super::record::{AppUpdateRecord, InstallResult, now_iso, read_settled_record, write_record};
use crate::constants::GXSERVER_VERSION;
use crate::server::AppState;

/// The renderer command the app answers (`RendererVerb::AppUpdate` in gx-core).
pub(super) const APP_UPDATE_RENDERER_ACTION: &str = "appUpdate";
const STATUS_TIMEOUT_MS: u64 = 4_000;
const ACTION_TIMEOUT_MS: u64 = 15_000;

/// What the app answered, or why nothing did.
pub(super) enum AppAnswer {
    Report(Map<String, Value>),
    NotRunning,
    Refused(String),
}

pub(super) async fn ask_app(state: &AppState, op: &str) -> AppAnswer {
    let mut payload = Map::new();
    payload.insert("op".to_string(), json!(op));
    let timeout = if op == "status" {
        STATUS_TIMEOUT_MS
    } else {
        ACTION_TIMEOUT_MS
    };
    match state
        .event_hub
        .dispatch_renderer_command(APP_UPDATE_RENDERER_ACTION.to_string(), payload, timeout)
        .await
    {
        Ok(Value::Object(answer)) => {
            if answer.get("ok").and_then(Value::as_bool) == Some(false) {
                let message = answer
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Ghostex could not start the update.");
                // An app from before this verb answers the generic unsupported error.
                if message.contains("Unsupported renderer command") {
                    return AppAnswer::Refused(
                        "Ghostex on this computer is too old to update from another device; update it there once."
                            .to_string(),
                    );
                }
                return AppAnswer::Refused(message.to_string());
            }
            AppAnswer::Report(
                answer
                    .get("appUpdate")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            )
        }
        Ok(_) => AppAnswer::Report(Map::new()),
        Err(error) if error.message.starts_with("Timed out") => AppAnswer::Refused(
            "Ghostex on this computer did not answer in time; try again.".to_string(),
        ),
        Err(error) if error.code == "dependencyUnavailable" => AppAnswer::NotRunning,
        Err(error) => AppAnswer::Refused(error.message),
    }
}

fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

fn text(report: &Map<String, Value>, key: &str) -> Option<String> {
    report
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The app's failure while an install was pending ends that install.
fn settle_failed_install(state: &AppState, record: &mut AppUpdateRecord, message: String) {
    let Some(pending) = record.pending_install.take() else {
        return;
    };
    record.last_result = Some(InstallResult {
        outcome: "failed".to_string(),
        to_version: pending.from_version.clone(),
        from_version: pending.from_version,
        finished_at: now_iso(),
        message: Some(message),
    });
    write_record(&state.paths, record);
}

/// The status object: the running version, what the app's updater says, the install in flight and the switch.
pub(super) fn compose_status(state: &AppState, answer: &AppAnswer) -> Value {
    let mut record = read_settled_record(&state.paths);
    let empty = Map::new();
    let (report, app_connected, app_error) = match answer {
        AppAnswer::Report(report) => (report, true, None),
        AppAnswer::NotRunning => (&empty, false, None),
        AppAnswer::Refused(message) => (&empty, true, Some(message.clone())),
    };
    let mut phase = text(report, "phase").unwrap_or_else(|| {
        if app_connected {
            "idle".to_string()
        } else {
            "appNotRunning".to_string()
        }
    });
    let mut message = text(report, "message").or(app_error.clone());
    if record.pending_install.is_some() {
        if phase == "failed" {
            settle_failed_install(
                state,
                &mut record,
                message
                    .clone()
                    .unwrap_or_else(|| "The update failed.".to_string()),
            );
        } else if !app_connected {
            // The app quit to install: the next gxserver to answer is the new build.
            phase = "restarting".to_string();
            message = None;
        } else if !matches!(phase.as_str(), "downloading" | "installing" | "checking") {
            phase = "installing".to_string();
        }
    }
    let available_version = text(report, "availableVersion");
    let app_platform = text(report, "platform");
    let linux_update =
        super::linux_install::linux_update(app_platform.as_deref(), available_version.as_deref());
    let unsupported_reason = if linux_update.is_null() {
        None
    } else {
        super::linux_install::unsupported_reason(&linux_update, available_version.as_deref())
    }
    .or_else(|| text(report, "unsupportedReason"));
    json!({
        "version": GXSERVER_VERSION,
        "platform": platform(),
        "appConnected": app_connected,
        "appPlatform": app_platform,
        "appVersion": text(report, "appVersion"),
        "supported": report.get("supported").and_then(Value::as_bool).unwrap_or(false),
        "unsupportedReason": unsupported_reason,
        "linuxUpdate": linux_update,
        "phase": phase,
        "updateAvailable": available_version.is_some(),
        "availableVersion": available_version,
        "progress": report.get("progress").and_then(Value::as_f64),
        "message": message,
        "autoUpdate": record.auto_update,
        "pendingInstall": record.pending_install,
        "lastResult": record.last_result,
        "readAt": now_iso(),
    })
}
