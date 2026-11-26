//! Which notes go to Grok Bot (`grokBotSends`, Settings > Grok Bot > What Grok Bot gets): Asks
//! only (`asks`), Asks and blockers (`asksAndBlockers`, the default) or Everything (`everything`).
//! A note of a kind the setting leaves out is never queued: it stays in Ghostex, where the agent's
//! own chat, Needs you and the Work panel already carry it, and `notify` says so to the agent.
//!
//! CDXC:GrokBot 2026-11-26 DECISION:
//! User, through Grok Bot: "The Grok Bot notifications are too noisy. My Grok Bot gets woken for every little 'done' note (PM3code 0.2.x published, pushed to main, VM running, setup built...) and I don't want that. ... Grok Bot only gets pinged when something really needs me: a decision, an approval, or a blocker only I can clear. Routine 'done, nothing needed from you' reports shouldn't go to Grok Bot at all; keep them in Ghostex. ... add a setting so I can pick which kinds get sent (asks only, asks + blockers, or everything)." So `done`, `report` and `info` (the kind a note without `--kind` gets) go out only with Everything, `blocked` with Asks and blockers too, and `ask` always. Read fresh on every note, like the switch (feature.rs).

use serde_json::Value;

/// The settings key.
pub(crate) const KEY: &str = "grokBotSends";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sends {
    Asks,
    AsksAndBlockers,
    Everything,
}

impl Sends {
    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("asks") => Self::Asks,
            Some("everything") => Self::Everything,
            _ => Self::AsksAndBlockers,
        }
    }

    /// The choice's label in Settings.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Asks => "Asks only",
            Self::AsksAndBlockers => "Asks and blockers",
            Self::Everything => "Everything",
        }
    }

    /// Whether a note of `kind` goes to Grok Bot.
    pub(crate) fn sends(self, kind: &str) -> bool {
        match kind {
            "ask" => true,
            "blocked" => self != Self::Asks,
            _ => self == Self::Everything,
        }
    }
}

/// The choice in `settings` (the default when there is none or it is unknown).
pub(crate) fn sends_in(settings: Option<&Value>) -> Sends {
    Sends::from_value(
        settings
            .and_then(|settings| settings.get(KEY))
            .and_then(Value::as_str),
    )
}

/// The choice, read fresh from this gxserver's settings file.
pub(crate) fn sends(paths: &crate::paths::GxserverPaths) -> Sends {
    sends_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}

/// What `notify` tells an agent whose note stays in Ghostex.
pub(crate) fn kept_message(kind: &str, sends: Sends) -> String {
    let goes = match sends {
        Sends::Asks => "only --kind ask",
        Sends::AsksAndBlockers => "only --kind ask and --kind blocked",
        Sends::Everything => "every kind",
    };
    let notes = match kind {
        "info" => "info notes (a note without --kind)".to_string(),
        kind => format!("{kind} notes"),
    };
    format!(
        "Not sent to Grok Bot: {notes} stay in Ghostex, where your final message, Needs you and the Work panel already show the user. The user's setting ({}) sends {goes}. Notify only when the user must decide, approve or unblock something only they can.",
        sends.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn asks_and_blockers_by_default() {
        assert_eq!(sends_in(None), Sends::AsksAndBlockers);
        assert_eq!(
            sends_in(Some(&json!({KEY: "nonsense"}))),
            Sends::AsksAndBlockers
        );
        assert_eq!(sends_in(Some(&json!({KEY: "asks"}))), Sends::Asks);
        assert_eq!(
            sends_in(Some(&json!({KEY: "everything"}))),
            Sends::Everything
        );
    }

    #[test]
    fn each_choice_sends_its_kinds() {
        let kinds = ["ask", "blocked", "done", "report", "info"];
        let sent = |sends: Sends| {
            kinds
                .iter()
                .filter(|kind| sends.sends(kind))
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(sent(Sends::Asks), ["ask"]);
        assert_eq!(sent(Sends::AsksAndBlockers), ["ask", "blocked"]);
        assert_eq!(sent(Sends::Everything), kinds);
    }
}
