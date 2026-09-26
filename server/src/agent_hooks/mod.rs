pub mod api;
pub(crate) mod claude_retention;
pub(crate) mod codex_status_line;
pub(crate) mod codex_trust;
pub mod config;
pub(crate) mod cursor_statusline;
pub mod event_mapping;
mod hook_store;
pub mod install;
pub mod notify_runtime;
pub mod plugin_sources;
pub(crate) mod opencode_v2;
mod probe_cache;
pub mod probing;
pub mod resolution;
pub mod statusline;
#[cfg(test)]
mod tests;
#[cfg(windows)]
pub(crate) mod windows;
mod zcode;

pub use api::{
    install_agent_hooks, read_agent_hook_status, repair_installed_agent_hook_paths,
    uninstall_agent_hooks,
};
pub use notify_runtime::run_notify_hook;
pub(crate) use resolution::read_claude_hook_surface_records;
pub(crate) use resolution::read_codex_hook_session_identities;
pub use statusline::run_statusline_hook;
