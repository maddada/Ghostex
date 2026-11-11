//! The Beads view's side of gxserver: one cache per Beads workspace that every Ghostex bd call
//! goes through, the `/api/beadsView` snapshot, Pull and Push, and title generation for a bead
//! created without a title.

mod cache;
mod change;
mod discovery;
mod log;
mod reads;
mod snapshot;
mod sync;
mod view;

pub(crate) use reads::run_beads_action_in_workspace;
pub(crate) use view::{beads_view_snapshot, generate_bead_title};
