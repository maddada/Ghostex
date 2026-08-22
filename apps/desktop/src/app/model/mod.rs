// C1 wave-3: Region A sub-model and value-type definitions moved verbatim
// out of main.rs. Each submodule is glob-re-exported here so every existing
// unqualified call site in main.rs (and in these modules themselves, via
// `use crate::app::model::*;`) keeps resolving without per-call-site
// qualification. If two submodules ever define the same name, drop the glob
// for one of them here and qualify its call sites instead.
//
// The former types1.rs..types6.rs chunk split was re-clustered into
// descriptively named domain modules per the FOLLOW-UPS.md note in
// docs/2026-08-22/repo-restructure/ (pure move, no logic changes).
pub(crate) mod agents_terminal_body_presentation;
pub(crate) mod agents_terminal_startup;
pub(crate) mod app_modal_kind;
pub(crate) mod browser_profiles;
pub(crate) mod browser_shell_state;
pub(crate) mod browser_tabs;
pub(crate) mod browser_toolbar_and_media;
pub(crate) mod browser_tree;
pub(crate) mod command_pane;
pub(crate) mod command_pane_action_run;
pub(crate) mod command_pane_delayed_send;
pub(crate) mod command_pane_focus_return;
pub(crate) mod command_pane_geometry;
pub(crate) mod command_pane_ids;
pub(crate) mod command_pane_shell_state;
pub(crate) mod command_pane_tab_chrome;
pub(crate) mod command_pane_tree;
pub(crate) mod command_terminal_attach;
pub(crate) mod command_terminal_session;
pub(crate) mod drag_transfer;
pub(crate) mod focus_and_keyboard;
pub(crate) mod focus_close_targets;
pub(crate) mod hotkeys_and_palette;
pub(crate) mod json_helpers;
pub(crate) mod launch_payload;
pub(crate) mod local_workspace_attach;
pub(crate) mod project_editor_placeholders;
pub(crate) mod project_editor_shell;
pub(crate) mod resize_drag_state;
pub(crate) mod runtime_state;
pub(crate) mod shell_focus_state;
pub(crate) mod shell_layout;
pub(crate) mod sidebar_bridge_messages;
pub(crate) mod sidebar_chrome;
pub(crate) mod tab_drag_preview_render;
pub(crate) mod tab_groups;
pub(crate) mod terminal_clipboard;
pub(crate) mod terminal_input_handling;
pub(crate) mod terminal_mount_slots;
pub(crate) mod terminal_parked_owner_reattach;
pub(crate) mod terminal_session_state;
pub(crate) mod terminal_surface_lifecycle;
pub(crate) mod titlebar_mode;
pub(crate) mod titlebar_panels;
pub(crate) mod windows_first_run_setup;
pub(crate) mod workspace;
pub(crate) mod workspace_shell_state;
pub(crate) mod workspace_tab_chrome;
pub(crate) mod workspace_tree;

pub(crate) use agents_terminal_body_presentation::*;
pub(crate) use agents_terminal_startup::*;
pub(crate) use app_modal_kind::*;
pub(crate) use browser_profiles::*;
pub(crate) use browser_shell_state::*;
pub(crate) use browser_tabs::*;
pub(crate) use browser_toolbar_and_media::*;
pub(crate) use browser_tree::*;
pub(crate) use command_pane::*;
pub(crate) use command_pane_action_run::*;
pub(crate) use command_pane_delayed_send::*;
pub(crate) use command_pane_focus_return::*;
pub(crate) use command_pane_geometry::*;
pub(crate) use command_pane_ids::*;
pub(crate) use command_pane_shell_state::*;
pub(crate) use command_pane_tab_chrome::*;
pub(crate) use command_pane_tree::*;
pub(crate) use command_terminal_attach::*;
pub(crate) use command_terminal_session::*;
pub(crate) use drag_transfer::*;
pub(crate) use focus_and_keyboard::*;
pub(crate) use focus_close_targets::*;
pub(crate) use hotkeys_and_palette::*;
pub(crate) use json_helpers::*;
pub(crate) use launch_payload::*;
pub(crate) use local_workspace_attach::*;
pub(crate) use project_editor_placeholders::*;
pub(crate) use project_editor_shell::*;
pub(crate) use resize_drag_state::*;
pub(crate) use runtime_state::*;
pub(crate) use shell_focus_state::*;
pub(crate) use shell_layout::*;
pub(crate) use sidebar_bridge_messages::*;
pub(crate) use sidebar_chrome::*;
#[allow(unused_imports)]
pub(crate) use tab_drag_preview_render::*;
pub(crate) use tab_groups::*;
pub(crate) use terminal_clipboard::*;
pub(crate) use terminal_input_handling::*;
pub(crate) use terminal_mount_slots::*;
pub(crate) use terminal_parked_owner_reattach::*;
pub(crate) use terminal_session_state::*;
pub(crate) use terminal_surface_lifecycle::*;
pub(crate) use titlebar_mode::*;
pub(crate) use titlebar_panels::*;
#[allow(unused_imports)]
pub(crate) use windows_first_run_setup::*;
pub(crate) use workspace::*;
pub(crate) use workspace_shell_state::*;
pub(crate) use workspace_tab_chrome::*;
pub(crate) use workspace_tree::*;
