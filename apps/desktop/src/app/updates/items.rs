//! The Updates items only this app can see, in gxserver's item shape (server/src/updates/items.rs):
//! each saved remote machine's Ghostex, the VS Code view's code-server while that view is on, and
//! the Chromium runtime (CEF) when Ghostex installed it. A component counts as having an update
//! when this release seals a newer version than the one on disk (an app update left the old one).
use serde_json::{Map, Value, json};

use crate::{GhostexGpuiApp, SourceCodeServerRuntimeLaunchState, TitlebarMode};
use crate::app::helpers::*;
use crate::app::remote_conn::app_update::{
    remote_app_update, remote_app_update_machine_ids, remote_app_update_view,
};

fn item(id: String, kind: &str, name: String, icon: &str, detail: &str) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".into(), json!(id));
    item.insert("kind".into(), json!(kind));
    item.insert("group".into(), json!("ghostex"));
    item.insert("name".into(), json!(name));
    item.insert("icon".into(), json!(icon));
    item.insert("detail".into(), json!(detail));
    item.insert("inUse".into(), json!(false));
    item
}

/// `counts` as gxserver sets it: a failure, or something to do on an item that lights the top bar.
fn finish(mut item: Map<String, Value>) -> Value {
    let state = item
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let top_bar = item.get("topBar") == Some(&json!(true));
    let counts =
        state == "failed" || (top_bar && matches!(state, "available" | "failed" | "needsYou"));
    item.insert("counts".into(), json!(counts));
    Value::Object(item)
}

impl GhostexGpuiApp {
    pub(super) fn desktop_update_items(&self, auto: &[String]) -> Vec<Value> {
        let mut items = self.remote_machine_update_items();
        items.extend(self.component_update_items(auto));
        items
    }

    fn remote_machine_update_items(&self) -> Vec<Value> {
        remote_app_update_machine_ids()
            .into_iter()
            .filter(|machine_id| {
                !self
                    .remote_gxserver_connections
                    .get(machine_id)
                    .is_some_and(|connection| connection.this_computer)
            })
            .filter_map(|machine_id| {
                let update = remote_app_update(&machine_id)?;
                let status = update.status.as_ref()?;
                let view = remote_app_update_view(&update);
                let text = |value: &Value, key: &str| {
                    value
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                };
                let name = gpui_remote_machine_name_from_settings(&machine_id)
                    .unwrap_or_else(|| "Remote machine".to_string());
                let mut item = item(
                    format!("machine:{machine_id}"),
                    "machine",
                    name,
                    "machine",
                    "Remote machine",
                );
                let phase = text(status, "phase").unwrap_or_default();
                let available = view["updateAvailable"] == true;
                let busy = view["busy"] == true
                    || matches!(phase.as_str(), "downloading" | "installing" | "restarting");
                let state = if busy {
                    "updating"
                } else if update.error.is_some() || phase == "failed" {
                    "failed"
                } else if available {
                    "available"
                } else if phase == "appNotRunning" {
                    "unknown"
                } else {
                    "upToDate"
                };
                item.insert(
                    "installed".into(),
                    json!(text(status, "appVersion").or_else(|| text(status, "version"))),
                );
                item.insert("latest".into(), json!(text(status, "availableVersion")));
                if busy {
                    item.insert("progress".into(), json!(text(&view, "detail")));
                } else if state == "failed" || state == "unknown" {
                    item.insert("error".into(), json!(text(&view, "detail")));
                } else if available {
                    item.insert(
                        "note".into(),
                        json!("Restarts Ghostex there; its agents keep running."),
                    );
                }
                let supported = status["supported"] == true;
                item.insert("state".into(), json!(state));
                item.insert("canUpdate".into(), json!(supported && available && !busy));
                item.insert("canAuto".into(), json!(supported));
                item.insert("auto".into(), json!(status["autoUpdate"] == true));
                item.insert("topBar".into(), json!(false));
                Some(finish(item))
            })
            .collect()
    }

