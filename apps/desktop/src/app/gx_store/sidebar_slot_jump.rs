//! The project slot hotkeys (Jump to Project 1 to 9, cmd+ctrl+1 to cmd+ctrl+9 by default), performed by the store when its list is drawn.
//!
//! CDXC:Hotkeys 2026-09-21 WHY:
//! The jump used to be the old sidebar page's: `gpuiProjectSlotHotkey` went to QuickJS, which
//! resolved the slot against its own projection, posted the row's `focusSession` and asked for a
//! reveal that came back one publish later. With the store's list drawn, the whole jump is planned
//! by gx-core (`project_slot_plan`, then `reveal_plan` against the list the plan's own changes
//! rebuilt) and performed here in the key's frame, and the message is no longer sent at all: the
//! store's list is the only list since M4d part 2 step 6.
//!
//! **Where each effect ends, read rather than assumed.** The FOCUS ends where a row click's ends,
//! because it is one: the row click's `selectSession` (which the old page turned into the runtime's
//! `focusSession`, the call `runNativeProjectSlotHotkey` itself made through the same
//! `selectNativeSidebarSession`) and the click's in-process reaction
//! (`react_to_native_sidebar_session_click`: the project swap, the tab selection or staged tab, and
//! `gx_store_select_local_session`). The REVEAL ends in `gx_store_apply_sidebar_reveal`, the one
//! function a published reveal request also ends in, and the scroll is the walk's
//! (`gx_store_reveal_walk_row`). A slot never names a remote project (gx-core
//! `sidebar_view/slot_hotkey.rs`), so the remote focus path is never reached from here.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! While the TypeScript page still resolved cmd+1..9 against its own copy of the sidebar state, the
//! first port told it a `revealSidebarSession` after each jump, under a request id it marked handled
//! on both Rust readers. That missed the jump's own two deletions (an empty collapsed project, the
//! show-less list flag), and it overwrote the handled id: the page never cleared its reveal request,
//! so the PREVIOUS request stayed in every projection, no longer matched the handled id, and ran
//! again (scroll, Space and machine tab, a group the user had collapsed since). The page is deleted
//! and so is the mirror that replaced that reveal, but the rule it left behind still holds: the slot
//! path writes no handled-reveal id and asks nobody else to reveal.
//!
//! **Counters** ride `gxStore.sidebarActions.summary` as `slotJump`, whose first line is written at
//! zero: `presses`, `nothing`, `emptyProjects`, `focuses`, `inProcess`, `staged`, `handedToRuntime`,
//! `reveals`, `revealChanges`, `declinedSource`, `planMaxUs`. A run in which the user pressed
//! cmd+ctrl+1 and `presses` is zero means the key never reached this file; `declinedSource` moving
//! means the list was not ready yet.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/slot_hotkey.rs,
//! apps/desktop/src/app/gx_store/sidebar_ui_paths.rs.

use std::time::Instant;

use serde_json::{Value, json};

use super::diagnostics::{MAX_SIDEBAR_ACTION_RECORDS, record, routine_logging_enabled};
use crate::GhostexGpuiApp;
use crate::app::sidebar_direct_focus::NativeSidebarClickReaction;

/// What this app run did with the slot hotkeys. Rides `gxStore.sidebarActions.summary`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlotJumpCounters {
    pub(crate) presses: u64,
    /// A slot outside 1 to 9, past the last project, or on a group that is not a project.
    pub(crate) nothing: u64,
    /// A project with no row: its collapse state moved and nothing was selected.
    pub(crate) empty_projects: u64,
    pub(crate) focuses: u64,
    pub(crate) in_process: u64,
    pub(crate) staged: u64,
    /// The click reaction did not apply (a row the drawn snapshot does not hold): the store's
    /// `focusSession` (focus_perform.rs; the runtime's until 2026-09-25), reached through the same
    /// `selectSession`, owns the whole selection.
    pub(crate) handed_to_runtime: u64,
    pub(crate) reveals: u64,
    /// Changes the reveals made to the sidebar's own state.
    pub(crate) reveal_changes: u64,
    pub(crate) declined_source: u64,
    pub(crate) plan_max_us: u64,
}

#[derive(Default)]
pub(crate) struct SlotJumpHost {
    pub(crate) counters: SlotJumpCounters,
    records: u32,
}

