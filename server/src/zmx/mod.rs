mod codex_wrapper;
pub mod endpoint;
pub mod env;
#[cfg(not(windows))]
mod fish_startup;
mod grok_startup;
#[cfg(unix)]
pub(crate) mod ipc_frame;
pub mod launch;
#[cfg(test)]
mod omp_identity_tests;
pub mod probe_cache;
mod process_context;
pub mod process_identity;
#[cfg(target_os = "macos")]
mod process_table_macos;
pub mod provider;
mod read_text_suggestion;
pub mod screen_capture;
pub mod scripts;
#[cfg(windows)]
pub(crate) mod scripts_windows;
#[cfg(windows)]
pub(crate) use scripts_windows::*;
pub mod session_glue;
#[cfg(test)]
mod tests;
pub mod types;
pub mod wire_cycle;
#[cfg(not(windows))]
mod zsh_startup;

pub use endpoint::*;
pub(crate) use env::*;
pub(crate) use launch::*;
pub use probe_cache::*;
pub(crate) use process_identity::*;
pub use provider::*;
pub(crate) use screen_capture::*;
pub(crate) use scripts::*;
pub(crate) use session_glue::*;
pub use types::*;
pub(crate) use wire_cycle::*;
