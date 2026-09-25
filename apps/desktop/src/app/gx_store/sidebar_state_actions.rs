//! The three renderer commands the sidebar page answered itself and no Rust path claimed: a row's
//! Delayed Send item, the agent launcher's run, and a machine tab's Hide Machine.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Each is performed through the function its old path ENDS in, named here because this port has
//! twice called an app function that was not that one:
//!
//! - Delayed Send: `openAppModal`, the bridge's `open` arm, `open_app_modal_from_bridge`. No close.
//! - The launcher's run with an agent: the runtime's `onSidebarHostMessage`, whose
//!   `runSidebarAgent` branch is the `writePrimaryAgentLauncherId` plus `handleSidebarMessage` pair
//!   the page ran (`dispatch_gpui_sidebar_host_message`). Without an agent: `open_app_modal_from_bridge`
//!   with Configure Agents. The mounting placeholder the old dispatcher staged for a launch
//!   (`stage_agent_launch_placeholder`) and the menu-host cache drop it made are made here too,
//!   in the same order, because answering here means the dispatcher never reaches either.
//! - Hide Machine: `saveSidebarSettingsPatch` forwards to the bridge's `sidebarCommand` arm, which
//!   calls `handle_gpui_app_modal_update_settings_patch_message`; called directly.
//!
//! **The counters that prove this path fires in the app** are the `state` object on
//! `gxStore.sidebarActions.summary`, which rides the periodic path and writes its first line at
//! zero, and the per-command `gxStore.sidebarOpen` line, which names the call and never the payload
//! (a machine's host, a project's agent choice and a session's title are the user's).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/{delayed_send,agent_run,machine_disable}.rs,
//! tooling/gx-core/sidebar-page-frozen/{session-actions,project-actions}.ts,
//! tooling/gx-core/state-action-parity.ts.

use web_time::Instant;

use ghostex_gx_core::{
    ActionEffect, SidebarActionPlan, owns_agent_run_command, owns_delayed_send_command,
    owns_machine_disable_command, plan_agent_run, plan_delayed_send_action, plan_machine_disable,
};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with the three. Rides `gxStore.sidebarActions.summary` as `state`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarStateActionCounters {
    /// Delayed Send dialogs opened.
    pub(crate) delayed_sends: u64,
    /// A Delayed Send on a row the drawn list does not hold, handed to the old runtime. A context
    /// menu opens only on a drawn row, so this should stay at zero.
    pub(crate) delayed_sends_declined_row: u64,
    /// Launches sent to the runtime's `runSidebarAgent`.
    pub(crate) agent_runs: u64,
    /// The launcher's Configure row (a run with no agent).
    pub(crate) configure_agents: u64,
    /// A run on a group the drawn list does not hold, or one with no project: the TypeScript's own
    /// early return.
    pub(crate) agent_nothing: u64,
    /// A launch with no sidebar page to receive it: the old dispatcher did nothing then either.
    pub(crate) agent_runs_without_runtime: u64,
    /// Hide Machine settings patches saved.
    pub(crate) machine_hides: u64,
    /// Commands of the three that went to the old runtime because the store's list is not drawn.
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers the three when the store owns them. Returns whether it did, in which case the
    /// command must NOT also reach the old runtime, which would open, launch or save twice.
    ///
    /// All three are RENDERER commands at the top level (controller.ts answers them itself), never
    /// `{ type: 'command', message }`.
    pub(crate) fn gx_store_run_sidebar_state_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let delayed_send = owns_delayed_send_command(command);
        let agent_run = owns_agent_run_command(command);
        if !delayed_send && !agent_run && !owns_machine_disable_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_open.state.declined_source += 1;
            return false;
        }
        let started = Instant::now();
        let plan = if delayed_send {
            plan_delayed_send_action(self.gx_store.sidebar_list.view(), command)
        } else if agent_run {
            plan_agent_run(self.gx_store.sidebar_list.view(), command)
        } else {
            // The settings as saved, which is the object the end function merges the patch over.
            let saved = crate::shared_settings::shared_sidebar_settings_snapshot();
            plan_machine_disable(command, saved.object().get("remoteMachines"))
        };
        let Some(plan) = plan else {
            if delayed_send {
                self.gx_store.sidebar_open.state.delayed_sends_declined_row += 1;
            }
            return false;
        };
        let plan_us = started.elapsed().as_micros() as u64;
        if agent_run {
            // What the old dispatcher did for this command before handing it on: the primary
            // agent id is one of the two client-storage values the menus read, and it is about to
            // be written.
            self.gx_store_note_menu_host_write(command);
            if plan.effects.is_empty() {
                self.gx_store.sidebar_open.state.agent_nothing += 1;
            }
        }
        self.run_sidebar_state_plan(command, &plan, cx);
        let kind = command
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.gx_store.diagnostics.sidebar_state_action_ran(
            &kind,
            &plan,
            plan_us,
            self.gx_store.sidebar_open.state,
        );
        true
    }

    fn run_sidebar_state_plan(
        &mut self,
        command: &Value,
        plan: &SidebarActionPlan,
        cx: &mut gpui::Context<Self>,
    ) {
        for effect in &plan.effects {
            match effect {
                ActionEffect::OpenAppModal { payload } => {
                    match payload.get("modal").and_then(Value::as_str) {
                        Some("delayedSend") => self.gx_store.sidebar_open.state.delayed_sends += 1,
                        _ => self.gx_store.sidebar_open.state.configure_agents += 1,
                    }
                    self.open_app_modal_from_bridge(payload.clone(), cx);
                }
                ActionEffect::SidebarHostMessage { message } => {
                    // The old dispatcher returned before staging anything when there was no page
                    // to send the command to, and a launch with no runtime cannot happen.
                    if self.sidebar.is_none() {
                        self.gx_store.sidebar_open.state.agent_runs_without_runtime += 1;
                        continue;
                    }
                    self.gx_store.sidebar_open.state.agent_runs += 1;
                    self.stage_agent_launch_placeholder(command, cx);
                    // Tells the runtime the newest local selection first (gx_store/burst.rs).
                    self.dispatch_gpui_sidebar_host_message(message.clone(), cx);
                }
                ActionEffect::UpdateSettingsPatch { message } => {
                    self.gx_store.sidebar_open.state.machine_hides += 1;
                    self.handle_gpui_app_modal_update_settings_patch_message(message, cx);
                }
                other => debug_assert!(false, "unexpected state-action effect: {other:?}"),
            }
        }
    }
}
