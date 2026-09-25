//! What a sidebar menu row, hover button or header button DOES, as data.
//!
//! `sidebar_menu/` builds the items and the payload each one carries; this module answers the next
//! question, which is what the host must do when one of those payloads comes back. Per-concern
//! files: `plan` is the call the host makes, `resolve` turns the ids a payload carries into the
//! things the daemon and the native bridge accept, `read_only` holds the actions that only read
//! (the copy actions, Open Folder, Open in Editor), `lifecycle` holds the ones with a round trip
//! in the middle, whose optimistic value is only applied once the daemon has accepted it, and
//! `close` holds the one whose optimistic update takes a row away before the call and puts it back
//! when the call does not come home, and `fork` the one where nothing local happens until the
//! daemon has already made the session. `flags` holds the four that are one call with
//! different fields: pin, park, tag and favorite, `modals` the two that call nothing and
//! only open a dialog, and `snooze` the only one whose answer depends on the clock and on the
//! local calendar, and `bulk` the plural payloads, which are a SET and an ORDER over the ones
//! above rather than new behaviour. `reload` and `split` are the same shape one row at a time:
//! Full Reload is the sleep and the wake in order, and Split Right is a focus that carries where
//! the pane goes, and `reload_set` is Full Reload over a project's rows or a user-made group's
//! members. `remote` is every per-session payload when the row is on a remote machine, where an
//! action is a call down that machine's tunnel and nothing local moves, and `remote_focus` the one
//! remote payload that is not a call at all: a row click, which opens a pane through the fixed
//! native project-path bridge. `open` holds the family
//! whose whole answer is an app-modal-host message: the More menu's rows, a machine's Configure,
//! the Space editor, and a project header's Add Worktree and History. `sort` holds the More menu's
//! two sort rows, whose TypeScript path ends in a no-op on the desktop. `delayed_send`, `agent_run`
//! and `machine_disable` are the three the sidebar page answered itself with no Rust owner: a row's
//! Delayed Send dialog, the agent launcher's run, and a machine tab's Hide Machine, which posts the
//! list `remote_machine_settings` normalizes.

mod agent_run;
mod bulk;
mod close;
mod delayed_send;
mod flags;
mod fork;
mod generate_title;
mod group_sleep;
mod lifecycle;
mod machine_disable;
mod modals;
mod open;
mod plan;
mod read_only;
mod reload;
mod reload_set;
mod remote;
mod remote_focus;
mod remote_machine_settings;
mod resolve;
mod sleep_sweep;
mod snooze;
mod sort;
mod split;
mod terminal_lifecycle;

pub use agent_run::{owns_agent_run_command, plan_agent_run};
pub use bulk::{
    bulk_request_summary, owns_batch_command, owns_bulk_message, plan_batch, plan_bulk_request,
    BatchPlan, BulkAction, BulkRequest, BULK_MESSAGE_TYPES, BULK_SLEEP_INTERVAL_MS,
};
pub use close::{
    apply_close_answer, close_optimistic_follow_ups, owns_close_message, plan_close_request,
    CloseAnswer, CloseFollowUp, CloseRequest,
};
pub use delayed_send::{
    delayed_send_seed, owns_delayed_send_command, plan_delayed_send_action,
};
pub use flags::{
    apply_flags_answer, owns_flags_message, plan_flags_request, FlagsFollowUp, FlagsRequest,
    SessionFlags, FLAGS_MESSAGE_TYPES,
};
pub use fork::{
    apply_fork_answer, owns_fork_message, plan_fork_request, ForkFollowUp, ForkRequest,
};
pub use generate_title::plan_generate_session_title;
pub use group_sleep::plan_group_sleep;
pub use lifecycle::{
    apply_lifecycle_answer, owns_lifecycle_message, plan_lifecycle_request, FocusOptions,
    LifecycleAnswer, LifecycleCall, LifecycleFollowUp, LifecycleRequest, LIFECYCLE_PATCH_TTL_MS,
};
pub use machine_disable::{
    owns_machine_disable_command, plan_machine_disable, MACHINE_DISABLE_SETTINGS_SOURCE,
};
pub use modals::{owns_modal_message, plan_modal_action, rename_seed_title, ModalAction};
pub use open::{owns_open_command, plan_open_action, OPEN_COMMAND_TYPES};
pub use plan::{ActionEffect, SidebarActionPlan, ToastLevel};
pub use read_only::{plan_read_only_action, READ_ONLY_MESSAGE_TYPES};
pub use reload::{
    owns_reload_message, plan_full_reload, reload_continues_after, ReloadPlan, RELOAD_MESSAGE_TYPES,
};
pub use reload_set::{
    owns_reload_set_message, plan_reload_set, ReloadSetPlan, RELOAD_SET_MESSAGE_TYPES,
};
pub use remote::{
    owns_remote_session_message, plan_remote_session_action, RemoteActionKind, RemoteCallMode,
    RemoteFailureToast, RemoteLeg, RemoteSessionPlan, RemoteStep, REMOTE_AWAITED_TIMEOUT_MS,
    REMOTE_FIRE_AND_FORGET_TIMEOUT_MS, REMOTE_SESSION_MESSAGE_TYPES,
};
pub use remote_focus::{
    open_remote_session_terminal, plan_remote_focus, remote_focus_group,
    PreferredInterfaceSettings, RemoteFocusPlan,
    RuntimeActiveGroup, REMOTE_FOCUS_MESSAGE_TYPES, RUNTIME_GROUP_SENT_TRUST_MS,
};
pub use remote_machine_settings::normalize_remote_machine_settings;
pub use resolve::{
    local_project_group_project_id, NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};
pub use snooze::{
    apply_snooze_answer, iso_string_from_ms, owns_snooze_action, owns_snooze_message,
    plan_snooze_action, plan_snooze_request, snooze_wake_ms, SnoozeAction, SnoozeCall, SnoozeClock,
    SnoozeFollowUp, SnoozeRequest, SESSION_SNOOZE_PRESETS, SNOOZE_MESSAGE_TYPES,
};
pub use sleep_sweep::{running_local_session_ids, titlebar_sleep_inactive_ids};
pub use sort::{plan_sort_action, SORT_ACTIONS};
pub use split::{owns_split_message, plan_split_right, SplitAction, SplitPlan};
pub use terminal_lifecycle::{provider_transition_committed, terminal_lifecycle_fallback_focus};
