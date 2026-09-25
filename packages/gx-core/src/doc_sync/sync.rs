//! The pending-push guard: why a local move does not jump back and then forward.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! A client-owned document is edited locally and pushed to gxserver as a DEBOUNCED write-through,
//! so between the edit and the push there is a window in which the daemon still holds the old
//! document and can echo it. Applying that echo is the oscillation the user sees as a row jumping
//! back and then forward: the local move lands, the stale echo undoes it, the push lands, the
//! daemon's next echo redoes it. The guard is one flag, and it is the whole feature: while a push
//! is outstanding or has failed, an echo is IGNORED rather than merged, because the local document
//! is newer by construction and the server's is the copy that is behind.
//!
//! **The pending flag is cleared by REVISION, not by identity.** The TypeScript captures the
//! document by reference and clears the flag only if `this.workspaceGroups === pushed` (and
//! `this.latestSidebarProjectCollectionsUpdate === pushed`, and the Spaces one), which is the
//! object-identity way of asking "did anything change while that push was in flight". A port that
//! compared `Arc` addresses would be the fourth pointer-identity key this port has rejected, and it
//! would also be wrong the moment a document is rebuilt with equal contents. Every edit bumps a
//! counter instead, the push carries the counter it left with, and the answer that clears the flag
//! is `pushed_revision == revision`.
//!
//! **A failed push retries for ever, and keeps the flag up while it does.** That is deliberate in
//! the TypeScript and preserved here: a document that cannot reach the server is still the newest
//! one, so an echo must not be allowed to overwrite it just because the network is down.
//!
//! **Two echoes are not ignored, even when nothing is pending.** An echo that leaves the held
//! document where it is does nothing at all (no storage write, no redraw), and an EMPTY server
//! document may be pushed back rather than adopted, per [`EmptyEchoRule`]. That second one is why
//! "ignore the echo while pending" is not the whole rule: the empty case is how a fresh server
//! learns the client's document.
//!
//! SEE-ALSO: packages/gx-core/src/doc_sync/document.rs,
//! apps/desktop/src/app/gx_store/workspace_groups.rs (the TypeScript twin,
//! `gxserver-runtime/workspace-groups-sync.ts`, was deleted with QuickJS on 2026-09-25).

use serde_json::Value;

use super::document::{EmptyEchoRule, SyncPolicy, SyncedDocument};

/// What the host must do after a call into the guard. Every one of these is an edge the core
/// cannot have: a clock, a client-storage write, a socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncEffect {
    /// Write the document to the stored key, or REMOVE the key when `remove` is set, which is what
    /// `writeStoredGpuiWorkspaceSessionGroupsState` does with an empty state.
    WriteStorage { document: Value, remove: bool },
    /// Book the push for this many milliseconds from now, replacing any booking already made.
    SchedulePush { delay_ms: u64 },
}

/// What an echo from the daemon did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptOutcome {
    /// There was no echo to judge: `serverState === undefined`, which the TypeScript returns on
    /// before the guard is asked at all.
    ///
    /// Its own outcome since 2026-09-21 rather than the `IgnoredPending` it used to share. The host
    /// maps outcomes straight onto counters, so folding "nothing arrived" into "the guard refused
    /// an echo" made `echoesRefused`, the one number that says the guard is doing its job, count
    /// the guard never being asked.
    NoEcho,
    /// Something arrived and it was not a document: `parseSidebarSpacesFromGxserver` answering
    /// `undefined`, whose caller returns without touching anything. Kept apart from [`Self::NoEcho`]
    /// because the two are different failures and only one of them says the wire changed shape.
    Unparsable,
    /// A push is outstanding, so the local document is newer and the echo is dropped. This is the
    /// case the whole guard exists for.
    IgnoredPending,
    /// The echo leaves the held document exactly where it is.
    IgnoredEqual,
    /// The server has nothing and the client has something, so the client's is pushed up rather
    /// than the client's being erased.
    ScheduledPush,
    /// The server's document is newer and is taken.
    Adopted,
}

/// A document plus everything needed to decide whether an echo may be applied.
#[derive(Clone, Debug)]
pub struct DocumentSync<D: SyncedDocument> {
    document: D,
    policy: SyncPolicy,
    /// Bumped by every local edit. The push carries the value it left with.
    revision: u64,
    /// A push is booked, in flight, or has failed and is waiting to be retried.
    pending: bool,
    /// Whether a push is already booked, which is the TypeScript's `timeoutId !== undefined`: a
    /// failed push re-books only when nothing else has.
    booked: bool,
    /// Whether any echo of this run has been parsed yet, which is `adoptedCollections.has(id)`.
    seen_echo: bool,
    /// Whether this document has a VALUE rather than the default it starts as.
    ///
    /// `ui.metadata.spaces[machineId]` is `undefined` until an echo or a local edit sets it, and
    /// `runNativeProjectDrop` branches on that: a drop onto the built-in Other view proceeds
    /// against an EMPTY document and returns against a MISSING one. A `Default` document cannot
    /// tell those apart, so the guard remembers which it is holding.
    has_document: bool,
}

impl<D: SyncedDocument> Default for DocumentSync<D> {
    fn default() -> Self {
        Self::with_policy(D::policy())
    }
}

