//! Run in terminal for the commands Ghostex shows a user (the Linux update command first).
mod installs;
mod registry;
mod terminal;

pub(crate) use installs::{failed_agent_cli_job_id, failed_tool_job_id};
pub(crate) use registry::LINUX_UPDATE;
pub(crate) use terminal::{
    handle_open_shown_command_terminal_http, OPEN_SHOWN_COMMAND_TERMINAL_ENDPOINT,
};
