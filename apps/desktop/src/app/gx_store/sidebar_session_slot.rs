//! The session slot hotkeys (Focus Session 1 to 9, cmd+1 to cmd+9 by default), performed by the
//! store when its list is drawn.
//!
//! CDXC:Hotkeys 2026-09-21 WHY:
//! cmd+N used to be the old sidebar page's: `nativeHotkey` went to QuickJS, where
//! `runNativeSidebarHotkey` resolved the Nth row from the page's own in-memory copy of the sidebar
//! and selected and revealed it. With the store's list drawn, the row is the Nth row of THAT list
//! (gx-core `session_slot_plan`), so what the user sees is what cmd+N means, and the message is no
//! longer sent at all: the store's list is the only list since M4d part 2 step 6.
//!
//! **Where each effect ends, read rather than assumed.** The press is a row click plus a reveal,
//! exactly as `runNativeSidebarHotkey` made it (`selectNativeSidebarSession`, then
//! `requestReveal`), so it goes through the slot jump's `gx_store_focus_and_reveal_slot_row`: the
//! row click's `selectSession` (for a LOCAL row the store's `focusSession` in `focus_perform.rs`, the runtime's until 2026-09-25, for a REMOTE row the
//! store's remote click in `sidebar_remote_focus.rs`, which is all a click on that row does), the
//! click's in-process reaction (`react_to_native_sidebar_session_click`, ending in
//! `gx_store_select_local_session`), then `gx_store_apply_sidebar_reveal` and the walk's scroll.
//!
//! Key repeats never reach here: the keyboard router repeats only the tab cycle and the session
//! walk, so a held cmd+N is one press.
//!
//! **Counters** ride `gxStore.sidebarActions.summary` as `sessionSlot`, whose first line is written
//! at zero: `presses`, `nothing`, `inProcess`, `staged`, `remote`, `handedToRuntime`, `reveals`,
//! `revealChanges`, `declinedSource`, `planMaxUs`. A cmd+N with `presses` still zero means the key
//! never reached this file; `declinedSource` moving means the list was not ready yet.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/session_slot.rs,
//! apps/desktop/src/app/gx_store/sidebar_slot_jump.rs.

use std::time::Instant;

use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use super::diagnostics::{MAX_SIDEBAR_ACTION_RECORDS, record, routine_logging_enabled};
use crate::GhostexGpuiApp;
use crate::app::sidebar_direct_focus::NativeSidebarClickReaction;

/// What this app run did with the session slot hotkeys. Rides `gxStore.sidebarActions.summary`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionSlotCounters {
    pub(crate) presses: u64,
    /// A slot past the last drawn row.
    pub(crate) nothing: u64,
    pub(crate) in_process: u64,
    pub(crate) staged: u64,
    /// A row on another machine: the store's remote open, as a click on it.
    pub(crate) remote: u64,
    /// A local row the click reaction did not apply to (a browser row, a project the store cannot
    /// swap to in process): the store's `focusSession` (focus_perform.rs; the runtime's until 2026-09-25), reached through the same `selectSession`.
    pub(crate) handed_to_runtime: u64,
    pub(crate) reveals: u64,
    pub(crate) reveal_changes: u64,
    pub(crate) declined_source: u64,
    pub(crate) plan_max_us: u64,
}

#[derive(Default)]
pub(crate) struct SessionSlotHost {
    pub(crate) counters: SessionSlotCounters,
    records: u32,
}

impl GhostexGpuiApp {
    /// A session slot hotkey, `slot_number` 1 to 9. The whole answer: nothing else is told,
    /// because nothing else draws the sidebar.
    pub(crate) fn gx_store_run_session_slot_hotkey(
        &mut self,
        slot_number: u8,
        cx: &mut gpui::Context<Self>,
    ) {
        // Nothing at all happens while the loading skeleton is drawn, the way
        // `dispatch_native_sidebar_ui` drops a command that arrives then: the slot names the Nth
        // row of a list that has not been built.
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.session_slot.counters.declined_source += 1;
            return;
        }
        let started = Instant::now();
        let plan = ghostex_gx_core::session_slot_plan(
            self.gx_store.sidebar_list.view(),
            u32::from(slot_number),
        );
        let plan_us = started.elapsed().as_micros() as u64;
        {
            let counters = &mut self.gx_store.session_slot.counters;
            counters.presses += 1;
            counters.plan_max_us = counters.plan_max_us.max(plan_us);
        }
        let Some(plan) = plan else {
            // `visibleSessionIds[slotNumber - 1]` is undefined: nothing is selected or revealed.
            self.gx_store.session_slot.counters.nothing += 1;
            self.gx_store_session_slot_ran("nothing", plan_us, 0);
            return;
        };
        self.gx_store_apply_sidebar_ui_intents(plan.intents(), cx);
        let target = plan.target_session_id;
        let (reaction, reveal) = self.gx_store_focus_and_reveal_slot_row(&target, true, cx);
        let remote = SessionKey::parse_remote_scoped_session_id(&target).is_some();
        let counters = &mut self.gx_store.session_slot.counters;
        let route = match reaction {
            NativeSidebarClickReaction::InProcess => {
                counters.in_process += 1;
                "inProcess"
            }
            NativeSidebarClickReaction::Staged => {
                counters.staged += 1;
                "staged"
            }
            NativeSidebarClickReaction::NotApplied if remote => {
                counters.remote += 1;
                "remote"
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
        cx.notify();
        self.gx_store_session_slot_ran(route, plan_us, reveal_us);
    }

    /// One line per answered press, while the budget lasts: the route and the two timings, never
    /// a row or project id.
    fn gx_store_session_slot_ran(&mut self, route: &'static str, plan_us: u64, reveal_us: u64) {
        let host = &mut self.gx_store.session_slot;
        if host.records >= MAX_SIDEBAR_ACTION_RECORDS || !routine_logging_enabled() {
            return;
        }
        host.records += 1;
        record(
            "gxStore.sidebarSessionSlot",
            json!({
                "route": route,
                "planUs": plan_us,
                "revealUs": reveal_us,
                "totals": session_slot_counters_json(&host.counters),
            }),
        );
    }
}

/// The counters as both records carry them: 11 keys at depth 2.
pub(super) fn session_slot_counters_json(counters: &SessionSlotCounters) -> Value {
    json!({
        "presses": counters.presses,
        "nothing": counters.nothing,
        "inProcess": counters.in_process,
        "staged": counters.staged,
        "remote": counters.remote,
        "handedToRuntime": counters.handed_to_runtime,
        "reveals": counters.reveals,
        "revealChanges": counters.reveal_changes,
        "declinedSource": counters.declined_source,
        "planMaxUs": counters.plan_max_us,
    })
}