impl<D: SyncedDocument> DocumentSync<D> {
    /// An instance whose policy is not the document's default. Collections use it: the same
    /// document type stores a key for this computer and holds a remote machine's copy in memory.
    pub fn with_policy(policy: SyncPolicy) -> Self {
        Self {
            document: D::default(),
            policy,
            revision: 0,
            pending: false,
            booked: false,
            seen_echo: false,
            has_document: false,
        }
    }

    pub fn document(&self) -> &D {
        &self.document
    }

    pub fn policy(&self) -> SyncPolicy {
        self.policy
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Whether a push is booked. The host fires its timer and calls [`Self::push_started`]; a probe
    /// has to ask first, because firing a timer nobody booked is not a thing that can happen in the
    /// app and the TypeScript's `clearTimeout`/`undefined` pair says so.
    pub fn is_booked(&self) -> bool {
        self.booked
    }

    /// Whether the document held is a real one rather than the default it starts as.
    pub fn has_document(&self) -> bool {
        self.has_document
    }

    /// The state read from the stored key at startup. Not an edit: it neither bumps the revision
    /// nor schedules a push, because nothing has changed that the server does not have.
    pub fn restore(&mut self, document: D) {
        self.document = document;
        self.has_document = true;
    }

    /// A local edit: write the key, then book the debounced push.
    ///
    /// The booking REPLACES an earlier one, so a drag that moves a row five times pushes once,
    /// `delay_ms` after the last move rather than five times.
    pub fn edit(&mut self, document: D) -> Vec<SyncEffect> {
        self.document = document;
        self.has_document = true;
        self.revision = self.revision.saturating_add(1);
        self.pending = true;
        self.booked = true;
        let mut effects = self.storage_effects();
        effects.push(SyncEffect::SchedulePush {
            delay_ms: self.policy.delay_ms,
        });
        effects
    }

    /// The booked push is running now. What comes back is the document to send and the revision to
    /// hand to [`Self::push_finished`], which is what makes the flag safe to clear.
    pub fn push_started(&mut self) -> (Value, u64) {
        self.booked = false;
        (self.document.to_wire(), self.revision)
    }

    /// The push came home. `ok` false is any failure at all, including a timeout: the TypeScript's
    /// `catch` does not distinguish them and neither does this.
    ///
    /// A success clears the flag ONLY if nothing was edited while the call was in flight, which is
    /// what `this.workspaceGroups === pushed` asks. A failure leaves the flag up and re-books,
    /// unless something else already has.
    pub fn push_finished(&mut self, revision: u64, ok: bool) -> Vec<SyncEffect> {
        if ok {
            if revision == self.revision {
                self.pending = false;
            }
            return Vec::new();
        }
        if self.booked || !self.pending {
            return Vec::new();
        }
        self.booked = true;
        vec![SyncEffect::SchedulePush {
            delay_ms: self.policy.retry_delay_ms,
        }]
    }

    /// An echo from the daemon. `None` is the `serverState === undefined` case, which is not an
    /// echo at all and is ignored before the guard is even asked.
    pub fn adopt(&mut self, server_state: Option<&Value>) -> (AdoptOutcome, Vec<SyncEffect>) {
        let Some(server_state) = server_state else {
            return (AdoptOutcome::NoEcho, Vec::new());
        };
        if self.pending {
            return (AdoptOutcome::IgnoredPending, Vec::new());
        }
        let Some(parsed) = D::parse_echo(server_state) else {
            return (AdoptOutcome::Unparsable, Vec::new());
        };
        // Set BEFORE the empty branch decides, because `adoptCollections` records the adoption and
        // then decides, so the push-back itself counts as the first echo and the next one adopts.
        let first_echo = !self.seen_echo;
        self.seen_echo = true;
        self.has_document = true;
        if parsed.is_empty() && !self.document.is_empty() {
            let push_back = match self.policy.empty_echo {
                EmptyEchoRule::AlwaysPushBack => true,
                EmptyEchoRule::PushBackFirstEcho => first_echo,
                EmptyEchoRule::Adopt => false,
            };
            if push_back {
                // The server has nothing and the user has something: push, never adopt. Adopting
                // here would delete the lot, because a server that has not been written yet looks
                // exactly like a server that was emptied.
                self.pending = true;
                self.booked = true;
                return (
                    AdoptOutcome::ScheduledPush,
                    vec![SyncEffect::SchedulePush {
                        delay_ms: self.policy.delay_ms,
                    }],
                );
            }
        }
        let merged = D::adopt(&self.document, parsed);
        if merged == self.document {
            return (AdoptOutcome::IgnoredEqual, Vec::new());
        }
        self.document = merged;
        // NOT a revision bump: this document came FROM the server, so there is nothing to push back
        // and bumping would make the next push's success fail to clear its own flag.
        (AdoptOutcome::Adopted, self.storage_effects())
    }

    fn storage_effects(&self) -> Vec<SyncEffect> {
        if !self.policy.stores {
            return Vec::new();
        }
        match self.document.to_storage() {
            Some(document) => vec![SyncEffect::WriteStorage {
                document,
                remove: false,
            }],
            None => vec![SyncEffect::WriteStorage {
                document: Value::Null,
                remove: true,
            }],
        }
    }
}
