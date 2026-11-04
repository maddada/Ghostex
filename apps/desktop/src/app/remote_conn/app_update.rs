//! A remote machine's Ghostex version and its Update button: reads and drives that machine's `/api/appUpdate*` (server/src/app_update/) through the machine's tunnel.
//!
//! CDXC:RemoteMachines 2026-10-16 DECISION:
//! User (#245): show each remote machine's Ghostex version, that an update is available, and an Update button with progress where the machines are listed (Settings > Remote and the sidebar's machine tabs), and reconnect cleanly when the machine's gxserver comes back. The install is that machine's own updater; this file only asks, follows the install through the restart (the tunnel drops while the new build replaces gxserver, so failed reads are waited out), and says how it ended.
//! SEE-ALSO: apps/desktop/src/app/os_integration/app_update_remote_control.rs (the far side), apps/desktop/src/app/window/settings_modal/tabs/remote/machines.rs, apps/desktop/src/app/native_sidebar/machines.rs.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::{GhostexGpuiApp, shared_settings};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const FOLLOW_POLL: Duration = Duration::from_secs(2);
/// An install is followed this long, through the machine's restart, before it is given up on.
const FOLLOW_LIMIT: Duration = Duration::from_secs(20 * 60);
/// How often a connected machine's version and update are read again.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteAppUpdate {
    /// The machine's last `/api/appUpdateStatus` (or action) answer.
    pub(crate) status: Option<Value>,
    /// Why the last request failed, in words for the user.
    pub(crate) error: Option<String>,
    /// A request is in flight.
    pub(crate) busy: bool,
    /// An install is being followed through the restart.
    pub(crate) following: bool,
    refreshed_at: Option<Instant>,
}

/// Every workspace window and the Settings window read the same machines.
static REMOTE_APP_UPDATES: LazyLock<Mutex<HashMap<String, RemoteAppUpdate>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_updates<T>(f: impl FnOnce(&mut HashMap<String, RemoteAppUpdate>) -> T) -> T {
    let mut updates = REMOTE_APP_UPDATES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut updates)
}

pub(crate) fn remote_app_update(machine_id: &str) -> Option<RemoteAppUpdate> {
    with_updates(|updates| updates.get(machine_id).cloned())
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// What the machine's row shows: `{ versionText, detail, tone, action, busy, progress, autoUpdate, updateAvailable }`.
pub(crate) fn remote_app_update_view(update: &RemoteAppUpdate) -> Value {
    let Some(status) = update.status.as_ref() else {
        return json!({
            "versionText": null,
            "detail": update.error,
            "tone": "warning",
            "action": update.error.as_ref().map(|_| json!({ "label": "Retry", "op": "refresh", "enabled": !update.busy })),
            "busy": update.busy,
            "progress": null,
            "autoUpdate": null,
            "updateAvailable": false,
        });
    };
    let version = text(status, "version");
    let available = text(status, "availableVersion");
    let mut phase = text(status, "phase").unwrap_or("idle");
    // A Linux server runs gxserver without the app; its package is updated from Settings > Remote's Update gxserver instead.
    if phase == "appNotRunning" && text(status, "platform") == Some("linux") {
        phase = "idle";
    }
    let supported = status.get("supported").and_then(Value::as_bool) == Some(true);
    let progress = status.get("progress").and_then(Value::as_f64);
    // "Updated to …" is news for a day, not a permanent label.
    let last_result = status.get("lastResult").filter(|result| {
        text(result, "finishedAt")
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .is_some_and(|at| {
                chrono::Utc::now().signed_duration_since(at) < chrono::Duration::hours(24)
            })
    });
    let in_progress = matches!(phase, "downloading" | "installing" | "restarting")
        || (phase == "checking" && update.following);
    let mut tone = "muted";
    let detail: Option<String> = if let Some(error) = update.error.as_ref() {
        tone = "warning";
        Some(error.clone())
    } else {
        match phase {
            "downloading" => Some(match progress {
                Some(progress) => format!("Downloading {}%", (progress * 100.0).round() as i64),
                None => "Downloading the update…".to_string(),
            }),
            "installing" => Some("Installing the update…".to_string()),
            "restarting" => Some("Restarting Ghostex there…".to_string()),
            "checking" => Some("Checking for updates…".to_string()),
            "appNotRunning" => {
                tone = "warning";
                Some(
                    "Ghostex is not open on this computer; open it there to update from here."
                        .to_string(),
                )
            }
            "failed" => {
                tone = "warning";
                Some(
                    text(status, "message")
                        .unwrap_or("The update failed.")
                        .to_string(),
                )
            }
            _ if last_result.is_some_and(|result| text(result, "outcome") == Some("installed")) => {
                tone = "success";
                last_result
                    .and_then(|result| text(result, "toVersion"))
                    .map(|to| format!("Updated to {to}"))
            }
            _ if last_result.is_some_and(|result| text(result, "outcome") == Some("failed")) => {
                tone = "warning";
                last_result
                    .and_then(|result| text(result, "message"))
                    .map(str::to_string)
            }
            _ if available.is_some() && !supported => text(status, "unsupportedReason")
                .map(|reason| format!("{} is out. {reason}", available.unwrap_or_default())),
            _ if available.is_some() => Some(format!(
                "Ghostex {} is available",
                available.unwrap_or_default()
            )),
            "upToDate" => Some("Up to date".to_string()),
            // Why there is no Update button (a build without its updater, a Linux package).
            _ if !supported => text(status, "unsupportedReason").map(str::to_string),
            _ => None,
        }
    };
    let action = if in_progress {
        None
    } else if let (Some(available), true) = (available, supported) {
        Some(
            json!({ "label": format!("Update to {available}"), "op": "install", "enabled": !update.busy }),
        )
    } else if supported || phase == "appNotRunning" {
        Some(json!({ "label": "Check for updates", "op": "check", "enabled": !update.busy }))
    } else {
        None
    };
    json!({
        "versionText": version.map(|version| format!("Ghostex {version}")),
        "detail": detail,
        "tone": tone,
        "action": action,
        "busy": update.busy || in_progress,
        "progress": progress,
        "autoUpdate": supported.then(|| status.get("autoUpdate").and_then(Value::as_bool).unwrap_or(false)),
        "updateAvailable": available.is_some() && supported && !in_progress,
    })
}

/// The machine's answer, or why there is none in words for the user.
fn remote_app_update_request(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &Value,
) -> Result<Value, String> {
    let (status_code, body) =
        gpui_remote_gxserver_post_typed_operation(target, path, params, REQUEST_TIMEOUT)?;
    let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if (200..300).contains(&status_code) {
        return parsed
            .get("result")
            .and_then(|result| result.get("status"))
            .cloned()
            .ok_or_else(|| "The machine sent an unreadable update status.".to_string());
    }
    // A gxserver from before these endpoints does not know the path.
    if status_code == 404 {
        return Err(
            "This computer's Ghostex is too old to update from here; update it there once."
                .to_string(),
        );
    }
    Err(parsed
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "The machine did not accept the update request.".to_string()))
}

