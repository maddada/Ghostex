// C1 wave-2 deferred split: apps/desktop/src/app/helpers/os_cli.rs (~4.7k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod app_state_persistence;
pub(crate) mod attention_and_menu_bar;
pub(crate) mod cli_install;
pub(crate) mod cli_settings_actions;
pub(crate) mod cli_status;
pub(crate) mod command_exec;
pub(crate) mod cua_driver_job;
pub(crate) mod cua_driver_skill;
pub(crate) mod cua_driver_status;
pub(crate) mod disclaimed_spawn;
pub(crate) mod folder_stats_and_zmx;
pub(crate) mod install_job;
pub(crate) mod keyboard_router;
pub(crate) mod macos_os_integration;
pub(crate) mod main_menus;
pub(crate) mod native_event_queue;
pub(crate) mod notifications;
pub(crate) mod open_target;
pub(crate) mod process_and_constants;
pub(crate) mod source_code_server_port_holder;
pub(crate) mod source_code_server_spawn;
pub(crate) mod spaceo_job;
pub(crate) mod spaceo_status;
pub(crate) mod spawn_reaped;
pub(crate) mod tool_permissions;
pub(crate) mod tool_updates;
#[cfg(target_os = "windows")]
pub(crate) mod windows_notifications;
#[cfg(target_os = "windows")]
pub(crate) mod windows_open_with;

pub(crate) use app_state_persistence::*;
pub(crate) use attention_and_menu_bar::*;
pub(crate) use cli_install::*;
pub(crate) use cli_settings_actions::*;
pub(crate) use cli_status::*;
pub(crate) use command_exec::*;
pub(crate) use cua_driver_job::*;
pub(crate) use cua_driver_skill::*;
pub(crate) use cua_driver_status::*;
pub(crate) use disclaimed_spawn::*;
pub(crate) use folder_stats_and_zmx::*;
pub(crate) use install_job::*;
pub(crate) use keyboard_router::*;
pub(crate) use macos_os_integration::*;
pub(crate) use main_menus::*;
pub(crate) use native_event_queue::*;
pub(crate) use notifications::*;
pub(crate) use open_target::*;
pub(crate) use process_and_constants::*;
pub(crate) use source_code_server_port_holder::*;
pub(crate) use source_code_server_spawn::*;
pub(crate) use spaceo_job::*;
pub(crate) use spaceo_status::*;
pub(crate) use spawn_reaped::*;
pub(crate) use tool_permissions::*;
pub(crate) use tool_updates::*;
#[cfg(target_os = "windows")]
pub(crate) use windows_notifications::*;
#[cfg(target_os = "windows")]
pub(crate) use windows_open_with::*;
