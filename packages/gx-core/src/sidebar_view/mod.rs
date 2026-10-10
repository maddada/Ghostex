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
mod close_after_done;
mod daemon_wait;
mod drop_landing;
mod assemble;
pub(crate) mod close_successor;
pub(crate) mod collections;
mod groups;
mod inputs;
mod machine_connection;
pub(crate) mod machine_spaces;
mod machines;
pub(crate) mod membership;
mod model;
pub(crate) mod ordering;
pub(crate) mod projects;
mod project_drop_landing;
mod reveal;
pub(crate) mod rows;
mod sections;
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
pub use close_successor::{
    close_project_group_is_active, close_project_successor_candidates,
    close_project_successor_group_order, first_awake_successor_session_id, is_awake_successor_row,
};
pub use collections::{Collection, CollectionsState};
pub use daemon_wait::{DAEMON_RETRY_INTERVAL_MS, SESSIONS_LOADING_DETAIL, SESSIONS_LOADING_TITLE};
pub use drop_landing::{DropLanding, DropWrites, TreeDropTarget};
pub use project_drop_landing::{ProjectDropLanding, ProjectDropRow};
pub use inputs::{
    BrowserTabInput, CloseAfterDoneInput, DelayedSendInput, MachineTabInput, ProjectDiffStats,
    SectionCollapse, SectionId, SessionSortMode, SidebarCollapseState, SidebarHiddenItems,
    SidebarHostInputs, SidebarInputs, SidebarMode, SidebarSettings, SidebarUiState,
    UnavailableState, GROUP_WORKING_SESSIONS_SETTING_KEY, LOCAL_MACHINE_ID, MACHINE_STATE_CONNECTED,
    MAX_RECENT_SPACE_SESSION_IDS,
};
pub use machine_connection::{machine_state_copy, machine_state_is_busy, machine_state_is_failure};
pub use model::{SidebarUpdateWork, SidebarViewModel};
pub use ordering::session_is_snoozed;
pub use reveal::{reveal_plan, space_for_focused_row, FocusedRowSpace, SidebarRevealPlan};
pub use session_slot::{rendered_session_ids, session_slot_plan, SessionSlotPlan};
pub use slot_hotkey::{project_slot_plan, ProjectSlotPlan};
pub use space_landing::space_landing_project_ids;
pub use space_sleep::{plan_space_sleep, SpaceSleepPlan, SpaceSleepPlans, SpaceSleepScope};
pub use space_switch::{
    plan_space_switch_restore, plan_workspace_switch_restore, SpaceSwitchFocus,
};
pub use spaces::{Space, SpacesState, OTHER_SPACE_ID};
pub use tags::{TagListItem, TagListItemKind, TagPresentation, UNTAGGED_TAG_FILTER};
pub use work::{SessionWork, WorkGithubIssue, WorkLinearIssue, WorkLinearProject, WorkPullRequest};
pub use workspaces::{
    project_workspace_id, window_machine_tabs, window_spaces, window_workspace, WindowWorkspace,
};
pub use view::{
    CollectionView, CoordinatorBadge, CoordinatorBadgeTone, DelayedSendView, RowNesting, ThreadTally, EmptyState, GroupCore, GroupSummary, GroupView, LabelDeadline,
    MachineNotice, MachineSummary, MachineTabView, OrderItem, OrderKind, ProjectContextView, RemoteMachineView,
    SectionView, SessionMenuFacts, SessionRow, SessionTiming, SessionView, SidebarView, SpaceView,
    WorktreeView,
};
