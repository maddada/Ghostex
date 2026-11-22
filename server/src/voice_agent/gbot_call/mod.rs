//! Calls with a GBot: a Voice agent call about one of the GBot's chats (`focus.rs`), spoken as
//! that GBot in its own voice. The start (`/api/voice?gbot=<botId>`) opens or wakes the chat
//! (server/src/gbots/call.rs); the persona prompt is `prompt.rs`; `talk_to_bot` and the call's
//! tools are `tools.rs`; a finished turn of its chat is `reply.rs`; what the GBot says while it works, the Ghostex approvals it waits on and
//! the call's line in its daily log are `watch.rs`. Client contract: "Calls with a GBot" in
//! docs/2026-10-20/voice-mode/voice-agent-client-contract.md.

mod prompt;
mod reply;
#[cfg(test)]
mod tests;
mod tools;
mod watch;

use std::sync::Arc;

use serde_json::{json, Value};

use super::keys;
use super::provider::Provider;
use crate::gbots::{CallBot, CallRefusal};
use crate::paths::GxserverPaths;
use crate::server::AppState;

pub(super) use prompt::{briefing_message, instructions, reply_message};
pub(super) use reply::finished_announcement;
pub(super) use tools::{definitions, refusal, talk, TOOLS};
pub(super) use watch::watch;

/// The words for a client asking to move or widen a call with a GBot.
pub(super) const STAYS_MESSAGE: &str =
    "A call with a GBot stays with it. Hang up to start a call about your sessions.";

/// The GBot `bot` names and the chat ref the call is about (`focus` when it is one of its open
/// chats, else your standing chat with it).
pub(super) async fn start(
    state: &Arc<AppState>,
    bot: &str,
    focus: Option<String>,
) -> Result<(String, CallBot), CallRefusal> {
    let (state, bot) = (state.clone(), bot.to_string());
    tokio::task::spawn_blocking(move || {
        crate::gbots::gbot_call_chat(&state, &bot, focus.as_deref())
            .map(|(bot, reference)| (reference, bot))
    })
    .await
    .unwrap_or_else(|error| {
        Err(CallRefusal {
            code: "internalError",
            message: format!("The call could not start: {error}"),
        })
    })
}

/// Why a call with `bot` can't start, before the key is read (GBots off, no such GBot, its chats
/// elsewhere).
pub(super) async fn refusal_before_key(state: &Arc<AppState>, bot: &str) -> Option<CallRefusal> {
    let (state, bot) = (state.clone(), bot.to_string());
    tokio::task::spawn_blocking(move || crate::gbots::gbot_call_refusal(&state, &bot))
        .await
        .unwrap_or(None)
}

/// `gbot:<botId>` or `gbot:<botId>:<projectId>:<sessionId>` in `focus`: the GBot and the chat.
pub(super) fn parse_focus_query(focus: &str) -> Option<(String, Option<String>)> {
    let rest = focus.trim().strip_prefix("gbot:")?;
    let (bot, chat) = match rest.split_once(':') {
        Some((bot, chat)) => (
            bot,
            Some(chat.to_string()).filter(|chat| chat.contains(':')),
        ),
        None => (rest, None),
    };
    let bot = bot.trim();
    (!bot.is_empty()).then(|| (bot.to_string(), chat))
}

/// The GBot's voice, when the provider the call uses has it.
pub(super) fn voice_for(bot: &CallBot, provider: Provider) -> Option<String> {
    bot.voice
        .as_deref()
        .filter(|voice| provider.voices().contains(voice))
        .map(str::to_string)
}

/// What GBot Settings offers for a GBot's voice: the provider calls use now, its voice when none
/// is picked, and every provider's voices.
pub(crate) fn voice_choices(paths: &GxserverPaths) -> Value {
    let provider = keys::selected_provider(paths);
    json!({
        "provider": provider.id(),
        "providerLabel": provider.label(),
        "defaultVoice": keys::selected_voice(paths, provider),
        "voiceOptions": {
            "xai": Provider::Xai.voices(),
            "openai": Provider::OpenAi.voices(),
        },
    })
}
