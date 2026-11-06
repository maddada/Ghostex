//! When your computer starts: start gxserver at login, also open the app, and resume the sessions
//! that were running when the computer shut down or lost power.

mod at_login;
mod boot_mark;
mod login_entry;
mod resume_after_restart;
mod settings;
mod watch;

pub(crate) use at_login::run as run_at_login;
pub(crate) use login_entry::AT_LOGIN_COMMAND;
pub(crate) use settings::ComputerStartupSettings;
pub(crate) use watch::{find_sessions_to_resume_after_restart, start_computer_startup};
