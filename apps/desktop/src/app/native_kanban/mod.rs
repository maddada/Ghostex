//! The desktop Kanban view, drawn natively: the Beads view's Board layout. The entry points are
//! `render_native_kanban_embedded` (under the Beads header) and `render_native_kanban` (the board's
//! own window) in `render.rs`. `host.rs`, `window.rs` and `install_beads.rs` are the desktop's
//! own; the GPUI web build compiles the rest (`apps/gpui-web/src/app/native_kanban/`).

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
pub(crate) mod host;
pub(crate) mod install_beads;
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
pub(crate) mod window;
