// C1 wave-4 re-cluster: app/terminal_sync.rs (~5,603 lines) further divided
// into responsibility-scoped submodules, pure move, no logic changes. Rust
// allows inherent impl blocks in any module of the crate that owns the type
// (see app/mod.rs's wave-4 note), so each submodule below is a plain
// `impl GhostexGpuiApp { .. }` slice and needs no re-export: declaring the
// module here is enough for its methods to stay callable from every sibling,
// exactly as terminal_sync.rs itself was declared in app/mod.rs before this
// split (that `pub(crate) mod terminal_sync;` line is unchanged).
pub(crate) mod agents_terminal_surface_sync;
pub(crate) mod cef_and_command_terminal_focus;
pub(crate) mod companion_terminal_surface_sync;
pub(crate) mod gpui_engine_terminal_attachment;
pub(crate) mod gpui_engine_terminal_sync;
pub(crate) mod gpui_engine_terminal_visibility;
pub(crate) use gpui_engine_terminal_visibility::*;
pub(crate) mod prompt_editor;
pub(crate) mod terminal_search;
pub(crate) mod terminal_surface_host_sync;
pub(crate) mod workspace_dispatch_and_focus;
pub(crate) mod workspace_terminal_dispatch;
