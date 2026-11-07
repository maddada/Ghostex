//! Every connected computer's own Updates: its agent CLIs, tools, account switchers and extensions,
//! read from that computer's gxserver (`/api/updates`, server/src/updates/) through the machine's
//! tunnel, the way Settings > Remote reads and installs its Ghostex (remote_conn/app_update.rs).
//! Its items carry ids `remote:<machineId>:<id>`, so Update, Retry, Dismiss and the switch go back
//! to the computer that listed them and run there.
//!
//! CDXC:Updates 2026-11-07 DECISION:
//! User (to a Discord tester): "I need to make it show all connected machines :) Forgot!". Updates
//! groups the items by computer: this computer first, then each remote computer the sidebar has a
//! tab for (headless Linux boxes too), with its Ghostex row at the top of its group. A computer that
//! is offline, or whose Ghostex predates `/api/updates`, shows one plain line instead of an error or
//! an empty group, and the top bar's count includes every computer's items, naming the computer.
//! WHY: the tunnel lands on the remote gxserver's local listener, where `/api/updates` is already
//! open to the machine's token (server/src/protocol.rs), so no new transport or route is needed.
//! The remote's own `app` item is left out: its Ghostex is the `machine:` row (items.rs), which
//! follows the install through that computer's restart.
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// The computer id of this computer's own items.
pub(crate) const THIS_COMPUTER: &str = "local";
const ITEM_PREFIX: &str = "remote:";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// A computer that could not be reached while its tunnel was up is asked again this soon.
const OFFLINE_RETRY: Duration = Duration::from_secs(60);

/// What the last read of a computer's list said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Not read yet.
    #[default]
    Unread,
    Ready,
    /// Not connected, or its tunnel didn't answer.
    Offline,
    /// Its Ghostex has no `/api/updates` (404).
    TooOld,
    /// Any other refusal, in its own words.
    Failed(String),
}

#[derive(Default)]
struct RemoteUpdates {
    /// The computer's items, ids already `remote:<machineId>:<id>`.
    items: Vec<Value>,
    running: bool,
    reach: Reach,
    refreshed_at: Option<Instant>,
    in_flight: bool,
}

static REMOTE: LazyLock<Mutex<HashMap<String, RemoteUpdates>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_remote<T>(f: impl FnOnce(&mut HashMap<String, RemoteUpdates>) -> T) -> T {
    let mut remote = REMOTE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut remote)
}

pub(crate) fn remote_item_id(machine_id: &str, id: &str) -> String {
    format!("{ITEM_PREFIX}{machine_id}:{id}")
}

/// `(machineId, the id that computer knows)` for a remote item's id.
pub(crate) fn split_remote_item_id(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix(ITEM_PREFIX)?;
    let (machine_id, inner) = rest.split_once(':')?;
    (!machine_id.is_empty() && !inner.is_empty()).then_some((machine_id, inner))
}

/// The computer's answer as this app lists it: ids prefixed, the computer named on every item,
/// and its own `app` item left to the `machine:` row.
pub(crate) fn remote_items(machine_id: &str, machine_name: &str, answer: &Value) -> Vec<Value> {
    answer["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["id"].as_str().is_some_and(|id| id != "app"))
        .map(|item| {
            let mut item = item.clone();
            let id = item["id"].as_str().unwrap_or_default().to_string();
            item["id"] = json!(remote_item_id(machine_id, &id));
            item["computer"] = json!(machine_id);
            item["computerName"] = json!(machine_name);
            item
        })
        .collect()
}

/// How a failed request reads: no answer at all is Offline, a 404 is a Ghostex from before
/// `/api/updates`, anything else is said as the computer said it.
pub(crate) fn reach_after_failure(status: Option<u16>, message: &str) -> Reach {
    match status {
        None => Reach::Offline,
        Some(404) => Reach::TooOld,
        Some(_) => Reach::Failed(message.to_string()),
    }
}

/// The one plain line a computer's group shows instead of its items, if any.
pub(crate) fn computer_line(reach: &Reach, name: &str) -> Option<String> {
    match reach {
        Reach::Offline => Some("Offline".to_string()),
        Reach::TooOld => Some(format!("Update Ghostex on {name} to see its updates")),
        Reach::Failed(message) => Some(format!("Couldn't read its updates: {message}")),
        Reach::Unread => Some("Checking for updates…".to_string()),
        Reach::Ready => None,
    }
}

fn reach_state(reach: &Reach) -> &'static str {
    match reach {
        Reach::Unread => "checking",
        Reach::Ready => "ready",
        Reach::Offline => "offline",
        Reach::TooOld => "tooOld",
        Reach::Failed(_) => "failed",
    }
}

