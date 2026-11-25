//! Run in terminal for the commands Ghostex shows a user (the Linux update command first).
mod registry;
mod terminal;

pub(crate) use registry::LINUX_UPDATE;
pub(crate) use terminal::{
    handle_open_shown_command_terminal_http, OPEN_SHOWN_COMMAND_TERMINAL_ENDPOINT,
};
