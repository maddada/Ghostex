//! A machine's Spaces as the Unified sidebar's filter offers them, and the Space its list is scoped
//! to while Spaces is on.

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::machine_spaces::spaces_enabled_on;
use crate::sidebar_view::spaces::resolve_selected_space;
use crate::sidebar_view::{
    window_spaces, SidebarInputs, SidebarMode, SpacesState, LOCAL_MACHINE_ID, OTHER_SPACE_ID,
};

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

/// The Space the Unified list is scoped to: the one the Project sidebar's Space row has selected,
/// on the machine that row belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedSpaceScope {
    pub machine: MachineId,
    /// A Space id, or [`OTHER_SPACE_ID`].
    pub space_id: String,
    /// The Space's name, for the empty list's sentence.
    pub label: String,
}

/// The Space the Project sidebar's Space row has selected, or `None` when the machine that row
/// belongs to (the Project sidebar's selected machine) has Spaces off, or while the Project sidebar
/// is left in Bots mode, where that row selects no Space.
///
/// CDXC:Spaces 2026-11-05 DECISION:
/// User: "please implement spaces for the unified sidebar so when that extension is enabled it applies for unified sidebar also not just projecst sidebar". The Unified sidebar draws the Project sidebar's own Space row under its header, so it is one selection read and written in one place: a switch in either sidebar is the same active Space in the other. The scope is that row's machine and Space, resolved exactly as the Project sidebar resolves it (a stored Space that is gone falls back to the first, then Other), so both sidebars always agree on which Space is active.
pub fn unified_space_scope(
    store: &PresentationStore,
    list_inputs: &SidebarInputs,
) -> Option<UnifiedSpaceScope> {
    if list_inputs.settings.bots_enabled
        && list_inputs.ui.collapse.sidebar_mode == SidebarMode::Bots
    {
        return None;
    }
    let machine = match list_inputs.ui.selected_machine_id.as_str() {
        LOCAL_MACHINE_ID => MachineId::Local,
        remote => MachineId::Remote(remote.to_string()),
    };
    let spaces = unified_machine_spaces(store, list_inputs, &machine)?;
    let selection = resolve_selected_space(
        &spaces,
        list_inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&list_inputs.ui.section_key())
            .map(String::as_str),
    );
    let space_id = selection.space_id().to_string();
    let label = spaces
        .spaces
        .get(&space_id)
        .filter(|_| space_id != OTHER_SPACE_ID)
        .map_or_else(|| "Other".to_string(), |space| space.name.clone());
    Some(UnifiedSpaceScope {
        machine,
        space_id,
        label,
    })
}
