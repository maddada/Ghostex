// C1 wave-2: window-entity types moved verbatim out of main.rs, grouped by
// window. Each submodule is glob-re-exported here so every existing
// unqualified call site in main.rs (and in these modules themselves, via
// `use crate::app::window::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead.
pub(crate) mod account_usage;
pub(crate) mod agent_hooks_required_modal;
pub(crate) mod copied_indicator;
pub(crate) mod frosted_host;
pub(crate) mod create_worktree_modal;
pub(crate) mod delayed_send_modal;
pub(crate) mod delete_worktree_modal;
pub(crate) mod export_transcript_modal;
pub(crate) mod extension_titlebar_panel;
pub(crate) mod missing_project_folder_modal;
pub(crate) mod modal_host;
pub(crate) mod native_modal_kit;
pub(crate) mod new_thread_picker;
pub(crate) mod popup_frame;
pub(crate) mod portless_setup_modal;
pub(crate) mod quick_access;
pub(crate) mod remote_gxserver_install_modal;
pub(crate) mod remote_setup_modal;
pub(crate) mod remote_sites;
pub(crate) mod rename_session_modal;
pub(crate) mod rename_worktree_modal;
mod resources_style;
pub(crate) mod session_note_modal;
pub(crate) mod space_editor_modal;
mod titlebar_notifications_panel;
pub(crate) mod titlebar_panels;
pub(crate) mod titlebar_popup_chrome;
pub(crate) mod toast;
pub(crate) mod update_available_modal;

pub(crate) use agent_hooks_required_modal::*;
pub(crate) use create_worktree_modal::*;
pub(crate) use delayed_send_modal::*;
pub(crate) use delete_worktree_modal::*;
pub(crate) use export_transcript_modal::*;
pub(crate) use extension_titlebar_panel::*;
pub(crate) use missing_project_folder_modal::*;
pub(crate) use modal_host::*;
pub(crate) use native_modal_kit::*;
pub(crate) use new_thread_picker::*;
pub(crate) use portless_setup_modal::*;
pub(crate) use remote_gxserver_install_modal::*;
pub(crate) use remote_setup_modal::*;
pub(crate) use rename_session_modal::*;
pub(crate) use rename_worktree_modal::*;
pub(crate) use session_note_modal::*;
pub(crate) use space_editor_modal::*;
pub(crate) use titlebar_panels::*;
pub(crate) use titlebar_popup_chrome::*;
pub(crate) use toast::*;
pub(crate) use update_available_modal::*;
