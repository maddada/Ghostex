//! What Handoff / Export remembers between opens, and what it makes of an older saved choice.
//!
//! CDXC:TranscriptExport 2026-10-31 DECISION:
//! User: "The 'include' part needs to be collapsed by default please, and default to all deselected except if user changes it then we remember", the top holds just Handoff to an agent and Handoff to MD, and "we should remember what the user picked last time so we keep that selection for next time". So this remembers the card, the Include switches and whether Include is open, the agent picked under Continue with, and the destination.
//! A value saved before this shape carries switches the old dialog defaulted on, written whenever anything changed, so they can't tell a choice from a default: everyone starts at the new default, and only a change from here on counts. An old mode for another computer or project (`handoffRemote`) comes back as Handoff to an agent with the destination choices open; it never saved a project, so there is none to restore.
//! A remembered destination is only a suggestion: one that is gone, or is the project the session is already in, falls back to this project here. Handing off from a different project can still open on a project remembered from before, so the dialog shows the destination row open and the button names it; nothing goes anywhere the button doesn't say.
//!
//!
//! CDXC:TranscriptExport 2026-11-03 DECISION:
//! User (Q1150 to Q1159): Include also holds the History: a "Whole conversation" switch, and under it, while it is off, a slider over the last 1, 2, 3, 5, 10, 20, 50 or 100 turns. It starts on the last 20 turns and is remembered like Include and the agent. A turn is the user's message and everything up to their next one; the computer cuts the file (`lastTurns`) and lists the older turns' messages. The Include heading says both, e.g. "Last 10 turns · Messages only", and the slider's hint says "10 of 120 turns · about 9k tokens (the whole is 87k)", a token being about four bytes of the file.
//!
//! SEE-ALSO: apps/desktop/src/app/window/export_transcript_modal.rs (the desktop's dialog),
//! apps/mobile/app/src/contract/handoff/handoff.ts (the phone's port, held to this by
//! `handoffFixture.json`).

use serde_json::{json, Value};

const PREFS_VERSION: u64 = 2;

/// The two cards at the top of the dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandoffCard {
    #[default]
    Agent,
    Markdown,
}

/// What the conversation's file holds beyond its messages. All off until the user changes them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandoffInclude {
    pub commands: bool,
    pub patches: bool,
    pub reasoning: bool,
}

/// Where a handoff starts its new session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffDestination {
    pub machine_id: String,
    pub project_id: String,
}

/// How much of the conversation the file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandoffHistory {
    /// The "Whole conversation" switch. Off, the file holds the last `turns` turns.
    pub all: bool,
    /// One of [`HANDOFF_HISTORY_STOPS`].
    pub turns: usize,
}

impl Default for HandoffHistory {
    fn default() -> Self {
        Self {
            all: false,
            turns: HANDOFF_HISTORY_DEFAULT_TURNS,
        }
    }
}

impl HandoffHistory {
    /// The computer's `lastTurns`: `None` for the whole conversation.
    pub fn last_turns(&self) -> Option<usize> {
        (!self.all).then_some(self.turns)
    }
}

/// The slider's stops.
pub const HANDOFF_HISTORY_STOPS: [usize; 8] = [1, 2, 3, 5, 10, 20, 50, 100];
pub const HANDOFF_HISTORY_DEFAULT_TURNS: usize = 20;

/// The stop nearest `turns`, the lower one on a tie.
pub fn handoff_history_stop(turns: usize) -> usize {
    HANDOFF_HISTORY_STOPS
        .into_iter()
        .min_by_key(|stop| stop.abs_diff(turns))
        .unwrap_or(HANDOFF_HISTORY_DEFAULT_TURNS)
}

/// The stop's index on the slider.
pub fn handoff_history_stop_index(turns: usize) -> usize {
    let stop = handoff_history_stop(turns);
    HANDOFF_HISTORY_STOPS
        .iter()
        .position(|candidate| *candidate == stop)
        .unwrap_or_default()
}