fn remote_machine_name(machine_id: &str) -> String {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    settings
        .object()
        .get("remoteMachines")
        .and_then(Value::as_array)
        .and_then(|machines| {
            machines
                .iter()
                .find(|machine| machine.get("id").and_then(Value::as_str) == Some(machine_id))
        })
        .and_then(|machine| text(machine, "name").map(str::to_string))
        .unwrap_or_else(|| "The remote machine".to_string())
}

impl GhostexGpuiApp {
    /// A machine just connected (or reconnected after its restart): read its version and update.
    pub(crate) fn refresh_remote_app_update(
        &mut self,
        machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if remote_app_update(&machine_id).is_some_and(|update| update.busy || update.following) {
            return;
        }
        self.run_remote_app_update_request(
            machine_id,
            "/api/appUpdateStatus",
            json!({}),
            false,
            cx,
        );
    }

    /// Hands an open Settings window every saved machine's connection state (the wire slug, e.g.
    /// `connected`), which Settings > Remote counts on its Remote machines tab.
    pub(crate) fn publish_remote_machine_connect_states(&mut self, cx: &mut gpui::Context<Self>) {
        let states: serde_json::Map<String, Value> = self
            .remote_machine_connect_states
            .iter()
            .map(|(machine_id, state)| (machine_id.clone(), json!(state)))
            .collect();
        // The machines that are this computer (no Install/Update for them) and each failing
        // machine's sanitized reason, which the Settings tiles draw.
        let this_computer: Vec<&String> = self
            .remote_gxserver_connections
            .iter()
            .filter(|(_, connection)| connection.this_computer)
            .map(|(machine_id, _)| machine_id)
            .collect();
        self.dispatch_open_gpui_app_modal_message(
            json!({
                "type": "remoteMachineConnectStates",
                "states": states,
                "thisComputer": this_computer,
                "messages": self.remote_machine_status_messages,
            }),
            cx,
        );
    }

