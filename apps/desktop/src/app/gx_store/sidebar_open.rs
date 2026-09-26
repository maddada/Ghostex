//! The payloads whose whole answer is an app-modal-host message, answered by the store.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Every one of these arrives as a TOP-LEVEL renderer command (`sidebarAction`, `machineAction`,
//! `editSpace`, `projectAction`), never wrapped as `{ type: 'command', message }`: the deleted
//! page's controller answered them itself rather than posting them to the runtime. Reading the wrong envelope is
//! silent and is exactly what made the whole dialog port dead in piece 3d while its gate passed,
//! so the check is spelled out here and the gate drives the same top-level shape.
//!
//! The host performs two things and decides none of them: it closes the app modal and it opens one
//! with the payload gx-core built (`sidebar_actions/open.rs`). Quick Access is not a third thing:
//! `openQuickAccess` is a translation table over `openAppModal`, so the translating happens in the
//! store and the host opens one kind of thing.
//!
//! **The counters that prove this path fires in the app** are `opens`, `closes`,
//! `gxserverStarts`, `nothing` and `sortRows` on `gxStore.sidebarOpen`. A run where the user used the More
//! menu, a machine's Configure, the Space editor or a project's Add Worktree and `opens` is zero
//! means the command never reached here.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/open.rs, the deleted sidebar page's
//! `navigation.ts`, `project-actions.ts` and `space-navigation.ts`.

use web_time::Instant;

use ghostex_gx_core::{SORT_ACTIONS, owns_open_command, plan_open_action};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with the open family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarOpenCounters {
    /// Commands the store answered.
    pub(crate) handled: u64,
    /// App modals opened, which is one per answered command except `loadSessions`.
    pub(crate) opens: u64,
    /// Modal closes, which only the More menu's navigation rows run.
    pub(crate) closes: u64,
    pub(crate) gxserver_starts: u64,
    /// A command whose answer is deliberately nothing: a project row the drawn list does not hold.
    pub(crate) nothing: u64,
    /// The More menu's two sort rows, whose answer is the empty plan because the old runtime's
    /// path ends in `handleUnsupportedSidebarMessage` (gx-core `sidebar_actions/sort.rs`). Counted
    /// apart from `nothing` so a click on either row is visible in the run.
    pub(crate) sort_rows: u64,
    /// Sort-row clicks dropped because the list was not ready yet. Also inside `declined_source`.
    pub(crate) sort_rows_declined: u64,
    /// Commands the store owns but did not answer because the list was not ready yet (the launch
    /// window: the sidebar's own state or the HUD had not landed).
    pub(crate) declined_source: u64,
    /// Delayed Send, the agent launcher's run and Hide Machine, which the host asks first
    /// (gx_store/sidebar_state_actions.rs).
    pub(crate) state: super::sidebar_state_actions::SidebarStateActionCounters,
}

impl GhostexGpuiApp {
    /// Answers the open family in Rust when the store owns it. Returns whether it did, in which
    /// case the command must NOT also reach the old runtime, which would open the modal twice.
    pub(crate) fn gx_store_run_sidebar_open(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // A row's Delayed Send, the launcher's run and Hide Machine are answered first: the last
        // two share this family's command types and are not opens.
        if self.gx_store_run_sidebar_state_action(command, cx) {
            return true;
        }
        // The TOP level, not `command.message`: see the note at the top of this file.
        if !owns_open_command(command) {
            return false;
        }
        let sort_row = command.get("type").and_then(Value::as_str) == Some("sidebarAction")
            && command
                .get("action")
                .and_then(Value::as_str)
                .is_some_and(|action| SORT_ACTIONS.contains(&action));
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_open.declined_source += 1;
            if sort_row {
                self.gx_store.sidebar_open.sort_rows_declined += 1;
            }
            return false;
        }
        let started = Instant::now();
        let Some(plan) = plan_open_action(self.gx_store.sidebar_list.view(), command) else {
            return false;
        };
        let plan_us = started.elapsed().as_micros() as u64;
        self.gx_store.sidebar_open.handled += 1;
        if sort_row {
            self.gx_store.sidebar_open.sort_rows += 1;
        } else if plan.effects.is_empty() {
            self.gx_store.sidebar_open.nothing += 1;
        }
        for effect in &plan.effects {
            match effect {
                ghostex_gx_core::ActionEffect::CloseAppModal => {
                    self.gx_store.sidebar_open.closes += 1;
                    self.close_app_modal_from_bridge(cx);
                }
                ghostex_gx_core::ActionEffect::OpenAppModal { payload } => {
                    self.gx_store.sidebar_open.opens += 1;
                    self.open_app_modal_from_bridge(payload.clone(), cx);
                }
                ghostex_gx_core::ActionEffect::StartLocalGxserver => {
                    self.gx_store.sidebar_open.gxserver_starts += 1;
                    self.start_local_gxserver_from_sidebar(cx);
                }
                // The three effects of the read-only family cannot be planned here, and answering
                // them silently would hide a planner that had drifted.
                other => debug_assert!(false, "unexpected open effect: {other:?}"),
            }
        }
        let kind = command
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.gx_store.diagnostics.sidebar_open_ran(
            &kind,
            &plan,
            plan_us,
            self.gx_store.sidebar_open,
        );
        true
    }

    /// Starts the local gxserver for the Load Sessions row of the empty state.
    pub(crate) fn start_local_gxserver_from_sidebar(&mut self, cx: &mut gpui::Context<Self>) {
        self.show_gpui_gxserver_bootstrap_toast(
            "info",
            "Loading sessions",
            "Starting gxserver and loading projects.",
            true,
            cx,
        );
        self.start_gpui_local_gxserver_bootstrap(false, cx);
    }
}
