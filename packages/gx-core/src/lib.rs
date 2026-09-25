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

mod active_project_context;
mod attention;
mod change;
mod connection;
mod core;
mod doc_sync;
mod focus;
pub mod hud;
pub mod indicators;
pub mod git_menu;
mod keys;
pub mod navigation_history;
pub mod app_shot;
mod notification_feed;
mod overlay;
mod presentation_store;
mod refetch;
mod renderer_commands;
mod project_activation;
mod project_docs;
mod quick_access;
mod selectors;
mod session_create;
mod sidebar_accounts;
mod sidebar_actions;
mod sidebar_command_run;
mod sidebar_drag;
mod sidebar_menu;
mod sidebar_ui;
mod sidebar_view;
mod workspace_groups;

pub use crate::active_project_context::{
    active_project_context_payload, project_context_payload, quick_automations_payload,
    quick_projectless_payload, ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE,
    ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION,
};
pub use crate::attention::{
    AgentActivityReport, ATTENTION_PATCH_TTL_MS, ESCAPE_DONE_SUPPRESSION_MS, MIN_ATTENTION_VISIBLE_MS,
};
pub use crate::change::{ChangeSummary, IgnoredReason, SideStateChanges};
pub use crate::connection::{ConnectionPhase, ConnectionState, ConnectionUpdate};
pub use crate::core::{Core, Effect, Event, Intent, Output, ResubscribeReason};
pub use crate::doc_sync::{
    document_reconcile_wanted, DocumentSync, EmptyEchoRule, SyncEffect, SyncPolicy, SyncedDocument,
};
pub use crate::focus::{
    default_group_for_project, empty_tab_list_confirmed, next_visible_sessions_for_local_focus,
    ActiveGroup, ExternalFocusUpdate, FocusField, FocusOutcome, FocusState,
};
pub use crate::keys::{
    decode_uri_component, encode_uri_component, encode_workspace_subgroup_id,
    parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey, CHATS_GROUP_ID,
};
pub use crate::notification_feed::{
    notification_feed_jump_target, notification_feed_state_message, NotificationFeedCommand,
    NOTIFICATION_FEED_READ_ENDPOINT, NOTIFICATION_FEED_STATE_MESSAGE_TYPE,
    NOTIFICATION_FEED_UPDATE_ENDPOINT,
};
pub use crate::overlay::SessionPatch;
pub use crate::sidebar_actions::{
    remote_focus_group, RuntimeActiveGroup, RUNTIME_GROUP_SENT_TRUST_MS,
};
pub use crate::project_activation::{
    plan_project_activation, project_last_session_storage_key, ProjectActivation,
    PROJECT_LAST_SESSION_KEY_PREFIX,
};
pub use crate::project_docs::{
    apply_space_row_reorder, can_drop_project_with_worktrees, create_collection,
    move_members_to_space, move_projects_to_collection, move_projects_with_worktrees,
    order_projects_with_worktrees, owns_collection_menu_command, plan_collection_menu_edit,
    plan_space_editor_result, reorder_collection_projects, reorder_spaces, toggle_space_member,
    CollectionsDocument, DropPosition, ProjectOrderItem, SpaceEditorMode, SpaceEditorResult,
    SpaceMemberKind, SpacesDocument, COLLECTIONS_SYNC_DELAY_MS, COLLECTIONS_SYNC_RETRY_DELAY_MS,
    COLLECTION_MENU_COMMAND_TYPE, COLLECTION_MENU_DOCUMENT_ACTIONS, SPACES_SYNC_DELAY_MS,
    SPACES_SYNC_RETRY_DELAY_MS,
};
pub use crate::presentation_store::{
    json_stringify, snapshot_storage_json, LoadedPresentation, MachinePresentation,
    PresentationState, PresentationStore, SideState, SideStateUpdate, SnapshotOrigin,
};
pub use crate::selectors::{
    is_chat_project_path, Loadable, TabDirection, TabSession, DEFAULT_TERMINAL_SESSION_TITLE,
    QUICK_AUTOMATIONS_PROJECT_ID, TAB_SESSION_TITLE_MAX_UTF16,
};
pub use crate::sidebar_actions::{
    apply_close_answer, apply_flags_answer, apply_fork_answer, apply_lifecycle_answer,
    apply_snooze_answer, bulk_request_summary, close_optimistic_follow_ups, iso_string_from_ms,
    local_project_group_project_id, owns_batch_command, owns_bulk_message, owns_close_message,
    owns_flags_message, owns_fork_message, owns_lifecycle_message, owns_modal_message,
    owns_open_command, owns_reload_message, owns_reload_set_message,
    owns_remote_session_message, owns_snooze_action,
    owns_snooze_message, owns_split_message, plan_batch, plan_bulk_request, plan_close_request,
    plan_flags_request, plan_fork_request, plan_full_reload, plan_lifecycle_request,
    plan_modal_action, plan_open_action, plan_read_only_action, plan_reload_set,
    plan_remote_focus, plan_remote_session_action,
    plan_snooze_action, plan_snooze_request, plan_sort_action, plan_split_right,
    reload_continues_after,
    rename_seed_title, snooze_wake_ms, ActionEffect, BatchPlan, BulkAction, BulkRequest,
    CloseAnswer, CloseFollowUp, CloseRequest, FlagsFollowUp, FlagsRequest, FocusOptions,
    ForkFollowUp, ForkRequest, LifecycleAnswer, LifecycleCall, LifecycleFollowUp, LifecycleRequest,
    ModalAction, PreferredInterfaceSettings, ReloadPlan, ReloadSetPlan, RemoteActionKind,
    RemoteCallMode, RemoteFailureToast, RemoteFocusPlan,
    RemoteLeg, RemoteSessionPlan, RemoteStep, SessionFlags, SidebarActionPlan, SnoozeAction,
    SnoozeCall, SnoozeClock, SnoozeFollowUp, SnoozeRequest, SplitAction, SplitPlan, ToastLevel,
    BULK_MESSAGE_TYPES, BULK_SLEEP_INTERVAL_MS, FLAGS_MESSAGE_TYPES, LIFECYCLE_PATCH_TTL_MS,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    OPEN_COMMAND_TYPES, READ_ONLY_MESSAGE_TYPES, RELOAD_MESSAGE_TYPES, RELOAD_SET_MESSAGE_TYPES,
    REMOTE_AWAITED_TIMEOUT_MS, REMOTE_FIRE_AND_FORGET_TIMEOUT_MS, REMOTE_FOCUS_MESSAGE_TYPES,
    REMOTE_SESSION_MESSAGE_TYPES,
    SESSION_SNOOZE_PRESETS, SNOOZE_MESSAGE_TYPES, SORT_ACTIONS,
};
pub use crate::sidebar_actions::{
    open_remote_session_terminal, plan_generate_session_title, plan_group_sleep,
    provider_transition_committed,
    running_local_session_ids,
    terminal_lifecycle_fallback_focus, titlebar_sleep_inactive_ids,
};
pub use crate::sidebar_actions::{
    daemon_delayed_send_seed, normalize_remote_machine_settings, owns_agent_run_command,
    owns_delayed_send_command,
    owns_machine_disable_command, plan_agent_run, plan_delayed_send_action, plan_machine_disable,
    MACHINE_DISABLE_SETTINGS_SOURCE,
};
pub use crate::session_create::*;
pub use crate::sidebar_command_run::{
    plan_sidebar_command_run, SidebarCommandRun, SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE,
    SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION,
};
pub use crate::sidebar_accounts::{
    account_headline_windows, account_session_working, account_usage_detail, account_usage_label,
    agent_accounts_http_answer, group_accounts_target, is_five_hour_window, is_weekly_window,
    js_round, mask_account_text, session_accounts_target, AccountAnswer, AccountMenuHost,
    AccountMenuStep, AccountSession, AccountUsageWindow, AccountsRequest, AccountsState,
    AccountsTarget, AgentAccount, LauncherAccounts, LauncherCommand, ResetCredits,
    SessionAccounts, SessionAccountsCommand, SidebarAccountMenus, AGENT_ACCOUNTS_PATH,
    INVALID_ACCOUNTS_ANSWER, SESSION_COMPUTER_UNAVAILABLE,
};
pub use crate::renderer_commands::*;
pub use crate::sidebar_drag::{
    owns_order_write_message, owns_project_move_command, owns_project_order_message,
    owns_session_move_command, plan_added_project_placement, plan_added_project_space_membership,
    plan_order_write, plan_project_move, plan_project_order_write,
    plan_session_move, sidebar_group_membership, sidebar_project_group_order,
    AddedProjectPlacement, OrderWrite,
    OrderWritePlan, ProjectMovePlan,
    ProjectWrite, SessionMovePlan, ORDER_WRITE_MESSAGE_TYPES, PROJECT_MOVE_COMMAND_TYPES,
    PROJECT_ORDER_MESSAGE_TYPE,
};
pub use crate::sidebar_menu::{
    agent_launcher_items, agent_launcher_items_with_accounts, agent_logo_icons, colored_agent_logo,
    hover_strip, menu_to_json,
    project_header_actions, HeaderCommand, HoverAction, HoverStrip, LauncherAgent, MenuCommand,
    MenuGroup, MenuHost, MenuItem, MenuSecondary, MenuSplit, SessionActions, SidebarMenus,
};
pub use crate::sidebar_ui::{
    collapse_into_storage, collapse_state_from_storage, hidden_items_from_storage,
    hidden_items_into_storage, machine_tab_from_storage,
    sidebar_window_storage_key, SidebarCollapseDiff, SidebarPersistSet, SidebarUiIntent,
    SidebarUiOutcome, SidebarUiStore, ToggleAllProjectsInput, COLLAPSE_STORAGE_KEY, COLLAPSE_STORAGE_VERSION,
    HIDDEN_ITEMS_STORAGE_KEY, MACHINE_TAB_STORAGE_KEY, PROJECT_COLLECTIONS_STORAGE_KEY,
    SIDEBAR_WINDOW_SCOPE_ID,
};
pub use crate::quick_access::{
    FixedClock, HotkeyPlatform, HotkeyPlatformWire, QuickAccessClock, QuickAccessCollection,
    QuickAccessContext, QuickAccessController, QuickAccessData, QuickAccessEffect,
    QuickAccessHiddenItems, QuickAccessOpenTarget, QuickAccessRecoveredDraft, QuickAccessRunState,
    QuickAccessSession, QuickAccessStorage, QuickAccessStoreGroup, QuickAccessTab,
    QuickAccessUpdate, quick_access_store_groups,
};
pub use crate::sidebar_view::{
    armed_actions_by_session, ArmedAction, ARMED_ACTION_CLOSE_AFTER_DONE,
    ARMED_ACTION_DELAYED_SEND,
    close_project_group_is_active, close_project_successor_candidates,
    close_project_successor_group_order, first_awake_successor_session_id, is_awake_successor_row,
    plan_space_sleep, plan_space_switch_restore, project_slot_plan, rendered_session_ids, reveal_plan,
    session_is_snoozed, session_slot_plan,
    space_for_focused_row, SpaceSleepPlan, SpaceSleepPlans, SpaceSleepScope, SpaceSwitchFocus, BrowserTabInput,
    CloseAfterDoneInput, Collection, CollectionView, CollectionsState,
    DelayedSendInput, DelayedSendView,
    EmptyState, FocusedRowSpace, GroupCore, GroupSummary, GroupView, LabelDeadline,
    MachineSummary,
    MachineTabInput, MachineTabView, OrderItem, OrderKind, ProjectContextView, ProjectDiffStats,
    ProjectSlotPlan, RemoteMachineView, SectionCollapse,
    SectionId, SectionView, SessionMenuFacts, SessionRow, SessionSortMode, SessionTiming,
    SessionSlotPlan, SessionView, SidebarCollapseState, SidebarHiddenItems, SidebarHostInputs, SidebarInputs,
    SidebarRevealPlan, SidebarSettings, SidebarUiState, SidebarUpdateWork, SidebarView,
    SidebarViewModel, Space, SpaceView, SpacesState, TagListItem, TagListItemKind,
    TagPresentation, UnavailableState,
    WorktreeView, LOCAL_MACHINE_ID, MACHINE_STATE_CONNECTED, OTHER_SPACE_ID, UNTAGGED_TAG_FILTER,
};
pub use crate::workspace_groups::{
    owns_group_command, plan_group_command, CustomTagsPush, CustomTagsPushEffect, GroupCommandPlan,
};
pub use crate::workspace_groups::{
    AdoptOutcome, ProjectWorkspaceGroups, WorkspaceGroupsDocument, WorkspaceGroupsEffect,
    WorkspaceGroupsSync, WorkspaceSubgroup, WORKSPACE_GROUPS_SYNC_DELAY_MS,
    WORKSPACE_GROUPS_SYNC_RETRY_DELAY_MS, WORKSPACE_SESSION_GROUP_MAX_COUNT,
};

/// The wire types, re-exported so a host needs one dependency.
pub use ghostex_gx_protocol as protocol;
