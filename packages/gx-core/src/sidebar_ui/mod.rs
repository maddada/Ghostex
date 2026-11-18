//! The sidebar's own state: what the user collapsed, hid, filtered, and selected.
//!
//! The list itself lives in [`crate::sidebar_view`]; this is the half of its input the user owns
//! rather than the daemon. `intents` is the one way it changes, `persist` is the exact shape the
//! client storage holds, and `store` keeps the two together with the pending writes.

mod autopilot;
mod diff;
mod intents;
mod persist;
mod store;

pub use autopilot::autopilot_project_group_ids;
pub use diff::SidebarCollapseDiff;
pub use intents::{SidebarUiIntent, SidebarUiOutcome, ToggleAllProjectsInput};
pub use persist::{
    collapse_into_storage, collapse_state_from_storage, hidden_items_from_storage,
    hidden_items_into_storage, machine_tab_from_storage, sidebar_window_storage_key,
    COLLAPSE_STORAGE_KEY, COLLAPSE_STORAGE_VERSION, HIDDEN_ITEMS_STORAGE_KEY,
    MACHINE_TAB_STORAGE_KEY, PROJECT_COLLECTIONS_STORAGE_KEY, SIDEBAR_WINDOW_SCOPE_ID,
};
pub use store::{SidebarPersistSet, SidebarUiStore};
