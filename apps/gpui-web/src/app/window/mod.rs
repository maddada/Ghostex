// The desktop's dialog files; the page opens a subset of their entry points.
#![allow(dead_code, unused_imports)]
pub(crate) mod agent_hooks_required_modal;
pub(crate) mod create_worktree_modal;
pub(crate) mod delayed_send_modal;
pub(crate) mod delete_worktree_modal;
pub(crate) mod frosted_host;
pub(crate) mod native_modal_kit;
pub(crate) mod popup_frame;
pub(crate) mod quick_access;
pub(crate) mod rename_session_modal;
pub(crate) mod rename_worktree_modal;
pub(crate) mod session_note_modal;
pub(crate) mod toast;

pub(crate) use agent_hooks_required_modal::*;
pub(crate) use create_worktree_modal::*;
pub(crate) use delayed_send_modal::*;
pub(crate) use delete_worktree_modal::*;
pub(crate) use native_modal_kit::*;
pub(crate) use rename_session_modal::*;
pub(crate) use rename_worktree_modal::*;
pub(crate) use session_note_modal::*;
pub(crate) use toast::*;

/// AppKit child-window attachment on the desktop. The web platform's windows are canvases of one page, which the platform itself stacks.
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    _window: &mut gpui::Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}
#[allow(dead_code, unused_imports)]
pub(crate) mod space_editor_modal {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/space_editor_modal.rs"));
}

