//! Which chats have their Session details panel open: one choice per chat, a default for the chats
//! the user never toggled, and a card dismissal that waits to see whether the press that caused
//! it was the user leaving the chat.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: each chat remembers its own open or closed state. Opening the panel in chat A and closing it in chat B shows it open on A and closed on B; a chat never toggled keeps the default (the app-wide switch saved before this, else closed). This supersedes the app-wide switch of Q6.
//!
//! CDXC:SessionDetails 2026-10-25 WHY:
//! The panel turned itself off on a chat switch: in a narrow pane it is a floating card, the press on the sidebar row that switches chats takes focus from the card at mouse down, before the switch, and that dismissal counted as "the user closed it". A dismissal is therefore held for `DISMISS_SETTLE` and applied only when the same chat is still the one on screen.

use web_time::{Duration, Instant};

/// How long a card dismissal waits before it counts: long enough for the click that caused it to
/// land (a sidebar row switches the chat on mouse up).
pub(crate) const DISMISS_SETTLE: Duration = Duration::from_millis(600);
/// The most chats remembered; the oldest choice goes first. Sessions are deleted in gxserver and
/// the app keeps no deletion hook, so the list is capped instead of growing forever.
const MAX_CHATS: usize = 200;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SessionDetailsChoices {
    /// For a chat the user never toggled.
    pub(crate) default_open: bool,
    /// Each toggled chat's choice, by session key, the most recent last.
    chats: Vec<(String, bool)>,
    /// A card dismissal not yet applied: the chat and when it came.
    pending: Option<(String, Instant)>,
}

// The web build keeps its choices for the page only, so it never reads or saves them.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl SessionDetailsChoices {
    pub(crate) fn new(default_open: bool) -> Self {
        Self {
            default_open,
            ..Self::default()
        }
    }

    /// The panel is open for this chat.
    pub(crate) fn is_open(&self, key: &str) -> bool {
        if self.pending.as_ref().is_some_and(|(chat, _)| chat == key) {
            return false;
        }
        self.chats
            .iter()
            .find(|(chat, _)| chat == key)
            .map_or(self.default_open, |(_, open)| *open)
    }

    /// The user opened or closed the panel of this chat.
    pub(crate) fn set(&mut self, key: &str, open: bool) {
        if self.pending.as_ref().is_some_and(|(chat, _)| chat == key) {
            self.pending = None;
        }
        self.chats.retain(|(chat, _)| chat != key);
        self.chats.push((key.to_string(), open));
        if self.chats.len() > MAX_CHATS {
            let excess = self.chats.len() - MAX_CHATS;
            self.chats.drain(..excess);
        }
    }

    /// This chat's floating card was dismissed; it counts once `settle` sees the chat still shown.
    pub(crate) fn dismissed(&mut self, key: &str, now: Instant) {
        self.pending = Some((key.to_string(), now));
    }

    /// Applies or drops a dismissal that has waited `DISMISS_SETTLE`. `shown` is the chat whose
    /// panel the button controls now, when it is on screen. True when a choice changed.
    pub(crate) fn settle(&mut self, shown: Option<&str>, now: Instant) -> bool {
        let Some((chat, at)) = self.pending.as_ref() else {
            return false;
        };
        if now.saturating_duration_since(*at) < DISMISS_SETTLE {
            return false;
        }
        let chat = chat.clone();
        self.pending = None;
        if shown == Some(chat.as_str()) {
            self.set(&chat, false);
            return true;
        }
        false
    }

    /// The saved form: `{"open": default, "chats": [[key, open], …]}`.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "open": self.default_open,
            "chats": self.chats.iter().map(|(chat, open)| serde_json::json!([chat, open])).collect::<Vec<_>>(),
        })
    }

    /// Reads the saved form; the app-wide switch saved before per-chat choices is the default.
    pub(crate) fn from_json(value: &serde_json::Value) -> Self {
        let mut choices =
            Self::new(value.get("open").and_then(serde_json::Value::as_bool) == Some(true));
        for entry in value
            .get("chats")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(chat), Some(open)) = (
                entry.get(0).and_then(serde_json::Value::as_str),
                entry.get(1).and_then(serde_json::Value::as_bool),
            ) {
                choices.set(chat, open);
            }
        }
        choices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_chat_keeps_its_own_choice() {
        let mut choices = SessionDetailsChoices::new(false);
        choices.set("a", true);
        choices.set("b", false);
        assert!(choices.is_open("a"));
        assert!(!choices.is_open("b"));
        assert!(!choices.is_open("never-toggled"));
        let open_default = SessionDetailsChoices::new(true);
        assert!(open_default.is_open("never-toggled"));
    }

    #[test]
    fn a_dismissal_counts_only_when_the_chat_is_still_shown() {
        let start = Instant::now();
        let mut choices = SessionDetailsChoices::new(false);
        choices.set("a", true);
        choices.dismissed("a", start);
        // Closed while it waits, so nothing reopens the card.
        assert!(!choices.is_open("a"));
        assert!(!choices.settle(Some("a"), start));
        // The press switched to chat b: a keeps its panel.
        assert!(!choices.settle(Some("b"), start + DISMISS_SETTLE));
        assert!(choices.is_open("a"));
        // A click on chat a itself closes it for good.
        choices.dismissed("a", start);
        assert!(choices.settle(Some("a"), start + DISMISS_SETTLE));
        assert!(!choices.is_open("a"));
    }

    #[test]
    fn toggling_cancels_a_waiting_dismissal() {
        let start = Instant::now();
        let mut choices = SessionDetailsChoices::new(false);
        choices.set("a", true);
        choices.dismissed("a", start);
        choices.set("a", true);
        assert!(!choices.settle(Some("a"), start + DISMISS_SETTLE));
        assert!(choices.is_open("a"));
    }

    #[test]
    fn saved_choices_round_trip_and_stay_capped() {
        let mut choices = SessionDetailsChoices::new(true);
        for index in 0..(MAX_CHATS + 5) {
            choices.set(&format!("chat-{index}"), index % 2 == 0);
        }
        let read = SessionDetailsChoices::from_json(&choices.to_json());
        assert_eq!(read, choices);
        assert_eq!(read.chats.len(), MAX_CHATS);
        // The oldest choices went; those chats fall back to the default.
        assert!(read.is_open("chat-1"));
        assert!(!read.is_open(&format!("chat-{}", MAX_CHATS + 3)));
    }

    #[test]
    fn the_old_app_wide_switch_becomes_the_default() {
        let read = SessionDetailsChoices::from_json(&serde_json::json!({"open": true}));
        assert!(read.is_open("any"));
        assert!(!SessionDetailsChoices::from_json(&serde_json::json!({})).is_open("any"));
    }
}
