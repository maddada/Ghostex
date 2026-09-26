//! The project collections document: the coloured "Group N" folders, and the counter that names
//! the next one.
//!
//! CDXC:Projects 2026-09-21 WHY:
//! This document was "checked and NOT a fifth key" for as long as nothing in Rust wrote it: the
//! daemon stores it, the sidebar page held an overlay, and the app only read the copy that arrived.
//! The project moves change that, so it is a client-owned document here now, with the same three
//! edges every such document has (a stored key, a debounced push, an echo the guard judges) and one
//! the others do not: **`nextCollectionNumber` is an in-memory MONOTONIC overlay**. `adoptCollections`
//! takes `Math.max(parsed, previous)` on every echo, so a server that has forgotten the counter, or
//! one whose copy is older than a group made a moment ago, cannot make the next new folder reuse a
//! name that is already on screen. Dropping that would be invisible until the day two folders were
//! both called "Group 3".
//!
//! The SANITIZER is not written again here. `CollectionsState` already ports
//! `sanitizeSidebarProjectCollections` for the drawn list, and a second copy is how this port's
//! twin bugs start; this file adds the counter, the two shapes (the stored ordered array, the
//! pushed map) and the guard policy around it.
//!
//! SEE-ALSO: packages/core-ui/project-collections.ts,
//! tooling/gx-core/project-docs-server-sync-typescript.ts (the runtime's frozen
//! `queueSidebarProjectCollectionsServerSync`), packages/gx-core/src/doc_sync/sync.rs. The
//! sidebar page's `adoptCollections` was frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/metadata.ts` (see git history).

use ghostex_gx_protocol::SidebarProjectCollectionsState as WireCollectionsState;
use serde_json::{json, Map, Value};

use crate::doc_sync::{EmptyEchoRule, SyncPolicy, SyncedDocument};
use crate::sidebar_view::CollectionsState;

/// `GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS`.
pub const COLLECTIONS_SYNC_DELAY_MS: u64 = 400;
/// `GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS`.
pub const COLLECTIONS_SYNC_RETRY_DELAY_MS: u64 = 5_000;

/// The whole document: the sanitized folders in their own order, and the next folder's number.
///
/// `Default` is written out rather than derived: a derived one gives `next_collection_number` 0,
/// which is not a value `readSidebarProjectCollections` can ever produce, and the guard builds its
/// starting document with `Default`, so the first folder made before any echo arrived would have
/// been "Group 0".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionsDocument {
    pub state: CollectionsState,
    /// `nextCollectionNumber`. Never below 1, and never allowed to move backwards by an echo.
    pub next_collection_number: i64,
}

impl CollectionsDocument {
    /// `readSidebarProjectCollections`: the stored key, an ordered array rather than the daemon's
    /// map. Anything that is not an object at all is the empty document, which is what makes a
    /// damaged key harmless.
    pub fn from_storage_json(value: &Value) -> Self {
        if !value.is_object() {
            return Self::empty();
        }
        let state = CollectionsState::from_local_json(value);
        let next_collection_number = next_number(value.get("nextCollectionNumber"), &state);
        Self {
            state,
            next_collection_number,
        }
    }

    /// The daemon's copy. `None` is `parseSidebarProjectCollectionsFromGxserver` answering
    /// `undefined`, whose caller returns without touching anything.
    ///
    /// The route to the sanitizer is through the WIRE types on purpose, rather than a second
    /// reader over raw JSON: the store's own side state is parsed the same way, so a document this
    /// refuses is one the store also refuses, and the two halves of the app cannot disagree about
    /// what the daemon just said. Declared difference: an entry the wire types reject takes the
    /// whole echo with it, where the TypeScript keeps the other entries.
    pub fn from_echo_json(value: &Value) -> Option<Self> {
        let record = value.as_object()?;
        if !record.get("collections").is_some_and(Value::is_object) {
            return None;
        }
        let wire: WireCollectionsState = serde_json::from_value(value.clone()).ok()?;
        Some(Self::from_wire(&wire))
    }

