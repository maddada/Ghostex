//! The Spaces document: the saved sidebar filters and who belongs to each of them.
//!
//! CDXC:Spaces 2026-09-21 WHY:
//! Unlike every other client-owned document in this app the Spaces document has NO stored key
//! (`CDXC:Spaces 2026-08-27`: gxserver owns the whole document, one Space set per daemon). So it is
//! a pusher and a guard with no storage leg, which is exactly what [`crate::doc_sync::SyncPolicy`]
//! expresses: `stores` false, and an empty server document adopted like any other, because there is
//! no local copy for it to erase. A cold start therefore draws no Spaces until the daemon's first
//! document arrives, which is what the shipped page does too.
//!
//! The SANITIZER is not written again here: `SpacesState` already ports
//! `sanitizeSidebarSpacesState`, including the decision that a project belongs to at most one Space
//! (`CDXC:Spaces 2026-09-07 DECISION` in sidebar_view/spaces.rs). This file adds the wire shape and
//! the guard policy.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/spaces.rs, packages/gx-core/src/doc_sync/sync.rs.
//! (The page's `spaces.ts` and the runtime's frozen `queueSidebarSpacesServerSync` were deleted on
//! 2026-10-01; see git history.)

use ghostex_gx_protocol::SidebarSpacesState as WireSpacesState;
use serde_json::{json, Map, Value};

use crate::doc_sync::{EmptyEchoRule, SyncPolicy, SyncedDocument};
use crate::sidebar_view::SpacesState;

/// `GPUI_SIDEBAR_SPACES_SERVER_SYNC_DELAY_MS`.
pub const SPACES_SYNC_DELAY_MS: u64 = 400;
/// `GPUI_SIDEBAR_SPACES_SERVER_SYNC_RETRY_DELAY_MS`.
pub const SPACES_SYNC_RETRY_DELAY_MS: u64 = 5_000;

/// One machine's Spaces, sanitized.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpacesDocument {
    pub state: SpacesState,
}

impl SpacesDocument {
    /// The daemon's copy. `None` is `parseSidebarSpacesFromGxserver` answering `undefined`, whose
    /// caller returns without touching anything.
    ///
    /// Through the WIRE types for the same reason the collections document is: the store's own side
    /// state is parsed that way, so a document this refuses is one the store also refuses.
    pub fn from_echo_json(value: &Value) -> Option<Self> {
        let record = value.as_object()?;
        if !record.get("spaces").is_some_and(Value::is_object) {
            return None;
        }
        let wire: WireSpacesState = serde_json::from_value(value.clone()).ok()?;
        Some(Self::from_wire(&wire))
    }

    pub fn from_wire(wire: &WireSpacesState) -> Self {
        Self {
            state: SpacesState::from_wire(wire),
        }
    }

    /// `serializeSidebarSpacesForGxserver`: the order array, and a map holding only the Spaces that
    /// array names.
    pub fn to_wire_json(&self) -> Value {
        let mut spaces = Map::new();
        for space_id in &self.state.order {
            let Some(space) = self.state.spaces.get(space_id) else {
                continue;
            };
            let mut wire = json!({
                "color": space.color,
                "icon": space.icon,
                "memberCollectionIds": space.member_collection_ids,
                "memberProjectIds": space.member_project_ids,
                "name": space.name,
                "spaceId": space.space_id,
            });
            if let Some(workspace_id) = &space.workspace_id {
                wire["workspaceId"] = json!(workspace_id);
            }
            // Always written, `false` too: gxserver keeps a stored `true` for a write that leaves
            // the field out (an older client's), so Unhide has to say so.
            wire["hidden"] = json!(space.hidden);
            spaces.insert(space_id.clone(), wire);
        }
        json!({ "order": self.state.order, "spaces": Value::Object(spaces) })
    }
}

impl SyncedDocument for SpacesDocument {
    fn policy() -> SyncPolicy {
        SyncPolicy {
            delay_ms: SPACES_SYNC_DELAY_MS,
            retry_delay_ms: SPACES_SYNC_RETRY_DELAY_MS,
            // No local copy to erase, and no `firstAdoption` branch in `adoptSpaces`.
            empty_echo: EmptyEchoRule::Adopt,
            stores: false,
        }
    }

    fn parse_echo(value: &Value) -> Option<Self> {
        Self::from_echo_json(value)
    }

    fn to_wire(&self) -> Value {
        self.to_wire_json()
    }

    /// Never stored. The policy's `stores` is what really decides; this answers so the trait has
    /// one shape, and a `None` here would read as "remove the key", which is a key that does not
    /// exist.
    fn to_storage(&self) -> Option<Value> {
        None
    }

    fn is_empty(&self) -> bool {
        self.state.order.is_empty()
    }
}