/// The slider's value and the Include heading's first half: "Last 20 turns", "Last turn",
/// "Whole conversation".
pub fn handoff_history_label(history: &HandoffHistory) -> String {
    match (history.all, history.turns) {
        (true, _) => "Whole conversation".to_string(),
        (false, 1) => "Last turn".to_string(),
        (false, turns) => format!("Last {turns} turns"),
    }
}

/// The Include heading: the history, then what the file holds beyond messages, e.g.
/// "Last 10 turns · Messages only".
pub fn handoff_include_summary(history: &HandoffHistory, include: &str) -> String {
    format!("{} · {include}", handoff_history_label(history))
}

/// About how many tokens `bytes` of the file are, as the hint writes them: "900", "9k", "87k",
/// "1.2M". A token is about four bytes.
pub fn handoff_tokens_text(bytes: usize) -> String {
    let tokens = bytes.div_ceil(4);
    if tokens < 1_000 {
        // Round to tens so it doesn't read as exact.
        return (tokens.div_ceil(10) * 10).max(10).to_string();
    }
    if tokens < 999_500 {
        return format!("{}k", (tokens + 500) / 1_000);
    }
    let tenths = (tokens + 50_000) / 100_000;
    if tenths % 10 == 0 {
        format!("{}M", tenths / 10)
    } else {
        format!("{}.{}M", tenths / 10, tenths % 10)
    }
}

