//! A workspace's own sign-ins for command-line tools its agents use (`gh`, `gws`): signing in and
//! out, their status, and the launch flags that hand them to agents. Split by concern; the routes
//! and the launch helpers are re-exported.

mod api;
mod client;
mod launch;
mod signin;
mod tools;

pub(crate) use api::*;
pub(crate) use launch::{
    project_cli_env_for_acp, project_cli_env_for_acp_resume, with_project_cli_env,
    with_project_cli_env_for_resume,
};
pub(crate) use signin::{
    cancel_machine_gh_login, machine_gh_login, set_changed_listener, sign_in_machine_gh,
    MachineGhLogin,
};
pub(crate) use tools::status;