/// The computer's answer, or `(HTTP status, why)`; no status when it didn't answer.
fn request(
    target: &GpuiRemoteGxserverRequestTarget,
    params: &Value,
) -> Result<Value, (Option<u16>, String)> {
    let (status, body) =
        gpui_remote_gxserver_post_typed_operation(target, super::ENDPOINT, params, REQUEST_TIMEOUT)
            .map_err(|error| (None, error))?;
    let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        return parsed
            .get("result")
            .filter(|result| result["items"].is_array())
            .cloned()
            .ok_or((Some(status), "It sent an unreadable list.".to_string()));
    }
    let message = parsed
        .get("message")
        .or_else(|| parsed.pointer("/error/message"))
        .and_then(Value::as_str)
        .unwrap_or("It refused the request.")
        .to_string();
    Err((Some(status), message))
}

impl GhostexGpuiApp {
    /// The remote computers Updates groups, in the sidebar's order: every machine with a tab,
    /// then any other connected machine; never one that is this computer.
    pub(crate) fn update_computers(&self) -> Vec<(String, String)> {
        let this_computer = |machine_id: &str| {
            self.remote_gxserver_connections
                .get(machine_id)
                .is_some_and(|connection| connection.this_computer)
        };
        let mut computers: Vec<(String, String)> = crate::app::gx_store::enabled_remote_machines()
            .0
            .into_iter()
            .filter(|(machine_id, _)| !this_computer(machine_id))
            .collect();
        let mut others: Vec<&String> = self
            .remote_gxserver_connections
            .iter()
            .filter(|(machine_id, connection)| {
                !connection.this_computer && !computers.iter().any(|(id, _)| id == *machine_id)
            })
            .map(|(machine_id, _)| machine_id)
            .collect();
        others.sort();
        for machine_id in others {
            let name = gpui_remote_machine_name_from_settings(machine_id)
                .unwrap_or_else(|| "Remote machine".to_string());
            computers.push((machine_id.clone(), name));
        }
        computers
    }

    pub(super) fn update_computer_connected(&self, machine_id: &str) -> bool {
        self.remote_machine_connect_states
            .get(machine_id)
            .is_some_and(|state| state == "connected")
            && self
                .gpui_remote_gxserver_request_target(machine_id)
                .is_some()
    }

    /// What the page and the top bar group by: this computer, then each remote computer with
    /// `state` (`ready`, `checking`, `offline`, `tooOld`, `failed`) and the line it shows.
    pub(crate) fn update_computers_payload(&self) -> Vec<Value> {
        let mut computers = vec![json!({
            "id": THIS_COMPUTER,
            "name": crate::host_name::local_computer_name(),
            "thisComputer": true,
            "state": "ready",
            "line": null,
        })];
        for (machine_id, name) in self.update_computers() {
            let reach = if self.update_computer_connected(&machine_id) {
                with_remote(|remote| {
                    remote
                        .get(&machine_id)
                        .map(|entry| entry.reach.clone())
                        .unwrap_or_default()
                })
            } else {
                Reach::Offline
            };
            computers.push(json!({
                "id": machine_id,
                "name": name,
                "thisComputer": false,
                "state": reach_state(&reach),
                "line": computer_line(&reach, &name),
            }));
        }
        computers
    }

