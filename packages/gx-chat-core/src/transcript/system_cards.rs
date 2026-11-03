//! Which card a system turn renders as.
//!
//! Ported from `packages/shared/session-chat-presentation/system-cards.ts`. Both renderers classify
//! here so a row cannot be a titled card in one and raw prose in the other; each renderer owns only
//! the layout of the card it is handed.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::agent_message::{agent_display_name, parse_agent_message};
use crate::transcript::foreign::{CODEX_GOAL_ID_PREFIX, FORK_BOUNDARY_ID_PREFIX};
use crate::transcript::jsstr::js_trim;

/// The id `sessionChatPendingMessages` writes for a local command's captured output.
const APP_COMMAND_OUTPUT_ID_PREFIX: &str = "app-command-output:";
/// The id it writes for a command Ghostex ran on the session's behalf, of which the rename is the
/// one with its own card.
const APP_COMMAND_ID_PREFIX: &str = "app-command:";
pub(crate) const AUTO_NAMED_TITLE_LEAD: &str = "Ghostex auto named this session";
/// CDXC:SessionTitles 2026-09-30 WHY: a name the user typed (sidebar Rename, rename modal) reaches the agent as the same `/rename` Ghostex sends for its own titles, so gxserver marks it `userRename` and the card says so instead of claiming Ghostex named the session.
pub(crate) const USER_RENAMED_TITLE_LEAD: &str = "Session renamed";

/// The `customType` of an IRC message another OMP agent sent this session
/// (`session/irc-bridge.ts` in oh-my-pi).
pub const OMP_IRC_INCOMING: &str = "irc:incoming";
/// The `customType` of OMP's advisor notes (`session/session-advisors.ts` in oh-my-pi).
pub const OMP_ADVISOR: &str = "advisor";

/// One OMP advisor severity: its name, the word its note is labelled with, and the tone and icon
/// its pill and label wear (`tone` as on a message card's status: `failed` red, `waiting` amber,
/// `neutral` the card's muted text).
struct AdvisorRank {
    severity: &'static str,
    label: &'static str,
    tone: &'static str,
    icon: &'static str,
}

/// OMP's advisor severities worst first; a note without one is a plain note (`RANK_LOOK` in
/// oh-my-pi's `tui/src/chat/advisor-message.ts`).
const ADVISOR_RANKS: [AdvisorRank; 4] = [
    AdvisorRank {
        severity: "blocker",
        label: "Blocker",
        tone: "failed",
        icon: "alert-octagon",
    },
    AdvisorRank {
        severity: "concern",
        label: "Concern",
        tone: "waiting",
        icon: "alert-triangle",
    },
    AdvisorRank {
        severity: "nit",
        label: "Nit",
        tone: "neutral",
        icon: "bulb",
    },
    AdvisorRank {
        severity: "note",
        label: "Note",
        tone: "neutral",
        icon: "info-circle",
    },
];

fn block_text(message: &ChatMessage, index: usize) -> &str {
    match message.blocks.get(index) {
        Some(ChatBlock::Text { text }) => text,
        _ => "",
    }
}

/// The card an OMP custom message the chat draws as its own card renders as, from the details
/// gxserver passed with it; `None` for any other row, or one whose details are missing, which
/// stays the marker line it always was.
///
/// CDXC:SessionChat 2026-10-30 WHY: an IRC message from another OMP agent arrives as a system row whose text wraps the message in instructions for the model ("reply via write …"), so it reads as the received `agent-message` card with OMP's logo whose body is only the message (`details.message`), like every other message between agents (the 2026-10-12 decision in agent_message_card.rs). Advisor notes arrive as `<advisory>` elements, so they become one `advisor` card: the notes worst first, each with its severity, and one count per severity; `text` keeps the row's text for a renderer that does not know the kind yet.
fn omp_card(message: &ChatMessage, markdown: &str) -> Option<Value> {
    let custom = message.custom_message.as_ref()?;
    let details = custom.details.as_ref()?;
    match custom.custom_type.as_str() {
        OMP_IRC_INCOMING => {
            let body = details.get("message")?.as_str()?;
            let sender = details
                .get("from")
                .and_then(Value::as_str)
                .map(js_trim)
                .unwrap_or_default();
            Some(json!({
                "kind": "agent-message",
                "sender": sender,
                "name": sender,
                "body": body,
                "card": crate::transcript::agent_message_card::agent_cli_received_card("omp", sender),
            }))
        }
        OMP_ADVISOR => advisor_card(details, markdown),
        _ => None,
    }
}

