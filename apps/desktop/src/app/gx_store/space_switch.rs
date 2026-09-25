//! A Space switch's focus restore inside the app.
//!
//! CDXC:Spaces 2026-09-21 DECISION:
//! User: switching to a Space reopens the session that Space was last left on
//! (`sidebarSpaceSwitchBehavior: restore`, the default). The memory has been the store's since M5
//! piece 7c (`sidebar_ui_paths.rs`); the READ was still `switchNativeSidebarSpace` in the sidebar
//! page, which is going away, so it moves here, for a REMOTE machine's tab as well as this
//! computer's.
//!
//! The focus itself still goes out as the `focusSession` / `focusGroup` the page posted, through
//! the same message route, because the runtime owns what a focus does to the panes.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/space_switch.rs (the rows it picks from, and the
//! one declared difference).

use ghostex_gx_core::{SpaceSwitchFocus, plan_space_switch_restore};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// What this app run did with the Space switches. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpaceSwitchCounters {
    /// Switches that really changed the selected Space.
    pub(crate) switches: u64,
    pub(crate) session_restores: u64,
    pub(crate) group_restores: u64,
    /// Switches that restored nothing: the Space draws no row at all.
    pub(crate) empty: u64,
    /// Switches left alone because the setting is `keep`.
    pub(crate) kept: u64,
    /// Of the switches, the ones made on a REMOTE machine's tab.
    pub(crate) remotes: u64,
}

/// The Space a section was filtered by before a `selectSpace` was applied, carried across the
/// apply so the "did the selection really move?" test is the TypeScript's.
pub(crate) struct SpaceSwitchBefore {
    section_key: String,
    next_space_id: String,
    previous_space_id: Option<String>,
}

impl GhostexGpuiApp {
    /// Reads the Space a `selectSpace` is leaving, before the intent moves it. `None` for any other
    /// command.
    ///
    /// The RESOLVED Space, not the stored key: a section with nothing stored, or with a Space that
    /// has since been deleted, is showing its first Space or Other, and clicking that button is a
    /// switch to the Space it is already on.
    pub(crate) fn gx_store_space_switch_before(
        &self,
        command: &Value,
    ) -> Option<SpaceSwitchBefore> {
        if command.get("type").and_then(Value::as_str) != Some("selectSpace") {
            return None;
        }
        let next_space_id = command.get("spaceId").and_then(Value::as_str)?.to_string();
        Some(SpaceSwitchBefore {
            section_key: self.gx_store.sidebar_ui.state().section_key(),
            next_space_id,
            // With `sidebarSpacesEnabled` off the list draws no Space row at all, which is
            // `describeNativeSidebarMachine`'s `spacesState` being undefined and its `selection`
            // with it.
            previous_space_id: self
                .gx_store
                .sidebar_list
                .view()
                .spaces
                .iter()
                .find(|space| space.selected)
                .map(|space| space.id.clone()),
        })
    }

    /// Restores the newly selected Space's remembered row. Called after the intent has been applied
    /// and the list rebuilt, so the rows it picks from are the ones the Space now draws.
    pub(crate) fn gx_store_restore_space_switch_focus(
        &mut self,
        before: SpaceSwitchBefore,
        cx: &mut gpui::Context<Self>,
    ) {
        // `if (previous === spaceId ...) return`: clicking the button of the Space already on
        // screen restores nothing, so a second click does not pull the focus back off a row the
        // user has just chosen inside it.
        if before.previous_space_id.as_deref() == Some(before.next_space_id.as_str()) {
            return;
        }
        // CDXC:RemoteMachines 2026-09-21 WHY:
        // A REMOTE machine's tab is restored here too. The restore reads the list the store has
        // just rebuilt, and that list is the SELECTED machine's whichever machine it is, so the
        // only thing the old leg in `switchNativeSidebarSpace` had that this does not is the page's
        // own projection. Its remote leg is gone with the local one, or the click would post two
        // `focusSession` messages.
        if self.gx_store_selected_remote_machine_id().is_some() {
            self.gx_store.space_switch.remotes += 1;
        }
        self.gx_store.space_switch.switches += 1;
        if !space_switch_restores() {
            self.gx_store.space_switch.kept += 1;
            self.gx_store
                .diagnostics
                .space_switch_ran("keep", self.gx_store.space_switch);
            return;
        }
        let plan = {
            let store = &self.gx_store;
            let recent = store
                .sidebar_ui
                .state()
                .collapse
                .recent_sessions_by_space
                .get(&before.section_key)
                .and_then(|by_space| by_space.get(&before.next_space_id))
                .cloned()
                .unwrap_or_default();
            plan_space_switch_restore(store.sidebar_list.view(), &recent)
        };
        let (outcome, message) = match plan {
            Some(SpaceSwitchFocus::Session { sidebar_session_id }) => {
                self.gx_store.space_switch.session_restores += 1;
                (
                    "session",
                    // `keepView: true`: the Space switch reopens the session in the view its
                    // project was in rather than forcing the terminal view on it.
                    json!({
                        "type": "focusSession",
                        "sessionId": sidebar_session_id,
                        "keepView": true,
                    }),
                )
            }
            Some(SpaceSwitchFocus::Group { group_id }) => {
                self.gx_store.space_switch.group_restores += 1;
                (
                    "group",
                    json!({ "type": "focusGroup", "groupId": group_id }),
                )
            }
            None => {
                self.gx_store.space_switch.empty += 1;
                self.gx_store
                    .diagnostics
                    .space_switch_ran("empty", self.gx_store.space_switch);
                return;
            }
        };
        self.gx_store
            .diagnostics
            .space_switch_ran(outcome, self.gx_store.space_switch);
        self.dispatch_native_sidebar_command(message, cx);
    }
}

/// `nativeSidebarSettings().sidebarSpaceSwitchBehavior === 'restore'`, the default.
///
/// Read here rather than carried on [`ghostex_gx_core::SidebarSettings`] because nothing else in
/// the list depends on it and a Space switch is a gesture, not a redraw: one settings read per
/// click costs nothing and keeps the value with the one decision it makes.
fn space_switch_restores() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("sidebarSpaceSwitchBehavior")
        .and_then(Value::as_str)
        != Some("keep")
}
