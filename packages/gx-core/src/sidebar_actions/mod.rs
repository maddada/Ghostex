//! What a sidebar menu row, hover button or header button DOES, as data.
//!
//! `sidebar_menu/` builds the items and the payload each one carries; this module answers the next
//! question, which is what the host must do when one of those payloads comes back. Per-concern
//! files: `plan` is the call the host makes, `resolve` turns the ids a payload carries into the
//! things the daemon and the native bridge accept, `read_only` holds the actions that only read
//! (the copy actions, Open Folder, Open in Editor), and `lifecycle` holds the ones with a round
//! trip in the middle, whose optimistic value is only applied once the daemon has accepted it.

mod lifecycle;
mod plan;
mod read_only;
mod resolve;

pub use lifecycle::{
    apply_lifecycle_answer, owns_lifecycle_message, plan_lifecycle_request, LifecycleAnswer,
    LifecycleCall, LifecycleFollowUp, LifecycleRequest, LIFECYCLE_PATCH_TTL_MS,
};
pub use plan::{ActionEffect, SidebarActionPlan, ToastLevel};
pub use read_only::{plan_read_only_action, READ_ONLY_MESSAGE_TYPES};
pub use resolve::{
    local_project_group_project_id, NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};
