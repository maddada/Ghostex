//! The sidebar list's own checks and the periodic counters that say the store is alive.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Until M4d part 2 step 7 this file compared the TypeScript projection's list with the Rust one.
//! That page is deleted, so there is no second list to compare with and the comparison, its
//! mismatch records, its settle window and its skip counters went with it. What is kept is the one
//! check that never needed the page: the incremental list is built from scratch every so often and
//! the two are compared, both Rust, which is this port's own cache failing to invalidate rather
//! than a difference with anything. It has caught a real bug once already.
//!
//! The summaries ride the sidebar's once-a-second tick for the reason that cost two live rounds: a
//! record emitted only from a push has no line at all in a run where the user moved no project, and
//! the counters it carries are exactly what says whether the path is alive. They used to ride the
//! comparison, which is what a publish drove.

use ghostex_gx_core::{MachineId, SidebarViewModel};

use super::sidebar_scratch_compare::compare_views;
use crate::GhostexGpuiApp;

/// How often the incremental list is also built from scratch and the two compared. Counted in list
/// updates, which is the cadence the publish-driven comparison gave it.
const SCRATCH_CHECK_EVERY: u32 = 20;

/// What happened since the app started. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarSelfCheckCounters {
    pub(crate) scratch_checks: u64,
    pub(crate) scratch_mismatches: u64,
}

#[derive(Default)]
pub(crate) struct SidebarSelfCheck {
    pub(crate) counters: SidebarSelfCheckCounters,
    updates_since_scratch_check: u32,
}

impl GhostexGpuiApp {
    /// Every so often the list is also built from scratch: a difference there is this port's own
    /// cache failing to invalidate, so it is named in its own record.
    pub(super) fn gx_store_sidebar_scratch_check(&mut self) {
        let check = &mut self.gx_store.sidebar_self_check;
        check.updates_since_scratch_check += 1;
        if check.updates_since_scratch_check < SCRATCH_CHECK_EVERY {
            return;
        }
        check.updates_since_scratch_check = 0;
        // Asking costs a stat of the settings file and a clone of its map under a global lock, so
        // it is asked once per check rather than once per update.
        if !super::diagnostics::routine_logging_enabled() {
            return;
        }
        let now_ms = self.gx_store.sidebar_list.last_built_at_ms;
        let inputs = std::mem::take(&mut self.gx_store.sidebar_list.last_inputs);
        let scratch = SidebarViewModel::build_from_scratch(&self.gx_store.core, &inputs, now_ms);
        self.gx_store.sidebar_list.last_inputs = inputs;
        let difference = compare_views(self.gx_store.sidebar_list.view(), &scratch);
        let check = &mut self.gx_store.sidebar_self_check;
        check.counters.scratch_checks += 1;
        if difference.is_some() {
            check.counters.scratch_mismatches += 1;
        }
        if let Some(difference) = &difference {
            let last_update = self.gx_store.sidebar_list.last_update;
            self.gx_store
                .diagnostics
                .sidebar_scratch_mismatch(difference, &last_update);
        }
    }

    /// Every periodic counter the store owns, written at most once a minute and only when it moved.
    pub(super) fn gx_store_sidebar_summaries(&mut self) {
        let counters = self.gx_store.sidebar_self_check.counters;
        let list = self.gx_store.sidebar_list.counters;
        let (groups, rows) = {
            let view = self.gx_store.sidebar_list.view();
            (
                view.groups.len(),
                view.groups
                    .iter()
                    .map(|group| group.core.sessions.len())
                    .sum::<usize>(),
            )
        };
        let ready = self.gx_store_sidebar_list_ready();
        let recovered = {
            let recovery = self.gx_store.sidebar_list.ready_recovery();
            (recovery.hud, recovery.state)
        };
        let deadline_kind = self.gx_store.sidebar_list.deadline_kind;
        let phases = self.gx_store.sidebar_list.install_phases();
        let remote = self.gx_store.remote.counters;
        let machines = self.gx_store.remote.tabs().len();
        self.gx_store.diagnostics.sidebar_summary(
            &counters,
            &list,
            ready,
            recovered,
            deadline_kind,
            groups,
            rows,
            phases,
            &remote,
            machines,
        );
        let ui = self.gx_store.sidebar_ui.counters;
        self.gx_store.diagnostics.sidebar_ui_summary(&ui);
        // The runtime facts holder rides the same path, and for the same reason: its counters are
        // what says it is alive (gx_store/diagnostics_runtime_facts.rs).
        let runtime_facts = self.gx_store.runtime_facts.counters;
        let runtime_route = self.gx_store.runtime_route;
        self.gx_store
            .diagnostics
            .runtime_facts_summary(runtime_facts, runtime_route);
        let groups = self.gx_store.workspace_groups.counters;
        let side_state_held = self
            .gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .is_some_and(|machine| machine.side_state().workspace_groups.is_some());
        self.gx_store
            .diagnostics
            .workspace_groups_summary(groups, side_state_held);
        // K5, K6 and the project moves ride the SAME path, for the reason that cost two live
        // rounds: a record emitted only from a push has no line at all in a run where the user
        // moved no project. The first line goes out with every counter at zero on purpose.
        let collections = self.gx_store.collections.counters;
        let spaces = self.gx_store.spaces.counters;
        let moves = self.gx_store.project_moves;
        self.gx_store
            .diagnostics
            .client_document_summary(collections, spaces, moves);
        // A remote row's actions ride it for the same reason: a run in which the user touched no
        // remote row must still say so (gx_store/sidebar_remote.rs).
        self.gx_store_sidebar_actions_summary();
        // And the last-seen copies, whose two halves both fire only when there is a remote machine
        // to seed or to store (gx_store/remote_last_seen.rs).
        self.gx_store_last_seen_summary();
    }
}
