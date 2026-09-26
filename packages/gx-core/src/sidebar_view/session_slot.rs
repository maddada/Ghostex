//! The session slot hotkeys, cmd+1 to cmd+9 by default (`focusSessionSlot1` to `focusSessionSlot9`):
//! the port of the numbered branch of `runNativeSidebarHotkey`.
//!
//! CDXC:Hotkeys 2026-09-21 WHY:
//! cmd+N means the Nth row the user SEES. The TypeScript resolved it from the old page's own copy of
//! the sidebar (`createNativeSidebarSnapshot(ui)`, then `renderedNativeSidebarSessionIds`), which is
//! the drawn list in drawn order: the top-level order, a collection's projects unless the collection
//! is collapsed, nothing of a collapsed project, and of an open project only the headings that are
//! not collapsed, each with the rows it draws (the compact "show less" list leaves the rest out).
//! Spaces, hidden projects, the tag filter and parked rows need no rule of their own, because the
//! drawn list already reflects them. The machine tab is the selected one, so on a remote machine's
//! tab the slot names a REMOTE row, and the host focuses it the way a click on that row is focused;
//! nothing here builds a second remote path.
//!
//! The rest of the press is `selectNativeSidebarSession` (the multi-selection is cleared) and
//! `requestReveal` on the same row, always, which the host performs with the reveal a published
//! request ends in. A slot past the last drawn row does nothing at all.
//!
//! SEE-ALSO: the deleted sidebar page's `hotkeys.ts` (`runNativeSidebarHotkey`) and
//! `selection.ts` (`renderedNativeSidebarSessionIds`),
//! apps/desktop/src/app/gx_store/sidebar_session_slot.rs. (The parity gate,
//! `tooling/gx-core/session-slot-parity.ts`, was deleted with the TypeScript.)

use super::view::{OrderKind, SidebarView};
use crate::sidebar_ui::SidebarUiIntent;

/// What a session slot hotkey does: focus and reveal one drawn row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSlotPlan {
    /// The sidebar id of the Nth drawn row.
    pub target_session_id: String,
}

impl SessionSlotPlan {
    /// The changes the press makes to the sidebar's own state: the row click's selection clears the
    /// multi-selection. The reveal's changes are the reveal plan's, asked after these.
    pub fn intents(&self) -> Vec<SidebarUiIntent> {
        vec![SidebarUiIntent::SetSelectedSessions {
            session_ids: Vec::new(),
        }]
    }
}

/// Every row the list draws, in the order it draws them (`renderedNativeSidebarSessionIds`).
pub fn rendered_session_ids(view: &SidebarView) -> impl Iterator<Item = &String> + '_ {
    view.order
        .iter()
        .flat_map(move |item| -> Vec<&String> {
            match item.kind {
                OrderKind::Project => vec![&item.id],
                OrderKind::Collection => view
                    .collections
                    .iter()
                    .find(|collection| collection.collection_id == item.id && !collection.collapsed)
                    .map(|collection| collection.group_ids.iter().collect())
                    .unwrap_or_default(),
            }
        })
        .filter_map(move |group_id| view.group(group_id))
        .filter(|group| !group.core.collapsed)
        .flat_map(|group| {
            group
                .core
                .sections
                .iter()
                .filter(|section| !section.collapsed)
                .flat_map(|section| section.session_ids.iter())
        })
}

/// The row a slot names. `None` for a slot outside 1 to 9 and for a slot past the last drawn row,
/// where the TypeScript's `visibleSessionIds[slotNumber - 1]` is `undefined` and nothing happens.
/// Stops at the Nth row; nothing is built.
pub fn session_slot_plan(view: &SidebarView, slot_number: u32) -> Option<SessionSlotPlan> {
    if !(1..=9).contains(&slot_number) {
        return None;
    }
    rendered_session_ids(view)
        .nth(slot_number as usize - 1)
        .map(|id| SessionSlotPlan {
            target_session_id: id.clone(),
        })
}
