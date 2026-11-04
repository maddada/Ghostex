// C1 wave-4 re-cluster: app/render.rs (~7,340 lines) further divided into
// responsibility-scoped submodules, pure move, no logic changes. Rust allows
// inherent impl blocks in any module of the crate that owns the type (see
// app/mod.rs's wave-4 note), so each submodule below is a plain
// `impl GhostexGpuiApp { .. }` slice and needs no re-export: declaring the
// module here is enough for its methods to stay callable from every sibling,
// exactly as render.rs itself was declared in app/mod.rs before this split
// (that `pub(crate) mod render;` line is unchanged).
pub(crate) mod agents_workspace_layout;
pub(crate) mod browser_body_and_tabs;
pub(crate) mod browser_page_tabs;
pub(crate) mod browser_workspace_layout;
pub(crate) mod code_window_tabs;
pub(crate) mod command_pane_structure;
pub(crate) mod command_pane_tabs_controls;
pub(crate) mod command_pane_tabs_core;
pub(crate) mod command_terminal_placeholder;
pub(crate) mod project_editor_surface_and_workarea;
pub(crate) mod resize_rail;
pub(crate) mod root;
pub(crate) mod session_chat_and_drop_feedback;
pub(crate) mod sleeping_card;
pub(crate) mod terminal_agent_action_bar;
pub(crate) mod terminal_body_slot;
pub(crate) mod terminal_content_layout;
pub(crate) mod terminal_placeholders_and_editor_shell;
pub(crate) mod terminal_view_surface;
pub(crate) mod view_instance_tabs;
pub(crate) mod view_picker;
pub(crate) mod view_tab_strip;
pub(crate) mod view_tab_strip_browser_tabs;
pub(crate) mod web_runtime_prompt;
pub(crate) mod window_drag_region;
pub(crate) mod workarea_header;
pub(crate) mod workarea_split;
pub(crate) mod workspace_pane_grip;
