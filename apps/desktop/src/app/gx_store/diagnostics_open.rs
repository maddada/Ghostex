//! The record line for the open family, in a sibling because `diagnostics.rs` is over the size
//! ceiling and waiting for a quiet window.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! What this line may carry is decided by what the payloads hold. Two of them carry a PROJECT PATH
//! (Add Worktree) and one a Space NAME the user typed (the Space editor), so the payload itself
//! never reaches a record: the line says which command ran, which calls it made and how many, and
//! nothing that came out of the user's file system or keyboard. The counters ride every line
//! because they are the only proof this path fires at all, and a counter record that only appeared
//! once something happened would be silent in exactly the run where it matters.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_open.rs,
//! apps/desktop/src/app/gx_store/diagnostics.rs.

use ghostex_gx_core::{ActionEffect, SidebarActionPlan};
use serde_json::json;

use super::diagnostics::{
    GxStoreDiagnostics, MAX_SIDEBAR_ACTION_RECORDS, log_text, record, routine_logging_enabled,
};
use super::sidebar_open::SidebarOpenCounters;
use super::sidebar_state_actions::SidebarStateActionCounters;

impl GxStoreDiagnostics {
    /// One line per answered command: which one, what it did, and the run's totals.
    pub(super) fn sidebar_open_ran(
        &mut self,
        kind: &str,
        plan: &SidebarActionPlan,
        plan_us: u64,
        counters: SidebarOpenCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        // The modal NAME is a fixed word from a closed set this store builds itself, so it is the
        // one field of the payload that may be named. Everything else in it is the user's.
        let modal = plan
            .effects
            .iter()
            .find_map(|effect| match effect {
                ActionEffect::OpenAppModal { payload } => payload
                    .get("modal")
                    .and_then(serde_json::Value::as_str)
                    .map(log_text),
                _ => None,
            })
            .unwrap_or_else(|| log_text("none"));
        record(
            "gxStore.sidebarOpen",
            json!({
                "command": log_text(kind),
                "modal": modal,
                "effects": plan.effects.len(),
                "planUs": plan_us,
                "handled": counters.handled,
                "opens": counters.opens,
                "closes": counters.closes,
                "gxserverStarts": counters.gxserver_starts,
                "nothing": counters.nothing,
                "sortRows": counters.sort_rows,
                "declinedSource": counters.declined_source,
            }),
        );
    }

    /// One line per answered Delayed Send, launcher run or Hide Machine: the call's NAME and the
    /// modal's, never the payload, which carries a session's title, an agent choice or a
    /// machine's host. The totals ride the periodic `gxStore.sidebarActions.summary` as well.
    pub(super) fn sidebar_state_action_ran(
        &mut self,
        kind: &str,
        plan: &SidebarActionPlan,
        plan_us: u64,
        counters: SidebarStateActionCounters,
    ) {
        if self.sidebar_lifecycle_records >= MAX_SIDEBAR_ACTION_RECORDS
            || !routine_logging_enabled()
        {
            return;
        }
        self.sidebar_lifecycle_records += 1;
        let call = plan
            .effects
            .first()
            .map(|effect| match effect {
                ActionEffect::OpenAppModal { payload } => payload
                    .get("modal")
                    .and_then(serde_json::Value::as_str)
                    .map(|modal| format!("openAppModal={modal}"))
                    .unwrap_or_else(|| "openAppModal".to_string()),
                other => other.call_name().to_string(),
            })
            .unwrap_or_else(|| "none".to_string());
        record(
            "gxStore.sidebarOpen",
            json!({
                "command": log_text(kind),
                "call": log_text(call),
                "effects": plan.effects.len(),
                "planUs": plan_us,
                "state": state_counters_json(&counters),
            }),
        );
    }
}

/// The state counters as both records carry them: 7 keys, at depth 2.
pub(super) fn state_counters_json(counters: &SidebarStateActionCounters) -> serde_json::Value {
    json!({
        "delayedSends": counters.delayed_sends,
        "delayedSendsDeclinedRow": counters.delayed_sends_declined_row,
        "agentRuns": counters.agent_runs,
        "configureAgents": counters.configure_agents,
        "agentNothing": counters.agent_nothing,
        "machineHides": counters.machine_hides,
        "declinedSource": counters.declined_source,
    })
}
