#[cfg(windows)]
pub(crate) mod desktop_session;
#[cfg(windows)]
mod desktop_shell;
pub(crate) mod git_program;
#[cfg(windows)]
pub(crate) mod launch_context;
pub(crate) mod live_path;
pub(crate) mod process;
#[cfg(windows)]
pub(crate) mod process_files;
pub mod resources;
pub mod shell;
#[cfg(windows)]
pub(crate) mod standard_user;
