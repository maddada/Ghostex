pub mod api;
pub(crate) mod codex_status_line;
pub(crate) mod codex_trust;
pub mod config;
pub mod event_mapping;
pub mod install;
pub mod notify_runtime;
pub mod plugin_sources;
mod probe_cache;
pub mod probing;
pub mod resolution;
pub mod statusline;
#[cfg(test)]
mod tests;

pub use api::{
    install_agent_hooks, read_agent_hook_status, repair_installed_agent_hook_paths,
    uninstall_agent_hooks,
};
pub use notify_runtime::run_notify_hook;
pub(crate) use resolution::read_codex_hook_session_identities;
pub use statusline::run_statusline_hook;
