//! Dragging a pinned Unified card to another place among the pinned cards of its list.
//!
//! CDXC:Sidebar 2026-11-06 DECISION:
//! User: "these sessions are pinned in the unified sidebar, please allow me to drag to rearrange them". A pinned card is the same `SidebarDrag` of kind "session" the Agents panes take (cards.rs), and every pinned card is also a drop target: over it the drop line shows above or below it, as in the Project sidebar, and the release renumbers the pinned cards of that list (the project's group with Group by Project on, else the whole list) 1000, 2000, ... in their new order. The numbers are gxserver's `sidebarOrder`, the order the Project sidebar keeps for its Pinned rows, written through the same pin call a Project sidebar drop into Pinned makes (gx-core `sidebar_drag/session_drop.rs`) on each card's own machine, and into the local store first so the card moves at once. So one order serves both views; the Unified sidebar reads it in gx-core `unified_sidebar/build.rs`. A pinned card from another computer drags too (the panes still refuse it). A drop is limited to the cards drawn in that list: unpinned cards, shelves and other groups take none, and a drop that moves nothing draws no line.

use ghostex_gx_core::{PinnedReorder, PinnedSessionOrder, plan_pinned_reorder};
use serde_json::{Value, json};

use super::model::UnifiedCard;
use crate::GhostexGpuiApp;

/// The drop kind a pinned card answers to, and the command its drop keeps.
pub(in crate::app::native_sidebar) const PINNED_TARGET: &str = "unified-pinned";
const REORDER_COMMAND: &str = "unifiedPinnedReorder";

/// The scope a pinned card is dragged within: its project's group, or the ungrouped list.
pub(super) fn pinned_scope(card: &UnifiedCard) -> String {
    if card.in_group {
        card.repo_key.clone()
    } else {
        String::new()
    }
}

impl GhostexGpuiApp {
    /// Pinned cards drag only while Pinned order is Manual (CDXC:Sidebar 2026-11-07 DECISION in
    /// gx-core sidebar_view/inputs.rs).
    pub(super) fn unified_pinned_reorder_on(&self) -> bool {
        self.gx_store
            .sidebar_list
            .last_inputs
            .settings
            .pinned_session_order
            == PinnedSessionOrder::Manual
    }

    /// The drop of `session_id` on `target_id` in one scope (gx-core `plan_pinned_reorder`, the
    /// Project sidebar's Pinned rule too), or `None` when it moves nothing or reordering is off.
    fn unified_pinned_reorder(
        &self,
        session_id: &str,
        target_id: &str,
        scope: &str,
        after: bool,
    ) -> Option<PinnedReorder> {
        if !self.unified_pinned_reorder_on() {
            return None;
        }
        let rows: Vec<(String, Option<i64>)> = self
            .unified_pinned_cards(scope)
            .iter()
            .map(|card| (card.session.session_id.clone(), card.row.sidebar_order))
            .collect();
        plan_pinned_reorder(&rows, session_id, target_id, after)
    }

    /// The pinned cards of one scope, in the order they are drawn.
    fn unified_pinned_cards(&self, scope: &str) -> Vec<UnifiedCard> {
        let Some(unified) = self.native_sidebar.unified.as_ref() else {
            return Vec::new();
        };
        let cards = if scope.is_empty() {
            &unified.list
        } else {
            match unified.groups.iter().find(|group| group.repo_key == scope) {
                Some(group) => &group.cards,
                None => return Vec::new(),
            }
        };
        cards
            .iter()
            .filter(|card| card.session.is_pinned && pinned_scope(card) == scope)
            .cloned()
            .collect()
    }

    /// The command a pinned card dropped on `target_id` keeps, or `None` when that drop moves
    /// nothing (another list, an unpinned card, or the place it already has).
    pub(in crate::app::native_sidebar) fn unified_pinned_drop_command(
        &self,
        session_id: &str,
        target_id: &str,
        scope: &str,
        position: &str,
    ) -> Option<Value> {
        self.unified_pinned_reorder(session_id, target_id, scope, position == "after")?;
        Some(json!({
            "type": REORDER_COMMAND,
            "sessionId": session_id,
            "targetSessionId": target_id,
            "position": position,
            "scope": scope,
        }))
    }

    /// Where the pending pinned drop draws its line on this card.
    pub(super) fn unified_pinned_drop_line(&self, session_id: &str) -> Option<&'static str> {
        let command = self.native_sidebar.drop_command.as_ref()?;
        if command["type"] != REORDER_COMMAND || command["targetSessionId"] != session_id {
            return None;
        }
        Some(if command["position"] == "after" {
            "after"
        } else {
            "before"
        })
    }

    /// Whether `finish_native_sidebar_drop` should hand this command to
    /// [`Self::finish_unified_pinned_drop`].
    pub(in crate::app::native_sidebar) fn is_unified_pinned_drop(command: &Value) -> bool {
        command["type"] == REORDER_COMMAND
    }

    /// The release: every pinned card of the scope takes its new `sidebarOrder` through the pin
    /// call on its own machine (the local rows move first, native_sidebar/actions.rs).
    pub(in crate::app::native_sidebar) fn finish_unified_pinned_drop(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let (Some(session_id), Some(target_id), Some(scope)) = (
            command["sessionId"].as_str(),
            command["targetSessionId"].as_str(),
            command["scope"].as_str(),
        ) else {
            return;
        };
        let Some(reorder) = self.unified_pinned_reorder(
            session_id,
            target_id,
            scope,
            command["position"] == "after",
        ) else {
            return;
        };
        for message in reorder.messages {
            self.dispatch_native_sidebar_command(message, cx);
        }
    }
}
