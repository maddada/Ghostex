//! The envelope the host drains from the core once per turn of the loop.
//!
//! This is the exact shape `nativeChat.take(lastRevision)` returned
//! (`packages/shared/session-chat-controller/native-host.ts`), which
//! `apps/desktop/src/app/native_chat/state.rs` destructures key by key. Keeping it identical is
//! what let the Rust core replace the QuickJS producer without touching any drawing code.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::Document;
use crate::effect::HostRequest;

/// One drain of the core: what changed since the host's last revision.
///
/// Every field is skipped when nothing changed, the way the TypeScript producer leaves it
/// `undefined`. The host treats a missing field as "keep what you have", so an empty frame is a
/// valid no-op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// The changed window of the main transcript list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_splice: Option<ItemsSplice>,
    /// The minimap rail, shipped whole and only when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimap: Option<Vec<MinimapMarker>>,
    /// The subagent viewer's own transcript, on its own channel so opening it never redraws the
    /// main list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_splice: Option<ItemsSplice>,
    /// Details for the rows the renderer currently draws open, keyed by the host's own row key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_details: Option<RowDetails>,
    /// Bumped once per publish. The host passes the last value it saw back into the next drain.
    pub revision: u64,
    /// The full view document, omitted when the host's revision is already current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Box<Document>>,
    /// Effects the host performs, in order.
    ///
    /// Always on the wire, even empty: `take` writes `requests.splice(0)`, which is an array on
    /// every drain, and the host reads it unconditionally.
    #[serde(default)]
    pub requests: Vec<HostRequest>,
    /// Milliseconds until the next timer is due, or `null` when no timer is armed.
    ///
    /// Present with an explicit `null` rather than omitted, because the host reads it as "there is
    /// nothing to wake for" and would otherwise keep its previous wake.
    pub next_wake_ms: Option<u64>,
}

/// A splice against the host's current item list: replace `delete_count` items at `start` with
/// `items`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemsSplice {
    pub start: usize,
    pub delete_count: usize,
    pub items: Vec<crate::document::TranscriptItem>,
    /// The producer's own count after the splice.
    ///
    /// `apps/desktop/src/app/native_chat/state.rs` never reads it and rebuilds the length from its
    /// own vector instead. Kept on the wire because the TypeScript producer's frame carried it.
    pub length: usize,
}

/// One dash on the transcript minimap.
///
/// Free-form for now: the marker shape is `crate::extras::minimap_rail`'s (ported from
/// `packages/shared/session-chat-presentation/minimap.ts`) and
/// `apps/desktop/src/app/native_chat/minimap.rs` reads it by key. Family f types it.
pub type MinimapMarker = Value;

/// Open-row details keyed by the renderer's row key.
///
/// Free-form for now: a key holds either a file card's diff lines or a tool call's arguments and
/// result, and both shapes belong to family b.
pub type RowDetails = serde_json::Map<String, Value>;
