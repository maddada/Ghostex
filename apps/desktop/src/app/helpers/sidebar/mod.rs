// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod command_id_helpers;
pub(crate) mod env_and_bridge;
pub(crate) mod hud_buttons;
pub(crate) mod icons_and_indicators;
pub(crate) mod metadata_apply;
pub(crate) mod metadata_write_builders;
pub(crate) mod native_action_exec;
pub(crate) mod native_action_types;
pub(crate) mod runtime_settings;
pub(crate) mod settings_messages_and_width;
pub(crate) mod sidebar_defaults_types;
pub(crate) mod workspace_terminal_actions;

pub(crate) use command_id_helpers::*;
pub(crate) use env_and_bridge::*;
pub(crate) use hud_buttons::*;
pub(crate) use icons_and_indicators::*;
pub(crate) use metadata_apply::*;
pub(crate) use metadata_write_builders::*;
pub(crate) use native_action_exec::*;
pub(crate) use native_action_types::*;
pub(crate) use runtime_settings::*;
pub(crate) use settings_messages_and_width::*;
pub(crate) use sidebar_defaults_types::*;
pub(crate) use workspace_terminal_actions::*;