    /// Every remote computer's items while it is connected, in the computers' order.
    pub(super) fn remote_update_items(&self) -> Vec<Value> {
        let mut items = Vec::new();
        for (machine_id, _) in self.update_computers() {
            if !self.update_computer_connected(&machine_id) {
                continue;
            }
            with_remote(|remote| {
                if let Some(entry) = remote.get(&machine_id) {
                    items.extend(entry.items.iter().cloned());
                }
            });
        }
        items
    }

    pub(super) fn remote_updates_running(&self) -> bool {
        with_remote(|remote| remote.values().any(|entry| entry.running))
    }

    /// The watch's pass: a computer that went offline drops its items (and is read as soon as it
    /// is back); a connected one is read when it was never read, its list is old, or it updates.
    /// `true` when something the page shows changed without a read.
    pub(super) fn sync_remote_updates(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let mut changed = false;
        for (machine_id, _) in self.update_computers() {
            if !self.update_computer_connected(&machine_id) {
                changed |= with_remote(|remote| {
                    let entry = remote.entry(machine_id.clone()).or_default();
                    let was = entry.reach != Reach::Offline || !entry.items.is_empty();
                    entry.reach = Reach::Offline;
                    entry.items.clear();
                    entry.running = false;
                    entry.refreshed_at = None;
                    was
                });
                continue;
            }
            let due = with_remote(|remote| {
                let entry = remote.entry(machine_id.clone()).or_default();
                let max_age = if entry.reach == Reach::Offline {
                    OFFLINE_RETRY
                } else {
                    super::REFRESH_INTERVAL
                };
                !entry.in_flight
                    && (entry.running || entry.refreshed_at.is_none_or(|at| at.elapsed() > max_age))
            });
            if due {
                self.post_remote_updates(&machine_id, json!({ "action": "list" }), cx);
            }
        }
        changed
    }

    /// Reads every connected computer's list again: `fresh` asks every source there again; else
    /// only a list older than `older_than` is read.
    pub(super) fn refresh_remote_updates(
        &mut self,
        fresh: bool,
        older_than: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        for (machine_id, _) in self.update_computers() {
            if !self.update_computer_connected(&machine_id) {
                continue;
            }
            let due = fresh
                || with_remote(|remote| {
                    remote.get(&machine_id).is_none_or(|entry| {
                        entry
                            .refreshed_at
                            .is_none_or(|at| at.elapsed() > older_than)
                    })
                });
            if due {
                self.post_remote_updates(
                    &machine_id,
                    json!({ "action": "list", "fresh": fresh }),
                    cx,
                );
            }
        }
    }

    /// Runs the updates of `ids` (as that computer knows them) on `machine_id`; their rows show
    /// "Waiting…" at once, and the watch follows them until that computer says they ended.
    pub(super) fn install_remote_updates(
        &mut self,
        machine_id: &str,
        ids: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let wanted: Vec<String> = ids
            .iter()
            .map(|id| remote_item_id(machine_id, id))
            .collect();
        with_remote(|remote| {
            let entry = remote.entry(machine_id.to_string()).or_default();
            entry.running = true;
            for item in &mut entry.items {
                if item["id"]
                    .as_str()
                    .is_some_and(|id| wanted.iter().any(|wanted| wanted == id))
                {
                    item["state"] = json!("updating");
                    item["progress"] = json!("Waiting…");
                    item["error"] = Value::Null;
                }
            }
        });
        self.post_remote_updates(machine_id, json!({ "action": "install", "ids": ids }), cx);
    }

