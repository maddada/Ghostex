//! The desktop's native board (`apps/desktop/src/app/native_kanban/`), compiled here file by file:
//! the Beads view's Board layout. The desktop's `host.rs` (its project snapshot, panel focus and
//! context menu windows), `window.rs` (Open in New Window) and `install_beads.rs` (the managed
//! tools install) are left out; `host` and `window` below name the page's answers in
//! `web_host/kanban.rs`.
pub(crate) mod actions;
pub(crate) mod beads;
pub(crate) mod blocker_edits;
pub(crate) mod blocker_panel;
pub(crate) mod blockers;
pub(crate) mod board_scroll;
pub(crate) mod card;
pub(crate) mod columns;
pub(crate) mod conversation;
pub(crate) mod data;
pub(crate) mod derived;
pub(crate) mod filters;
pub(crate) mod forms;
pub(crate) mod lane;
pub(crate) mod model;
pub(crate) mod mutations;
pub(crate) mod notice;
pub(crate) mod palette;
pub(crate) mod panel;
pub(crate) mod render;
pub(crate) mod state;
pub(crate) mod text;
pub(crate) mod toolbar;
pub(crate) mod view;
pub(crate) mod widgets;

/// The desktop's `host.rs` names, answered by the page.
pub(crate) mod host {
    pub(crate) use crate::app::web_host::kanban::{
        GpuiContextMenu, ProjectBoardBridgeRuntimeContext, skeleton_pulse,
    };
}

/// The desktop's `window.rs` type the board's state names; the page never opens the board's own
/// window, so it is never made.
pub(crate) mod window {
    pub(crate) use crate::app::web_host::kanban::KanbanDetachedWindow;
}