    /// Settings > Remote opened: every connected machine read once more if its answer is old.
    pub(crate) fn refresh_stale_remote_app_updates(&mut self, cx: &mut gpui::Context<Self>) {
        self.publish_remote_machine_connect_states(cx);
        let machine_ids: Vec<String> = self.remote_gxserver_connections.keys().cloned().collect();
        for machine_id in machine_ids {
            let fresh = remote_app_update(&machine_id).is_some_and(|update| {
                update
                    .refreshed_at
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
            });
            if fresh {
                self.publish_remote_app_update(&machine_id, cx);
            } else {
                self.refresh_remote_app_update(machine_id, cx);
            }
        }
    }

    /// Re-reads connected machines whose answer is older than `REFRESH_INTERVAL` (the lead window's periodic pass).
    pub(crate) fn refresh_expired_remote_app_updates(&mut self, cx: &mut gpui::Context<Self>) {
        let machine_ids: Vec<String> = self.remote_gxserver_connections.keys().cloned().collect();
        for machine_id in machine_ids {
            let expired = remote_app_update(&machine_id).is_none_or(|update| {
                update
                    .refreshed_at
                    .is_none_or(|at| at.elapsed() >= REFRESH_INTERVAL)
            });
            if expired {
                self.refresh_remote_app_update(machine_id, cx);
            }
        }
    }