    /// Sends `params` to the computer's `/api/updates` and keeps its answer (every action answers
    /// with the whole list). A list read already on its way is not asked again.
    pub(super) fn post_remote_updates(
        &mut self,
        machine_id: &str,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let is_list = params["action"] == "list";
        let install_ids: Option<Vec<String>> = (params["action"] == "install").then(|| {
            params["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| remote_item_id(machine_id, id))
                .collect()
        });
        let Some(target) = self.gpui_remote_gxserver_request_target(machine_id) else {
            return;
        };
        let busy = with_remote(|remote| {
            let entry = remote.entry(machine_id.to_string()).or_default();
            if is_list && entry.in_flight {
                return true;
            }
            entry.in_flight = true;
            false
        });
        if busy {
            return;
        }
        let name = gpui_remote_machine_name_from_settings(machine_id)
            .unwrap_or_else(|| "Remote machine".to_string());
        let machine_id = machine_id.to_string();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { request(&target, &params) })
                .await;
            with_remote(|remote| {
                let entry = remote.entry(machine_id.clone()).or_default();
                entry.in_flight = false;
                entry.refreshed_at = Some(Instant::now());
                match result {
                    Ok(answer) => {
                        entry.items = remote_items(&machine_id, &name, &answer);
                        entry.running = answer["running"] == true;
                        entry.reach = Reach::Ready;
                    }
                    Err((status, message)) => {
                        let reach = reach_after_failure(status, &message);
                        match install_ids.as_ref() {
                            // An Update that didn't reach the computer fails its rows, so the
                            // user can retry; the list it had stays.
                            Some(ids) => {
                                let why = computer_line(&reach, &name).unwrap_or(message);
                                for item in &mut entry.items {
                                    let waiting = item["state"] == "updating"
                                        && item["id"]
                                            .as_str()
                                            .is_some_and(|id| ids.iter().any(|w| w == id));
                                    if waiting {
                                        item["state"] = json!("failed");
                                        item["error"] = json!(why);
                                        item["counts"] = json!(true);
                                    }
                                }
                            }
                            None => {
                                entry.items.clear();
                                entry.reach = reach;
                            }
                        }
                        entry.running = false;
                    }
                }
            });
            let _ = this.update(cx, |this, cx| this.publish_updates(cx));
        })
        .detach();
    }
}

/// Groups `items` by computer in `computers`' order (this computer first): a computer's Ghostex
/// row at the top of its group, then problems, updates and the rest, as on a single computer.
pub(crate) fn order_by_computer(items: &mut [Value], computers: &[String]) {
    let computer_rank = |item: &Value| {
        let computer = item["computer"].as_str().unwrap_or(THIS_COMPUTER);
        if computer == THIS_COMPUTER {
            return 0;
        }
        computers
            .iter()
            .position(|id| id == computer)
            .map_or(usize::MAX, |index| index + 1)
    };
    items.sort_by(|a, b| {
        (
            computer_rank(a),
            a["kind"] != "machine",
            super::state_rank(a),
            super::kind_rank(a),
            a["name"].as_str().unwrap_or_default(),
        )
            .cmp(&(
                computer_rank(b),
                b["kind"] != "machine",
                super::state_rank(b),
                super::kind_rank(b),
                b["name"].as_str().unwrap_or_default(),
            ))
    });
}

