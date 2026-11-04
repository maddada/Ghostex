//! The workarea header: the row at the top of the workspace column that took over
//! the deleted titlebar's job in the 2026-09-20 titlebarless revamp.
//!
//! Rust allows inherent impl blocks in any module of the crate that owns the type, so each file
//! below is a plain `impl GhostexGpuiApp { .. }` slice and needs no re-export; declaring the module
//! here is enough for its methods to stay callable from every sibling. Phases 3 and 4 of the
//! revamp add to this directory, so keep new header concerns in their own file rather than here.
pub(crate) mod action_buttons;
pub(crate) mod anchor;
pub(crate) mod band;
pub(crate) mod box_chip;
pub(crate) mod breadcrumb;
pub(crate) mod claude_cloud_button;
pub(crate) mod overlap;
pub(crate) mod session_details_button;
pub(crate) mod session_links;
pub(crate) mod shell;
pub(crate) mod toggles;

pub(crate) use anchor::*;
