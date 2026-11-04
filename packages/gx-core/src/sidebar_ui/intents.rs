//! What the user can do to the sidebar's own state, and what each of those does.

use serde::{Deserialize, Serialize};

use crate::sidebar_view::SectionId;

/// One change to the sidebar's own state. Applied synchronously; nothing here waits on the daemon.
///
/// New variants are added by later milestones; match with a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum SidebarUiIntent {
    /// Collapse or expand a project row or a user-made group.
    ToggleGroupCollapsed {
        group_id: String,
    },
    /// Show the full hover-button row on a project's cards.
    ToggleHoverActions {
        storage_id: String,
    },
    /// Collapse or expand one heading of a project's session list.
    ToggleSection {
        storage_id: String,
        section: SectionId,
    },
    /// Collapse or expand a collection, by its `<section key>:<collection id>` storage id.
    ToggleCollectionCollapsed {
        storage_id: String,
    },
    /// Fold or unfold the threads drawn under a coordinator, by the coordinator's sidebar row id.
    ToggleCoordinatorCollapsed {
        sidebar_session_id: String,
    },
    /// List or tuck away a coordinator's older threads (its "N older threads" row), by the
    /// coordinator's sidebar row id.
    ToggleCoordinatorOlderThreads {
        sidebar_session_id: String,
    },
    /// Filter the section the machine tab is on by a Space. The renderer's `selectSpace`.
    SelectSpace {
        space_id: String,
    },
    /// Filter a NAMED section by a Space, which is what the Space follow needs: a focused row on
    /// another machine moves that machine's section without switching the tab, exactly as
    /// `rememberNativeSidebarFocus` writes `selectedSpaceIdBySectionKey[section.sectionKey]`.
    SetSectionSpace {
        section_key: String,
        space_id: String,
    },
    /// Show the user's projects or the Hermes bots. The Hermes button's toggle.
    SetSidebarMode {
        mode: crate::sidebar_view::SidebarMode,
    },
    /// Switch the machine tab.
    SelectMachine {
        machine_id: String,
    },
    /// Shrink a machine's failure notice to its one-line pill for this failure state (`Some`), or
    /// open the full card again (`None`).
    SetMachineNoticeCollapsed {
        machine_id: String,
        state: Option<String>,
    },
    /// Tick or untick one tag filter.
    ToggleTagFilter {
        tag: String,
    },
    /// Show or hide the projects and collections the user hid.
    ToggleShowHidden,
    /// Show or hide the projects `sidebarHideProjectsWithoutSessions` leaves out.
    ToggleShowAllProjects,
    HideGroup {
        group_id: String,
    },
    UnhideGroup {
        group_id: String,
    },
    /// By the collection's `<section key>:<collection id>` storage id.
    HideCollection {
        storage_id: String,
    },
    UnhideCollection {
        storage_id: String,
    },
    /// Replace the multi-selection with exactly these sidebar row ids.
    SetSelectedSessions {
        session_ids: Vec<String>,
    },
    /// Collapse every drawn project of the machine, or put back the ones that were expanded.
    ToggleAllProjects(ToggleAllProjectsInput),
    /// Put this row at the front of the Space's memory, so switching back to that Space restores
    /// it. `rememberSidebarSpaceSession`, which runs on every focus change and every reveal.
    RememberSpaceSession {
        section_key: String,
        space_id: String,
        sidebar_session_id: String,
    },
    /// Drop a section's chosen Space, which is what deleting the Space the section was filtered by
    /// leaves behind. The section then resolves to its first Space again.
    ForgetSectionSpace {
        section_key: String,
    },
    /// Expand a project row. A slot hotkey DELETES the key rather than toggling it, which is why
    /// this is not a `ToggleGroupCollapsed`: pressing cmd+ctrl+1 on an already-expanded project
    /// must leave it expanded.
    ExpandProjectForSlotJump {
        group_id: String,
    },
}

/// The drawn project rows of a machine, in the order the list draws them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleAllProjectsInput {
    pub machine_id: String,
    /// Every project group the machine currently draws.
    pub group_ids: Vec<String>,
}

/// What applying one intent changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SidebarUiOutcome {
    /// The state moved, so the list must be built again.
    pub changed: bool,
    /// The values that have to reach client storage.
    pub persist: super::store::SidebarPersistSet,
}