/// The ids of `ids` that run elsewhere, by computer (as that computer knows them).
pub(crate) fn remote_ids_by_computer(ids: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut by_computer: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in ids {
        if let Some((machine_id, inner)) = split_remote_item_id(id) {
            by_computer
                .entry(machine_id.to_string())
                .or_default()
                .push(inner.to_string());
        }
    }
    by_computer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_items_id_names_its_computer_and_the_id_it_knows() {
        let id = remote_item_id("remote-linux-box", "agent:claude");
        assert_eq!(id, "remote:remote-linux-box:agent:claude");
        assert_eq!(
            split_remote_item_id(&id),
            Some(("remote-linux-box", "agent:claude"))
        );
        assert_eq!(split_remote_item_id("agent:claude"), None);
        assert_eq!(split_remote_item_id("machine:remote-linux-box"), None);
        assert_eq!(split_remote_item_id("remote:remote-x:"), None);
    }

    #[test]
    fn a_computers_list_leaves_its_own_ghostex_to_the_machine_row() {
        let answer = json!({ "items": [
            { "id": "app", "kind": "app", "name": "Ghostex" },
            { "id": "agent:codex", "kind": "agent", "name": "Codex", "counts": true },
            { "id": "tool:hex", "kind": "tool", "name": "Hex" },
        ], "running": false, "auto": [] });
        let items = remote_items("remote-box", "Linux box", &answer);
        let ids: Vec<&str> = items.iter().filter_map(|i| i["id"].as_str()).collect();
        assert_eq!(
            ids,
            [
                "remote:remote-box:agent:codex",
                "remote:remote-box:tool:hex"
            ]
        );
        assert!(
            items
                .iter()
                .all(|i| i["computer"] == "remote-box" && i["computerName"] == "Linux box")
        );
        assert_eq!(items[0]["counts"], true);
    }

    #[test]
    fn an_unreachable_or_old_computer_reads_as_one_plain_line() {
        assert_eq!(
            reach_after_failure(None, "Remote gxserver tunnel is not reachable."),
            Reach::Offline
        );
        assert_eq!(
            reach_after_failure(Some(404), "No gxserver endpoint"),
            Reach::TooOld
        );
        assert_eq!(
            reach_after_failure(Some(500), "boom"),
            Reach::Failed("boom".into())
        );
        assert_eq!(
            computer_line(&Reach::Offline, "Studio").as_deref(),
            Some("Offline")
        );
        assert_eq!(
            computer_line(&Reach::TooOld, "Studio").as_deref(),
            Some("Update Ghostex on Studio to see its updates")
        );
        assert_eq!(computer_line(&Reach::Ready, "Studio"), None);
    }

    #[test]
    fn items_group_by_computer_with_its_ghostex_row_first() {
        let mut items = vec![
            json!({ "id": "remote:remote-b:agent:codex", "kind": "agent", "name": "Codex", "state": "failed", "computer": "remote-b" }),
            json!({ "id": "machine:remote-b", "kind": "machine", "name": "B", "state": "upToDate", "computer": "remote-b" }),
            json!({ "id": "remote:remote-a:agent:claude", "kind": "agent", "name": "Claude Code", "state": "available", "computer": "remote-a" }),
            json!({ "id": "agent:claude", "kind": "agent", "name": "Claude Code", "state": "upToDate" }),
            json!({ "id": "app", "kind": "app", "name": "Ghostex", "state": "available", "computer": "local" }),
            json!({ "id": "machine:remote-a", "kind": "machine", "name": "A", "state": "available", "computer": "remote-a" }),
        ];
        order_by_computer(
            &mut items,
            &["remote-a".to_string(), "remote-b".to_string()],
        );
        let ids: Vec<&str> = items.iter().filter_map(|i| i["id"].as_str()).collect();
        assert_eq!(
            ids,
            [
                "app",
                "agent:claude",
                "machine:remote-a",
                "remote:remote-a:agent:claude",
                "machine:remote-b",
                "remote:remote-b:agent:codex",
            ]
        );
    }

    #[test]
    fn update_all_sends_each_computer_its_own_ids() {
        let ids = vec![
            "app".to_string(),
            "remote:remote-a:agent:claude".to_string(),
            "remote:remote-b:tool:hex".to_string(),
            "remote:remote-a:helper:claude".to_string(),
            "machine:remote-a".to_string(),
        ];
        let by = remote_ids_by_computer(&ids);
        assert_eq!(by.len(), 2);
        assert_eq!(by["remote-a"], ["agent:claude", "helper:claude"]);
        assert_eq!(by["remote-b"], ["tool:hex"]);
    }
}
