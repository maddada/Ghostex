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
    /// The user's own folds, remembered across restarts: the ids of sections they folded, and
    /// `<id>:open` for a section folded by default ([`FOLDED_BY_DEFAULT`]) that they unfolded.
    /// Read it through [`is_collapsed`].
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

/// CDXC:SessionDetails 2026-11-14 DECISION:
/// User: "please let me collapse this one, and it should start collapsed, and when it's collapsed
/// it should still show the big bar up top". The Context card folds again (it had been pinned open
/// with the Overview tab, Q2763) and starts folded; folded it still shows its usage line, the
/// overall bar and Compact context, and unfolding shows the breakdown. The fold stays one
/// remembered choice for the whole app (as every card's fold is): a user who never touched
/// Context's fold sees it folded, one who unfolded it keeps it open.
pub const FOLDED_BY_DEFAULT: &[&str] = &["context"];

/// The stored mark of a section folded by default that the user unfolded.
fn opened_mark(id: &str) -> String {
    format!("{id}:open")
}

/// Whether the section `id` is folded, given the stored folds.
pub fn is_collapsed(stored: &[String], id: &str) -> bool {
    if FOLDED_BY_DEFAULT.contains(&id) {
        let mark = opened_mark(id);
        !stored.iter().any(|known| *known == mark)
    } else {
        stored.iter().any(|known| known == id)
    }
}

/// Folds or unfolds the section `id` in the stored folds.
pub fn toggle_collapsed(stored: &mut Vec<String>, id: &str) {
    let entry = if FOLDED_BY_DEFAULT.contains(&id) {
        opened_mark(id)
    } else {
        id.to_string()
    };
    if let Some(at) = stored.iter().position(|known| *known == entry) {
        stored.remove(at);
    } else {
        stored.push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_starts_folded_and_remembers_being_unfolded() {
        let mut stored = read_collapsed(None);
        assert!(is_collapsed(&stored, "context"));
        assert!(!is_collapsed(&stored, "environment"));
        toggle_collapsed(&mut stored, "context");
        assert!(!is_collapsed(&stored, "context"));
        assert_eq!(stored, vec!["context:open".to_string()]);
        let stored_again = read_collapsed(Some(&stored.join(",")));
        assert!(!is_collapsed(&stored_again, "context"));
        toggle_collapsed(&mut stored, "context");
        assert!(is_collapsed(&stored, "context"));
        assert!(stored.is_empty());
    }

    #[test]
    fn other_cards_still_fold_only_when_folded() {
        let mut stored = read_collapsed(Some("environment"));
        assert!(is_collapsed(&stored, "environment"));
        toggle_collapsed(&mut stored, "environment");
        assert!(!is_collapsed(&stored, "environment"));
        // A fold of Context stored by an older build ("context") still reads as folded.
        assert!(is_collapsed(&read_collapsed(Some("context")), "context"));
    }
}