/// The slider's hint, from the computer's count (`countSessionTranscriptTurns`): "10 of 120 turns ·
/// about 9k tokens (the whole is 87k)". `last_turns_bytes[i]` is the size of the last `i + 1` turns.
/// The whole conversation, or a stop past the conversation's turns, is all of it: "All 12 turns ·
/// about 9k tokens".
pub fn handoff_history_estimate(
    history: &HandoffHistory,
    turns: usize,
    whole_bytes: usize,
    last_turns_bytes: &[usize],
) -> String {
    let all_of_it = |turns: usize| match turns {
        1 => "The one turn".to_string(),
        turns => format!("All {turns} turns"),
    };
    if history.all || history.turns >= turns {
        return format!(
            "{} · about {} tokens",
            all_of_it(turns),
            handoff_tokens_text(whole_bytes)
        );
    }
    let kept = last_turns_bytes
        .get(history.turns.saturating_sub(1))
        .copied()
        .unwrap_or(whole_bytes);
    format!(
        "{} of {turns} turns · about {} tokens (the whole is {})",
        history.turns,
        handoff_tokens_text(kept),
        handoff_tokens_text(whole_bytes)
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandoffPrefs {
    pub card: HandoffCard,
    pub include: HandoffInclude,
    pub include_open: bool,
    /// The agent the user picked under Continue with; absent means the session's own.
    pub agent_id: Option<String>,
    pub destination: Option<HandoffDestination>,
    /// The destination's computer and project choices open with nothing remembered to show.
    pub destination_open: bool,
    pub history: HandoffHistory,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}

/// Reads a saved value, of this shape or an older one. Anything unreadable is the default.
pub fn parse_handoff_prefs(value: &Value) -> HandoffPrefs {
    let card = match text(value, "mode") {
        Some("export") => HandoffCard::Markdown,
        _ => HandoffCard::Agent,
    };
    if value.get("version").and_then(Value::as_u64) != Some(PREFS_VERSION) {
        return HandoffPrefs {
            card,
            destination_open: text(value, "mode") == Some("handoffRemote"),
            ..HandoffPrefs::default()
        };
    }
    HandoffPrefs {
        card,
        include: HandoffInclude {
            commands: flag(value, "includeCommands"),
            patches: flag(value, "includePatches"),
            reasoning: flag(value, "includeReasoning"),
        },
        include_open: flag(value, "includeOpen"),
        agent_id: text(value, "agentId").map(str::to_string),
        destination: value.get("destination").and_then(|destination| {
            Some(HandoffDestination {
                machine_id: text(destination, "machineId")?.to_string(),
                project_id: text(destination, "projectId")?.to_string(),
            })
        }),
        destination_open: flag(value, "destinationOpen"),
        // Saved before the slider: its default.
        history: HandoffHistory {
            all: flag(value, "historyAll"),
            turns: value
                .get("historyTurns")
                .and_then(Value::as_u64)
                .map_or(HANDOFF_HISTORY_DEFAULT_TURNS, |turns| {
                    handoff_history_stop(turns as usize)
                }),
        },
    }
}

/// The value to save, in this shape.
pub fn handoff_prefs_json(prefs: &HandoffPrefs) -> Value {
    let mut value = json!({
        "version": PREFS_VERSION,
        "mode": match prefs.card {
            HandoffCard::Agent => "handoff",
            HandoffCard::Markdown => "export",
        },
        "includeCommands": prefs.include.commands,
        "includePatches": prefs.include.patches,
        "includeReasoning": prefs.include.reasoning,
        "includeOpen": prefs.include_open,
        "destinationOpen": prefs.destination_open,
        "historyAll": prefs.history.all,
        "historyTurns": prefs.history.turns,
    });
    if let Some(agent_id) = &prefs.agent_id {
        value["agentId"] = json!(agent_id);
    }
    if let Some(destination) = &prefs.destination {
        value["destination"] = json!({
            "machineId": destination.machine_id,
            "projectId": destination.project_id,
        });
    }
    value
}

/// The remembered destination this session opens on, or `None` for this project here: `None` when
/// it is the session's own project, or `offered` says that computer no longer offers that project.
pub fn remembered_handoff_destination(
    remembered: Option<&HandoffDestination>,
    own: &HandoffDestination,
    offered: impl Fn(&HandoffDestination) -> bool,
) -> Option<HandoffDestination> {
    remembered
        .filter(|destination| *destination != own && offered(destination))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination(machine: &str, project: &str) -> HandoffDestination {
        HandoffDestination {
            machine_id: machine.to_string(),
            project_id: project.to_string(),
        }
    }

    #[test]
    fn with_nothing_saved_it_is_the_agent_card_with_include_closed_and_off() {
        let prefs = parse_handoff_prefs(&Value::Null);
        assert_eq!(prefs, HandoffPrefs::default());
        assert_eq!(prefs.card, HandoffCard::Agent);
        assert!(!prefs.include_open);
        assert_eq!(prefs.include, HandoffInclude::default());
        assert!(!prefs.include.commands && !prefs.include.patches && !prefs.include.reasoning);
    }

    #[test]
    fn an_old_saved_value_starts_at_the_new_default_and_keeps_its_card() {
        // Exactly what the old dialog wrote, with its switches at its old defaults.
        let old = json!({
            "mode": "export",
            "includeCommands": true,
            "includePatches": true,
            "includeReasoning": false,
        });
        let prefs = parse_handoff_prefs(&old);
        assert_eq!(prefs.card, HandoffCard::Markdown);
        assert_eq!(prefs.include, HandoffInclude::default());
        assert!(!prefs.include_open);
    }

    #[test]
    fn an_old_handoff_to_remote_is_a_handoff_to_an_agent_with_its_choices_open() {
        let prefs = parse_handoff_prefs(&json!({"mode": "handoffRemote", "includeCommands": true}));
        assert_eq!(prefs.card, HandoffCard::Agent);
        assert!(prefs.destination_open);
        assert_eq!(prefs.destination, None);
    }

    #[test]
    fn what_the_user_changes_comes_back() {
        let prefs = HandoffPrefs {
            card: HandoffCard::Markdown,
            include: HandoffInclude {
                commands: true,
                patches: false,
                reasoning: true,
            },
            include_open: true,
            agent_id: Some("codex".to_string()),
            destination: Some(destination("studio", "acme")),
            destination_open: false,
            history: HandoffHistory {
                all: true,
                turns: 5,
            },
        };
        assert_eq!(parse_handoff_prefs(&handoff_prefs_json(&prefs)), prefs);
        // Switching a switch back off stays off, rather than reading as an old value.
        let cleared = HandoffPrefs::default();
        assert_eq!(parse_handoff_prefs(&handoff_prefs_json(&cleared)), cleared);
    }

    #[test]
    fn history_starts_on_the_last_20_turns_and_a_saved_number_snaps_to_a_stop() {
        assert_eq!(
            parse_handoff_prefs(&Value::Null).history,
            HandoffHistory {
                all: false,
                turns: 20
            }
        );
        // Saved before the slider existed.
        assert_eq!(
            parse_handoff_prefs(&json!({"version": 2, "includeCommands": true})).history,
            HandoffHistory::default()
        );
        let saved = |turns: u64| {
            parse_handoff_prefs(&json!({"version": 2, "historyTurns": turns}))
                .history
                .turns
        };
        assert_eq!(saved(50), 50);
        assert_eq!(saved(7), 5);
        assert_eq!(saved(4000), 100);
        assert_eq!(saved(0), 1);
        assert_eq!(HandoffHistory::default().last_turns(), Some(20));
        let whole = HandoffHistory {
            all: true,
            turns: 20,
        };
        assert_eq!(whole.last_turns(), None);
    }

    #[test]
    fn the_heading_and_the_hint_say_what_the_file_holds() {
        let last = |turns| HandoffHistory { all: false, turns };
        assert_eq!(
            handoff_include_summary(&last(10), "Messages only"),
            "Last 10 turns · Messages only"
        );
        assert_eq!(handoff_history_label(&last(1)), "Last turn");
        let whole = HandoffHistory {
            all: true,
            turns: 10,
        };
        assert_eq!(
            handoff_include_summary(&whole, "Reasoning"),
            "Whole conversation · Reasoning"
        );
        let mut sizes = vec![0; 120];
        sizes[9] = 36_000;
        assert_eq!(
            handoff_history_estimate(&last(10), 120, 348_000, &sizes),
            "10 of 120 turns · about 9k tokens (the whole is 87k)"
        );
        // More turns than the conversation has, or the switch on: all of it.
        assert_eq!(
            handoff_history_estimate(&last(20), 12, 36_000, &sizes[..12]),
            "All 12 turns · about 9k tokens"
        );
        assert_eq!(
            handoff_history_estimate(&whole, 1, 1_000, &[1_000]),
            "The one turn · about 250 tokens"
        );
        assert_eq!(handoff_tokens_text(7), "10");
        assert_eq!(handoff_tokens_text(4_000), "1k");
        assert_eq!(handoff_tokens_text(5_000_000), "1.3M");
        assert_eq!(handoff_tokens_text(8_000_000), "2M");
    }

    #[test]
    fn a_half_written_destination_is_no_destination() {
        let prefs = parse_handoff_prefs(&json!({
            "version": 2,
            "destination": {"machineId": "studio"},
        }));
        assert_eq!(prefs.destination, None);
    }

    #[test]
    fn a_remembered_destination_that_no_longer_applies_falls_back_to_here() {
        let own = destination("local", "scratch");
        let acme = destination("local", "acme");
        // Still offered, and elsewhere: it comes back.
        assert_eq!(
            remembered_handoff_destination(Some(&acme), &own, |_| true),
            Some(acme.clone())
        );
        // Its computer or project is gone.
        assert_eq!(
            remembered_handoff_destination(Some(&acme), &own, |_| false),
            None
        );
        // It is the project this session is already in.
        assert_eq!(
            remembered_handoff_destination(Some(&own), &own, |_| true),
            None
        );
        // Nothing remembered.
        assert_eq!(remembered_handoff_destination(None, &own, |_| true), None);
    }
}
