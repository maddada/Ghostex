//! Settings > Updates and the top bar's update badge on the desktop: gxserver's list
//! (`/api/updates`, server/src/updates/) plus what only this app can see (remote machines, the VS
//! Code view, the Chromium runtime), kept once for every workspace window and handed to an open
//! Settings window as `updatesState`.
//!
//! CDXC:Updates 2026-11-05 DECISION:
//! The user's picks are on server/src/updates/mod.rs. Here: the top bar's ↑ button shows while
//! anything counts (Q1362) with the count beside it, and opens a small list with Update all and
//! Open Updates… (Q1361, a quiet badge); Update all hands this app's own items to the paths their
//! Settings rows use (a remote machine's Update, Reinstall of the VS Code view or the Chromium
//! runtime) and the rest to gxserver, which updates the Ghostex app last (Q1364).
//! SEE-ALSO: app/window/settings_modal/tabs/updates/ (the page), app/titlebar/updates_menu.rs.
mod items;

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;

const ENDPOINT: &str = "/api/updates";
const RPC_TIMEOUT: Duration = Duration::from_secs(90);
/// How often the list is read again while nothing runs.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// How often the watch looks; it reads the list only while something updates, or when it is old.
const WATCH_TICK: Duration = Duration::from_secs(3);

#[derive(Default)]
struct UpdatesSnapshot {
    /// gxserver's items.
    items: Vec<Value>,
    /// Ids whose "Update automatically" switch is on (gxserver keeps them, desktop items too).
    auto: Vec<String>,
    running: bool,
    error: Option<String>,
    refreshed_at: Option<Instant>,
    /// When the list was last read, in milliseconds since 1970, for "Checked 4 min ago".
    checked_at_ms: Option<i64>,
    in_flight: bool,
    /// What the top bar's badge counts, as last published (rendering reads this, not the sources).
    badge: Vec<Value>,
}

/// Every workspace window and the Settings window read the same list.
static UPDATES: LazyLock<Mutex<UpdatesSnapshot>> =
    LazyLock::new(|| Mutex::new(UpdatesSnapshot::default()));

fn with_snapshot<T>(f: impl FnOnce(&mut UpdatesSnapshot) -> T) -> T {
    let mut snapshot = UPDATES.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut snapshot)
}

fn state_rank(item: &Value) -> u8 {
    match item["state"].as_str() {
        Some("failed") => 0,
        Some("needsYou") => 1,
        Some("available" | "updating") => 2,
        Some("checking") => 3,
        Some("unknown") => 4,
        _ => 5,
    }
}

fn kind_rank(item: &Value) -> u8 {
    match item["kind"].as_str() {
        Some("app") => 0,
        Some("machine") => 1,
        Some("agent") => 2,
        Some("helper") => 3,
        Some("tool") => 4,
        Some("component") => 5,
        _ => 6,
    }
}

impl GhostexGpuiApp {
    /// Every item the Updates page lists, problems first, then updates, then the rest.
    pub(crate) fn updates_items(&self) -> Vec<Value> {
        let (mut all, auto) =
            with_snapshot(|snapshot| (snapshot.items.clone(), snapshot.auto.clone()));
        all.extend(self.desktop_update_items(&auto));
        all.sort_by(|a, b| {
            (
                state_rank(a),
                kind_rank(a),
                a["name"].as_str().unwrap_or_default(),
            )
                .cmp(&(
                    state_rank(b),
                    kind_rank(b),
                    b["name"].as_str().unwrap_or_default(),
                ))
        });
        all
    }

    /// What the top bar's badge counts (each item's `counts`, Q1362), as last published.
    pub(crate) fn updates_badge_items(&self) -> Vec<Value> {
        with_snapshot(|snapshot| snapshot.badge.clone())
    }

