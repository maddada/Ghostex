//! The Beads view (the side panel's view that was the Kanban): the Tree layout, the header both
//! layouts share, the details panel and the new-bead form, drawn natively. The Board layout is
//! `native_kanban`, drawn by the app under this view's header. The GPUI web build compiles the
//! same folder.

mod cards;
mod create;
mod details;
mod host;
mod layout;
mod links;
mod menus;
mod render;
mod tree;
mod view;

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
pub(crate) use view::BeadsLayout;
pub(crate) use view::BeadsView;