/// The `advisor` card for OMP's `details.notes`, or `None` when no note has text.
fn advisor_card(details: &Value, markdown: &str) -> Option<Value> {
    let notes = details.get("notes")?.as_array()?;
    let rank_of = |note: &Value| {
        note.get("severity")
            .and_then(Value::as_str)
            .and_then(|severity| ADVISOR_RANKS.iter().find(|rank| rank.severity == severity))
            .map_or("note", |rank| rank.severity)
    };
    let mut drawn = Vec::new();
    let mut counts = Vec::new();
    for rank in &ADVISOR_RANKS {
        let before = drawn.len();
        for note in notes.iter().filter(|note| rank_of(note) == rank.severity) {
            let Some(text) = note
                .get("note")
                .and_then(Value::as_str)
                .map(js_trim)
                .filter(|text| !text.is_empty())
            else {
                continue;
            };
            drawn.push(json!({
                "severity": rank.severity,
                "label": rank.label,
                "tone": rank.tone,
                "icon": rank.icon,
                "text": text,
            }));
        }
        let count = drawn.len() - before;
        if count > 0 {
            let plural = if count == 1 { "" } else { "s" };
            counts.push(json!({
                "severity": rank.severity,
                "label": format!("{count} {}{plural}", rank.severity),
                "tone": rank.tone,
                "icon": rank.icon,
            }));
        }
    }
    if drawn.is_empty() {
        return None;
    }
    let tally: Vec<&str> = counts
        .iter()
        .filter_map(|count| count["label"].as_str())
        .collect();
    Some(json!({
        "kind": "advisor",
        "name": "Advisor shared notes",
        "title": format!("Advisor shared notes: {}", tally.join(", ")),
        "counts": counts,
        "notes": drawn,
        "text": markdown,
    }))
}

/// Which card a system turn renders as, or `Value::Null` when the turn is not a system turn.
///
/// Suppressed harness turns are decided first by the noise classifier and never reach this.
/// `marker` is the ordinary case: one muted line of the text the daemon wrote.
pub fn classify_system_card(message: &ChatMessage, markdown: &str) -> Value {
    if message.role != ChatRole::System {
        return Value::Null;
    }
    if let Some(card) = omp_card(message, markdown) {
        return card;
    }
    let lead = block_text(message, 0);
    let user_renamed = lead == USER_RENAMED_TITLE_LEAD;
    let renamed_title = if message.id.starts_with(APP_COMMAND_ID_PREFIX)
        && (lead == AUTO_NAMED_TITLE_LEAD || user_renamed)
    {
        js_trim(block_text(message, 1))
    } else {
        ""
    };
    if !renamed_title.is_empty() {
        return json!({
            "kind": "auto-named",
            "title": renamed_title,
            "lead": lead,
            "userRenamed": user_renamed,
        });
    }
    if message.id.starts_with(FORK_BOUNDARY_ID_PREFIX) {
        return json!({ "kind": "fork-boundary", "text": markdown });
    }
    if message.id.starts_with(CODEX_GOAL_ID_PREFIX) {
        return json!({
            "kind": "goal",
            "status": block_text(message, 0),
            "objective": block_text(message, 1),
            "usage": block_text(message, 2),
        });
    }
    if message.id.starts_with(APP_COMMAND_OUTPUT_ID_PREFIX) {
        return json!({
            "kind": "command-output",
            "command": block_text(message, 0),
            "output": block_text(message, 1),
        });
    }
    if let Some(agent_message) = parse_agent_message(markdown) {
        return json!({
            "kind": "agent-message",
            "sender": agent_message.sender,
            "name": agent_display_name(&agent_message.sender),
            "body": agent_message.body,
        });
    }
    json!({ "kind": "marker", "text": markdown })
}
