mod ancestor_session;
pub mod api;
mod claude_daemon;
pub(crate) mod claude_retention;
pub(crate) mod codex_status_line;
pub(crate) mod codex_trust;
pub mod config;
pub(crate) mod cursor_statusline;
pub mod event_mapping;
mod foreign_owner;
mod hermes_gateway;
mod hook_store;
pub mod install;
pub(crate) mod nested_agent;
pub mod notify_runtime;
pub(crate) mod on_demand;
pub mod plugin_sources;
mod probe_cache;
pub mod probing;
pub mod resolution;
pub mod statusline;
#[cfg(test)]
mod tests;
#[cfg(windows)]
pub(crate) mod windows;
#[cfg(any(windows, test))]
mod wmx_pane;
mod zcode;

pub(crate) use ancestor_session::adopt_ancestor_session_routing;
pub(crate) use ancestor_session::zmx_session_owning_agent;
pub use api::{
    install_agent_hooks, read_agent_hook_status, repair_installed_agent_hook_paths,
    uninstall_agent_hooks,
};
pub(crate) use hook_store::write_claude_surface_record;
pub use notify_runtime::run_notify_hook;
pub(crate) use resolution::read_claude_hook_surface_records;
pub(crate) use resolution::read_codex_hook_session_identities;
pub use statusline::run_statusline_hook;
#[cfg(windows)]
pub(crate) use wmx_pane::claude_record_process_running;
pub(crate) use zcode::zcode_settings_files;
