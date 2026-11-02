//! The Cloud Box segment of the new chat's tray: the third choice beside This folder and New
//! worktree, the picker its chip opens (a snapshot to start a new box from, a running box to reuse,
//! the provider and size, the agentbox places under More providers), and the boot progress card
//! the first message waits with.
//!
//! gxserver owns the facts and the start (`/api/draftWorktree` answers `cloudBox` with the
//! providers, snapshots, boxes, sizes and spend, keeps the draft's picks in its
//! `runtimeSettings.cloudBox`, and starts the box with the first message). This module decides
//! what the segment and the picker show and what is picked, and builds the card; the tray itself
//! (`draft_worktree.rs`) carries all of it in the `worktreeTray` document key.
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1027): Cloud Box is a third segment of the tray, not a separate button, and when it is on
//! the snapshot chip takes the branch's place; the branch name sits right after the switch (Q1006
//! note "move the branch name to align to the end of the left aligned elements in that bar").
//! Q1007: a new box per session by default, running boxes listed to reuse. Q1008: provider, size
//! and about-$ per hour in the picker's footer. D3 ("into cloud box menu"): the agentbox places move
//! from the Run on row into this picker under More providers, and the Run on row goes wherever
//! the segment is offered. D4 ("agent list only"): Claude's own cloud is not listed here. D10
//! ("allow and explain"): a plan limit is explained, not hidden. D8: at the cap new boxes are
//! refused with Raise the cap; running boxes keep running.
//! SEE-ALSO: server/src/server/worktree_ops/draft_cloud_box.rs (facts and start),
//! apps/desktop/src/app/native_chat/box_picker.rs, apps/mobile/app/src/chat/native/composer/BoxPicker.tsx.

use serde_json::{json, Value};

use crate::state::ChatState;

pub const MODE_CLOUD_BOX: &str = "cloudBox";
pub const SEGMENT_LABEL: &str = "Cloud Box";
const SETTINGS_PAGE: &str = "cloudBoxes";
const SNAPSHOTS_PAGE: &str = "projects";
/// The chip's icon (the renderers' `titlebar/cloud.svg`).
pub const ICON: &str = "cloud";

