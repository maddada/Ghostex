//! Quick Access as a model: the four tabs (Commands, Projects, Sessions, Saved Prompts), their data
//! requests, filters, ranking and rows, and every command the native window sends back.
//!
//! The host feeds it the sidebar's groups and HUD ([`QuickAccessData`]), the client storage it reads
//! ([`QuickAccessStorage`]) and a clock ([`QuickAccessClock`]), and performs the effects it returns.

mod commands;
mod controller;
mod controller_rows;
mod data;
mod hotkey_table;
mod hotkeys;
mod icons;
mod projects;
mod prompts;
mod row_actions;
mod session_titles;
mod sessions;
mod store_groups;
mod text;
mod wire;

pub use controller::{QuickAccessContext, QuickAccessController, QuickAccessEffect, QuickAccessTab};
pub use data::{
    HotkeyPlatformWire, QuickAccessCollection, QuickAccessData, QuickAccessHiddenItems,
    QuickAccessOpenTarget, QuickAccessRecoveredDraft, QuickAccessRunState, QuickAccessSession,
    QuickAccessStorage, QuickAccessStoreGroup,
};
pub use store_groups::quick_access_store_groups;
pub use text::{FixedClock, HotkeyPlatform, QuickAccessClock};
pub use wire::QuickAccessUpdate;