    /// `remoteAppUpdate` from Settings or the sidebar: `op` is `refresh`, `check`, `install` or `setAuto`.
    pub(crate) fn handle_remote_app_update_command(
        &mut self,
        command: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command.get("op").and_then(Value::as_str) == Some("refreshAll") {
            self.refresh_stale_remote_app_updates(cx);
            return;
        }
        let Some(machine_id) = command
            .get("remoteMachineId")
            .and_then(Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        match command.get("op").and_then(Value::as_str) {
            Some("refresh") => self.run_remote_app_update_request(
                machine_id,
                "/api/appUpdateStatus",
                json!({}),
                false,
                cx,
            ),
            Some("check") => self.run_remote_app_update_request(
                machine_id,
                "/api/appUpdateCheck",
                json!({}),
                true,
                cx,
            ),
            Some("install") => self.run_remote_app_update_request(
                machine_id,
                "/api/appUpdateInstall",
                json!({}),
                true,
                cx,
            ),
            Some("setAuto") => {
                let enabled = command.get("enabled").and_then(Value::as_bool) == Some(true);
                self.run_remote_app_update_request(
                    machine_id,
                    "/api/appUpdateSetAuto",
                    json!({ "enabled": enabled }),
                    false,
                    cx,
                )
            }
            _ => {}
        }
    }

    fn run_remote_app_update_request(
        &mut self,
        machine_id: String,
        path: &'static str,
        params: Value,
        follow: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.gpui_remote_gxserver_request_target(&machine_id) else {
            with_updates(|updates| {
                let entry = updates.entry(machine_id.clone()).or_default();
                entry.error = Some("Connect to this machine first.".to_string());
            });
            self.publish_remote_app_update(&machine_id, cx);
            return;
        };
        with_updates(|updates| updates.entry(machine_id.clone()).or_default().busy = true);
        self.publish_remote_app_update(&machine_id, cx);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { remote_app_update_request(&target, path, &params) })
                .await;
            let follow = follow && result.is_ok();
            with_updates(|updates| {
                let entry = updates.entry(machine_id.clone()).or_default();
                entry.busy = false;
                entry.refreshed_at = Some(Instant::now());
                match result {
                    Ok(status) => {
                        entry.status = Some(status);
                        entry.error = None;
                    }
                    Err(error) => entry.error = Some(error),
                }
                if follow {
                    entry.following = true;
                }
            });
            let _ = this.update(cx, |this, cx| {
                this.publish_remote_app_update(&machine_id, cx);
                if follow {
                    this.follow_remote_app_update(machine_id, path == "/api/appUpdateInstall", cx);
                }
            });
        })
        .detach();
    }

    /// Polls the machine until the check or the install is over; through the restart the tunnel is gone or replaced, so a missing target or failed read only waits for the next poll.
    fn follow_remote_app_update(
        &mut self,
        machine_id: String,
        install: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let started = Instant::now();
        let from_version = remote_app_update(&machine_id)
            .and_then(|update| update.status)
            .and_then(|status| text(&status, "version").map(str::to_string));
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                background.timer(FOLLOW_POLL).await;
                let target = this
                    .update(cx, |this, _| {
                        this.gpui_remote_gxserver_request_target(&machine_id)
                    })
                    .ok()
                    .flatten();
                let Some(target) = target else {
                    if started.elapsed() > FOLLOW_LIMIT {
                        break;
                    }
                    continue;
                };
                let read = background
                    .spawn(async move {
                        remote_app_update_request(&target, "/api/appUpdateStatus", &json!({}))
                    })
                    .await;
                let Ok(status) = read else {
                    if started.elapsed() > FOLLOW_LIMIT {
                        break;
                    }
                    continue;
                };
                let phase = text(&status, "phase").unwrap_or_default().to_string();
                let pending = status
                    .get("pendingInstall")
                    .is_some_and(|pending| !pending.is_null());
                let done = !pending
                    && !matches!(
                        phase.as_str(),
                        "checking" | "downloading" | "installing" | "restarting"
                    );
                with_updates(|updates| {
                    let entry = updates.entry(machine_id.clone()).or_default();
                    entry.status = Some(status.clone());
                    entry.error = None;
                    entry.refreshed_at = Some(Instant::now());
                    if done {
                        entry.following = false;
                    }
                });
                let finished = (done && install).then(|| status.clone());
                let machine = machine_id.clone();
                let from = from_version.clone();
                let _ = this.update(cx, |this, cx| {
                    this.publish_remote_app_update(&machine, cx);
                    if let Some(status) = finished {
                        this.announce_remote_app_update_result(
                            &machine,
                            from.as_deref(),
                            &status,
                            cx,
                        );
                    }
                });
                if done || started.elapsed() > FOLLOW_LIMIT {
                    break;
                }
            }
            with_updates(|updates| {
                if let Some(entry) = updates.get_mut(&machine_id) {
                    entry.following = false;
                }
            });
            let _ = this.update(cx, |this, cx| {
                this.publish_remote_app_update(&machine_id, cx)
            });
        })
        .detach();
    }

    fn announce_remote_app_update_result(
        &mut self,
        machine_id: &str,
        from_version: Option<&str>,
        status: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let name = remote_machine_name(machine_id);
        let result = status.get("lastResult").filter(|result| !result.is_null());
        let version = text(status, "version");
        let installed = result.is_some_and(|result| text(result, "outcome") == Some("installed"))
            || (from_version.is_some() && version != from_version);
        if installed {
            self.dispatch_gpui_app_modal_toast(
                "success",
                "Ghostex updated",
                &format!("{name} now runs Ghostex {}.", version.unwrap_or_default()),
                cx,
            );
        } else if let Some(message) = result
            .filter(|result| text(result, "outcome") == Some("failed"))
            .and_then(|result| text(result, "message"))
            .or_else(|| {
                (text(status, "phase") == Some("failed"))
                    .then(|| text(status, "message"))
                    .flatten()
            })
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Update did not finish",
                &format!("{name}: {message}"),
                cx,
            );
        }
    }

    /// Redraws every window's machine tabs and hands the row to an open Settings window.
    pub(crate) fn publish_remote_app_update(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = remote_app_update(machine_id)
            .map(|update| remote_app_update_view(&update))
            .unwrap_or(Value::Null);
        let message =
            json!({ "type": "remoteAppUpdateState", "remoteMachineId": machine_id, "view": view });
        // The Settings window belongs to whichever workspace window opened it.
        self.dispatch_open_gpui_app_modal_message(message.clone(), cx);
        cx.notify();
        self.update_other_workspace_windows(cx, move |app, cx| {
            app.dispatch_open_gpui_app_modal_message(message.clone(), cx);
            cx.notify();
        });
    }
}

impl GhostexGpuiApp {
    /// The machine tab's update mark: its tooltip and the menu item, while an update is available or running there.
    pub(crate) fn remote_app_update_tab_badge(
        &self,
        machine_id: &str,
    ) -> Option<RemoteAppUpdateBadge> {
        let view = remote_app_update_view(&remote_app_update(machine_id)?);
        let busy = view.get("busy").and_then(Value::as_bool) == Some(true);
        let available = view.get("updateAvailable").and_then(Value::as_bool) == Some(true);
        if !busy && !available {
            return None;
        }
        let detail = text(&view, "detail")
            .unwrap_or("Ghostex update")
            .to_string();
        let menu_label = (!busy)
            .then(|| {
                view.get("action")
                    .and_then(|action| text(action, "label"))
                    .map(|label| format!("{label} on this machine"))
            })
            .flatten();
        Some(RemoteAppUpdateBadge {
            busy,
            tooltip: detail,
            menu_label,
        })
    }
}

/// What a machine tab shows for its Ghostex update.
pub(crate) struct RemoteAppUpdateBadge {
    pub(crate) busy: bool,
    pub(crate) tooltip: String,
    /// The context menu's Update item, absent while an update runs.
    pub(crate) menu_label: Option<String>,
}
