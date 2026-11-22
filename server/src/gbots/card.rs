//! The "GBot added" card Chief of Staff's chat shows when it adds a GBot on its own: `ghostex gbots
//! create` prints it as a card line (packages/gx-protocol/src/chat_card.rs), the chat finds it in
//! the command's output, and its Undo and Open settings call `/api/gbots` (`change`, `undo`).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2103 A, mockup "Researcher added"): Chief of Staff "adds it, tells you, Undo": the card's title says the bot was added, one line says what it runs on and what it works on, and its buttons are Undo and Open settings. After Undo it reads "Researcher removed" with "Undone". The desktop and web chat (apps/desktop/src/app/native_chat/gbot_card.rs) and the phone (apps/mobile/app/src/chat/native/transcript/GbotCard.tsx) draw the same card.

use ghostex_gx_protocol::chat_card::GBOT_ADDED;
use serde_json::{json, Value};

use super::record::{color_hex, Bot};

/// The card a GBot Chief of Staff added carries.
pub(crate) fn gbot_added_card(change_id: &str, bot: &Bot) -> Value {
    json!({
        "kind": GBOT_ADDED,
        "changeId": change_id,
        "botId": bot.bot_id,
        "name": bot.name,
        "title": bot.title,
        "icon": bot.icon,
        "color": bot.color,
        "colorHex": color_hex(&bot.color),
        "agentId": bot.agent_id,
    })
}
