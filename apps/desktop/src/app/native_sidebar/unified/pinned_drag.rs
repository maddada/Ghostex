//! Dragging a pinned Unified card to another place among the pinned cards of its list.
//!
//! CDXC:Sidebar 2026-11-06 DECISION:
//! User: "these sessions are pinned in the unified sidebar, please allow me to drag to rearrange them". A pinned card is the same `SidebarDrag` of kind "session" the Agents panes take (cards.rs), and every pinned card is also a drop target: over it the drop line shows above or below it, as in the Project sidebar, and the release renumbers the pinned cards of that list (the project's group with Group by Project on, else the whole list) 1000, 2000, ... in their new order. The numbers are gxserver's `sidebarOrder`, the order the Project sidebar keeps for its Pinned rows, written through the same pin call a Project sidebar drop into Pinned makes (gx-core `sidebar_drag/session_drop.rs`) on each card's own machine, and into the local store first so the card moves at once. So one order serves both views; the Unified sidebar reads it in gx-core `unified_sidebar/build.rs`. A pinned card from another computer drags too (the panes still refuse it). A drop is limited to the cards drawn in that list: unpinned cards, shelves and other groups take none, and a drop that moves nothing draws no line.

use ghostex_gx_core::SessionKey;
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

/// `ids` with `moved` placed before or after `target`; `None` when either is not in `ids` or
/// nothing moves.
fn reordered(ids: &[String], moved: &str, target: &str, after: bool) -> Option<Vec<String>> {
    if moved == target || !ids.iter().any(|id| id == moved) {
        return None;
    }
    let mut order: Vec<String> = ids.iter().filter(|id| *id != moved).cloned().collect();
    let index = order.iter().position(|id| id == target)? + usize::from(after);
    order.insert(index, moved.to_string());
    (order != ids).then_some(order)
}

impl GhostexGpuiApp {
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
        let ids: Vec<String> = self
            .unified_pinned_cards(scope)
            .iter()
            .map(|card| card.session.session_id.clone())
            .collect();
        reordered(&ids, session_id, target_id, position == "after")?;
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

    /// The release: every pinned card of the scope takes its new `sidebarOrder`, locally first,
    /// then on its machine through the pin call.
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
        let cards = self.unified_pinned_cards(scope);
        let ids: Vec<String> = cards
            .iter()
            .map(|card| card.session.session_id.clone())
            .collect();
        let Some(order) = reordered(&ids, session_id, target_id, command["position"] == "after")
        else {
            return;
        };
        let mut writes: Vec<(SessionKey, String, i64)> = Vec::new();
        for (index, id) in order.iter().enumerate() {
            let Some(card) = cards.iter().find(|card| card.session.session_id == *id) else {
                continue;
            };
            let Some(key) = card.row.key.clone() else {
                continue;
            };
            let sidebar_order = ((index + 1) * 1000) as i64;
            if card.row.sidebar_order != Some(sidebar_order) {
                writes.push((key, id.clone(), sidebar_order));
            }
        }
        if writes.is_empty() {
            return;
        }
        self.gx_store_set_session_sidebar_orders(
            writes
                .iter()
                .map(|(key, _, order)| (key.clone(), *order))
                .collect(),
            cx,
        );
        for (_, id, sidebar_order) in writes {
            self.dispatch_native_sidebar_command(
                json!({
                    "type": "setSessionPinned",
                    "sessionId": id,
                    "pinned": true,
                    "sidebarOrder": sidebar_order,
                }),
                cx,
            );
        }
    }
}
