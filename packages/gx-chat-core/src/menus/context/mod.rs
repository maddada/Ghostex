//! Family e2: the context meter, the context editor and the context details rows.
//!
//! This subdirectory is family e2's alone. Family e1 owns the rest of `src/menus/` and calls the
//! two entry points below from `menus/document.rs` and `menus/actions.rs`, so e2 never edits an
//! e1 file. Document keys: `contextMeter`, `contextEditor`. Actions: `context*`.

pub mod actions;
pub mod codex;
pub mod computer_pins;
pub mod document;
pub mod editor;
pub mod meter;
pub mod preferences;
pub mod rows;
pub mod status;
pub mod time;
pub mod usage;
pub mod windows;

pub use crate::menus::context::actions::handle;
pub use crate::menus::context::document::document;
pub use crate::menus::context::editor::ContextEditorState;
pub use crate::menus::context::preferences::{ContextDetailItem, ContextDetailsPreferences};
pub use crate::menus::context::rows::{ContextDetailSession, GroupId, RowDefinition};
pub use crate::menus::context::status::{
    AgentAccount, ContextDetailStatus, ContextDetailsAgent, DetectedOptions,
};
