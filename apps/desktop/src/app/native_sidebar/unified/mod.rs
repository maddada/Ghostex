//! The Unified sidebar's renderer: the header (filter bar, Add project, New session), the session
//! cards and the folded shelves. The list itself is built by gx-core (`unified_sidebar/`) and
//! installed by the store (gx_store/unified_sidebar.rs).

mod cards;
mod filter_bar;
mod groups;
mod header;
mod hover;
mod list;
pub(crate) mod model;
mod shelves;
