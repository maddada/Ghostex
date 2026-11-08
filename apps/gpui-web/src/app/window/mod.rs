// The desktop's dialog files; the page opens a subset of their entry points.
#![allow(dead_code, unused_imports)]
pub(crate) mod add_session_link_modal;
pub(crate) mod agent_hooks_required_modal;
pub(crate) mod close_sessions_confirm_modal;
pub(crate) mod coordinator_details_modal;
pub(crate) mod create_linear_ticket_modal;
pub(crate) mod create_worktree_modal;
pub(crate) mod delayed_send_modal;
pub(crate) mod delete_worktree_modal;
pub(crate) mod export_transcript_modal;
pub(crate) mod feedback_modal;
pub(crate) mod find_prompts;
pub(crate) mod frosted_host;
pub(crate) mod modal_popover_host;
pub(crate) mod modal_window_frame;
pub(crate) mod native_modal_kit;
pub(crate) mod new_coordinator_modal;
pub(crate) mod new_gbot_modal;
pub(crate) mod new_thread_picker;
pub(crate) mod popup_dismissal;
pub(crate) mod popup_frame;
pub(crate) mod quick_access;
pub(crate) mod rename_session_modal;
pub(crate) mod rename_worktree_modal;
pub(crate) mod session_handoff_modal;
pub(crate) mod session_note_modal;
pub(crate) mod toast;
pub(crate) mod work_link_picker_modal;

pub(crate) use add_session_link_modal::*;
pub(crate) use agent_hooks_required_modal::*;
pub(crate) use close_sessions_confirm_modal::*;
pub(crate) use coordinator_details_modal::*;
pub(crate) use create_linear_ticket_modal::*;
pub(crate) use create_worktree_modal::*;
pub(crate) use delayed_send_modal::*;
pub(crate) use delete_worktree_modal::*;
pub(crate) use export_transcript_modal::*;
pub(crate) use feedback_modal::*;
pub(crate) use find_prompts::*;
pub(crate) use modal_window_frame::*;
pub(crate) use native_modal_kit::*;
pub(crate) use new_coordinator_modal::*;
pub(crate) use new_gbot_modal::*;
pub(crate) use new_thread_picker::*;
pub(crate) use rename_session_modal::*;
pub(crate) use rename_worktree_modal::*;
pub(crate) use session_handoff_modal::*;
pub(crate) use session_note_modal::*;
pub(crate) use toast::*;
pub(crate) use work_link_picker_modal::*;

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

/// The Security upgrade notice's size (desktop `window/security_upgrade_modal.rs`), which the
/// shared `model/app_modal_kind.rs` names; the page never opens that window (it reads Settings
/// widgets the web build does not compile), so only the numbers come here.
pub(crate) const SECURITY_UPGRADE_MODAL_WIDTH: f32 = 640.0;
pub(crate) const SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT: f32 = 520.0;
