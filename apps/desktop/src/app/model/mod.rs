// C1 wave-3: Region A sub-model and value-type definitions moved verbatim
// out of main.rs. Each submodule is glob-re-exported here so every existing
// unqualified call site in main.rs (and in these modules themselves, via
// `use crate::app::model::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead.
pub(crate) mod command_pane;
pub(crate) mod launch_payload;
pub(crate) mod runtime_state;
pub(crate) mod shell_layout;
pub(crate) mod tab_groups;
pub(crate) mod types1;
pub(crate) mod types2;
pub(crate) mod types3;
pub(crate) mod types4;
pub(crate) mod types5;
pub(crate) mod types6;
pub(crate) mod workspace;

pub(crate) use command_pane::*;
pub(crate) use launch_payload::*;
pub(crate) use runtime_state::*;
pub(crate) use shell_layout::*;
pub(crate) use tab_groups::*;
pub(crate) use types1::*;
pub(crate) use types2::*;
pub(crate) use types3::*;
pub(crate) use types4::*;
pub(crate) use types5::*;
pub(crate) use types6::*;
pub(crate) use workspace::*;
