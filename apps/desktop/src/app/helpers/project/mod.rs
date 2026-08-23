// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod add_project_dialog;
pub(crate) mod attach_terminal;
pub(crate) mod colors;
pub(crate) mod contract_and_paths;
pub(crate) mod editor_launch;
pub(crate) mod menu_bar_status;
pub(crate) mod project_beads;
pub(crate) mod project_settings;
pub(crate) mod status_indicator;
pub(crate) mod terminal_state_types;

pub(crate) use add_project_dialog::*;
pub(crate) use attach_terminal::*;
pub(crate) use colors::*;
pub(crate) use contract_and_paths::*;
pub(crate) use editor_launch::*;
pub(crate) use menu_bar_status::*;
pub(crate) use project_beads::*;
pub(crate) use project_settings::*;
pub(crate) use status_indicator::*;
pub(crate) use terminal_state_types::*;
