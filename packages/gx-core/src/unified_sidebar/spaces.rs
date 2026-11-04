//! A machine's Spaces as the Unified sidebar's filter offers them.

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::machine_spaces::spaces_enabled_on;
use crate::sidebar_view::{window_spaces, SidebarInputs, SpacesState};

/// `machine`'s Spaces as this window sees them, or `None` when that machine has Spaces off.
/// `list_inputs` are the Project sidebar's own inputs, which carry this computer's Spaces switch.
pub fn unified_machine_spaces(
    store: &PresentationStore,
    list_inputs: &SidebarInputs,
    machine: &MachineId,
) -> Option<SpacesState> {
    if !spaces_enabled_on(store, list_inputs, machine) {
        return None;
    }
    window_spaces(store, list_inputs, machine).map(|spaces| SpacesState::from_wire(&spaces))
}
