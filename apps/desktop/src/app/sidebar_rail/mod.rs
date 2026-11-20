//! The rail: a 40pt column of pinnable icons at the window's left edge, left of the sidebar, that
//! stays when the sidebar hides (`sidebarLayout` "rail"). The rules (items, default pins, pin and
//! move) are gx-core's `sidebar_rail`; this draws it and runs its icons. Per-concern files; this
//! barrel stays thin. The web build compiles the same files and supplies its own `host`.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_rail.rs (the decision), render/root.rs (where the column
//! sits), native_sidebar/navigation.rs, native_sidebar/selectors.rs and native_sidebar/usage.rs (what
//! the sidebar leaves to the rail), claude_web/place.rs (`TakeoverPage`).

#[cfg(not(ghostex_web))]
mod host;
#[cfg(not(ghostex_web))]
mod intro;
mod menu;
mod model;
mod render;

pub(crate) use model::*;
