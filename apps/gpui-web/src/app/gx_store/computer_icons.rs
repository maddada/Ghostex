//! The page's stand-in for the desktop's `gx_store/computer_icons.rs`, which `unified_new_session.rs` (the New Thread picker's machine columns) reads. The page has one computer and no remote connections, so no gxserver answer is kept (`computer_icon_facts` is always `None`) and the sidebar draws no mark (`gx_store_computer_marks` is empty); the picker then falls back to this computer's own system logo, the same fallback the desktop uses with only this computer.

use std::collections::HashMap;

use crate::GhostexGpuiApp;

/// What one computer's gxserver answered: the desktop's field set, never filled on the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComputerIconFacts {
    pub(crate) icon: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) logo: Option<String>,
}

/// A computer's mark in the sidebar: its icon's drawing and its full name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComputerMark {
    pub(crate) asset: &'static str,
    pub(crate) name: String,
}

pub(crate) fn computer_icon_facts(_machine_id: &str) -> Option<ComputerIconFacts> {
    None
}

impl GhostexGpuiApp {
    pub(super) fn gx_store_computer_marks(
        &self,
        _tabs: &[ghostex_gx_core::MachineTabInput],
    ) -> HashMap<String, ComputerMark> {
        HashMap::new()
    }
}