    /// Reads the list every half hour, and every few seconds while an update runs.
    pub(crate) fn start_updates_watch(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WATCH_TICK).await;
                let alive = this.update(cx, |this, cx| {
                    let (running, stale) = with_snapshot(|snapshot| {
                        (
                            snapshot.running,
                            snapshot
                                .refreshed_at
                                .is_none_or(|at| at.elapsed() > REFRESH_INTERVAL),
                        )
                    });
                    let desktop_running = this.desktop_updates_running();
                    if running || stale || desktop_running {
                        this.refresh_updates(false, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Reads gxserver's list (`fresh` asks every source again) and hands it to every window.
    pub(crate) fn refresh_updates(&mut self, fresh: bool, cx: &mut gpui::Context<Self>) {
        self.post_updates_request(json!({ "action": "list", "fresh": fresh }), cx);
    }

    fn post_updates_request(&mut self, params: Value, cx: &mut gpui::Context<Self>) {
        let is_list = params["action"] == "list";
        if is_list && with_snapshot(|snapshot| snapshot.in_flight) {
            return;
        }
        with_snapshot(|snapshot| snapshot.in_flight = true);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_gxserver_rpc_result(ENDPOINT, &params, RPC_TIMEOUT) })
                .await;
            with_snapshot(|snapshot| {
                snapshot.in_flight = false;
                snapshot.refreshed_at = Some(Instant::now());
                match &result {
                    Ok(answer) => {
                        snapshot.items = answer["items"].as_array().cloned().unwrap_or_default();
                        snapshot.auto = answer["auto"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect();
                        snapshot.running = answer["running"] == true;
                        snapshot.error = None;
                        snapshot.checked_at_ms = Some(chrono::Utc::now().timestamp_millis());
                    }
                    Err(message) => snapshot.error = Some(message.clone()),
                }
            });
            let _ = this.update(cx, |this, cx| {
                this.run_desktop_auto_updates(cx);
                this.publish_updates(cx);
            });
        })
        .detach();
    }

    /// Hands the list to an open Settings window and redraws every window's top bar.
    pub(crate) fn publish_updates(&mut self, cx: &mut gpui::Context<Self>) {
        let items = self.updates_items();
        let badge: Vec<Value> = items
            .iter()
            .filter(|item| item["counts"] == true)
            .cloned()
            .collect();
        let count = badge.len();
        with_snapshot(|snapshot| snapshot.badge = badge);
        let (running, error, checked_at) = with_snapshot(|snapshot| {
            (
                snapshot.running,
                snapshot.error.clone(),
                snapshot.checked_at_ms,
            )
        });
        let message = json!({
            "type": "updatesState",
            "items": items,
            "count": count,
            "running": running,
            "error": error,
            "checkedAt": checked_at,
        });
        self.dispatch_open_gpui_app_modal_message(message.clone(), cx);
        cx.notify();
        self.update_other_workspace_windows(cx, move |app, cx| {
            app.dispatch_open_gpui_app_modal_message(message.clone(), cx);
            cx.notify();
        });
    }

    /// `updates` from Settings > Updates or the top bar: `op` is `refresh`, `check`, `install`
    /// (`ids`, or `all`) or `setAuto` (`id`, `enabled`).
    pub(crate) fn handle_updates_command(
        &mut self,
        command: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match command.get("op").and_then(Value::as_str) {
            Some("refresh") => {
                self.publish_updates(cx);
                // A Settings window that asked while opening is listening a moment later.
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    let _ = this.update(cx, |this, cx| this.publish_updates(cx));
                })
                .detach();
                let stale = with_snapshot(|snapshot| {
                    snapshot
                        .refreshed_at
                        .is_none_or(|at| at.elapsed() > Duration::from_secs(60))
                });
                if stale {
                    self.refresh_updates(false, cx);
                }
                self.refresh_stale_remote_app_updates(cx);
            }
            Some("check") => {
                self.refresh_updates(true, cx);
                self.refresh_stale_remote_app_updates(cx);
            }
            Some("install") => {
                let ids: Vec<String> = if command.get("all").and_then(Value::as_bool) == Some(true)
                {
                    self.updates_items()
                        .into_iter()
                        .filter(|item| {
                            matches!(item["state"].as_str(), Some("available" | "failed"))
                                && item["canUpdate"] == true
                        })
                        .filter_map(|item| item["id"].as_str().map(str::to_string))
                        .collect()
                } else {
                    command
                        .get("ids")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                };
                self.install_updates(ids, cx);
            }
            Some("setAuto") => {
                let (Some(id), Some(enabled)) = (
                    command.get("id").and_then(Value::as_str),
                    command.get("enabled").and_then(Value::as_bool),
                ) else {
                    return;
                };
                if let Some(machine_id) = id.strip_prefix("machine:") {
                    // A machine's switch is its own "Keep this machine up to date".
                    let mut remote = serde_json::Map::new();
                    remote.insert("op".into(), json!("setAuto"));
                    remote.insert("remoteMachineId".into(), json!(machine_id));
                    remote.insert("enabled".into(), json!(enabled));
                    self.handle_remote_app_update_command(&remote, cx);
                    return;
                }
                self.post_updates_request(
                    json!({ "action": "setAuto", "id": id, "enabled": enabled }),
                    cx,
                );
            }
            _ => {}
        }
    }

    /// Runs the updates of `ids`: this app's own items here, the rest in gxserver.
    pub(crate) fn install_updates(&mut self, ids: Vec<String>, cx: &mut gpui::Context<Self>) {
        let mut gx_ids = Vec::new();
        for id in ids {
            if let Some(machine_id) = id.strip_prefix("machine:") {
                let mut remote = serde_json::Map::new();
                remote.insert("op".into(), json!("install"));
                remote.insert("remoteMachineId".into(), json!(machine_id));
                self.handle_remote_app_update_command(&remote, cx);
            } else if let Some(plugin) = id.strip_prefix("component:") {
                self.install_component_update(plugin, cx);
            } else {
                gx_ids.push(id);
            }
        }
        if !gx_ids.is_empty() {
            with_snapshot(|snapshot| snapshot.running = true);
            self.post_updates_request(json!({ "action": "install", "ids": gx_ids }), cx);
        }
        self.publish_updates(cx);
    }
}
