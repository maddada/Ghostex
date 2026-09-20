// C1 repo-restructure split: home for main.rs content extracted out of the
// monolithic file. Wave 1 populated `helpers` (stateless free fns); wave 2
// added `window` (window entities) and `element` (gpui Element impls); wave 3
// added `actions`, `hotkeys`, `ffi`, `consts`, and `model` (Region A types and
// sub-models).
//
// Wave 4 moved the `GhostexGpuiApp` god object itself: `core` owns the struct
// (all 301 fields `pub(crate)`) plus the `Drop`/`EntityInputHandler`/`Render`
// impls, and every module below it holds one slice of the former 53k-line
// inherent `impl GhostexGpuiApp` block. Rust allows inherent impl blocks in any
// module of the crate that owns the type, so those are plain moves; the only
// edit is the `pub(crate) ` prefix each moved method needs to stay callable
// from its siblings.
pub(crate) mod actions;
pub(crate) mod consts;
pub(crate) mod context_menu;
pub(crate) mod core;
pub(crate) mod element;
pub(crate) mod extensions;
pub(crate) mod ffi;
pub(crate) mod helpers;
pub(crate) mod hotkeys;
pub(crate) mod model;
pub(crate) mod remote_browser;
pub(crate) mod window;

pub(crate) mod agent_hooks_required_modal_lifecycle;
pub(crate) mod app_new;
pub(crate) mod browser_history;
pub(crate) mod browser_pane;
pub(crate) mod browser_parked_runtime;
pub(crate) mod cef_deferred_startup;
pub(crate) mod chrome_input_focus;
pub(crate) mod command_pane_auto_minimize;
pub(crate) mod command_pane_remote_action;
#[cfg(target_os = "macos")]
pub(crate) mod companion_reveal;
pub(crate) mod create_worktree_modal_lifecycle;
pub(crate) mod delayed_send;
pub(crate) mod delayed_send_modal_lifecycle;
pub(crate) mod delayed_send_sessions;
pub(crate) mod delete_worktree_modal_lifecycle;
pub(crate) mod docs_annotation_feedback;
pub(crate) mod drag_resize;
pub(crate) mod export_transcript_modal_lifecycle;
pub(crate) mod focus;
pub(crate) mod gx_store;
pub(crate) mod keyboard_owner;
pub(crate) mod missing_project_folder_modal_lifecycle;
pub(crate) mod modals;
pub(crate) mod native_app_modal_lifecycle;
pub(crate) mod native_chat;
pub(crate) mod native_service;
pub(crate) mod native_sidebar;
pub(crate) mod new_thread_picker_lifecycle;
pub(crate) mod os_integration;
pub(crate) mod portless_setup_modal_lifecycle;
pub(crate) mod project_editor;
pub(crate) mod project_keep_alive;
pub(crate) mod project_views;
pub(crate) mod quick_access_modal_lifecycle;
pub(crate) mod remote_conn;
pub(crate) mod remote_gxserver_install_modal_lifecycle;
pub(crate) mod remote_setup_modal_lifecycle;
pub(crate) mod rename_session_modal_lifecycle;
pub(crate) mod rename_worktree_modal_lifecycle;
pub(crate) mod render;
pub(crate) mod session_chat;
mod session_chat_armed_actions;
pub(crate) mod session_chat_context_menu;
pub(crate) mod session_chat_diagnostics;
mod session_chat_draft_handoff;
pub(crate) mod session_chat_eviction;
pub(crate) mod session_chat_focus;
mod session_chat_fork_branches;
pub(crate) mod session_chat_image_save;
mod session_chat_launch;
pub(crate) mod session_chat_model_picker;
mod session_chat_presentation;
pub(crate) mod session_chat_prewarm;
mod session_chat_renderers;
mod session_chat_runtime;
pub(crate) mod session_chat_skeleton;
mod session_chat_surfaces;
pub(crate) mod session_chat_warm_pool;
pub(crate) mod session_note_modal_lifecycle;
pub(crate) mod sidebar_agent_launch_placeholder;
pub(crate) mod sidebar_direct_focus;
pub(crate) mod sidebar_dispatch;
pub(crate) mod space_editor_modal_lifecycle;
pub(crate) mod stashed_prompt_jump;
pub(crate) mod status_pet;
pub(crate) mod tab_actions;
pub(crate) mod terminal_input;
pub(crate) mod terminal_sync;
pub(crate) mod titlebar;
pub(crate) mod update_available_modal_lifecycle;
mod view_pane_state;
pub(crate) mod view_scopes;
pub(crate) mod view_skeletons;
pub(crate) mod workarea;
pub(crate) mod workspace_events;
pub(crate) mod workspace_reconcile;
pub(crate) mod workspace_terminals;
