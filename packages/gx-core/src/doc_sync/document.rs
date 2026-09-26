//! What a client-owned document has to be able to do for one guard to hold it.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! Three documents in this app have the SAME synchroniser: the workspace session groups document
//! (K4), the project collections document and the Spaces document. Each is edited locally, written
//! to a stored key (the Spaces document is the exception and has none), and pushed to gxserver as a
//! debounced write-through with an indefinite retry, and each refuses the daemon's echo while its
//! push is outstanding. The TypeScript this replaced wrote that machine out three times,
//! `queueSidebarProjectCollectionsServerSync` / `pushSidebarProjectCollectionsToGxserver` /
//! `forwardSidebarProjectCollectionsFromGxserver` being a copy of the Spaces trio being a copy of
//! the workspace-groups trio, and this port has hit "a fix in one function left its twin unfixed"
//! five times already. (The two project trios were deleted as dead code on 2026-09-21 and are
//! frozen in tooling/gx-core/project-docs-server-sync-typescript.ts.) So the guard is ONE generic, and what really differs between the three is
//! named here as data rather than duplicated as code:
//!
//! - **Whether a stored key is written at all**, and what it holds. The workspace groups document
//!   stores the same shape it pushes; the collections document stores an ORDERED ARRAY where it
//!   pushes a MAP; the Spaces document stores nothing, because gxserver owns it outright.
//! - **What an empty server document means.** The workspace groups guard never adopts one over a
//!   non-empty local document, because a server that has never been written looks exactly like one
//!   that was emptied. `adoptCollections` makes the same decision but only for the FIRST echo of a
//!   run and only for this computer. The Spaces document makes it never.
//! - **What an adopted echo becomes.** The collections document carries `nextCollectionNumber`
//!   forward monotonically (`Math.max(parsed, previous)`), so a server that has forgotten the
//!   counter cannot make the next new group reuse a name that is on screen. The other two adopt
//!   what arrived.
//! - **Whether a malformed echo is a document at all.** `parseSidebarProjectCollectionsFromGxserver`
//!   and `parseSidebarSpacesFromGxserver` answer `undefined` and their callers return;
//!   `parseGpuiWorkspaceSessionGroupsState` answers an EMPTY document on purpose, so a corrupt key
//!   is harmless. Folding those two together would make a damaged Spaces echo read as "the server
//!   has nothing", which is the one input the empty rule exists to distinguish.
//!
//! SEE-ALSO: packages/gx-core/src/doc_sync/sync.rs,
//! apps/desktop/src/app/gx_store/workspace_groups.rs (the TypeScript twin,
//! `gxserver-runtime/workspace-groups-sync.ts`, was deleted with QuickJS on 2026-09-25).

use serde_json::Value;

/// What an empty server document does when the client holds a non-empty one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyEchoRule {
    /// Never adopt it; push the client's document up instead. The workspace session groups
    /// document, where adopting would delete every group the user has.
    AlwaysPushBack,
    /// Push back only on the first echo of the run, and adopt every later one. The project
    /// collections document on this computer (`adoptCollections`, `firstAdoption`).
    PushBackFirstEcho,
    /// Adopt it like any other document. The Spaces document, and any document of a machine whose
    /// copy this app does not seed.
    Adopt,
}

/// The three things that differ between the documents this guard holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncPolicy {
    /// How long an edit waits before it is pushed, replacing any earlier booking.
    pub delay_ms: u64,
    /// How long a FAILED push waits before it is tried again. It never gives up.
    pub retry_delay_ms: u64,
    pub empty_echo: EmptyEchoRule,
    /// Whether this instance writes a stored key. A remote machine's collections are held in
    /// memory only, exactly as `saveNativeCollections` writes the key for `local` alone.
    pub stores: bool,
}

/// A document the guard can hold.
pub trait SyncedDocument: Clone + Default + PartialEq {
    /// The default policy for this document. An instance may be built with another one
    /// ([`super::DocumentSync::with_policy`]) where the machine decides, which is collections.
    fn policy() -> SyncPolicy;

    /// An echo from the daemon. `None` is "this is not a document", which is what
    /// `parseSidebarProjectCollectionsFromGxserver` and `parseSidebarSpacesFromGxserver` answer for
    /// a value that is not an object or carries no map, and what their callers return on.
    fn parse_echo(value: &Value) -> Option<Self>;

    /// The payload the push carries as its `state`.
    fn to_wire(&self) -> Value;

    /// What the stored key holds. `None` REMOVES the key, which is what
    /// `writeStoredGpuiWorkspaceSessionGroupsState` does with an empty document. Only consulted
    /// when [`SyncPolicy::stores`] is set.
    fn to_storage(&self) -> Option<Value>;

    /// Whether the document holds nothing, which is what the empty-echo rule is asked about.
    fn is_empty(&self) -> bool;

    /// What an adopted echo becomes, given what is currently held. The identity for every document
    /// but collections, which carries its monotonic counter forward.
    fn adopt(held: &Self, parsed: Self) -> Self {
        let _ = held;
        parsed
    }
}
