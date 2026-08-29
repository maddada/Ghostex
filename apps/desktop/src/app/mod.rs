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
pub(crate) mod core;
pub(crate) mod element;
pub(crate) mod extensions;
pub(crate) mod ffi;
pub(crate) mod helpers;
pub(crate) mod hotkeys;
pub(crate) mod model;
pub(crate) mod window;

pub(crate) mod app_new;
pub(crate) mod browser_pane;
pub(crate) mod browser_parked_runtime;
pub(crate) mod command_pane_remote_action;
pub(crate) mod delayed_send;
pub(crate) mod drag_resize;
pub(crate) mod focus;
pub(crate) mod modals;
pub(crate) mod os_integration;
pub(crate) mod project_editor;
pub(crate) mod remote_conn;
pub(crate) mod render;
pub(crate) mod session_chat;
pub(crate) mod sidebar_dispatch;
pub(crate) mod stashed_prompt_jump;
pub(crate) mod status_pet;
pub(crate) mod tab_actions;
pub(crate) mod terminal_input;
pub(crate) mod terminal_sync;
pub(crate) mod titlebar;
pub(crate) mod workarea;
pub(crate) mod workspace_events;
pub(crate) mod workspace_reconcile;
pub(crate) mod workspace_terminals;
