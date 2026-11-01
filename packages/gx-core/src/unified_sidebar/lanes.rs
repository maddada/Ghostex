//! The per-machine lists the Unified sidebar is built from.

use crate::indicators::neutral_indicator_inputs;
use crate::keys::MachineId;
use crate::sidebar_menu::unified_hover_buttons;
use crate::sidebar_view::{SidebarHiddenItems, SidebarInputs};

/// A lane's inputs for building a project's menu as the Project sidebar shows it: the lane lifts
/// Spaces so every project is in its list, which would also drop the menu's Spaces submenu.
///
/// One machine's list as the Unified sidebar reads it: every Space, no tag filter and no fold, but
/// without what the user hid, and with the Unified sidebar's own hover strip, so a card's buttons
/// and the buttons' mirror in its right-click menu both follow it.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q852, "respect hide"): a project hidden from the Project sidebar leaves the Unified sidebar too, its sessions and its filter row, and Show Hidden brings them back, because someone with a hundred projects hides the ones they don't want to see anywhere. The neutral list keeps feeding the status indicators, which must count everything; only the Unified lane takes the hide list.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q846): "i think let's save different strip for unified vs project sidebar". Each sidebar saves its own strip, and the Project sidebar's setting is not touched.
pub fn unified_lane_menu_inputs(lane_inputs: &SidebarInputs) -> SidebarInputs {
    let mut inputs = lane_inputs.clone();
    inputs.spaces_lifted = false;
    inputs
}

/// One machine's list as the Unified sidebar reads it (see `unified_lane_inputs`).
pub fn unified_lane_inputs(list_inputs: &SidebarInputs, machine: &MachineId) -> SidebarInputs {
    let mut inputs = neutral_indicator_inputs(list_inputs, machine);
    inputs.ui.hidden_items = SidebarHiddenItems::clone(&list_inputs.ui.hidden_items);
    inputs.ui.show_hidden = list_inputs.ui.show_hidden;
    inputs.settings.session_card_hover_buttons = unified_hover_buttons(&list_inputs.settings);
    inputs
}
