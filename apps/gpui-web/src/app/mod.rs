//! CDXC:WebGpui 2026-09-22 WHY: the entries in this folder that are symlinks ARE the desktop app's source files, compiled here unchanged. Symlinks rather than `#[path]` attributes because rustc resolves the nested `mod` lines of a `#[path]`-loaded file as if it were a `mod.rs`, which breaks every desktop file that has a sibling directory (`consts.rs` + `consts/`). Real files in this folder are the web replacements for the desktop's native-only halves.
mod agent_hooks_required_modal_lifecycle;
pub(crate) mod chat_host;
mod create_worktree_modal_lifecycle;
mod delayed_send_modal_lifecycle;
mod delete_worktree_modal_lifecycle;
/// The desktop's constants, of which the page reads a subset.
#[allow(dead_code)]
pub(crate) mod consts;
pub(crate) mod element;
pub(crate) mod floating_reveal;
pub(crate) mod gx_chat;
pub(crate) mod gx_store;
pub(crate) mod helpers;
pub(crate) mod hotkeys;
pub(crate) mod model;
pub(crate) mod native_chat;
pub(crate) mod native_sidebar;
#[allow(dead_code, unused_imports)]
pub(crate) mod native_app_modal_lifecycle {
    use crate::app::helpers::*;
    use crate::app::window::*;
    use crate::*;
    use gpui::{AnyEntity, WindowHandle};
    use gpui_component::Root;
    include!(concat!(env!("OUT_DIR"), "/native_app_modal_lifecycle.rs"));
}
#[allow(dead_code, unused_imports)]
pub(crate) mod project_views {
    include!(concat!(env!("OUT_DIR"), "/project_views.rs"));
}
pub(crate) mod quick_access;
mod quick_access_modal_lifecycle;
pub(crate) mod remote_conn;
pub(crate) mod render;
mod rename_session_modal_lifecycle;
mod rename_worktree_modal_lifecycle;
mod session_note_modal_lifecycle;
#[allow(dead_code)]
pub(crate) mod sidebar_direct_focus {
    include!(concat!(env!("OUT_DIR"), "/sidebar_direct_focus.rs"));
}
pub(crate) mod terminal_host;
pub(crate) mod titlebar;
pub(crate) mod web_app;
pub(crate) mod web_host;
pub(crate) mod window;
