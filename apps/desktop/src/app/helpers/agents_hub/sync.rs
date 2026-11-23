//! Agent Sync bridge: the Hub's fifth tab asks for a report, a plan, or an
//! apply run; each answer is one JSON message built from the shared
//! `ghostex-agent-sync` crate (the same code `ghostex agent-sync` runs).

use ghostex_agent_sync::{
    PlanGroupKind, PlanOptions, PointerPolicy, SyncScope, apply, build_plan, scan,
};
use std::path::PathBuf;

fn gpui_agent_sync_home() -> Option<PathBuf> {
    ghostex_agent_sync::scan::default_home()
}

fn gpui_agent_sync_error(kind: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "errorMessage": message,
        "type": kind,
    })
}

pub(crate) fn gpui_agent_sync_report_message() -> serde_json::Value {
    let Some(home) = gpui_agent_sync_home() else {
        return gpui_agent_sync_error("agentSyncReport", "HOME is not set.");
    };
    let report = scan(&home);
    match serde_json::to_value(&report) {
        Ok(mut value) => {
            value["type"] = serde_json::Value::String("agentSyncReport".to_string());
            value
        }
        Err(error) => gpui_agent_sync_error("agentSyncReport", &error.to_string()),
    }
}

pub(crate) fn gpui_agent_sync_plan_message(
    scope: String,
    seed_main_from: Option<String>,
    pointer_policy: Option<String>,
) -> serde_json::Value {
    let Some(home) = gpui_agent_sync_home() else {
        return gpui_agent_sync_error("agentSyncPlan", "HOME is not set.");
    };
    let policy = match pointer_policy.as_deref() {
        None | Some("") => PointerPolicy::Replace,
        Some(raw) => match PointerPolicy::parse(raw) {
            Some(parsed) => parsed,
            None => {
                return gpui_agent_sync_error(
                    "agentSyncPlan",
                    &format!("Unknown pointer policy: {raw}"),
                );
            }
        },
    };
    let report = scan(&home);
    let options = PlanOptions {
        scope: SyncScope::parse(Some(&scope)),
        pointer_policy: policy,
        seed_main_from: seed_main_from.filter(|value| !value.trim().is_empty()),
        ..PlanOptions::default()
    };
    let plan = build_plan(&home, &report, &options);
    match serde_json::to_value(&plan) {
        Ok(mut value) => {
            value["type"] = serde_json::Value::String("agentSyncPlan".to_string());
            value
        }
        Err(error) => gpui_agent_sync_error("agentSyncPlan", &error.to_string()),
    }
}

pub(crate) fn gpui_agent_sync_apply_message(
    scope: String,
    groups: Vec<String>,
    seed_main_from: Option<String>,
    pointer_policy: Option<String>,
) -> serde_json::Value {
    let Some(home) = gpui_agent_sync_home() else {
        return gpui_agent_sync_error("agentSyncApplyResult", "HOME is not set.");
    };
    let enabled: Vec<PlanGroupKind> = groups
        .iter()
        .filter_map(|group| PlanGroupKind::parse(group))
        .collect();
    if enabled.is_empty() {
        return gpui_agent_sync_error("agentSyncApplyResult", "No plan groups were selected.");
    }
    let policy = match pointer_policy.as_deref() {
        None | Some("") => PointerPolicy::Replace,
        Some(raw) => match PointerPolicy::parse(raw) {
            Some(parsed) => parsed,
            None => {
                return gpui_agent_sync_error(
                    "agentSyncApplyResult",
                    &format!("Unknown pointer policy: {raw}"),
                );
            }
        },
    };
    let options = PlanOptions {
        scope: SyncScope::parse(Some(&scope)),
        pointer_policy: policy,
        seed_main_from: seed_main_from.filter(|value| !value.trim().is_empty()),
        ..PlanOptions::default()
    };
    let result = apply(&home, &options, &enabled);
    match serde_json::to_value(&result) {
        Ok(mut value) => {
            value["type"] = serde_json::Value::String("agentSyncApplyResult".to_string());
            value
        }
        Err(error) => gpui_agent_sync_error("agentSyncApplyResult", &error.to_string()),
    }
}