    /// The daemon's copy, already typed.
    pub fn from_wire(wire: &WireCollectionsState) -> Self {
        let state = CollectionsState::from_wire(wire);
        // `Number.isSafeInteger` plus `Math.max(1, ...)`; the wire type has already turned anything
        // that is not a number into 0, which falls back exactly as a missing field does.
        let raw = i64::try_from(wire.next_collection_number).unwrap_or(0);
        let next_collection_number = match raw >= 1 {
            true => raw,
            false => state.collections.len() as i64 + 1,
        };
        Self {
            state,
            next_collection_number,
        }
    }

    /// The document with no folders at all. `nextCollectionNumber` is 1, which is the empty
    /// `readSidebarProjectCollections` answer.
    pub fn empty() -> Self {
        Self {
            state: CollectionsState::default(),
            next_collection_number: 1,
        }
    }

    /// `JSON.stringify(state)` as `writeSidebarProjectCollections` stores it.
    pub fn to_storage_json(&self) -> Value {
        json!({
            "collections": self
                .state
                .collections
                .iter()
                .map(|collection| json!({
                    "collectionId": collection.collection_id,
                    "color": collection.color,
                    "projectIds": collection.project_ids,
                    "title": collection.title,
                }))
                .collect::<Vec<_>>(),
            "nextCollectionNumber": self.next_collection_number,
        })
    }

    /// `serializeSidebarProjectCollectionsForGxserver`: the map plus the order array.
    pub fn to_wire_json(&self) -> Value {
        let mut collections = Map::new();
        for collection in &self.state.collections {
            collections.insert(
                collection.collection_id.clone(),
                json!({
                    "collectionId": collection.collection_id,
                    "color": collection.color,
                    "projectIds": collection.project_ids,
                    "title": collection.title,
                }),
            );
        }
        json!({
            "collections": Value::Object(collections),
            "nextCollectionNumber": self.next_collection_number,
            "order": self
                .state
                .collections
                .iter()
                .map(|collection| collection.collection_id.clone())
                .collect::<Vec<_>>(),
        })
    }
}

/// `typeof raw === 'number' && Number.isSafeInteger(raw) ? Math.max(1, raw) : collections.length + 1`.
fn next_number(raw: Option<&Value>, state: &CollectionsState) -> i64 {
    match raw.and_then(Value::as_i64) {
        // `as_i64` is exactly `Number.isSafeInteger` for the values that reach it: a fractional or
        // out-of-range JSON number answers `None` and falls back, as the TypeScript does.
        Some(number) => number.max(1),
        None => state.collections.len() as i64 + 1,
    }
}

impl SyncedDocument for CollectionsDocument {
    /// This computer's policy, and the only one: a REMOTE machine's collections are HELD for
    /// drawing and never edited here, because `updateRemoteSidebarProjectCollections` is a direct
    /// call down that machine's tunnel, which this app cannot reach. The planners refuse a remote
    /// gesture by name, so no second policy exists to get wrong.
    fn policy() -> SyncPolicy {
        SyncPolicy {
            delay_ms: COLLECTIONS_SYNC_DELAY_MS,
            retry_delay_ms: COLLECTIONS_SYNC_RETRY_DELAY_MS,
            empty_echo: EmptyEchoRule::PushBackFirstEcho,
            stores: true,
        }
    }

    fn parse_echo(value: &Value) -> Option<Self> {
        Self::from_echo_json(value)
    }

    fn to_wire(&self) -> Value {
        self.to_wire_json()
    }

    /// Always stored, never removed: `writeSidebarProjectCollections` calls `setItem` with whatever
    /// it is given, an empty document included, which is what makes "the user deleted their last
    /// folder" survive a restart.
    fn to_storage(&self) -> Option<Value> {
        Some(self.to_storage_json())
    }

    /// The COLLECTIONS decide this, not the counter: `!parsed.collections.length &&
    /// previous?.collections.length` is what `adoptCollections` asks.
    fn is_empty(&self) -> bool {
        self.state.collections.is_empty()
    }

    /// `{ ...parsed, nextCollectionNumber: Math.max(parsed.nextCollectionNumber, previous ?? 1) }`.
    fn adopt(held: &Self, parsed: Self) -> Self {
        Self {
            next_collection_number: parsed
                .next_collection_number
                .max(held.next_collection_number.max(1)),
            state: parsed.state,
        }
    }
}

impl Default for CollectionsDocument {
    fn default() -> Self {
        Self::empty()
    }
}
