// C1 wave-3 re-cluster: command terminal create-input and attach-plan types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandTerminalCreateInput {
    pub(crate) command_id: Option<String>,
    pub(crate) command_title: Option<String>,
    pub(crate) cwd: String,
    pub(crate) project_id: String,
    pub(crate) startup_text: Option<String>,
    pub(crate) title: String,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandTerminalCreateInputResolution {
    Ready(GpuiCommandTerminalCreateInput),
    NotReady,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandTerminalAttachPlan {
    pub(crate) attach_command: String,
    pub(crate) command_id: Option<String>,
    pub(crate) initial_input: Option<String>,
    pub(crate) key: GpuiLocalWorkspaceSessionKey,
    pub(crate) title: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) zmx_name: Option<String>,
}