/// What this chat picked in the picker since it opened; unset parts take the draft's saved picks,
/// then the project's defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloudPicks {
    pub provider: Option<String>,
    pub snapshot_id: Option<String>,
    pub size: Option<String>,
    /// `Some(None)`: a new box was picked over a saved reuse.
    pub reuse_box_id: Option<Option<String>>,
    pub blank: Option<bool>,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn list(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The `cloudBox` part of a tray answer, when Cloud Boxes is on.
pub fn facts(answer: &Value) -> Option<&Value> {
    answer
        .get("cloudBox")
        .filter(|facts| facts.get("enabled").and_then(Value::as_bool) == Some(true))
}

/// Whether the tray offers the Cloud Box segment: Cloud Boxes is on and the folder is a git
/// checkout (a box clones the project).
pub fn offered(answer: &Value) -> bool {
    facts(answer).is_some() && answer.get("available").and_then(Value::as_bool) == Some(true)
}

/// The project remembers Cloud Box, or the draft already carries Cloud Box picks (Set up cloud
/// box stages one).
pub fn remembered(answer: &Value) -> bool {
    facts(answer).is_some_and(|facts| {
        facts.get("remembered").and_then(Value::as_bool) == Some(true)
            || facts
                .get("draft")
                .is_some_and(|draft| draft.get("pending").and_then(Value::as_bool) == Some(true))
    })
}

/// The agentbox place picked under More providers, when the draft runs on one.
pub fn agentbox_picked(state: &ChatState) -> Option<Value> {
    let answer = state.menus.run_location.answer.as_ref()?;
    let current = text(answer, "runLocation")?;
    if current == crate::menus::run_location::LOCAL_RUN_LOCATION {
        return None;
    }
    let label = agentbox_places(state)
        .into_iter()
        .find(|place| text(place, "runLocation").as_deref() == Some(current.as_str()))
        .and_then(|place| text(&place, "label"))
        .or_else(|| {
            answer
                .get("current")
                .and_then(|current| text(current, "label"))
        })
        .unwrap_or_else(|| current.clone());
    Some(json!({ "runLocation": current, "label": label }))
}

/// The agentbox places the draft's agent can run on (`/api/draftRunLocation`).
fn agentbox_places(state: &ChatState) -> Vec<Value> {
    let Some(answer) = state.menus.run_location.answer.as_ref() else {
        return Vec::new();
    };
    if answer.get("draft").and_then(Value::as_bool) != Some(true)
        || answer.get("boxStarted").and_then(Value::as_bool) == Some(true)
        || text(answer, "boxAgent").is_none()
    {
        return Vec::new();
    }
    answer
        .pointer("/agentbox/locations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn connected_providers(facts: &Value) -> Vec<Value> {
    list(facts, "providers")
        .into_iter()
        .filter(|provider| {
            text(provider, "state").as_deref() == Some("connected")
                && provider.get("available").and_then(Value::as_bool) != Some(false)
        })
        .collect()
}

fn ready_snapshots(facts: &Value, provider: &str) -> Vec<Value> {
    list(facts, "snapshots")
        .into_iter()
        .filter(|snapshot| {
            text(snapshot, "provider").as_deref() == Some(provider)
                && text(snapshot, "status").is_none_or(|status| status == "ready")
        })
        .collect()
}

/// Boxes of this project a draft can join (Q1007): not deleted, not failed.
fn reusable_boxes(facts: &Value) -> Vec<Value> {
    list(facts, "boxes")
        .into_iter()
        .filter(|row| {
            !matches!(
                text(row, "state").as_deref(),
                Some("deleted" | "deleting" | "failed" | "pending")
            )
        })
        .collect()
}

/// What the picker has picked, everything resolved: `{provider, providerLabel, snapshot, blank,
/// size, reuse}` (each `null` when nothing fits).
pub fn resolved(state: &ChatState, facts: &Value) -> Value {
    let picks = &state.menus.draft_worktree.cloud;
    let draft = facts.get("draft").cloned().unwrap_or(Value::Null);
    let settings = facts.get("settings").cloned().unwrap_or(Value::Null);
    let connected = connected_providers(facts);
    let is_connected = |id: &str| {
        connected
            .iter()
            .any(|provider| text(provider, "id").as_deref() == Some(id))
    };
    let snapshot_of = |id: &str| {
        list(facts, "snapshots")
            .into_iter()
            .find(|snapshot| text(snapshot, "snapshotId").as_deref() == Some(id))
    };
    let wanted_snapshot = picks
        .snapshot_id
        .clone()
        .or_else(|| text(&draft, "snapshotId"));
    let provider = picks
        .provider
        .clone()
        .or_else(|| text(&draft, "provider"))
        .or_else(|| {
            wanted_snapshot
                .as_deref()
                .and_then(snapshot_of)
                .and_then(|snapshot| text(&snapshot, "provider"))
        })
        .or_else(|| text(&settings, "provider"))
        .filter(|id| is_connected(id))
        .or_else(|| connected.first().and_then(|provider| text(provider, "id")));
    let provider_label = provider.as_deref().and_then(|id| {
        connected
            .iter()
            .find(|provider| text(provider, "id").as_deref() == Some(id))
            .and_then(|provider| text(provider, "label"))
    });
    let snapshots = provider
        .as_deref()
        .map(|id| ready_snapshots(facts, id))
        .unwrap_or_default();
    let blank = picks
        .blank
        .or_else(|| draft.get("blank").and_then(Value::as_bool))
        .unwrap_or(false)
        || snapshots.is_empty();
    let snapshot = (!blank)
        .then(|| {
            let by_id = |id: &str| {
                snapshots
                    .iter()
                    .find(|snapshot| text(snapshot, "snapshotId").as_deref() == Some(id))
                    .cloned()
            };
            wanted_snapshot
                .as_deref()
                .and_then(by_id)
                .or_else(|| text(&settings, "snapshotId").as_deref().and_then(by_id))
                .or_else(|| {
                    snapshots
                        .iter()
                        .find(|snapshot| {
                            snapshot.get("isDefault").and_then(Value::as_bool) == Some(true)
                        })
                        .cloned()
                })
                .or_else(|| snapshots.first().cloned())
        })
        .flatten();
    let sizes: Vec<Value> = list(facts, "sizes")
        .into_iter()
        .filter(|size| text(size, "provider") == provider)
        .collect();
    let size_of = |id: &str| {
        sizes
            .iter()
            .find(|size| text(size, "id").as_deref() == Some(id))
            .cloned()
    };
    let size = picks
        .size
        .clone()
        .or_else(|| text(&draft, "size"))
        .and_then(|id| size_of(&id))
        .or_else(|| {
            snapshot
                .as_ref()
                .and_then(|snapshot| text(snapshot, "size"))
                .and_then(|id| size_of(&id))
        })
        .or_else(|| text(&settings, "size").and_then(|id| size_of(&id)))
        .or_else(|| {
            sizes
                .iter()
                .find(|size| size.get("isDefault").and_then(Value::as_bool) == Some(true))
                .cloned()
        })
        .or_else(|| sizes.first().cloned());
    let reuse_id = match &picks.reuse_box_id {
        Some(choice) => choice.clone(),
        None => text(&draft, "reuseBoxId"),
    };
    let reuse = reuse_id.and_then(|id| {
        reusable_boxes(facts)
            .into_iter()
            .find(|row| text(row, "boxId").as_deref() == Some(id.as_str()))
    });
    json!({
        "provider": provider,
        "providerLabel": provider_label,
        "snapshot": snapshot,
        "blank": blank,
        "size": size,
        "sizes": sizes,
        "reuse": reuse,
        "connected": !connected.is_empty(),
    })
}

fn money(usd: f64) -> String {
    if usd <= 0.0 {
        "$0".to_string()
    } else if usd < 0.1 {
        format!("${usd:.3}")
    } else {
        format!("${usd:.2}")
    }
}

fn size_detail(size: &Value) -> String {
    let label = text(size, "label").unwrap_or_default();
    match size.get("usdPerHour").and_then(Value::as_f64) {
        Some(rate) => format!("{label} · ≈ {}/h", money(rate)),
        None => label,
    }
}

fn spend_state(facts: &Value) -> (String, Option<String>) {
    let spend = facts.get("spend").cloned().unwrap_or(Value::Null);
    let state = text(&spend, "state").unwrap_or_else(|| "ok".to_string());
    let line = match (
        spend.get("totalUsd").and_then(Value::as_f64),
        spend.get("capUsd").and_then(Value::as_f64),
    ) {
        (Some(total), Some(cap)) if cap > 0.0 => Some(format!(
            "{} of your {} monthly cap",
            money(total),
            money(cap)
        )),
        _ => None,
    };
    (state, line)
}

/// Whether the cap refuses new boxes now (D8).
pub fn cap_reached(facts: &Value) -> bool {
    spend_state(facts).0 == "reached"
}

fn box_detail(row: &Value) -> String {
    let state = text(row, "state").unwrap_or_default();
    let sessions = row
        .get("sessions")
        .and_then(Value::as_array)
        .map(|sessions| {
            sessions
                .iter()
                .filter(|session| session.get("archived").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(0);
    let mut parts = Vec::new();
    parts.push(match state.as_str() {
        "running" => "Running".to_string(),
        "idle" => "Idle".to_string(),
        "paused" => "Paused · sending wakes it".to_string(),
        "stopped" => "Stopped · sending wakes it".to_string(),
        "creating" | "booting" => "Starting".to_string(),
        other => other.to_string(),
    });
    if sessions > 0 {
        parts.push(format!(
            "{sessions} session{}",
            if sessions == 1 { "" } else { "s" }
        ));
    }
    if let Some(cost) = row.get("costTotalUsd").and_then(Value::as_f64) {
        parts.push(format!("≈ {}", money(cost)));
    }
    parts.join(" · ")
}

fn heading(label: &str) -> Value {
    json!({ "label": label, "heading": true })
}

fn separator() -> Value {
    json!({ "separator": true })
}

fn settings_command(page: &str) -> Value {
    json!({ "type": "openCloudBoxSettings", "page": page })
}

/// The picker as menu rows (the chat menu's shape: `heading`, `separator`, `label`, `detail`,
/// `description`, `checked`, `disabled`, `children`, `command`), drawn as they are by both
/// renderers.
pub fn menu(state: &ChatState, facts: &Value, picked: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    let agentbox = agentbox_picked(state);
    let on_agentbox = agentbox.is_some();
    if picked.get("connected").and_then(Value::as_bool) != Some(true) {
        rows.push(heading("Cloud Box"));
        rows.push(json!({
            "label": "Connect a provider…",
            "description": "Boxes run on your own Instabox or Boat account. Connect one in Settings.",
            "iconPath": "titlebar/cloud.svg",
            "command": settings_command(SETTINGS_PAGE),
        }));
    } else {
        let provider = text(picked, "provider").unwrap_or_default();
        let reuse_id = picked.get("reuse").and_then(|row| text(row, "boxId"));
        let new_box = reuse_id.is_none() && !on_agentbox;
        let snapshot_id = picked
            .get("snapshot")
            .and_then(|row| text(row, "snapshotId"));
        let blank = picked.get("blank").and_then(Value::as_bool) == Some(true);
        let (spend, line) = spend_state(facts);
        if spend == "reached" {
            rows.push(json!({
                "label": "Monthly cap reached",
                "description": format!(
                    "{}. New boxes are paused; running boxes keep running.",
                    line.clone().unwrap_or_else(|| "This month's spend reached your cap".to_string())
                ),
                "disabled": true,
            }));
            rows.push(json!({
                "label": "Raise the cap…",
                "command": settings_command(SETTINGS_PAGE),
            }));
            rows.push(separator());
        }
        rows.push(heading("New box from a snapshot"));
        for snapshot in ready_snapshots(facts, &provider) {
            let id = text(&snapshot, "snapshotId").unwrap_or_default();
            let star = snapshot.get("isDefault").and_then(Value::as_bool) == Some(true);
            let detail = match (text(&snapshot, "baseBranch"), text(&snapshot, "commitSha")) {
                (Some(branch), Some(sha)) => {
                    format!("{branch}@{}", sha.chars().take(7).collect::<String>())
                }
                (Some(branch), None) => branch,
                _ => text(&snapshot, "createdAt")
                    .map(|at| at.chars().take(10).collect())
                    .unwrap_or_default(),
            };
            rows.push(json!({
                "label": format!("{}{}", if star { "★ " } else { "" }, text(&snapshot, "name").unwrap_or_else(|| "Snapshot".to_string())),
                "detail": detail,
                "checked": new_box && !blank && snapshot_id.as_deref() == Some(id.as_str()),
                "command": { "type": "setDraftCloudBox", "snapshotId": id, "blank": false, "reuseBoxId": null },
            }));
        }
        rows.push(json!({
            "label": "Blank box",
            "detail": "Provider template, no project setup",
            "checked": new_box && blank,
            "command": { "type": "setDraftCloudBox", "blank": true, "reuseBoxId": null },
        }));
        let boxes = reusable_boxes(facts);
        if !boxes.is_empty() {
            rows.push(separator());
            rows.push(heading("Running boxes"));
            for row in boxes {
                let id = text(&row, "boxId").unwrap_or_default();
                rows.push(json!({
                    "label": text(&row, "name").unwrap_or_else(|| id.clone()),
                    "detail": box_detail(&row),
                    "checked": reuse_id.as_deref() == Some(id.as_str()) && !on_agentbox,
                    "command": { "type": "setDraftCloudBox", "reuseBoxId": id },
                }));
            }
        }
        rows.push(separator());
        let providers: Vec<Value> = connected_providers(facts)
            .into_iter()
            .filter_map(|row| {
                let id = text(&row, "id")?;
                Some(json!({
                    "label": text(&row, "label").unwrap_or_else(|| id.clone()),
                    "detail": text(&row, "plan").unwrap_or_default(),
                    "checked": id == provider,
                    "command": { "type": "setDraftCloudBox", "provider": id, "snapshotId": null, "size": null },
                }))
            })
            .collect();
        rows.push(json!({
            "label": "Provider",
            "detail": text(picked, "providerLabel").unwrap_or_default(),
            "children": providers,
        }));
        let size_id = picked.get("size").and_then(|size| text(size, "id"));
        let sizes: Vec<Value> = list(picked, "sizes")
            .into_iter()
            .filter_map(|size| {
                let id = text(&size, "id")?;
                Some(json!({
                    "label": text(&size, "label").unwrap_or_else(|| id.clone()),
                    "detail": size
                        .get("usdPerHour")
                        .and_then(Value::as_f64)
                        .map(|rate| format!("≈ {}/h", money(rate)))
                        .unwrap_or_default(),
                    "checked": size_id.as_deref() == Some(id.as_str()),
                    "command": { "type": "setDraftCloudBox", "size": id },
                }))
            })
            .collect();
        if !sizes.is_empty() {
            rows.push(json!({
                "label": "Size",
                "detail": picked.get("size").map(size_detail).unwrap_or_default(),
                "children": sizes,
            }));
        }
        if spend != "reached" {
            if let Some(line) = line.filter(|_| spend == "warn") {
                rows.push(json!({ "label": line, "disabled": true }));
            }
        }
        rows.push(json!({
            "label": "Manage snapshots…",
            "command": settings_command(SNAPSHOTS_PAGE),
        }));
    }
    let places = agentbox_places(state);
    if !places.is_empty() {
        let current = agentbox
            .as_ref()
            .and_then(|place| text(place, "runLocation"));
        rows.push(separator());
        rows.push(heading("More providers"));
        for place in places {
            let Some(run_location) = text(&place, "runLocation") else {
                continue;
            };
            let kind = text(&place, "kind").unwrap_or_default();
            rows.push(json!({
                "label": text(&place, "label").unwrap_or_else(|| run_location.clone()),
                "detail": text(&place, "description").unwrap_or_default(),
                "icon": ghostex_gx_protocol::agentbox::agentbox_location_icon(&kind),
                "checked": current.as_deref() == Some(run_location.as_str()),
                "command": { "type": "switchDraftRunLocation", "runLocation": run_location },
            }));
        }
    }
    rows
}

/// The chip beside the segment: `{label, star, provider}`.
pub fn chip(state: &ChatState, picked: &Value) -> Value {
    if let Some(place) = agentbox_picked(state) {
        return json!({ "label": text(&place, "label"), "star": false, "agentbox": true });
    }
    if picked.get("connected").and_then(Value::as_bool) != Some(true) {
        return json!({ "label": "Connect a provider", "star": false });
    }
    if let Some(row) = picked.get("reuse").filter(|row| row.is_object()) {
        return json!({ "label": text(row, "name"), "star": false });
    }
    if picked.get("blank").and_then(Value::as_bool) == Some(true) {
        return json!({ "label": "Blank box", "star": false });
    }
    let snapshot = picked.get("snapshot").cloned().unwrap_or(Value::Null);
    json!({
        "label": text(&snapshot, "name").unwrap_or_else(|| "Snapshot".to_string()),
        "star": snapshot.get("isDefault").and_then(Value::as_bool) == Some(true),
    })
}

/// The picks a start or a remember sends gxserver.
pub fn start_picks(picked: &Value) -> Value {
    json!({
        "provider": text(picked, "provider"),
        "snapshotId": picked.get("snapshot").and_then(|row| text(row, "snapshotId")),
        "size": picked.get("size").and_then(|row| text(row, "id")),
        "reuseBoxId": picked.get("reuse").and_then(|row| text(row, "boxId")),
        "blank": picked.get("blank").and_then(Value::as_bool) == Some(true),
    })
}

/// Why a Cloud Box send can't start now, or `None`.
pub fn refusal(facts: &Value, picked: &Value) -> Option<String> {
    if picked.get("connected").and_then(Value::as_bool) != Some(true) {
        return Some(
            "Connect Instabox or Boat in Settings › Cloud Boxes to start a cloud box.".to_string(),
        );
    }
    if cap_reached(facts) && picked.get("reuse").is_none_or(Value::is_null) {
        return Some(
            "This month's Cloud Box spend reached your cap. Raise it in Settings › Cloud Boxes."
                .to_string(),
        );
    }
    None
}

/// The label the tray shows while the box starts.
pub fn start_label(picked: &Value) -> String {
    if let Some(row) = picked.get("reuse").filter(|row| row.is_object()) {
        return format!(
            "Starting the chat on {}…",
            text(row, "name").unwrap_or_default()
        );
    }
    if picked.get("blank").and_then(Value::as_bool) == Some(true) {
        return "Starting a blank cloud box…".to_string();
    }
    match picked
        .get("snapshot")
        .and_then(|snapshot| text(snapshot, "name"))
    {
        Some(name) => format!("Starting a cloud box from “{name}”…"),
        None => "Starting a cloud box…".to_string(),
    }
}

fn seconds_between(start: Option<String>, end: Option<String>) -> Option<f64> {
    let start = crate::menus::time::parse_iso_millis(&start?)?;
    let end = crate::menus::time::parse_iso_millis(&end?)?;
    (end >= start).then(|| (end - start) as f64 / 1000.0)
}

/// The boot progress card (Q1009) from the box row a running start reports: `{title, message,
/// boxName, steps: [{label, state, seconds?, detail?}]}`. Skipped steps are left out; the last
/// step, connecting, runs while the chat opens the box session.
pub fn card(title: &str, message: Option<&str>, row: &Value, opening: bool) -> Value {
    let steps: Vec<Value> = row
        .pointer("/progress/steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let id = text(step, "step").unwrap_or_default();
            let mut state = text(step, "state").unwrap_or_else(|| "waiting".to_string());
            if id == "connect" {
                state = if opening { "running" } else { "waiting" }.to_string();
            } else if state == "skipped" {
                return None;
            }
            Some(json!({
                "label": text(step, "label").unwrap_or(id),
                "state": state,
                "seconds": seconds_between(text(step, "startedAt"), text(step, "finishedAt")),
                "detail": text(step, "message"),
            }))
        })
        .collect();
    json!({
        "title": title,
        "message": message,
        "boxName": text(row, "name"),
        "steps": steps,
    })
}
