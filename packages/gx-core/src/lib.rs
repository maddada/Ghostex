//! Platform-neutral core of the Ghostex client: the one store that owns product state.
//!
//! Inputs are [`Event`]s (wire frames, user [`Intent`]s, clock ticks). Outputs are state, read
//! through selectors, plus [`Effect`]s: requests the host performs. Nothing in this crate touches
//! gpui, threads, sockets, the file system, or a clock; the host passes `now_ms` in. That is what
//! lets the desktop app, the mobile app (through UniFFI), and the web build share it.
//!
//! Design rules:
//!
//! - One owner per piece of state. Focus lives in [`FocusState`] and nowhere else.
//! - A local intent applies at once and always wins over a server or echo update older than it.
//! - "Not loaded yet" and "empty" are different types ([`PresentationState`], [`Loadable`]).
//! - Daemon rows are replaced whole, never merged; local edits are overlays that never renumber
//!   the revision.

mod change;
mod connection;
mod core;
mod focus;
mod keys;
mod overlay;
mod presentation_store;
mod selectors;
mod sidebar_actions;
mod sidebar_menu;
mod sidebar_ui;
mod sidebar_view;

pub use crate::change::{ChangeSummary, IgnoredReason, SideStateChanges};
pub use crate::connection::{ConnectionPhase, ConnectionState, ConnectionUpdate};
pub use crate::core::{Core, Effect, Event, Intent, Output, ResubscribeReason};
pub use crate::focus::{
    default_group_for_project, next_visible_sessions_for_local_focus, ActiveGroup,
    ExternalFocusUpdate, FocusField, FocusOutcome, FocusState,
};
pub use crate::keys::{
    decode_uri_component, encode_uri_component, encode_workspace_subgroup_id,
    parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey, CHATS_GROUP_ID,
};
pub use crate::overlay::SessionPatch;
pub use crate::presentation_store::{
    LoadedPresentation, MachinePresentation, PresentationState, PresentationStore, SideState,
    SideStateUpdate, SnapshotOrigin,
};
pub use crate::selectors::{
    is_chat_project_path, Loadable, TabDirection, TabSession, DEFAULT_TERMINAL_SESSION_TITLE,
    QUICK_AUTOMATIONS_PROJECT_ID, TAB_SESSION_TITLE_MAX_UTF16,
};
pub use crate::sidebar_actions::{
    apply_lifecycle_answer, local_project_group_project_id, owns_lifecycle_message,
    plan_lifecycle_request, plan_read_only_action, ActionEffect, LifecycleAnswer, LifecycleCall,
    LifecycleFollowUp, LifecycleRequest, SidebarActionPlan, ToastLevel, LIFECYCLE_PATCH_TTL_MS,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    READ_ONLY_MESSAGE_TYPES,
};
pub use crate::sidebar_menu::{
    agent_launcher_items, agent_logo_icons, colored_agent_logo, hover_strip, menu_to_json,
    project_header_actions, HeaderCommand, HoverAction, HoverStrip, LauncherAgent, MenuCommand,
    MenuGroup, MenuHost, MenuItem, MenuSecondary, MenuSplit, SessionActions, SidebarMenus,
};
pub use crate::sidebar_ui::{
    collapse_into_storage, collapse_state_from_storage, hidden_items_from_storage,
    hidden_items_into_storage, machine_tab_from_storage, sidebar_window_storage_key,
    SidebarCollapseDiff, SidebarPersistSet, SidebarUiIntent, SidebarUiOutcome, SidebarUiStore,
    ToggleAllProjectsInput, COLLAPSE_STORAGE_KEY, COLLAPSE_STORAGE_VERSION,
    HIDDEN_ITEMS_STORAGE_KEY, MACHINE_TAB_STORAGE_KEY, PROJECT_COLLECTIONS_STORAGE_KEY,
    SIDEBAR_WINDOW_SCOPE_ID,
};
pub use crate::sidebar_view::{
    reveal_plan, space_for_focused_row, BrowserTabInput, CloseAfterDoneInput, CollectionView,
    DelayedSendInput, DelayedSendView, EmptyState, GroupCore, GroupSummary, GroupView,
    LabelDeadline, MachineSummary, MachineTabInput, MachineTabView, OrderItem, OrderKind,
    ProjectContextView, ProjectDiffStats, RemoteMachineView, SectionCollapse, SectionId,
    SectionView, SessionMenuFacts, SessionRow, SessionSortMode, SessionTiming, SessionView,
    SidebarCollapseState, SidebarHiddenItems, SidebarHostInputs, SidebarInputs, SidebarRevealPlan,
    SidebarSettings, SidebarUiState, SidebarUpdateWork, SidebarView, SidebarViewModel, SpaceView,
    TagListItem, TagListItemKind, TagPresentation, UnavailableState, WorktreeView,
    LOCAL_MACHINE_ID, MACHINE_STATE_CONNECTED, OTHER_SPACE_ID, UNTAGGED_TAG_FILTER,
};

/// The wire types, re-exported so a host needs one dependency.
pub use ghostex_gx_protocol as protocol;
