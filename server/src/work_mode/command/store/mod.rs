//! Each workspace's ticket command: the record gxserver keeps on this computer (`document.rs`)
//! and every change to it, made under one writer lock (`changes.rs`), its copy in memory for
//! readers that do no I/O (`snapshot.rs`), the interned ids (`ids.rs`) and its summary
//! (`summary.rs`).

mod changes;
mod document;
mod ids;
mod snapshot;
mod summary;

pub(crate) use changes::*;
pub(crate) use document::*;
pub(crate) use ids::*;
pub(crate) use snapshot::*;
pub(crate) use summary::*;

use super::*;
