//! The three routes into the sidebar's own state that are not sidebar commands.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! Everything else that moves this state arrives as a sidebar command and is turned into an intent
//! in `sidebar_ui_commands.rs`. These three do not, and until M5 piece 7c they were the reason the
//! old runtime still had to write the collapse key: nothing in Rust saw them.
//!
//! - **The per-Space session memory.** `rememberSidebarSpaceSession`, written on every focus change
//!   and every reveal, and read by a Space switch with `sidebarSpaceSwitchBehavior` set to
//!   `restore`. The Space it is keyed by comes from `space_for_focused_row` and from the reveal
//!   plan, which answer it with the same function.
//! - **The Space-editor delete.** Deleting the Space a section is filtered by leaves that section
//!   naming a Space that is gone. The renderer forwards the dialog's result to the sidebar page,
//!   which owns the Spaces document; this takes the one consequence the SIDEBAR STATE has.
//! - **`gpuiProjectSlotHotkey`.** A THIRD route, neither of the two sidebar-command envelopes. It
//!   deletes the jumped-to project's collapsed flag and, with `showLessForExpandedProjectJumps` on,
//!   its session list's expanded flag, and that second one fought the Rust reveal that follows the
//!   same jump over one key on every cmd+ctrl+1..9. What moves here is the state, whether or not
//!   the list is ready to be jumped in; the jump itself (the focus and the reveal) is
//!   `sidebar_slot_jump.rs`.
//!
//! **The counters that prove these fire** are `spaceMemoryWrites`, `spaceForgets` and `slotJumps`
//! on `gxStore.sidebarUi`. A run in which the user pressed cmd+ctrl+1 on a collapsed project and
//! `slotJumps` is zero means this file never saw the hotkey and the old runtime is still the only
//! thing that expanded it.
//!
//! SEE-ALSO: the deleted sidebar page's `space-navigation.ts`, `events.ts` and `hotkeys.ts`,
//! packages/gx-core/src/sidebar_view/slot_hotkey.rs.

use ghostex_gx_core::{FocusedRowSpace, SidebarUiIntent};
use serde_json::Value;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Puts a row at the front of its Space's memory.
    pub(crate) fn gx_store_remember_space_session(
        &mut self,
        resolved: &FocusedRowSpace,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store_apply_sidebar_ui_intent(
            SidebarUiIntent::RememberSpaceSession {
                section_key: resolved.section_key.clone(),
                space_id: resolved.space_id.clone(),
                sidebar_session_id: sidebar_session_id.to_string(),
            },
            cx,
        ) {
            self.gx_store.sidebar_ui.counters.space_memory_writes += 1;
        }
    }

    /// The Space editor's result, on its way to the sidebar page. Only a DELETE of the Space a
    /// section is currently filtered by changes anything here; the document itself is the page's.
    ///
    /// The section key is built the way `receiveNativeSidebarEvent` builds it, from the message's
    /// own machine id rather than from the selected tab: a Space can be deleted from a dialog
    /// opened on another machine's section.
    pub(crate) fn gx_store_note_sidebar_space_editor_result(
        &mut self,
        message: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if message.get("mode").and_then(Value::as_str) != Some("delete") {
            return;
        }
        let Some(space_id) = message.get("spaceId").and_then(Value::as_str) else {
            return;
        };
        let section_key = match message.get("remoteMachineId").and_then(Value::as_str) {
            Some(machine_id) => format!("remote:{machine_id}"),
            None => ghostex_gx_core::LOCAL_MACHINE_ID.to_string(),
        };
        if self
            .gx_store
            .sidebar_ui
            .state()
            .collapse
            .selected_space_by_section
            .get(&section_key)
            .map(String::as_str)
            != Some(space_id)
        {
            return;
        }
        if self.gx_store_apply_sidebar_ui_intent(
            SidebarUiIntent::ForgetSectionSpace { section_key },
            cx,
        ) {
            self.gx_store.sidebar_ui.counters.space_forgets += 1;
        }
    }

    /// A project slot hotkey: the two deletions the jump makes to this state and the
    /// multi-selection its selection clears, applied in one batch. Returns the plan, which
    /// `sidebar_slot_jump.rs` performs the rest of when the store's list is drawn.
    ///
    /// The slot names the Nth drawn project, resolved against this store's list, which is the only
    /// list there is. This state is the only writer of the collapse key even while the list is not
    /// ready, so answering only when it is would silently stop storing the jump.
    ///
    /// The multi-selection is cleared by the SELECTION the jump makes, not by the jump, so a slot
    /// naming a project with no drawn row leaves it alone (`ProjectSlotPlan::intents`). That is what
    /// `runNativeProjectSlotHotkey` does, where `selectNativeSidebarSession` is inside
    /// `if (session)`.
    pub(crate) fn gx_store_note_project_slot_hotkey(
        &mut self,
        slot_number: u8,
        cx: &mut gpui::Context<Self>,
    ) -> Option<ghostex_gx_core::ProjectSlotPlan> {
        let plan = {
            let store = &self.gx_store;
            ghostex_gx_core::project_slot_plan(
                store.sidebar_list.view(),
                store.sidebar_ui.state(),
                &store.sidebar_list.last_inputs.settings,
                u32::from(slot_number),
            )
        }?;
        self.gx_store.sidebar_ui.counters.slot_jumps += 1;
        self.gx_store_apply_sidebar_ui_intents(plan.intents(), cx);
        Some(plan)
    }
}
