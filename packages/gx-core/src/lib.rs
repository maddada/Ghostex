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
mod agentbox;
mod app_lifecycle;
mod attention;
pub mod bot_feed;
mod change;
mod claude_cloud;
pub mod cloud_boxes;
mod connection;
mod core;
mod doc_sync;
mod focus;
pub mod git_menu;
#[cfg(test)]
mod handoff_fixture_tests;
pub mod handoff_prefs;
pub mod handoff_target;
pub mod hud;
pub mod indicators;
mod keys;
pub mod machine_short_names;
pub mod navigation_history;
mod notification_feed;
mod notification_feeds;
mod overlay;
mod presentation_store;
mod project_activation;
mod project_docs;
mod quick_access;
mod refetch;
mod renderer_commands;
mod selectors;
mod session_create;
mod session_deep_link;
pub mod session_handoff;
pub mod settings_sync;
mod sidebar_accounts;
mod sidebar_actions;
mod sidebar_command_run;
mod sidebar_drag;
mod sidebar_menu;
mod sidebar_ui;
mod sidebar_view;
pub mod unified_sidebar;
mod workspace_groups;

pub use crate::active_project_context::{
    active_project_context_payload, project_context_payload, quick_automations_payload,
    quick_projectless_payload, ACTIVE_PROJECT_CONTEXT_MESSAGE_TYPE,
    ACTIVE_PROJECT_CONTEXT_MESSAGE_VERSION,
};
pub use crate::agentbox::{
    agentbox_agent_family, agentbox_location_label, agentbox_locations_from_status,
    is_agentbox_run_location, session_chat_view_unavailable,
    session_chat_view_unavailable_showing_cloud_pages, AgentboxLocation, AgentboxLocations,
    SessionAgentbox, LOCAL_RUN_LOCATION,
};
pub use crate::app_lifecycle::{
    AppLifecycleAction, APP_LIFECYCLE_ACTIONS, APP_LIFECYCLE_MESSAGE_TYPE,
};
pub use crate::attention::{
    AgentActivityReport, ATTENTION_PATCH_TTL_MS, ESCAPE_DONE_SUPPRESSION_MS,
    MIN_ATTENTION_VISIBLE_MS,
};
pub use crate::change::{ChangeSummary, IgnoredReason, SideStateChanges};
pub use crate::claude_cloud::{
    claude_cloud_menu, ClaudeCloudMenuAccount, ClaudeCloudMenuEntry, ClaudeCloudMenuPick,
    ClaudeCloudMenuRow,
};
pub use crate::claude_cloud::{
    claude_cloud_page_account, claude_cloud_session_link, close_keeps_claude_cloud_row,
    is_claude_cloud_agent, is_claude_cloud_run_location, is_claude_cloud_session,
    is_running_claude_cloud_page, project_last_cloud_account, session_claude_cloud_page,
    CLAUDE_CLOUD_AGENT_ID, CLAUDE_CLOUD_RUN_LOCATION, THIS_COMPUTER_DESCRIPTION,
};
pub use crate::connection::{ConnectionPhase, ConnectionState, ConnectionUpdate};
pub use crate::core::{Core, Effect, Event, Intent, Output, ResubscribeReason};
pub use crate::doc_sync::{
    document_reconcile_wanted, DocumentSync, EmptyEchoRule, SyncEffect, SyncPolicy, SyncedDocument,
};
pub use crate::focus::{
    empty_tab_list_confirmed, next_visible_sessions_for_local_focus, ActiveGroup,
    ExternalFocusUpdate, FocusField, FocusOutcome, FocusState,
};
pub use crate::keys::{
    decode_uri_component, encode_uri_component, encode_workspace_subgroup_id,
    parse_workspace_subgroup_id, MachineId, ProjectKey, SessionKey,
};
pub use crate::notification_feed::{
    notification_feed_jump_target, notification_feed_state_message, NotificationFeedCommand,
    NOTIFICATION_FEED_READ_ENDPOINT, NOTIFICATION_FEED_STATE_MESSAGE_TYPE,
    NOTIFICATION_FEED_UPDATE_ENDPOINT,
};
pub use crate::notification_feeds::{
    merged_notification_feed_jump_target, merged_notification_feed_message,
    notification_feed_item_machine, MachineNotificationFeed, NotificationFeedTab,
};
pub use crate::overlay::{SessionPatch, RENAME_PATCH_TTL_MS};
pub use crate::presentation_store::{
    json_stringify, snapshot_storage_json, LoadedPresentation, MachinePresentation,
    PresentationState, PresentationStore, SideState, SideStateUpdate, SnapshotOrigin,
};
pub use crate::project_activation::{
    plan_project_activation, project_last_session_storage_key, ProjectActivation,
    PROJECT_LAST_SESSION_KEY_PREFIX,
};
pub use crate::project_docs::{
    apply_space_row_reorder, can_drop_project_with_worktrees, create_collection,
    keep_spaces_across_collection_edit, move_members_to_space, move_projects_to_collection,
    move_projects_with_worktrees, order_projects_with_worktrees, owns_collection_menu_command,
    plan_collection_menu_edit, plan_space_editor_result, reorder_collection_projects,
    reorder_spaces, toggle_space_member, CollectionsDocument, DropPosition, ProjectOrderItem,
    SpaceEditorMode, SpaceEditorResult, SpaceMemberKind, SpacesDocument, COLLECTIONS_SYNC_DELAY_MS,
    COLLECTIONS_SYNC_RETRY_DELAY_MS, COLLECTION_MENU_COMMAND_TYPE,
    COLLECTION_MENU_DOCUMENT_ACTIONS, SPACES_SYNC_DELAY_MS, SPACES_SYNC_RETRY_DELAY_MS,
};
pub use crate::quick_access::{
    quick_access_store_groups, FixedClock, HotkeyPlatform, HotkeyPlatformWire, QuickAccessClock,
    QuickAccessCollection, QuickAccessContext, QuickAccessController, QuickAccessData,
    QuickAccessEffect, QuickAccessExtension, QuickAccessHiddenItems, QuickAccessOpenTarget,
    QuickAccessRecoveredDraft, QuickAccessRunState, QuickAccessSession, QuickAccessStorage,
    QuickAccessStoreGroup, QuickAccessTab, QuickAccessUpdate,
};
pub use crate::renderer_commands::*;
pub use crate::selectors::{
    Loadable, TabDirection, TabSession, DEFAULT_TERMINAL_SESSION_TITLE,
    QUICK_AUTOMATIONS_PROJECT_ID, TAB_SESSION_TITLE_MAX_UTF16,
};
pub use crate::session_create::*;
pub use crate::session_deep_link::*;
pub use crate::sidebar_accounts::{
    account_headline_windows, account_session_working, account_usage_detail, account_usage_label,
    agent_accounts_http_answer, group_accounts_target, is_five_hour_window, is_weekly_window,
    js_round, mask_account_text, session_accounts_target, AccountAnswer, AccountMenuHost,
    AccountMenuStep, AccountSession, AccountUsageWindow, AccountsRequest, AccountsState,
    AccountsTarget, AgentAccount, LauncherAccounts, LauncherCommand, ResetCredits, SessionAccounts,
    SessionAccountsCommand, SidebarAccountMenus, AGENT_ACCOUNTS_PATH, INVALID_ACCOUNTS_ANSWER,
    SESSION_COMPUTER_UNAVAILABLE,
};
pub use crate::sidebar_actions::{
    apply_close_answer, apply_flags_answer, apply_fork_answer, apply_lifecycle_answer,
    apply_snooze_answer, bulk_request_summary, close_as_claude_cloud_sleep,
    close_optimistic_follow_ups, iso_string_from_ms, local_project_group_project_id,
    owns_batch_command, owns_bulk_message, owns_close_message, owns_flags_message,
    owns_fork_message, owns_lifecycle_message, owns_modal_message, owns_open_command,
    owns_reload_message, owns_reload_set_message, owns_remote_session_message, owns_snooze_action,
    owns_snooze_message, plan_batch, plan_bulk_request, plan_close_request, plan_flags_request,
    plan_fork_request, plan_full_reload, plan_lifecycle_request, plan_modal_action,
    plan_open_action, plan_read_only_action, plan_reload_set, plan_remote_focus,
    plan_remote_session_action, plan_snooze_action, plan_snooze_request, plan_sort_action,
    reload_continues_after, rename_seed_title, snooze_wake_ms, ActionEffect, BatchPlan, BulkAction,
    BulkRequest, CloseAnswer, CloseFollowUp, CloseRequest, FlagsFollowUp, FlagsRequest,
    FocusOptions, ForkFollowUp, ForkRequest, LifecycleAnswer, LifecycleCall, LifecycleFollowUp,
    LifecycleRequest, ModalAction, PreferredInterfaceSettings, ReloadPlan, ReloadSetPlan,
    RemoteActionKind, RemoteCallMode, RemoteFailureToast, RemoteFocusPlan, RemoteLeg,
    RemoteSessionPlan, RemoteStep, SessionFlags, SidebarActionPlan, SnoozeAction, SnoozeCall,
    SnoozeClock, SnoozeFollowUp, SnoozeRequest, ToastLevel, BULK_MESSAGE_TYPES,
    BULK_SLEEP_INTERVAL_MS, FLAGS_MESSAGE_TYPES, LIFECYCLE_PATCH_TTL_MS,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    OPEN_COMMAND_TYPES, READ_ONLY_MESSAGE_TYPES, RELOAD_MESSAGE_TYPES, RELOAD_SET_MESSAGE_TYPES,
    REMOTE_AWAITED_TIMEOUT_MS, REMOTE_FIRE_AND_FORGET_TIMEOUT_MS, REMOTE_FOCUS_MESSAGE_TYPE,
    REMOTE_SESSION_MESSAGE_TYPES, SESSION_SNOOZE_PRESETS, SNOOZE_MESSAGE_TYPES, SORT_ACTIONS,
};
pub use crate::sidebar_actions::{
    close_confirmation, close_source, sidebar_close_targets, CloseConfirmation,
};
pub use crate::sidebar_actions::{
    delayed_send_seed, normalize_remote_machine_settings, owns_agent_run_command,
    owns_delayed_send_command, owns_group_working_sessions_command, owns_machine_disable_command,
    plan_agent_run, plan_delayed_send_action, plan_group_working_sessions, plan_machine_disable,
    GROUP_WORKING_SESSIONS_ACTION, GROUP_WORKING_SESSIONS_SETTINGS_SOURCE,
    HIDE_PROJECTS_WITHOUT_SESSIONS_ACTION, MACHINE_DISABLE_SETTINGS_SOURCE,
    UNIFIED_GROUP_BY_PROJECT_ACTION, UNIFIED_WORKING_SECTION_ACTION,
};
pub use crate::sidebar_actions::{
    open_remote_session_terminal, plan_generate_session_title, plan_group_sleep,
    provider_transition_committed, running_local_session_ids, terminal_lifecycle_fallback_focus,
    titlebar_sleep_inactive_ids,
};
pub use crate::sidebar_actions::{
    plan_remote_project_open, RemoteProjectOpen, ToastButton, REMOTE_SSH_OPEN_TARGET_IDS,
};
pub use crate::sidebar_actions::{
    remote_focus_group, RuntimeActiveGroup, RUNTIME_GROUP_SENT_TRUST_MS,
};
pub use crate::sidebar_command_run::{
    plan_sidebar_command_run, SidebarCommandRun, SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE,
    SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION,
};
pub use crate::sidebar_drag::{
    check_session_handoff, owns_order_write_message, owns_project_move_command,
    owns_project_order_message, owns_session_drop_command, owns_session_move_command,
    plan_added_project_placement, plan_added_project_space_membership, plan_new_project_placement,
    plan_new_project_space_membership, plan_order_write, plan_project_move,
    plan_project_order_write, plan_section_move, plan_session_drop, plan_session_move,
    project_body_drop_command, project_drop_command, project_drop_landing,
    sidebar_group_membership, sidebar_project_group_order, AddedProjectPlacement,
    NewProjectPlaceChoice, NewProjectPlacement, OrderWrite, OrderWritePlan, ProjectMovePlan,
    ProjectWrite, SessionDrop, SessionHandoffCheck, SessionHandoffDrop, SessionMovePlan,
    ORDER_WRITE_MESSAGE_TYPES, PROJECT_MOVE_COMMAND_TYPES, PROJECT_ORDER_MESSAGE_TYPE,
    SESSION_DROP_ZONE_LIST_END,
};
pub use crate::sidebar_menu::{
    agent_launcher_items, agent_launcher_items_with_accounts, agent_logo_icons, agent_logo_svg,
    colored_agent_logo, hover_slots, hover_strip, machine_workspace_menu, menu_to_json,
    project_header_actions, unified_default_hover_buttons, unified_delete_worktree_menu,
    unified_hover_buttons, workspace_menu, workspace_switch_target, HeaderCommand, HoverAction,
    HoverSlot, HoverStrip, LauncherAgent, MenuCommand, MenuGroup, MenuHost, MenuItem,
    MenuOpenTarget, MenuSecondary, MenuSplit, SessionActions, SidebarMenus,
    UNIFIED_TOGGLE_GROUPS_ACTION,
};
pub use crate::sidebar_menu::{
    AgentChoicePlace, AgentChoiceScope, AgentChoices, AGENT_CHOICES_STORAGE_KEY,
    AGENT_CHOICE_SCOPE_SETTING_KEY,
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
    armed_actions_by_session, close_project_group_is_active, close_project_successor_candidates,
    close_project_successor_group_order, first_awake_successor_session_id, is_awake_successor_row,
    link_chips_that_fit, machine_state_copy, machine_state_is_busy, machine_state_is_failure,
    machine_tab_layout, machine_tab_reveal_offset, machine_tabs_out_of_view, plan_space_sleep,
    plan_space_switch_restore, project_slot_plan, rename_shake_key, rendered_session_ids,
    reveal_plan, session_is_snoozed, session_slot_plan, space_for_focused_row,
    space_landing_project_ids, ArmedAction, BotsSource, BrowserTabInput, CloseAfterDoneInput,
    Collection, CollectionView, CollectionsState, CoordinatorBadge, CoordinatorBadgeTone,
    DelayedSendInput, DelayedSendView, DropLanding, DropWrites, EmptyState, FocusedRowSpace,
    FoldedStatus, FoldedWorktree, GroupActionTarget, GroupCore, GroupSummary, GroupView,
    LabelDeadline, MachineChip, MachineNotice, MachineSummary, MachineTabInput, MachineTabLayout,
    MachineTabView, NeedsYou, OrderItem, OrderKind, ProjectContextView, ProjectDiffStats,
    ProjectDropLanding, ProjectDropRow, ProjectSlotPlan, RemoteMachineView, RowNesting,
    RowWorktree, SectionCollapse, SectionId, SectionView, SessionLink, SessionLinkGlyph,
    SessionLinkKind, SessionLinkTone, SessionLinks, SessionLinksBadge, SessionLinksChatPill,
    SessionLinksDisplay, SessionLinksRow, SessionMenuFacts, SessionRow, SessionSlotPlan,
    SessionSortMode, SessionTiming, SessionView, SessionWork, ShowAllProjectsRow,
    SidebarCollapseState, SidebarHiddenItems, SidebarHostInputs, SidebarInputs, SidebarMode,
    SidebarRevealPlan, SidebarSettings, SidebarUiState, SidebarUpdateWork, SidebarView,
    SidebarViewModel, Space, SpaceSleepPlan, SpaceSleepPlans, SpaceSleepScope, SpaceSwitchFocus,
    SpaceView, SpacesState, SuggestedSessionLink, TagListItem, TagListItemKind, TagPresentation,
    ThreadTally, TreeDropTarget, UnavailableState, WorkGithubIssue, WorkLinearIssue,
    WorkLinearProject, WorkPullRequest, WorktreeView, ARMED_ACTION_CLOSE_AFTER_DONE,
    ARMED_ACTION_DELAYED_SEND, DAEMON_RETRY_INTERVAL_MS, LOCAL_MACHINE_ID, MACHINE_STATE_CONNECTED,
    OTHER_SPACE_ID, RENAME_SHAKE_FRESH_MS, SERVICE_START_FAILED_TITLE, SESSIONS_LOADING_DETAIL,
    SESSIONS_LOADING_TITLE, SUGGESTED_SHOWN_FIRST, THIS_COMPUTER_NOTICE_STATE, UNTAGGED_TAG_FILTER,
};
pub use crate::sidebar_view::{
    collection_groups, machine_switch_memory, owner_project_group_id, plan_machine_switch_restore,
    plan_workspace_switch_restore, project_workspace_id, toggle_all_project_ids,
    window_machine_tabs, window_spaces, window_workspace, WindowWorkspace,
    MACHINE_SESSION_MEMORY_SPACE_ID,
};
pub use crate::sidebar_view::{
    open_draft_headings, project_draft_rows, DraftArrival, DraftArrivals, DraftRow,
    DRAFT_ARRIVAL_WINDOW_MS, DRAFT_CLOCK_SLACK_MS, DRAFT_FOCUS_WAIT_MS,
};
pub use crate::sidebar_view::{tag_choice, tag_choices, TagChoice};
pub use crate::workspace_groups::{
    owns_group_command, plan_group_command, CustomTagsPush, CustomTagsPushEffect, GroupCommandPlan,
};
pub use crate::workspace_groups::{
    AdoptOutcome, ProjectWorkspaceGroups, SessionGroupOp, SessionGroupOutcome, SessionGroupRequest,
    WorkspaceGroupsDocument, WorkspaceGroupsEffect, WorkspaceGroupsSync, WorkspaceSubgroup,
    WORKSPACE_GROUPS_SYNC_DELAY_MS, WORKSPACE_GROUPS_SYNC_RETRY_DELAY_MS,
    WORKSPACE_SESSION_GROUP_MAX_COUNT,
};

/// The wire types, re-exported so a host needs one dependency.
pub use ghostex_gx_protocol as protocol;
