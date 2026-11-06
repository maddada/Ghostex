//! Updating this computer's Ghostex app from another device (`ghostex app-update`, `/api/appUpdate*`).
//!
//! CDXC:RemoteMachines 2026-10-16 DECISION:
//! User (#245, Discord): update Ghostex on a remote machine from the computer you are on instead of opening its screen to click Update and Restart, and offer a "keep this machine up to date" switch. gxserver owns the verb so every client (the desktop, the `ghostex` CLI, later the phone) asks the same thing; the install itself is the app's own updater on that machine (Sparkle on macOS, Velopack on Windows, also for a Windows computer whose sessions run in WSL), reached through the renderer command the app already answers, so the download, signature checks and relaunch stay on the path a local Update and Restart takes. Linux packages only announce a new version, as on the desktop.
//! SEE-ALSO: apps/desktop/src/app/os_integration/app_update_remote_control.rs (the app's answer), apps/desktop/src/app/remote_conn/app_update.rs (the remote Update button), packages/gx-core/src/renderer_commands/verbs.rs (`appUpdate`).
mod auto;
mod http;
mod record;
mod status;

pub(crate) use auto::spawn_app_auto_update_task;
pub(crate) use http::{
    current_app_update_status, handle_app_update_http, request_app_update_install,
    set_app_auto_update,
};
