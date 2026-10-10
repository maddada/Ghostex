//! Ghostex's workspace windows: the ones the app reopens at launch and the ones File > New Window
//! opens. `registry.rs` tracks the open windows and which one runs the app-wide work, `slots.rs`
//! keeps each window's saved layout, focus and frame apart, `open.rs` opens a window, `close.rs`
//! closes one while others stay open, `routing.rs` decides which window a click, a command or
//! an app-wide change reaches, `owned_windows.rs` moves the windows a workspace window owns
//! along with it, `floating_windows.rs` closes its floating popups when it is resized,
//! `session_routing.rs` sends an open of a session or project to the window that shows its
//! workspace, and `workspace_landing.rs` keeps a window on the projects of that workspace.

mod close;
mod floating_windows;
mod open;
mod owned_windows;
mod registry;
mod routing;
mod session_routing;
mod slots;
mod window_workspace;
mod workspace_landing;

pub(crate) use open::*;
pub(crate) use registry::*;
pub(crate) use session_routing::ProjectWindowRoute;
pub(crate) use slots::*;
pub(crate) use workspace_landing::WindowWorkspaceLanding;
