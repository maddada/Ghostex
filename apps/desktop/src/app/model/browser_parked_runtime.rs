// Browser runtime parking types: the per-project bundle of live CEF surfaces
// and per-pane/per-tab input state that a project keeps while another project
// owns the Browser workarea, plus the owner key a surface's async CEF
// callbacks carry so they can find their own model again.

use std::collections::HashMap;
use std::collections::HashSet;

use gpui::Entity;
use gpui_component::input::InputState;

use crate::*;

/*
CDXC:GPUIBrowserProjectParking 2026-08-26:
Switching the active project must not sleep or destroy the outgoing project's
browser pages. BrowserTabId/BrowserPaneId are project-local counters, so the
live `browser_surfaces`/`browser_address_inputs`/`browser_find_states` maps can
only describe one project at a time; the outgoing project's runtime moves into
this bundle (hidden, not destroyed) and moves back verbatim when the project
becomes active again. Nothing here is serialized: it is process-local CEF and
input ownership, parked beside the already-parked `BrowserTabModel`.
*/
#[derive(Default)]
pub(crate) struct ParkedBrowserRuntime {
    /// Identity of the parked browser tab model instance. A surface's CEF
    /// callbacks capture the key that was live when the surface was created,
    /// so a late title/favicon/load update can be routed to the model that
    /// actually owns the tab id instead of colliding with another project's.
    pub(crate) runtime_key: u64,
    pub(crate) surfaces: HashMap<BrowserTabId, Entity<CefSurface>>,
    pub(crate) address_inputs: HashMap<BrowserPaneId, Entity<InputState>>,
    pub(crate) address_input_subscriptions: HashMap<BrowserPaneId, gpui::Subscription>,
    pub(crate) address_input_editing: HashSet<BrowserPaneId>,
    pub(crate) find_states: HashMap<BrowserTabId, GpuiBrowserFindState>,
    pub(crate) find_inputs: HashMap<BrowserTabId, Entity<InputState>>,
    pub(crate) find_input_subscriptions: HashMap<BrowserTabId, gpui::Subscription>,
}

impl ParkedBrowserRuntime {
    pub(crate) fn holds_runtime_state(&self) -> bool {
        !self.surfaces.is_empty()
            || !self.address_inputs.is_empty()
            || !self.find_states.is_empty()
            || !self.find_inputs.is_empty()
    }

    pub(crate) fn surface_tab_ids(&self) -> HashSet<BrowserTabId> {
        self.surfaces.keys().copied().collect()
    }

    /// Drop one tab's parked runtime. Dropping the `Entity<CefSurface>` closes
    /// the parked browser, which is exactly what an explicit sleep or a close
    /// of a parked project's tab must do.
    pub(crate) fn forget_tab(&mut self, tab_id: BrowserTabId) -> bool {
        let had_surface = self.surfaces.remove(&tab_id).is_some();
        let had_find_state = self.find_states.remove(&tab_id).is_some();
        self.find_inputs.remove(&tab_id);
        self.find_input_subscriptions.remove(&tab_id);
        had_surface || had_find_state
    }
}

/// Which browser tab model a surface's callback belongs to: the one currently
/// mounted in the Browser workarea, or a named project's parked one.
pub(crate) enum BrowserRuntimeOwner {
    Live,
    Parked(String),
}
