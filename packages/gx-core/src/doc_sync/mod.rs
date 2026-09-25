//! One guard for every client-owned document that is write-through-synced to gxserver.
//!
//! `document` is what a document must be able to do and the three things that differ between them;
//! `sync` is the guard itself; `trigger` is when a host has to ask it, which is not the same as
//! the store reporting a change for a document the host seeded into the store itself. The workspace session groups document, the project collections
//! document and the Spaces document are all instances of it, so a fix to the pending-push window
//! lands in one place rather than in three copies that drift.

mod document;
mod sync;
mod trigger;

pub use document::{EmptyEchoRule, SyncPolicy, SyncedDocument};
pub use sync::{AdoptOutcome, DocumentSync, SyncEffect};
pub use trigger::document_reconcile_wanted;
