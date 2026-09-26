//! Sleep Space: the projects and rows a Space icon's "Sleep Space" acts on.
//!
//! CDXC:Spaces 2026-09-22 DECISION:
//! User: the Space icon menu's Sleep group shows only Sleep Inactive. This supersedes the same-day
//! request for Sleep Space and Sleep Others on that menu. Sleep Inactive is the Space's idle
//! sessions. Sleep Space still means every session and open view in the Space, left in place
//! asleep, and Sleep Others means the same outside it; the menu just no longer offers those two.
//!
//! The set is the Space's, not the drawn list's. The Space under the pointer need not be the
//! selected one, and the drawn list is filtered to the selected Space, Show Hidden and the tag
//! filters, so membership is asked of a list built with nothing filtering, the way
//! `space_for_focused_row` asks it. One build per click is the cost.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/space_sleep.rs (what the app does with the plan).

use std::collections::BTreeSet;

use crate::core::Core;
use crate::keys::ProjectKey;

use super::inputs::SidebarInputs;
use super::model::SidebarViewModel;
use super::reveal::{machine_key, unfiltered};
use super::spaces::{OTHER_SPACE_ID, SpaceSelection, SpacesState, selection_shows_project};

/// What one of the Space menu's three sleeps acts on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceSleepPlan {
    /// The workspace project id of every project the sleep covers, in list order: the key the app
    /// keeps a project's views and parked browser pages under. A user-made session group has no
    /// project of its own and contributes rows only. Empty for Sleep Inactive, which is sessions
    /// only.
    pub project_ids: Vec<String>,
    /// The sidebar session ids of the rows the sleep covers, in list order, each once even when a
    /// row is drawn in its project and again in a user-made group. Browser rows are not sessions
    /// and are left to the view half.
    pub session_ids: Vec<String>,
}

impl SpaceSleepPlan {
    pub fn is_empty(&self) -> bool {
        self.project_ids.is_empty() && self.session_ids.is_empty()
    }
}

/// The three sleeps a `sleepSpace` command can name, resolved from one list build.
/// The Space icon menu offers only Sleep Inactive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceSleepPlans {
    pub space_id: String,
    /// Sleep Space: every awake row and every project of the Space.
    pub space: SpaceSleepPlan,
    /// Sleep Inactive: the Space's awake rows that are neither working nor waiting on the user,
    /// and no views. It is the project menu's Sleep Inactive over the Space
    /// (`CDXC:Resources 2026-09-04 DECISION`: inactive is about the daemon session, not the panes
    /// or pages).
    pub inactive: SpaceSleepPlan,
    /// Sleep Others: every awake row and every project the Space does NOT show. A row the Space
    /// shows (through its project, or a user-made group the Space draws) is never in it, even
    /// when the same session is also drawn in a group outside the Space.
    pub others: SpaceSleepPlan,
}

/// The scope a `sleepSpace` command names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceSleepScope {
    Space,
    Inactive,
    Others,
}

impl SpaceSleepScope {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "space" => Some(Self::Space),
            "inactive" => Some(Self::Inactive),
            "others" => Some(Self::Others),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Space => "space",
            Self::Inactive => "inactive",
            Self::Others => "others",
        }
    }
}

impl SpaceSleepPlans {
    pub fn plan(&self, scope: SpaceSleepScope) -> &SpaceSleepPlan {
        match scope {
            SpaceSleepScope::Space => &self.space,
            SpaceSleepScope::Inactive => &self.inactive,
            SpaceSleepScope::Others => &self.others,
        }
    }
}

/// `isGpuiInactiveProjectPresentationSession` over a drawn row: awake, and neither working nor
/// waiting on the user.
fn row_is_inactive(row: &super::view::SessionRow) -> bool {
    row.lifecycle_state == "running"
        && row.activity != "working"
        && row.activity != "attention"
        && row.pending_question_count == 0
        && !row.has_background_work
}

/// The plans for `space_id` on the selected machine, or `None` when Spaces are off, the machine
/// has published no Spaces document, or no Space has that id. `other` is the built-in Other view:
/// every project no Space claims.
pub fn plan_space_sleep(
    core: &Core,
    inputs: &SidebarInputs,
    space_id: &str,
    now_ms: u64,
) -> Option<SpaceSleepPlans> {
    if !inputs.settings.sidebar_spaces_enabled {
        return None;
    }
    let machine = machine_key(&inputs.ui.selected_machine_id);
    let spaces = SpacesState::from_wire(
        core.presentation()
            .machine(&machine)?
            .side_state()
            .spaces
            .as_ref()?,
    );
    let selection = if space_id == OTHER_SPACE_ID {
        SpaceSelection::Other
    } else if spaces.spaces.contains_key(space_id) {
        SpaceSelection::Space(space_id.to_string())
    } else {
        return None;
    };
    let built = SidebarViewModel::build_from_scratch(core, &unfiltered(inputs), now_ms);
    let shown: Vec<bool> = built
        .groups
        .iter()
        .map(|group| {
            let context = group.core.project_context.as_ref();
            selection_shows_project(
                &selection,
                &spaces,
                context.map(|context| context.project_id.as_str()),
                group.collection_id.as_deref(),
                context
                    .and_then(|context| context.worktree.as_ref())
                    .map(|worktree| worktree.parent_project_id.as_str()),
            )
        })
        .collect();

    let mut plans = SpaceSleepPlans {
        space_id: space_id.to_string(),
        ..SpaceSleepPlans::default()
    };
    // The Space's own rows first, in full, so the others' pass can exclude them.
    let mut in_space: BTreeSet<&str> = BTreeSet::new();
    let mut inactive_seen: BTreeSet<&str> = BTreeSet::new();
    for (group, _) in built.groups.iter().zip(&shown).filter(|(_, shown)| **shown) {
        push_project(&mut plans.space.project_ids, &group.core.group_id);
        for session in &group.core.sessions {
            let row = &session.row;
            if row.is_browser || row.lifecycle_state != "running" {
                continue;
            }
            if in_space.insert(row.sidebar_session_id.as_str()) {
                plans.space.session_ids.push(row.sidebar_session_id.clone());
            }
            if row_is_inactive(row) && inactive_seen.insert(row.sidebar_session_id.as_str()) {
                plans
                    .inactive
                    .session_ids
                    .push(row.sidebar_session_id.clone());
            }
        }
    }
    let mut others_seen: BTreeSet<&str> = BTreeSet::new();
    for (group, _) in built
        .groups
        .iter()
        .zip(&shown)
        .filter(|(_, shown)| !**shown)
    {
        push_project(&mut plans.others.project_ids, &group.core.group_id);
        for session in &group.core.sessions {
            let row = &session.row;
            if row.is_browser
                || row.lifecycle_state != "running"
                || in_space.contains(row.sidebar_session_id.as_str())
            {
                continue;
            }
            if others_seen.insert(row.sidebar_session_id.as_str()) {
                plans
                    .others
                    .session_ids
                    .push(row.sidebar_session_id.clone());
            }
        }
    }
    Some(plans)
}

/// A project group's workspace project id, once. A user-made group parses to nothing.
fn push_project(project_ids: &mut Vec<String>, group_id: &str) {
    let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
        return;
    };
    let project_id = project.to_workspace_project_id();
    if !project_ids.contains(&project_id) {
        project_ids.push(project_id);
    }
}
