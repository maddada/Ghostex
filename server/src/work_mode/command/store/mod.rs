//! Each workspace's ticket command: the record gxserver keeps on this computer (`document.rs`)
//! and every change to it, made under one writer lock (`changes.rs`), and its summary
//! (`summary.rs`).

mod changes;
mod document;
mod summary;

pub(crate) use changes::*;
pub(crate) use document::*;
pub(crate) use summary::*;

use super::*;
