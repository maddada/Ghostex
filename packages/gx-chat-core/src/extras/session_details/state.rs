//! What the Session details panel remembers between frames.

use serde_json::Value;

use crate::event::StorageKey;

/// The panel's own state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionDetailsState {
    /// The panel is on screen for this chat, docked or floating. Only then does it read.
    pub visible: bool,
    /// The last answer of `/api/readSessionDetails`.
    pub data: Option<Value>,
    /// Why the last read failed, in words for the user.
    pub error: Option<String>,
    /// The read in flight; an answer to any other id is dropped.
    pub request: Option<u64>,
    /// The read in flight was asked by the refresh button.
    pub refreshing: bool,
    /// A refresh press that waits for the read in flight.
    pub refresh_queued: bool,
    /// Stop requests in flight, with the task each one stops.
    pub stops: Vec<(u64, String)>,
    /// Background commands asked to stop, until they leave the running list.
    pub stopping: Vec<String>,
    /// Folded sections, remembered across restarts.
    pub collapsed: Vec<String>,
    /// The folds were read from storage (or never will be).
    pub collapsed_read: bool,
    /// The lists unfolded inside a section: `finished`, `skills`, `mcp`, `messages`.
    pub open_lists: Vec<String>,
    /// How many "Show more" presses the Tasks card's done list has had; 0 is the short form.
    pub tasks_step: u32,
}

/// Where the folded sections are remembered: a preference of the user, not of one session.
pub fn collapsed_key() -> StorageKey {
    StorageKey {
        store: "sessionDetailsCollapsed".to_string(),
        suffix: String::new(),
    }
}

/// The stored folds: section ids separated by commas.
pub fn read_collapsed(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}
