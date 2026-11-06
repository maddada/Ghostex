//! What the Unified sidebar draws, as the store installs it.

use std::sync::Arc;

use ghostex_gx_core::SessionRow;
use ghostex_gx_core::unified_sidebar::{
    UnifiedBranchLine, UnifiedFilter, UnifiedFilterOptions, UnifiedShelf, UnifiedStatus,
};

use crate::app::native_sidebar::model::NativeSidebarSession;

/// One session card.
#[derive(Clone, Debug)]
pub(crate) struct UnifiedCard {
    /// The classic row of the same session, whose menu, hover buttons and agent icon the card
    /// reuses.
    pub(crate) session: Arc<NativeSidebarSession>,
    pub(crate) row: Arc<SessionRow>,
    /// The repository's icon, from whichever machine's project has one.
    pub(crate) repo_icon: Option<Arc<gpui::Image>>,
    pub(crate) repo_label: String,
    /// gx-core `RepoIdentity::key`: the group a pinned card is dragged within.
    pub(crate) repo_key: String,
    /// The machine's name for a session on another machine.
    pub(crate) machine_label: Option<String>,
    /// That machine's short name (gx-core `machine_short_names`), which the card draws.
    pub(crate) machine_short_label: Option<String>,
    pub(crate) status: Option<UnifiedStatus>,
    /// What its third line says (gx-core `unified_branch_line`).
    pub(crate) branch_line: UnifiedBranchLine,
    /// It sits under its project's header, which names the project.
    pub(crate) in_group: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct UnifiedShelfView {
    pub(crate) shelf: UnifiedShelf,
    pub(crate) cards: Vec<UnifiedCard>,
}

/// One project's header and what sits under it, while Group by Project is on.
#[derive(Clone, Debug)]
pub(crate) struct UnifiedGroupView {
    pub(crate) repo_key: String,
    pub(crate) label: String,
    /// Pinned then the rest, in the order they last came back to the user.
    pub(crate) cards: Vec<UnifiedCard>,
    /// Its own Working, Snoozed and Parked; an empty shelf is left out.
    pub(crate) shelves: Vec<UnifiedShelfView>,
    pub(crate) count: usize,
    /// The computers that have the project, as `(name, short name)`, for the header's pills: only
    /// when it is on more than one, or only on another one, as the Project sidebar's header does.
    pub(crate) machines: Vec<(String, String)>,
}

/// The whole Unified sidebar.
#[derive(Clone, Debug, Default)]
pub(crate) struct UnifiedSidebarSnapshot {
    /// Sessions never sent a message that hold unsent text, newest first, above everything.
    pub(crate) drafts: Vec<UnifiedCard>,
    /// Pinned then the rest, in the order they last came back to the user.
    pub(crate) list: Vec<UnifiedCard>,
    /// Working, Snoozed and Parked, in that order; an empty shelf is left out.
    pub(crate) shelves: Vec<UnifiedShelfView>,
    /// While Group by Project is on, the whole list; `list` and `shelves` are empty then.
    pub(crate) groups: Vec<UnifiedGroupView>,
    pub(crate) options: UnifiedFilterOptions,
    pub(crate) filter: UnifiedFilter,
    /// Sessions before the filter.
    pub(crate) total: usize,
    /// Some machine has a project, so an empty list is not the first run.
    pub(crate) has_projects: bool,
    /// The icons of the picked repositories that have one, for their chips.
    pub(crate) repo_icons: std::collections::HashMap<String, Arc<gpui::Image>>,
    /// The project whose Project sidebar menu each repository's row opens: its main checkout, on
    /// this computer when it has one, as a sidebar group id.
    pub(crate) repo_menu_groups: std::collections::HashMap<String, String>,
    /// The machine whose Spaces the Project sidebar's Space menu acts on (its selected machine).
    pub(crate) space_menu_machine: String,
    /// The name of the Space the list is scoped to, while Spaces is on (gx-core
    /// `unified_space_scope`).
    pub(crate) space: Option<String>,
}

impl UnifiedSidebarSnapshot {
    pub(crate) fn is_empty(&self) -> bool {
        self.drafts.is_empty()
            && self.list.is_empty()
            && self.shelves.is_empty()
            && self.groups.is_empty()
    }

    /// Whether any card shows a running timer, so the list redraws once a second.
    pub(crate) fn has_running_timer(&self) -> bool {
        self.drafts
            .iter()
            .chain(&self.list)
            .chain(self.shelves.iter().flat_map(|shelf| shelf.cards.iter()))
            .chain(self.groups.iter().flat_map(|group| {
                group
                    .cards
                    .iter()
                    .chain(group.shelves.iter().flat_map(|shelf| shelf.cards.iter()))
            }))
            .any(|card| card.status == Some(UnifiedStatus::Working))
    }
}
