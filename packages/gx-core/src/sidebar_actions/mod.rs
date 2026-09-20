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
//! above rather than new behaviour.

mod bulk;
mod close;
mod flags;
mod fork;
mod lifecycle;
mod modals;
mod plan;
mod read_only;
mod resolve;
mod snooze;

pub use bulk::{
    bulk_request_summary, owns_batch_command, owns_bulk_message, plan_batch, plan_bulk_request,
    BatchPlan, BulkAction, BulkRequest, BULK_MESSAGE_TYPES, BULK_SLEEP_INTERVAL_MS,
};
pub use close::{
    apply_close_answer, close_optimistic_follow_ups, owns_close_message, plan_close_request,
    CloseAnswer, CloseFollowUp, CloseRequest,
};
pub use flags::{
    apply_flags_answer, owns_flags_message, plan_flags_request, FlagsFollowUp, FlagsRequest,
    SessionFlags, FLAGS_MESSAGE_TYPES,
};
pub use fork::{
    apply_fork_answer, owns_fork_message, plan_fork_request, ForkFollowUp, ForkRequest,
};
pub use lifecycle::{
    apply_lifecycle_answer, owns_lifecycle_message, plan_lifecycle_request, LifecycleAnswer,
    LifecycleCall, LifecycleFollowUp, LifecycleRequest, LIFECYCLE_PATCH_TTL_MS,
};
pub use modals::{owns_modal_message, plan_modal_action, rename_seed_title, ModalAction};
pub use plan::{ActionEffect, SidebarActionPlan, ToastLevel};
pub use read_only::{plan_read_only_action, READ_ONLY_MESSAGE_TYPES};
pub use resolve::{
    local_project_group_project_id, NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};
pub use snooze::{
    apply_snooze_answer, iso_string_from_ms, owns_snooze_action, owns_snooze_message,
    plan_snooze_action, plan_snooze_request, snooze_wake_ms, SnoozeAction, SnoozeCall, SnoozeClock,
    SnoozeFollowUp, SnoozeRequest, SESSION_SNOOZE_PRESETS, SNOOZE_MESSAGE_TYPES,
};
