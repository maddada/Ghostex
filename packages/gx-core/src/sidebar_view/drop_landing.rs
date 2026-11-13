//! Where a dragged session row lands: the same layout the list draws, run once more with the
//! drop's writes applied.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/session_drop.rs (the writes),
//! apps/desktop/src/app/native_sidebar/drag.rs (the line).

use std::sync::Arc;

use super::groups::lay_out_group_rows;
use super::model::SidebarViewModel;
use super::project_drop_landing::carried_rows;
use super::view::{SectionView, SessionView};

/// What a drop changes about one group: the moved row's flags and the group's new row order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropWrites {
    /// The group the row ends up in.
    pub group_id: String,
    /// The group's rows (sidebar session ids) in their new order; `None` keeps the order.
    pub order: Option<Vec<String>>,
    pub pinned: Option<bool>,
    pub parked: Option<bool>,
    /// New `sidebarOrder` numbers (sidebar session id, number), which order pinned rows.
    pub sidebar_orders: Vec<(String, i64)>,
}

/// Where the moved row is drawn after the drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropLanding {
    pub group_id: String,
    /// The section heading the row lands under.
    pub section: String,
    /// The drawn row right above the landing slot in that section, and the one right below it.
    /// Both absent when the section draws no other row (or the row lands where nothing is drawn,
    /// such as a collapsed section).
    pub after_session_id: Option<String>,
    pub before_session_id: Option<String>,
    /// The drop leaves every drawn row where it is.
    pub unchanged: bool,
}

impl SidebarViewModel {
    /// The landing of `moved` once `writes` have happened, from the list's own layout rules.
    /// `None` when the group or the row is not one this list built.
    pub fn preview_session_drop(
        &self,
        moved: &str,
        writes: &DropWrites,
        now_ms: u64,
    ) -> Option<DropLanding> {
        let (build, inputs) = self.cached_group(&writes.group_id)?;
        let mut rows: Vec<SessionView> = build.store_rows.clone();
        let joins = !rows
            .iter()
            .any(|session| session.row.sidebar_session_id == moved);
        if joins {
            let carried = self.cached_groups().find_map(|other| {
                other
                    .store_rows
                    .iter()
                    .find(|session| session.row.sidebar_session_id == moved)
            })?;
            rows.push(carried.clone());
        }
        for session in &mut rows {
            if let Some((_, order)) = writes
                .sidebar_orders
                .iter()
                .find(|(id, _)| *id == session.row.sidebar_session_id)
            {
                let mut row = (*session.row).clone();
                row.sidebar_order = Some(*order);
                session.row = Arc::new(row);
            }
            if session.row.sidebar_session_id != moved {
                continue;
            }
            let mut row = (*session.row).clone();
            if let Some(pinned) = writes.pinned {
                row.is_pinned = pinned;
            }
            if let Some(parked) = writes.parked {
                row.is_parked = parked;
            }
            session.row = Arc::new(row);
        }
        if let Some(order) = &writes.order {
            // Rows the order does not name keep their place after the named ones, as a daemon
            // order write leaves them.
            let rank = |session: &SessionView| {
                order
                    .iter()
                    .position(|id| *id == session.row.sidebar_session_id)
                    .unwrap_or(usize::MAX)
            };
            rows.sort_by_key(rank);
        }
        let core = &build.core;
        let laid_out = lay_out_group_rows(
            &rows,
            &core.storage_id,
            core.is_active,
            &inputs.ui,
            &inputs.settings,
            now_ms,
        );
        let sections = &laid_out.layout.sections;
        // A row joining from another group always moves, even into a collapsed section, where
        // the drawn rows stay the same (a session group's row dropped on its project's folded
        // Sessions heading).
        let unchanged = !joins && same_drawn_rows(sections, &core.sections);
        let section_id = laid_out
            .sessions
            .iter()
            .position(|session| session.row.sidebar_session_id == moved)
            .map(|index| laid_out.section_by_session[index])?;
        let section = sections.iter().find(|section| section.id == section_id)?;
        let index = section.session_ids.iter().position(|id| id == moved);
        // A coordinator's threads move with it, so the row below the landing slot is the first one
        // after the rows that travel along.
        let drawn_before: Vec<&String> = core
            .sections
            .iter()
            .flat_map(|section| section.session_ids.iter())
            .collect();
        let (after_session_id, before_session_id) = match index {
            Some(index) => {
                let from_moved: Vec<&String> = section.session_ids[index..].iter().collect();
                let carried = carried_rows(&drawn_before, &from_moved, &|id: &&String| {
                    id.as_str() == moved
                });
                (
                    index
                        .checked_sub(1)
                        .and_then(|previous| section.session_ids.get(previous))
                        .cloned(),
                    section.session_ids.get(index + 1 + carried).cloned(),
                )
            }
            None => (None, None),
        };
        Some(DropLanding {
            group_id: writes.group_id.clone(),
            section: section.id.as_str().to_string(),
            after_session_id,
            before_session_id,
            unchanged,
        })
    }
}

fn same_drawn_rows(left: &[SectionView], right: &[SectionView]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.id == right.id && left.session_ids == right.session_ids)
}

/// What a drop on a row of a coordinator's tree aims at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeDropTarget {
    /// The hovered row is in no tree, or the dragged row belongs to it: aim at the row itself.
    Row,
    /// The dragged row is the coordinator and the hovered row is one of its own threads.
    OwnTree,
    /// Aim at the tree's coordinator, `before` it or `after` its whole tree.
    Coordinator {
        session_id: String,
        position: &'static str,
    },
}

impl SidebarViewModel {
    /// CDXC:Sidebar 2026-10-01 DECISION:
    /// User: "If I drag over the coordinator, we should show the drop above the coordinator when coming from below, and below it when the session is coming from above it." A coordinator and the open threads drawn under it are one block for a drop: a session dragged over any row of that block lands above the coordinator when it comes from below the block and after the block's last thread when it comes from above, whichever half of the row the pointer is in.
    pub fn tree_drop_target(&self, group_id: &str, moved: &str, hovered: &str) -> TreeDropTarget {
        let Some((build, _)) = self.cached_group(group_id) else {
            return TreeDropTarget::Row;
        };
        let rows = &build.core.sessions;
        let index_of = |id: &str| rows.iter().position(|row| row.row.sidebar_session_id == id);
        let Some(hovered_index) = index_of(hovered) else {
            return TreeDropTarget::Row;
        };
        let Some(root) = (0..=hovered_index)
            .rev()
            .find(|index| rows[*index].nesting.depth == 0)
        else {
            return TreeDropTarget::Row;
        };
        let end = (root + 1..rows.len())
            .find(|index| rows[*index].nesting.depth == 0)
            .unwrap_or(rows.len());
        // A folded coordinator's block is just its own row.
        if end == root + 1 || rows[root].nesting.collapsed {
            return TreeDropTarget::Row;
        }
        let root_id = &rows[root].row.sidebar_session_id;
        let moved_index = index_of(moved);
        if moved_index == Some(root) {
            return TreeDropTarget::OwnTree;
        }
        if moved_index.is_some_and(|index| index > root && index < end) {
            return TreeDropTarget::Row;
        }
        // A row the group does not draw (another group's) counts as coming from below.
        let from_above = moved_index.is_some_and(|index| index < root);
        TreeDropTarget::Coordinator {
            session_id: root_id.clone(),
            position: if from_above { "after" } else { "before" },
        }
    }
}