    /// The VS Code view (code-server) while its view is on, and the Chromium runtime.
    fn component_update_items(&self, auto: &[String]) -> Vec<Value> {
        let status = crate::plugins_modal::plugin_settings_status_message(self);
        let store = crate::on_demand_component_store()
            .ok()
            .flatten();
        let mut items = Vec::new();
        for (plugin, component, name) in [
            ("code", "code-server", "VS Code view"),
            ("cef", "cef", "Chromium runtime (CEF)"),
        ] {
            if plugin == "code" && gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source) {
                continue;
            }
            let Some(row) = status["plugins"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["id"] == plugin)
            else {
                continue;
            };
            let Some(current) = store
                .as_ref()
                .and_then(|store| store.component(component))
                .map(|definition| definition.component_version.clone())
            else {
                continue;
            };
            let older = store
                .as_ref()
                .map(|store| store.other_installed_versions(component))
                .unwrap_or_default();
            let row_status = row["status"].as_str().unwrap_or_default();
            let installed = row_status == "installed";
            let installing = matches!(
                row_status,
                "checking" | "downloading" | "verifying" | "installing" | "finishing"
            );
            let failed = row_status == "failed";
            // Nothing on disk, old or new: nothing to update (the view installs it when it opens).
            if !installed && !installing && !failed && older.is_empty() {
                continue;
            }
            let id = format!("component:{plugin}");
            let mut item = item(
                id.clone(),
                "component",
                name.to_string(),
                plugin,
                "Part of Ghostex",
            );
            let state = if installing {
                "updating"
            } else if failed {
                "failed"
            } else if installed {
                "upToDate"
            } else {
                "available"
            };
            item.insert(
                "installed".into(),
                json!(if installed {
                    Some(current.clone())
                } else {
                    older.last().cloned()
                }),
            );
            item.insert("latest".into(), json!(current));
            if installing {
                item.insert("progress".into(), json!(row["statusLabel"].as_str()));
            }
            if failed {
                item.insert(
                    "error".into(),
                    json!(
                        row["errorMessage"]
                            .as_str()
                            .unwrap_or("The install failed.")
                    ),
                );
            }
            if state == "available" {
                item.insert(
                    "note".into(),
                    json!("This Ghostex uses a newer version; it also installs the next time it's needed."),
                );
            }
            let in_use = match plugin {
                "code" => {
                    self.source_code_server_runtime.state
                        == SourceCodeServerRuntimeLaunchState::Ready
                }
                _ => self.web_runtime_started_in_process(),
            };
            item.insert("state".into(), json!(state));
            item.insert("canUpdate".into(), json!(row["canReinstall"] == true));
            item.insert("canAuto".into(), json!(true));
            item.insert("auto".into(), json!(auto.contains(&id)));
            item.insert("topBar".into(), json!(true));
            item.insert("inUse".into(), json!(in_use));
            items.push(finish(item));
        }
        items
    }

    /// "Update automatically" for the components: installs the new version while its view isn't
    /// open (Q1365). A remote machine's switch runs on that machine (its "Keep this machine up to
    /// date"), and gxserver runs everything else.
    pub(super) fn run_desktop_auto_updates(&mut self, cx: &mut gpui::Context<Self>) {
        let auto = super::with_snapshot(|snapshot| snapshot.auto.clone());
        let due: Vec<String> = self
            .component_update_items(&auto)
            .into_iter()
            .filter(|item| {
                item["auto"] == true
                    && item["state"] == "available"
                    && item["canUpdate"] == true
                    && item["inUse"] != true
            })
            .filter_map(|item| item["id"].as_str().map(str::to_string))
            .collect();
        for id in due {
            if let Some(plugin) = id.strip_prefix("component:") {
                self.install_component_update(plugin, cx);
            }
        }
    }

    /// Installs the version this release seals, the way the view that needs it would.
    pub(super) fn install_component_update(&mut self, plugin: &str, cx: &mut gpui::Context<Self>) {
        match plugin {
            "code" => self.install_source_code_server_component(cx),
            "cef" => self.install_web_runtime(false, cx),
            _ => {}
        }
    }

    /// Whether one of this app's own items is updating (the watch then reads the list often).
    pub(super) fn desktop_updates_running(&self) -> bool {
        !self.plugin_settings_action_progress.is_empty()
            || self.source_code_server_runtime.state == SourceCodeServerRuntimeLaunchState::Installing
            || remote_app_update_machine_ids().iter().any(|machine_id| {
                remote_app_update(machine_id).is_some_and(|update| update.busy || update.following)
            })
    }
}
