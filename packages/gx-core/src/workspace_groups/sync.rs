//! The workspace session groups document as an instance of the shared guard.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! The guard this document needs is the same guard the project collections document and the Spaces
//! document need, so it lives in `crate::doc_sync` and this file only says how THIS document
//! answers the three questions that differ: it stores the same shape it pushes and removes the key
//! when the document is empty; an empty server document is never adopted over a non-empty local one
//! (a server that has never been written looks exactly like one that was emptied, and adopting it
//! would delete every group the user has); and a malformed echo is an EMPTY document rather than no
//! document at all, which is what `parseGpuiWorkspaceSessionGroupsState` does on purpose so a
//! corrupt key is harmless. Supersedes nothing: this is where the guard's own text used to be, and
//! it moved rather than changed, which the guard gate proves by reporting the same numbers.
//!
//! SEE-ALSO: packages/gx-core/src/doc_sync/sync.rs,
//! apps/desktop/src/app/gx_store/workspace_groups.rs.

use serde_json::Value;

use crate::doc_sync::{DocumentSync, EmptyEchoRule, SyncPolicy, SyncedDocument};

use super::document::WorkspaceGroupsDocument;

/// `GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS`.
pub const WORKSPACE_GROUPS_SYNC_DELAY_MS: u64 = 400;
/// `GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS`.
pub const WORKSPACE_GROUPS_SYNC_RETRY_DELAY_MS: u64 = 5_000;

/// What the host must do after a call into the guard.
pub type WorkspaceGroupsEffect = crate::doc_sync::SyncEffect;

/// The document plus everything needed to decide whether an echo may be applied.
pub type WorkspaceGroupsSync = DocumentSync<WorkspaceGroupsDocument>;

impl SyncedDocument for WorkspaceGroupsDocument {
    fn policy() -> SyncPolicy {
        SyncPolicy {
            delay_ms: WORKSPACE_GROUPS_SYNC_DELAY_MS,
            retry_delay_ms: WORKSPACE_GROUPS_SYNC_RETRY_DELAY_MS,
            empty_echo: EmptyEchoRule::AlwaysPushBack,
            stores: true,
        }
    }

    /// Always a document. `parseGpuiWorkspaceSessionGroupsState` answers an empty one for anything
    /// that is not an object, which is what makes a corrupt key harmless, so this never refuses.
    fn parse_echo(value: &Value) -> Option<Self> {
        Some(Self::parse(value))
    }

    fn to_wire(&self) -> Value {
        self.to_json()
    }

    /// `writeStoredGpuiWorkspaceSessionGroupsState`, which REMOVES the key for an empty document
    /// rather than storing an empty object.
    fn to_storage(&self) -> Option<Value> {
        (!self.is_empty()).then(|| self.to_json())
    }

    fn is_empty(&self) -> bool {
        WorkspaceGroupsDocument::is_empty(self)
    }
}
