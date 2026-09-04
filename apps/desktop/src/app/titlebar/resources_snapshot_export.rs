//! JSON export of the titlebar Resources panel snapshot, served to the
//! `ghostex resources` CLI verb through the sidebar bridge.
//!
//! CDXC:Resources 2026-09-04 WHY:
//! The Resources panel is sampled inside the running GPUI app from in-memory
//! workspace state (session titles, zmx session names, browser tab ids), so no
//! separate process can reproduce its numbers without re-implementing and
//! drifting from `gpui_native_resources_snapshot_from_samples`. The CLI
//! therefore asks the app for the very snapshot the panel renders: gxserver
//! forwards a `readResourcesSnapshot` renderer command to the sidebar runtime,
//! which posts `postResourcesSnapshotRequest` here, and this module answers
//! with `onResourcesSnapshotResult`. Rows, section totals, and header totals
//! are computed by the same code and arithmetic the panel uses, plus the
//! per-process sample every row was summed from, so a RAM figure can be
//! audited pid by pid.
//! SEE-ALSO: apps/desktop/sidebar/gxserver-runtime/resources-snapshot.ts,
//! server/src/ghostex_cli/resources.rs, packages/shared/gxserver-protocol.ts.

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_resources_snapshot_request_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let request = serde_json::from_str::<Value>(payload).unwrap_or_default();
        let request_id = request
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if request_id.is_empty() {
            return;
        }
        let processes = gpui_read_native_resource_processes();
        let servers = gpui_read_native_resource_servers();
        let snapshot =
            self.gpui_native_resources_snapshot_from_samples(processes.clone(), servers, cx);
        let response = json!({
            "requestId": request_id,
            "snapshot": gpui_native_resources_snapshot_json(&snapshot, &processes),
        });
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let script = gpui_resources_snapshot_result_script(&response);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
    }
}

pub(crate) fn gpui_resources_snapshot_result_script(message: &Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onResourcesSnapshotResult==='function'){{bridge.onResourcesSnapshotResult(payload);}}else{{const pending=Array.isArray(bridge.pendingResourcesSnapshotResults)?bridge.pendingResourcesSnapshotResults:[];pending.push(payload);bridge.pendingResourcesSnapshotResults=pending;}}}})(); undefined;"
    )
}

/// The memory ledger `memoryMb` values come from on this platform, so a reader
/// of the export knows which Activity Monitor / Task Manager column to compare
/// against.
fn gpui_native_resource_memory_metric() -> &'static str {
    if cfg!(target_os = "macos") {
        "phys_footprint"
    } else if cfg!(target_os = "windows") {
        "working_set"
    } else {
        "rss"
    }
}

/// Mirrors `render_resource_sections` / `render_resource_section`: the same
/// section order, the same visibility rule (empty sections are dropped), and
/// section totals that are the plain sum of the row values.
pub(crate) fn gpui_native_resources_snapshot_json(
    snapshot: &GpuiNativeResourcesSnapshot,
    processes: &[GpuiNativeResourceProcess],
) -> Value {
    let sections = [
        (
            "devServers",
            "DEV SERVERS".to_string(),
            &snapshot.server_rows,
        ),
        (
            "project",
            snapshot.project_label.to_uppercase(),
            &snapshot.session_rows,
        ),
        ("codeIde", "CODE IDE".to_string(), &snapshot.code_rows),
        (
            "browserTabs",
            "BROWSER TABS".to_string(),
            &snapshot.browser_rows,
        ),
        (
            "orphaned",
            "ORPHANED / DETACHED".to_string(),
            &snapshot.orphan_rows,
        ),
    ]
    .into_iter()
    .filter(|(_, _, rows)| !rows.is_empty())
    .map(|(key, label, rows)| {
        json!({
            "cpu": rows.iter().map(|row| row.cpu).sum::<f64>(),
            "key": key,
            "label": label,
            "memoryMb": rows.iter().map(|row| row.memory_mb).sum::<f64>(),
            "rowCount": rows.len(),
            "rows": rows.iter().map(gpui_native_resource_row_json).collect::<Vec<_>>(),
        })
    })
    .collect::<Vec<_>>();

    let mut row_pids = HashSet::new();
    for rows in [
        &snapshot.server_rows,
        &snapshot.session_rows,
        &snapshot.code_rows,
        &snapshot.browser_rows,
        &snapshot.orphan_rows,
    ] {
        for row in rows.iter() {
            row_pids.extend(row.pids.iter().copied());
        }
    }
    let mut sampled_processes = processes
        .iter()
        .filter(|process| row_pids.contains(&process.system_pid))
        .map(|process| {
            json!({
                "cpu": process.cpu,
                "memoryMb": process.memory_mb,
                "name": gpui_native_resource_process_name(process),
                "pid": process.system_pid,
                "ppid": processes
                    .iter()
                    .find(|parent| parent.pid == process.ppid)
                    .map(|parent| parent.system_pid),
            })
        })
        .collect::<Vec<_>>();
    sampled_processes.sort_by_key(|process| process["pid"].as_u64().unwrap_or_default());

    json!({
        "header": {
            "cpu": snapshot.total_cpu,
            "inactiveTerminalSleepCount": snapshot.inactive_terminal_sleep_count,
            "memoryMb": snapshot.total_memory_mb,
            "sleepAllSessionCount": snapshot.sleep_all_session_count,
        },
        "memoryMetric": gpui_native_resource_memory_metric(),
        "processes": sampled_processes,
        "projectLabel": snapshot.project_label,
        "sampledAt": gpui_status_generated_at(),
        "sections": sections,
    })
}

fn gpui_native_resource_row_json(row: &GpuiNativeResourceRow) -> Value {
    json!({
        "action": match row.action {
            GpuiNativeResourceAction::Browser(_) => "browser",
            GpuiNativeResourceAction::Code => "code",
            GpuiNativeResourceAction::None => "none",
            GpuiNativeResourceAction::Orphan => "orphan",
            GpuiNativeResourceAction::Server => "server",
            GpuiNativeResourceAction::Session => "session",
        },
        "agentIcon": row.agent_icon,
        "children": row
            .children
            .iter()
            .map(|child| {
                json!({
                    "cpu": child.cpu,
                    "label": child.label,
                    "memoryMb": child.memory_mb,
                    "pid": child.pid,
                })
            })
            .collect::<Vec<_>>(),
        "cpu": row.cpu,
        "detail": row.detail,
        "label": row.label,
        "memoryMb": row.memory_mb,
        "pids": row.pids,
        "sessionId": row.session_id,
        "sleepCandidate": row.sleep_candidate,
        "url": row.url,
    })
}
