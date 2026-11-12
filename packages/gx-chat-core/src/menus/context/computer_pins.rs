//! The status line of the session's computer: the rows its own chat (its desktop app) pins under
//! the chat box, which every chat read from it carries (`statusLinePins`), drawn by a host that
//! follows the computer in place of its own pins.
//!
//! CDXC:SessionChat 2026-11-12 DECISION:
//! User, asked how the phone's status line should follow the desktop (Q1, answer C "With each
//! chat"): the computer sends its status line pins with each chat, the way Status line in the chat
//! already reaches the phone, and the phone shows whatever the open chat's computer uses, with no
//! Settings > Sync switch; the phone's own pins apply only when the computer sends none (an older
//! computer, or one with no desktop app). gxserver reads the desktop's saved records
//! (`server/src/session_chat_status_line.rs`); only the pins (`starred`, `starredOrder`) are the
//! computer's, while which rows More details shows stays the host's own. The host asks for it
//! with `ChatSettings::status_line_from_computer`: the phone does, and the desktop and the web
//! build keep their own line, which is the one their gxserver sends.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::Value;

use crate::menus::context::preferences::{
    context_preferences_key, normalize_preferences, ContextDetailsPreferences,
};
use crate::menus::context::ContextDetailsAgent;

/// What the status line follows, kept beside the host's own records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputerPins {
    /// The host draws the computer's pins (`ChatSettings::status_line_from_computer`).
    pub followed: bool,
    /// The newest read's `statusLinePins`, per store; `None` until a read from a computer that
    /// sends them arrives.
    pub records: Option<BTreeMap<String, Value>>,
}

impl ComputerPins {
    /// The status line drawn is the computer's: the host follows it and it sent pins. Context
    /// details then says so (`statusLineFromComputer` on `contextEditor`), as its stars still edit
    /// this host's own pins (CDXC:SessionChat 2026-11-12, the user's answer 2A: "keep editing the
    /// phone's own pins and add the line in Context details saying the status line comes from the
    /// computer's Ghostex app").
    pub fn applies(&self) -> bool {
        self.followed && self.records.is_some()
    }

    /// The record the meter draws for `agent`: `own` with its pins replaced by the computer's
    /// while the host follows a computer that sent them, else `own` itself. An agent the
    /// computer's app never saved a record for pins nothing there, so it pins nothing here.
    pub fn preferences<'a>(
        &self,
        agent: ContextDetailsAgent,
        own: &'a ContextDetailsPreferences,
    ) -> Cow<'a, ContextDetailsPreferences> {
        let Some(records) = self.records.as_ref().filter(|_| self.followed) else {
            return Cow::Borrowed(own);
        };
        let computer =
            normalize_preferences(records.get(&context_preferences_key(agent).store), agent);
        Cow::Owned(ContextDetailsPreferences {
            starred: computer.starred,
            starred_order: computer.starred_order,
            ..own.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::context::preferences::default_preferences;
    use serde_json::json;

    fn own() -> ContextDetailsPreferences {
        let mut own = default_preferences(ContextDetailsAgent::Codex);
        own.starred.insert("branch".into(), true);
        own.starred_order = vec!["branch".into()];
        own
    }

    fn computer(records: Value) -> ComputerPins {
        ComputerPins {
            followed: true,
            records: serde_json::from_value(records).ok(),
        }
    }

    #[test]
    fn a_host_that_does_not_follow_keeps_its_own_pins() {
        let pins = ComputerPins {
            followed: false,
            ..computer(json!({ "codexContext": { "starred": { "sevenDayLimit": true } } }))
        };
        assert_eq!(*pins.preferences(ContextDetailsAgent::Codex, &own()), own());
    }

    #[test]
    fn applies_only_while_following_a_computer_that_sent_pins() {
        assert!(computer(json!({})).applies());
        assert!(!ComputerPins {
            followed: false,
            ..computer(json!({}))
        }
        .applies());
        assert!(!ComputerPins {
            followed: true,
            records: None
        }
        .applies());
    }

    #[test]
    fn a_computer_that_sends_no_pins_leaves_the_hosts_own() {
        let pins = ComputerPins {
            followed: true,
            records: None,
        };
        assert_eq!(*pins.preferences(ContextDetailsAgent::Codex, &own()), own());
    }

    #[test]
    fn the_computers_pins_replace_the_hosts_and_keep_its_shown_rows() {
        let pins = computer(json!({
            "codexContext": {
                "shown": { "pr": true },
                "starred": { "sevenDayLimit": true, "sevenDayReset": true },
                "starredOrder": ["sevenDayReset", "sevenDayLimit"],
            }
        }));
        let own = own();
        let drawn = pins.preferences(ContextDetailsAgent::Codex, &own);
        assert_eq!(drawn.starred.get("branch"), None);
        assert_eq!(drawn.starred.get("sevenDayLimit"), Some(&true));
        assert_eq!(drawn.starred_order, ["sevenDayReset", "sevenDayLimit"]);
        assert_eq!(drawn.shown, own.shown);
    }

    #[test]
    fn an_agent_the_computer_never_saved_pins_nothing() {
        let pins = computer(json!({ "claudeContext": { "starred": { "repo": true } } }));
        let own = own();
        let drawn = pins.preferences(ContextDetailsAgent::Codex, &own);
        assert!(drawn.starred.values().all(|starred| !starred));
        assert!(drawn.starred_order.is_empty());
    }
}
