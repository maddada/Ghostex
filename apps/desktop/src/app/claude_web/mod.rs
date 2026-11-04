//! Claude, a built-in extension: claude.ai opened from the sidebar's bottom bar in place of the
//! sessions column, on the picked saved Claude account's own claude.ai sign-in. Per-concern files;
//! this barrel stays thin.
mod accounts;
mod column;
mod header;
mod sidebar_button;
mod takeover;
mod view;

pub(crate) use accounts::*;
pub(crate) use header::*;
pub(crate) use takeover::*;
pub(crate) use view::*;
