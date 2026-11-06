//! The sidebar list as data: the groups, sections, rows, collections, Spaces, and empty state the
//! native renderer draws.
//!
//! The pipeline mirrors the TypeScript one it replaces, in the same order: project facts
//! (`projects`), which sessions belong to which group (`membership`), one row per session
//! (`rows`, `session_text`), the order and sections of a group (`ordering`, `sections`,
//! `groups`), and the list around them (`spaces`, `collections`, `assemble`). `model` holds the
//! cache that keeps all of it up to date from a `ChangeSummary`, and `inputs` and `view` are the
//! two ends a host talks to.

pub(crate) mod agents;
mod armed_actions;
mod assemble;
mod close_after_done;
pub(crate) mod close_successor;
pub(crate) mod collections;
mod daemon_wait;
mod draft_arrivals;
mod drop_landing;
mod groups;
#[cfg(test)]
mod hide_empty_tests;
mod inputs;
mod links;
mod machine_connection;
pub(crate) mod machine_spaces;
mod machine_switch;
mod machine_tab_layout;
mod machines;
pub(crate) mod membership;
pub(crate) mod merged;
mod model;
pub(crate) mod ordering;
mod program_status;
mod project_drop_landing;
pub(crate) mod projects;
mod reveal;
pub(crate) mod rows;
mod sections;
#[cfg(test)]
mod session_notes_tests;
pub(crate) mod session_slot;
mod session_text;
pub(crate) mod slot_hotkey;
mod space_landing;
mod space_sleep;
mod space_switch;
pub(crate) mod spaces;
pub(crate) mod tags;
pub(crate) mod text;
mod threads;
pub(crate) mod view;
mod work;
pub(crate) mod workspaces;

pub use armed_actions::{
    armed_actions_by_session, ArmedAction, ARMED_ACTION_CLOSE_AFTER_DONE, ARMED_ACTION_DELAYED_SEND,
};
pub use assemble::{collection_groups, owner_project_group_id, toggle_all_project_ids};
pub use close_successor::{
    close_project_group_is_active, close_project_successor_candidates,
    close_project_successor_group_order, first_awake_successor_session_id, is_awake_successor_row,
};
pub use collections::{Collection, CollectionsState};
pub use daemon_wait::{
    DAEMON_RETRY_INTERVAL_MS, SERVICE_START_FAILED_TITLE, SESSIONS_LOADING_DETAIL,
    SESSIONS_LOADING_TITLE,
};
pub use draft_arrivals::{
    open_draft_headings, project_draft_rows, DraftArrival, DraftArrivals, DraftRow,
    DRAFT_ARRIVAL_WINDOW_MS, DRAFT_CLOCK_SLACK_MS, DRAFT_FOCUS_WAIT_MS,
};
pub use drop_landing::{DropLanding, DropWrites, TreeDropTarget};
pub use inputs::{
    BrowserTabInput, CloseAfterDoneInput, DelayedSendInput, MachineTabInput, ProjectDiffStats,
    SectionCollapse, SectionId, SessionSortMode, SidebarCollapseState, SidebarHiddenItems,
    SidebarHostInputs, SidebarInputs, SidebarMode, SidebarSettings, SidebarUiState,
    UnavailableState, GROUP_WORKING_SESSIONS_SETTING_KEY, LOCAL_MACHINE_ID,
    MACHINE_STATE_CONNECTED, MAX_RECENT_SPACE_SESSION_IDS,
};
pub use links::{
    link_chips_that_fit, SessionLink, SessionLinkGlyph, SessionLinkKind, SessionLinkTone,
    SessionLinks, SessionLinksBadge, SessionLinksChatPill, SessionLinksDisplay, SessionLinksRow,
    SuggestedSessionLink, SUGGESTED_SHOWN_FIRST,
};
pub use machine_connection::{
    machine_state_copy, machine_state_is_busy, machine_state_is_failure, THIS_COMPUTER_NOTICE_STATE,
};
pub use machine_switch::{
    machine_switch_memory, plan_machine_switch_restore, MACHINE_SESSION_MEMORY_SPACE_ID,
};
pub use machine_tab_layout::{
    machine_tab_layout, machine_tab_reveal_offset, machine_tabs_out_of_view, MachineTabLayout,
};
pub use model::{SidebarUpdateWork, SidebarViewModel};
pub use ordering::session_is_snoozed;
pub use project_drop_landing::{ProjectDropLanding, ProjectDropRow};
pub use reveal::{reveal_plan, space_for_focused_row, FocusedRowSpace, SidebarRevealPlan};
pub use session_slot::{rendered_session_ids, session_slot_plan, SessionSlotPlan};
pub use slot_hotkey::{project_slot_plan, ProjectSlotPlan};
pub use space_landing::space_landing_project_ids;
pub use space_sleep::{plan_space_sleep, SpaceSleepPlan, SpaceSleepPlans, SpaceSleepScope};
pub use space_switch::{
    plan_space_switch_restore, plan_workspace_switch_restore, SpaceSwitchFocus,
};
pub use spaces::{Space, SpacesState, OTHER_SPACE_ID};
pub use tags::{
    tag_choice, tag_choices, TagChoice, TagListItem, TagListItemKind, TagPresentation,
    UNTAGGED_TAG_FILTER,
};
pub use view::{
    rename_shake_key, CollectionView, CoordinatorBadge, CoordinatorBadgeTone, DelayedSendView,
    EmptyState, FoldedStatus, GroupActionTarget, GroupCore, GroupSummary, GroupView, LabelDeadline,
    MachineChip, MachineNotice, MachineSummary, MachineTabView, NeedsYou, OrderItem, OrderKind,
    ProjectContextView, RemoteMachineView, RowNesting, SectionView, SessionMenuFacts, SessionRow,
    SessionTiming, SessionView, ShowAllProjectsRow, SidebarView, SpaceView, ThreadTally,
    WorktreeView, RENAME_SHAKE_FRESH_MS,
};
pub use work::{SessionWork, WorkGithubIssue, WorkLinearIssue, WorkLinearProject, WorkPullRequest};
pub use workspaces::{
    project_workspace_id, window_machine_tabs, window_spaces, window_workspace, WindowWorkspace,
};
