//! The Beads view (the side panel's view that was the Kanban): the Tree layout, the header both
//! layouts share, the details panel and the new-bead form, drawn natively. The Board layout is
//! `native_kanban`, drawn by the app under this view's header. The GPUI web build compiles the
//! same folder.

mod cards;
mod create;
mod details;
mod host;
mod menus;
mod render;
mod tree;
mod view;

pub(crate) use view::{BeadsLayout, BeadsView};
