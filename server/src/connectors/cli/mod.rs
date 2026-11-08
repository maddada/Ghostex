//! A workspace's own sign-ins for command-line tools its agents use (`gh`, `gws`): signing in and
//! out, their status, and the launch flags that hand them to agents. Split by concern; the routes
//! and the launch helpers are re-exported.

mod api;
mod client;
mod launch;
mod signin;
mod tools;

pub(crate) use api::*;
pub(crate) use launch::{with_project_cli_env, with_project_cli_env_for_resume};
pub(crate) use signin::set_changed_listener;
pub(crate) use tools::status;
