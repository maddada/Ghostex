//! Rename and Note: the two sidebar actions that open a dialog and call nothing.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! The pair is a close followed by an open, in that order, and both halves are the app modal
//! host's own: the close is the bridge's `close` arm, extracted so this runs it rather than a
//! second copy of it, and the open is the one entry that handles a native dialog and a web one
//! alike. The title the user then types never comes back this way. It returns through the app
//! modal host as its own message and reaches `/api/requestSessionRename`, which is not part of the
//! sidebar's action surface at all.
//!
//! Nothing the user typed is written to a log here. The dialog is seeded with the row's current
//! title and note, which are the user's own words, and the record says only which dialog opened
//! and whether a seed was present.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/modals.rs.

use ghostex_gx_core::{owns_modal_message, plan_modal_action};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with the dialog actions the store owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarModalCounters {
    pub(crate) renames: u64,
    pub(crate) notes: u64,
    pub(crate) declined_source: u64,
    /// The row was not in the drawn list, so the store could not seed the dialog and the old
    /// runtime answered instead. A context menu can only be opened on a drawn row, so this
    /// should stay at zero; a number here means a payload arrived from somewhere else.
    pub(crate) declined_row: u64,
}

impl GhostexGpuiApp {
    /// Answers `sessionAction: rename` and `sessionAction: note` when the store owns them.
    ///
    /// CDXC:ContextMenus 2026-09-20 WHY:
    /// A sidebar command arrives in one of TWO envelopes and confusing them is silent. A gxserver
    /// message is wrapped as `{ type: 'command', message }` and the controller posts the inner
    /// message to the runtime (controller.ts:131); `sessionAction` is a RENDERER command and
    /// arrives at the TOP level, because the controller answers it itself (controller.ts:160,
    /// `runNativeSessionAction`). This path unwrapped it, found nothing, and returned false for
    /// every Rename and Note the user clicked, so the whole dialog port was dead in the app while
    /// its gate passed: the gate drives gx-core, which was given the right shape all along.
    pub(crate) fn gx_store_run_sidebar_modal(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_modal_message(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_modals.declined_source += 1;
            return false;
        }
        let Some(action) = plan_modal_action(self.gx_store.sidebar_list.view(), command) else {
            self.gx_store.sidebar_modals.declined_row += 1;
            return false;
        };
        let is_rename = action.open.get("modal") == Some(&Value::from("renameSession"));
        match is_rename {
            true => self.gx_store.sidebar_modals.renames += 1,
            false => self.gx_store.sidebar_modals.notes += 1,
        }
        // The order is the TypeScript's: the open dialog goes first, so a Settings window does not
        // stay behind the one about to appear.
        self.close_app_modal_from_bridge(cx);
        let seeded = action
            .open
            .get(match is_rename {
                true => "initialTitle",
                false => "initialNote",
            })
            .and_then(Value::as_str)
            .is_some_and(|seed| !seed.is_empty());
        self.open_app_modal_from_bridge(action.open, cx);
        self.gx_store.diagnostics.sidebar_modal_opened(
            is_rename,
            seeded,
            self.gx_store.sidebar_modals,
        );
        true
    }
}
