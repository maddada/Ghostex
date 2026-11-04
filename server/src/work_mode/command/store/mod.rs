//! Each workspace's ticket command: the record gxserver keeps on this computer (`document.rs`)
//! and every change to it, made under one writer lock (`changes.rs`), its copy in memory for
//! readers that do no I/O (`snapshot.rs`), the interned ids (`ids.rs`), its summary
//! (`summary.rs`) and the command as its workspace's tracker (`tracker.rs`).

mod changes;
mod credential_save;
mod document;
mod ids;
mod snapshot;
mod summary;
mod tracker;

pub(crate) use changes::*;
pub(crate) use credential_save::*;
pub(crate) use document::*;
pub(crate) use ids::*;
pub(crate) use snapshot::*;
pub(crate) use summary::*;
pub(crate) use tracker::*;

use super::*;
