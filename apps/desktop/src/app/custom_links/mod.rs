//! The user's own links in the sidebar's ⋯ menu (CDXC:Sidebar 2026-11-22 DECISION in
//! packages/gx-core/src/custom_links.rs): the saved links and the page they open in, opening,
//! editing and removing them, and the address bar above the page. Per-concern files; this barrel
//! stays thin.
mod commands;
mod page;
mod store;

pub(crate) use page::*;
pub(crate) use store::*;
