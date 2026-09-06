// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules, pure move,
// no logic changes. Each submodule is glob-re-exported here so every
// existing unqualified call site in main.rs (and in the helpers themselves,
// via `use crate::app::helpers::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.
pub(crate) mod attach;
pub(crate) mod browser_tunnel;
pub(crate) mod browser_sites;
pub(crate) mod attach_terminal_process;
pub(crate) mod command_action;
pub(crate) mod config;
pub(crate) mod connect;
pub(crate) mod easy_connect_forward;
pub(crate) mod editor_daemon;
pub(crate) mod ide_open;
pub(crate) mod install;
pub(crate) mod ports;
pub(crate) mod previous_sessions;
pub(crate) mod sidebar_bridge;
pub(crate) mod sidebar_hud;
pub(crate) mod source_code_server;
pub(crate) mod ssh_exec;
pub(crate) mod ssh_process;
pub(crate) mod tunnel_and_auth;
pub(crate) mod types;
pub(crate) mod websocket_presentation;

pub(crate) use attach::*;
pub(crate) use browser_tunnel::*;
pub(crate) use browser_sites::*;
pub(crate) use attach_terminal_process::*;
pub(crate) use command_action::*;
pub(crate) use config::*;
pub(crate) use connect::*;
pub(crate) use easy_connect_forward::*;
pub(crate) use editor_daemon::*;
pub(crate) use ide_open::*;
pub(crate) use install::*;
pub(crate) use ports::*;
pub(crate) use previous_sessions::*;
pub(crate) use sidebar_bridge::*;
pub(crate) use sidebar_hud::*;
pub(crate) use source_code_server::*;
pub(crate) use ssh_exec::*;
pub(crate) use ssh_process::*;
pub(crate) use tunnel_and_auth::*;
pub(crate) use types::*;
pub(crate) use websocket_presentation::*;
