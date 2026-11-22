//! The tools of a call with a GBot: `talk_to_bot` (the user's words to the GBot's chat) and the
//! focused-call tools that read that chat, answer what it waits on and stop its turn. There is no
//! prompt to another session, no draft and no widening: the call stays with the GBot.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::{json, Value};

use super::super::tools::{arg_text, send_prompt, ToolMemory};
use crate::gbots::CallBot;
use crate::server::AppState;

/// The tools a call with a GBot is offered.
pub(in crate::voice_agent) const TOOLS: &[&str] = &[
    "talk_to_bot",
    "read_session",
    "get_pending_prompt",
    "answer_prompt",
    "stop_session",
    "show_session",
];

/// The tools copied from the focused call's set, with their session argument dropped.
const SHARED: &[&str] = &[
    "read_session",
    "get_pending_prompt",
    "answer_prompt",
    "stop_session",
    "show_session",
];

/// The longest message one `talk_to_bot` sends.
const MESSAGE_MAX_CHARS: usize = 4_000;

/// The marks on a message spoken in a call: the first of a call says how to answer; later ones
/// only say they came from the call.
const FIRST_MARK: &str = "[Voice call: the user is talking to you by voice and hears your reply read aloud, so answer in a few short spoken sentences, without code, paths, links or tables, until the call ends.]";
const MARK: &str = "[Voice call]";

/// Messages each call sent, by the chat it is about (`watch.rs` sets it to 0 when the call
/// starts and takes it when the call ends, for the daily log).
static SENT: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn reset_sent(chat: &str) {
    if let Ok(mut sent) = SENT.lock() {
        sent.insert(chat.to_string(), 0);
    }
}

pub(super) fn take_sent(chat: &str) -> u64 {
    SENT.lock()
        .ok()
        .and_then(|mut sent| sent.remove(chat))
        .unwrap_or(0)
}

/// Counts one message, answering how many the call had sent before it.
fn count_sent(chat: &str) -> u64 {
    let Ok(mut sent) = SENT.lock() else {
        return 1;
    };
    let count = sent.entry(chat.to_string()).or_insert(0);
    *count += 1;
    *count - 1
}

/// The tools' definitions for the model.
pub(in crate::voice_agent) fn definitions(bot: &CallBot, all: &Value) -> Value {
    let mut tools: Vec<Value> = vec![json!({
        "type": "function",
        "name": "talk_to_bot",
        "description": format!("Send what the user said to your chat ({}'s), where the agent acting as you does the work. Use it for every request, question or piece of information the user gives you. Your reply arrives later as a [Ghostex update].", bot.name),
        "parameters": {
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "The user's request or question, in their words, written clearly as a message to you."}
            },
            "required": ["message"]
        }
    })];
    tools.extend(
        all.as_array()
            .into_iter()
            .flatten()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| SHARED.contains(&name))
            })
            .cloned()
            .map(|mut tool| {
                if let Some(parameters) = tool.get_mut("parameters") {
                    if let Some(properties) = parameters
                        .get_mut("properties")
                        .and_then(Value::as_object_mut)
                    {
                        properties.remove("session");
                    }
                    if let Some(required) =
                        parameters.get_mut("required").and_then(Value::as_array_mut)
                    {
                        required.retain(|key| key != "session");
                    }
                }
                tool
            }),
    );
    Value::Array(tools)
}

/// The refusal for a tool a call with a GBot doesn't have.
pub(in crate::voice_agent) fn refusal(bot: &CallBot) -> String {
    format!(
        "This call is with {} only. Pass what the user wants to your chat with talk_to_bot, or tell them to hang up and start a call about their sessions.",
        bot.name
    )
}

/// `talk_to_bot`: the user's words, marked as spoken in a call, sent to the GBot's chat the way
/// `send_prompt` sends a prompt.
pub(in crate::voice_agent) async fn talk(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    bot: &CallBot,
    args: &Value,
) -> Result<Value, String> {
    let message = arg_text(args, "message");
    if message.is_empty() {
        return Err("The message is empty.".to_string());
    }
    if message.chars().count() > MESSAGE_MAX_CHARS {
        return Err(format!(
            "That message is longer than {MESSAGE_MAX_CHARS} characters. Send it in parts."
        ));
    }
    let Some(scope) = memory.scope() else {
        return Err("This call isn't with a GBot anymore.".to_string());
    };
    let chat = scope.reference();
    let mark = if count_sent(&chat) == 0 {
        FIRST_MARK
    } else {
        MARK
    };
    let sent = send_prompt(state, memory, &json!({"text": format!("{mark} {message}")})).await;
    if sent.is_err() {
        // Not sent: it does not count, and the next one still carries the first mark.
        if let Ok(mut counts) = SENT.lock() {
            if let Some(count) = counts.get_mut(&chat) {
                *count = count.saturating_sub(1);
            }
        }
    }
    sent?;
    Ok(json!({
        "ok": true,
        "sent": true,
        "note": format!("Sent to {}'s chat. Say in a few words that you're on it; the reply arrives as a [Ghostex update].", bot.name),
    }))
}
