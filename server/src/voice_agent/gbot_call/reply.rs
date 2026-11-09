//! A turn of the GBot's chat finishing while a call is open: the call speaks the reply only when
//! the turn answered something the call sent, and names the GBot, not the chat, when it does.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! The first live call opened a fresh chat whose startup brief ("Researcher is ready.") finished while the call's question waited behind it, and the call retold that brief as the answer ("your chat didn't actually give a number") seconds before the real one. A finished turn is the call's answer only when the chat's newest user message is one the call sent (every one starts `[Voice call`, `tools.rs`); any other finished turn (the startup brief, a turn someone started in the chat itself) is not announced in the call. The notice says "Researcher finished its turn.", not the chat's title ("Calls").

use std::sync::Arc;

use serde_json::Value;

use super::super::conversation::Announcement;
use super::super::focus::{self, FocusScope};
use super::super::sessions;
use super::super::tools::read_chat;
use crate::gbots::CallBot;
use crate::ghostex_cli::session_chat_transcript::effective_role;
use crate::server::AppState;

/// How every message a call sends starts (`tools.rs` FIRST_MARK and MARK).
pub(super) const CALL_MARK_START: &str = "[Voice call";

/// Transcript rows read for the finished turn and its reply.
const ROWS: usize = 60;

/// The finished-turn announcement of the call's chat as the call gives it: named after the GBot
/// and carrying its reply, or `None` when the turn didn't answer anything the call sent. A chat
/// that can't be read still announces the finished turn.
pub(in crate::voice_agent) async fn finished_announcement(
    state: &Arc<AppState>,
    scope: &FocusScope,
    bot: &CallBot,
    mut announcement: Announcement,
) -> Option<Announcement> {
    let chat = read_scope_chat(state, scope).await;
    let reply = match &chat {
        Some(chat) if !answers_call(chat) => return None,
        Some(chat) => focus::last_reply_markdown(chat).map(|reply| focus::speakable_reply(&reply)),
        None => None,
    };
    announcement.text = finished_text(bot);
    announcement.model_text = Some(super::reply_message(bot, reply.as_deref()));
    Some(announcement)
}

async fn read_scope_chat(state: &Arc<AppState>, scope: &FocusScope) -> Option<Value> {
    let all = sessions::list_sessions_async(state).await.ok()?;
    let reference = scope.reference();
    let session = all
        .iter()
        .find(|session| session.reference() == reference)?;
    read_chat(state, session, ROWS).await.ok()
}

/// Whether the turn that just finished answered the call: the chat's newest user message (harness
/// turns left out) is one the call sent. With no user message in the rows read (a long turn), it
/// is taken to answer the call.
pub(super) fn answers_call(chat: &Value) -> bool {
    let Some(messages) = chat.get("messages").and_then(Value::as_array) else {
        return true;
    };
    let Some(last_user) = messages
        .iter()
        .rev()
        .find(|message| effective_role(message) == "user")
    else {
        return true;
    };
    last_user
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::trim_start)
        .find(|text| !text.is_empty())
        .is_some_and(|text| text.starts_with(CALL_MARK_START))
}

/// The client's line for the GBot's finished turn.
pub(super) fn finished_text(bot: &CallBot) -> String {
    format!("{} finished its turn.", bot.name)
}
