//! Cards a `ghostex` command prints into an agent's tool output for the chat to draw: one line,
//! [`CHAT_CARD_MARKER`] then a JSON object whose `kind` names the card. The chat core finds the line
//! in the tool call's result, so no transcript format changes and every agent (Claude, Codex, …)
//! gets the card the same way.
//!
//! Kinds: `gbotAdded` (`ghostex gbots create` run by Chief of Staff): `{ kind, changeId, botId,
//! name, title, icon, color, colorHex, agentId }`; `routineSet` ([`ROUTINE_SET`]).
//!
//! CDXC:GBots 2026-11-09 SEE-ALSO: server/src/gbots/card.rs (writes it), server/src/ghostex_cli/gbots_cli.rs (prints it), packages/gx-chat-core/src/transcript/gbot_cards.rs (reads it).

use serde_json::Value;

/// The start of a card line.
pub const CHAT_CARD_MARKER: &str = "ghostex-chat-card: ";

/// The `gbotAdded` kind.
pub const GBOT_ADDED: &str = "gbotAdded";

/// The `routineSet` kind (`ghostex gbots routine add` run in an agent's chat): `{ kind, changeId,
/// routineId, botId, botName, icon, colorHex, name, scheduleSentence, precheckSentence }`.
pub const ROUTINE_SET: &str = "routineSet";

/// The line a command prints for `card`.
pub fn chat_card_line(card: &Value) -> String {
    format!("{CHAT_CARD_MARKER}{card}")
}

/// Every card in a command's output: its marker lines, and the `chatCard` of a JSON answer
/// (`--json`), in order.
pub fn chat_cards_in(output: &str) -> Vec<Value> {
    let mut cards: Vec<Value> = output
        .lines()
        .filter_map(|line| line.trim().strip_prefix(CHAT_CARD_MARKER))
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter(|card| card.get("kind").and_then(Value::as_str).is_some())
        .collect();
    if cards.is_empty() && output.contains("\"chatCard\"") {
        if let Ok(answer) = serde_json::from_str::<Value>(output.trim()) {
            if let Some(card) = answer
                .get("chatCard")
                .filter(|card| card.get("kind").and_then(Value::as_str).is_some())
            {
                cards.push(card.clone());
            }
        }
    }
    cards
}
