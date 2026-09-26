//! Quick Access host glue: `host.rs` runs the gx-core Quick Access model (store, client storage and
//! clock in, effects out), `storage.rs` is the client storage it reads and writes, and
//! `commands.rs` performs the Commands rows that have no other owner.

pub(crate) mod commands;
pub(crate) mod host;
pub(crate) mod storage;
