//! Executor through the user's own Executor CLI (an open-source tool hub): Ghostex finds the
//! computer's `executor`, shows and changes its one sign-in (`executor login`, `whoami`,
//! `logout`), runs Executor's installer when there is none, and gives agents Executor's tools
//! through one `executor mcp` relay. Apps and their accounts are managed in Executor itself.

pub(super) mod agent;
mod cli;
mod legacy;
mod relay;
mod signin;
mod status;

pub(crate) use legacy::remove as remove_old_executor;
pub(crate) use relay::stop as stop_relay;
pub(crate) use signin::{cancel_sign_in, sign_in, sign_out};
pub(crate) use status::{install, set_changed_listener, status};
