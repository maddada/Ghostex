// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs (~4.3k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod automation;
pub(crate) mod command_terminal_sessions;
pub(crate) mod daemon_status_and_install;
pub(crate) mod focus_state;
pub(crate) mod git_actions_and_project_paths;
pub(crate) mod gxserver_health_and_daemon;
pub(crate) mod os_integration;
pub(crate) mod project_board_bridge;
pub(crate) mod project_board_images;
pub(crate) mod projects_and_previous_sessions;
pub(crate) mod sidebar_state;
pub(crate) mod typed_operations;

pub(crate) use automation::*;
pub(crate) use command_terminal_sessions::*;
pub(crate) use daemon_status_and_install::*;
pub(crate) use focus_state::*;
pub(crate) use git_actions_and_project_paths::*;
pub(crate) use gxserver_health_and_daemon::*;
pub(crate) use os_integration::*;
pub(crate) use project_board_bridge::*;
pub(crate) use project_board_images::*;
pub(crate) use projects_and_previous_sessions::*;
pub(crate) use sidebar_state::*;
pub(crate) use typed_operations::*;
