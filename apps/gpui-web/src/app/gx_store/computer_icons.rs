//! The web build's stand-in for the desktop's `gx_store/computer_icons.rs`, whose remote-machine
//! icon reads this page does not run: the page knows no computer's picked icon or system, so
//! `unified_new_session.rs` draws this computer's own system logo and no marks for others.

use std::collections::HashMap;

/// What a computer said about its icon (the desktop's type): nothing, in a page.
#[derive(Clone, Debug, Default)]
pub(crate) struct ComputerIconFacts {
    pub(crate) icon: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) logo: Option<String>,
}

/// A machine's logo and name as the sidebar draws them (the desktop's type).
pub(crate) struct ComputerMark {
    pub(crate) asset: &'static str,
    pub(crate) name: String,
}

pub(crate) fn computer_icon_facts(_machine_id: &str) -> Option<ComputerIconFacts> {
    None
}

impl crate::GhostexGpuiApp {
    pub(super) fn gx_store_computer_marks(
        &self,
        _tabs: &[ghostex_gx_core::MachineTabInput],
    ) -> HashMap<String, ComputerMark> {
        HashMap::new()
    }
}
