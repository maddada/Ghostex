//! Claude and Grok, built-in extensions: claude.ai or grok.com opened from the sidebar's bottom bar
//! in place of the sessions column, on the picked saved Claude account's own claude.ai sign-in or
//! the picked Grok login's own profile. Per-concern files; this barrel stays thin.
mod accounts;
mod column;
mod grok;
mod header;
mod place;
mod sidebar_button;
mod takeover;
mod view;

pub(crate) use accounts::*;
pub(crate) use grok::*;
pub(crate) use header::*;
pub(crate) use place::*;
pub(crate) use takeover::*;
pub(crate) use view::*;
