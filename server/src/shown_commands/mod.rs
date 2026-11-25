//! Run in terminal for the commands Ghostex shows a user (the Linux update command first).
mod installs;
mod registry;
mod terminal;

pub(crate) use installs::{failed_agent_cli_job_id, failed_tool_job_id};
pub(crate) use registry::{
    BUN_UPGRADE, CLAUDE_SWAP_INSTALL, CODEX_SWAP_INSTALL, GH_AUTH_LOGIN, GH_PROJECTS_SCOPE,
    GLAB_AUTH_LOGIN, GWS_AUTH_SETUP, LINUX_UPDATE, SSH_SERVER, SYSTEM_TOOLS_INSTALL,
};
pub(crate) use terminal::{
    handle_open_shown_command_terminal_http, OPEN_SHOWN_COMMAND_TERMINAL_ENDPOINT,
};
