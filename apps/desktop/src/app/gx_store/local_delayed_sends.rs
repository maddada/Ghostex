//! This app's own Delayed Sends of workspace sessions, handed straight to the sidebar list.
//!
//! CDXC:DelayedSend 2026-09-25 WHY:
//! The summaries used to go to the QuickJS runtime (`onWorkspaceSessionDelayedSendsChanged`), which
//! normalized them and posted them back on the facts channel's `rows`, for the list to read what
//! this app had computed in the first place (app runtime port ledger R033). The list takes them
//! here now, with the runtime's normalization: at most 128 sessions, the remaining time dropped
//! when it is out of range, and a summary that says nothing dropped.

use std::collections::HashMap;

use ghostex_gx_core::DelayedSendInput;
use serde_json::Value;

use crate::GhostexGpuiApp;

/// `GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT`.
const SUMMARY_LIMIT: usize = 128;
/// `GPUI_COMMAND_PANE_TIMER_REMAINING_MS_MAX`.
const REMAINING_MS_MAX: i64 = 2_147_483_647;

fn delayed_send(summary: &Value) -> Option<(String, DelayedSendInput)> {
    let session_id = summary.get("sessionId")?.as_str()?.to_string();
    let text = |key: &str| {
        summary
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let flag = |key: &str| summary.get(key).and_then(Value::as_bool) == Some(true);
    let input = DelayedSendInput {
        deadline_at: text("delayedSendDeadlineAt"),
        remaining_label: text("delayedSendRemainingLabel"),
        remaining_ms: summary
            .get("delayedSendRemainingMs")
            .and_then(Value::as_i64)
            .filter(|ms| (0..=REMAINING_MS_MAX).contains(ms)),
        send_when_all_project_sessions_stop_active: flag("sendWhenAllProjectSessionsStopActive"),
        send_when_agent_stops_active: flag("sendWhenAgentStopsActive"),
    };
    let says_something = input.deadline_at.is_some()
        || input.remaining_label.is_some()
        || input.remaining_ms.is_some()
        || input.send_when_all_project_sessions_stop_active
        || input.send_when_agent_stops_active;
    says_something.then_some((session_id, input))
}

impl GhostexGpuiApp {
    /// The workspace sessions' Delayed Send summaries, as `refresh_sidebar_agents_delayed_sends_if_changed`
    /// builds them.
    pub(crate) fn gx_store_set_local_delayed_sends(
        &mut self,
        summaries: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let next: HashMap<String, DelayedSendInput> = summaries
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .take(SUMMARY_LIMIT)
            .filter_map(delayed_send)
            .collect();
        let facts = &mut self.gx_store.runtime_facts;
        if facts.delayed_sends == next {
            return;
        }
        facts.delayed_sends = next;
        facts.rows_generation += 1;
        self.gx_store_sidebar_state_changed(cx);
    }
}
