// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod agent_hook_status;
pub(crate) mod catalog_builder;
pub(crate) mod catalog_fs_scan;
pub(crate) mod keep_awake;
pub(crate) mod open_targets;
pub(crate) mod pet_overlay_status;
pub(crate) mod resource_urls_and_drop_actions;
pub(crate) mod status_pet_visuals;
pub(crate) mod workspace_agent_actions;
pub(crate) mod workspace_tab_visuals;

pub(crate) use agent_hook_status::*;
pub(crate) use catalog_builder::*;
pub(crate) use catalog_fs_scan::*;
pub(crate) use keep_awake::*;
pub(crate) use open_targets::*;
pub(crate) use pet_overlay_status::*;
pub(crate) use resource_urls_and_drop_actions::*;
pub(crate) use status_pet_visuals::*;
pub(crate) use workspace_agent_actions::*;
pub(crate) use workspace_tab_visuals::*;