impl GhostexGpuiApp {
    /// A project slot hotkey. The whole answer: nothing else is told, because nothing else draws
    /// the sidebar.
    pub(crate) fn gx_store_run_project_slot_hotkey(
        &mut self,
        slot_number: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        // Nothing at all happens while the loading skeleton is drawn, the way
        // `dispatch_native_sidebar_ui` drops a command that arrives then: the slot names the Nth
        // row of a list that has not been built, so there is no row to focus, and the state half
        // must not run either. It used to run, on the reasoning that this app is the only writer
        // of the collapse key; what that really did was queue a collapse intent
        // (`queued_intents`) that replayed on top of the state the read brought back, so a project
        // the user left open silently expanded or closed in the launch window with nothing
        // focused.
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.slot_jump.counters.declined_source += 1;
            return;
        }
        let started = Instant::now();
        let plan = self.gx_store_note_project_slot_hotkey(slot_number, cx);
        let plan_us = started.elapsed().as_micros() as u64;
        let (route, reveal_us) = self.gx_store_perform_project_slot_jump(plan, cx);
        let counters = &mut self.gx_store.slot_jump.counters;
        counters.presses += 1;
        counters.plan_max_us = counters.plan_max_us.max(plan_us);
        cx.notify();
        self.gx_store_slot_jump_ran(route, plan_us, reveal_us);
    }

    /// The focus and the reveal of a planned jump, whose state intents are already applied.
    /// Returns the route and the reveal's time.
    fn gx_store_perform_project_slot_jump(
        &mut self,
        plan: Option<ghostex_gx_core::ProjectSlotPlan>,
        cx: &mut gpui::Context<Self>,
    ) -> (&'static str, u64) {
        let Some(plan) = plan else {
            self.gx_store.slot_jump.counters.nothing += 1;
            return ("nothing", 0);
        };
        let Some(target) = plan.target_session_id.clone() else {
            self.gx_store.slot_jump.counters.empty_projects += 1;
            return ("emptyProject", 0);
        };
        let (reaction, reveal) = self.gx_store_focus_and_reveal_slot_row(&target, plan.reveal, cx);
        let counters = &mut self.gx_store.slot_jump.counters;
        counters.focuses += 1;
        let route = match reaction {
            NativeSidebarClickReaction::InProcess => {
                counters.in_process += 1;
                "inProcess"
            }
            NativeSidebarClickReaction::Staged => {
                counters.staged += 1;
                "staged"
            }
            NativeSidebarClickReaction::NotApplied => {
                counters.handed_to_runtime += 1;
                "runtime"
            }
        };
        let mut reveal_us = 0;
        if let Some((changes, us)) = reveal {
            reveal_us = us;
            counters.reveals += 1;
            counters.reveal_changes += changes as u64;
        }
        (route, reveal_us)
    }

    /// Focuses a row exactly as a click on it does, in the click's order, and reveals it when asked.
    /// Shared by both slot hotkeys (this file and `sidebar_session_slot.rs`). Returns the click's
    /// reaction and, for a reveal, the changes it made to the sidebar's own state and its time.
    ///
    /// The focus is the row click's `selectSession` (which also reaches the remote row machinery of
    /// `gx_store/sidebar_remote_focus.rs` for a row on another machine) and then the click's own
    /// reaction. The reveal is `gx_store_apply_sidebar_reveal`, the one a published request ends in,
    /// and the walk's scroll.
    pub(super) fn gx_store_focus_and_reveal_slot_row(
        &mut self,
        target: &str,
        reveal: bool,
        cx: &mut gpui::Context<Self>,
    ) -> (NativeSidebarClickReaction, Option<(usize, u64)>) {
        self.dispatch_native_sidebar_ui(
            json!({"type": "selectSession", "sessionId": target, "mode": "focus"}),
            cx,
        );
        let reaction = self.react_to_native_sidebar_session_click(target, cx);
        if !reveal {
            return (reaction, None);
        }
        let started = Instant::now();
        let changes = self.gx_store_apply_sidebar_reveal(target, cx).unwrap_or(0);
        let reveal_us = started.elapsed().as_micros() as u64;
        self.gx_store_reveal_walk_row(target);
        (reaction, Some((changes, reveal_us)))
    }

    /// One line per answered press, while the budget lasts: the route and the two timings, never
    /// a row or project id.
    fn gx_store_slot_jump_ran(&mut self, route: &'static str, plan_us: u64, reveal_us: u64) {
        let host = &mut self.gx_store.slot_jump;
        if host.records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        host.records += 1;
        record(
            "gxStore.sidebarSlotJump",
            json!({
                "route": route,
                "planUs": plan_us,
                "revealUs": reveal_us,
                "totals": slot_jump_counters_json(&host.counters),
            }),
        );
    }
}

/// The counters as both records carry them: 12 keys at depth 2.
pub(super) fn slot_jump_counters_json(counters: &SlotJumpCounters) -> Value {
    json!({
        "presses": counters.presses,
        "nothing": counters.nothing,
        "emptyProjects": counters.empty_projects,
        "focuses": counters.focuses,
        "inProcess": counters.in_process,
        "staged": counters.staged,
        "handedToRuntime": counters.handed_to_runtime,
        "reveals": counters.reveals,
        "revealChanges": counters.reveal_changes,
        "declinedSource": counters.declined_source,
        "planMaxUs": counters.plan_max_us,
    })
}
