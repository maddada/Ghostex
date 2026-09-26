//! The remote machines' client-parked projects, read once at launch from client storage
//! (`ghostex-gpui-remote-recent-projects`), for the HUD's Recent Projects and the parked set that
//! hides those projects from the machine's tab.
//!
//! CDXC:RemoteMachines 2026-09-25 WHY:
//! The old runtime read this key when it started and posted the rows on its facts channel; nothing
//! writes the key any more (a remote project's Close parks it on its own daemon since the app
//! runtime port's F4, gx_store/create/projects.rs), so what is left is rows parked by older builds.
//! They are read here, at the same moment and in the same shape (`readStoredGpuiRemoteRecentProjects`:
//! one list per machine, newest first), so those projects stay parked exactly as before.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/runtime_facts.rs (`remote_recent_projects`),
//! packages/gx-core/src/hud/recent.rs (the reader).

use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;

/// The old runtime's client-storage key (`GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY`).
const STORED_KEY: &str = "ghostex-gpui-remote-recent-projects";

impl GhostexGpuiApp {
    /// Reads the stored rows into the HUD's source, and recomposes when there were any.
    pub(crate) fn gx_store_load_remote_recent_projects(&mut self, cx: &mut gpui::Context<Self>) {
        let rows = match crate::app::gx_store::read_preference_value(STORED_KEY) {
            Ok(Some(raw)) => stored_remote_recent_projects(&raw),
            _ => Vec::new(),
        };
        if rows == self.gx_store.runtime_facts.remote_recent_projects {
            return;
        }
        self.gx_store.runtime_facts.remote_recent_projects = rows;
        self.gx_store_sidebar_state_changed(cx);
        self.gx_store_hud_sources_changed(cx);
    }
}

/// `groupGpuiRemoteRecentProjectsByMachine(normalizeStoredGpuiRemoteRecentProjects(JSON.parse(raw)))`,
/// as `[[machineId, rows], ...]` in first-seen machine order.
fn stored_remote_recent_projects(raw: &str) -> Vec<(String, Vec<Value>)> {
    let Ok(Value::Array(stored)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let mut machines: Vec<(String, Vec<Value>)> = Vec::new();
    for candidate in &stored {
        let Some((machine_id, project)) = normalized_row(candidate) else {
            continue;
        };
        let project_id = project["projectId"].clone();
        let index = match machines.iter().position(|(id, _)| *id == machine_id) {
            Some(index) => index,
            None => {
                machines.push((machine_id, Vec::new()));
                machines.len() - 1
            }
        };
        let rows = &mut machines[index].1;
        let mut next = vec![project];
        next.extend(
            rows.drain(..)
                .filter(|existing| existing["projectId"] != project_id),
        );
        *rows = newest_first(next);
    }
    machines
}

/// One stored row, or `None` when its machine id, project id or title is not a non-blank string.
fn normalized_row(candidate: &Value) -> Option<(String, Value)> {
    let record = candidate.as_object()?;
    let non_empty = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    };
    let machine_id = non_empty("machineId")?;
    let project_id = non_empty("projectId")?;
    let title = non_empty("title")?;
    let path = record
        .get("path")
        .and_then(Value::as_str)
        .map(|path| path.trim().to_string())
        .unwrap_or_default();
    let recent_closed_at = record
        .get("recentClosedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && parse_date_ms(value).is_some());
    let session_count = match record.get("sessionCount") {
        Some(value) => js_number(value)
            .filter(|count| count.is_finite() && *count > 0.0)
            .map(f64::floor)
            .unwrap_or(0.0),
        None => 0.0,
    };
    let mut project = Map::new();
    project.insert("path".into(), json!(path));
    project.insert("projectId".into(), json!(project_id));
    if let Some(recent_closed_at) = recent_closed_at {
        project.insert("recentClosedAt".into(), json!(recent_closed_at));
    }
    project.insert("sessionCount".into(), json_count(session_count));
    project.insert("title".into(), json!(title));
    Some((machine_id, Value::Object(project)))
}

/// `Number(value)` for the JSON values a stored row can hold.
fn js_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) if text.trim().is_empty() => Some(0.0),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Value::Null => Some(0.0),
        _ => None,
    }
}

/// A whole count as JSON writes it (`3`, not `3.0`).
fn json_count(count: f64) -> Value {
    if count <= i64::MAX as f64 {
        json!(count as i64)
    } else {
        json!(count)
    }
}

/// `orderGpuiRecentProjects`: newest `recentClosedAt` first, a stable sort in which a row with no
/// date compares equal to every other row.
fn newest_first(mut rows: Vec<Value>) -> Vec<Value> {
    let closed_at = |row: &Value| {
        row.get("recentClosedAt")
            .and_then(Value::as_str)
            .and_then(parse_date_ms)
    };
    // An insertion sort, because "no date compares equal to everything" is not a total order and
    // the standard sorts may panic on one.
    for index in 1..rows.len() {
        let mut position = index;
        while position > 0 {
            let later = match (closed_at(&rows[position - 1]), closed_at(&rows[position])) {
                (Some(left), Some(right)) => right > left,
                _ => false,
            };
            if !later {
                break;
            }
            rows.swap(position - 1, position);
            position -= 1;
        }
    }
    rows
}

/// `Date.parse` for the timestamps the old runtime wrote (`toISOString`) and plain dates.
fn parse_date_ms(value: &str) -> Option<i64> {
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(date.timestamp_millis());
    }
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|date| date.and_utc().timestamp_millis())
}
