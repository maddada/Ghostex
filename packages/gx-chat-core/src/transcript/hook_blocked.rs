//! A prompt the agent's own prompt hook refused: the user's bubble, then a red one-line card that
//! says a hook blocked it and why.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! GitHub #543: a Claude Code `UserPromptSubmit` hook that blocks a prompt (`{"decision":"block"}`
//! or exit code 2) makes Claude print "UserPromptSubmit operation blocked by hook" with the reason
//! and the prompt, and record only that as an informational system row (no user row,
//! `promptSubmitEnding.ending.kind` `blocked_by_hook`). The chat drew it as a muted line in the
//! agent's words, or not at all, and the sent message looked delivered. The row is split here into
//! the prompt as the user's own bubble and a status card under it, so the thread reads like any
//! other message that did not go through. Parsing Claude's own text keeps gxserver's row unchanged
//! for phones whose chat core predates this. Claude Code 2.1.319 writes the row as
//! "<lead>:\n<reason>\n\nOriginal prompt: <prompt>"; an exit-code-2 hook's reason starts with the
//! hook's command in brackets, which the card leaves out.
//! SEE-ALSO: server/src/session_chat_hook_blocked.rs (the send hands the text back to the chat box),
//! session/composition.rs (`surface_skill_invocation_user_turns_in`), transcript/noise.rs.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::transcript::prose::joined_text;

/// The line Claude Code opens a blocked prompt's row with.
const CLAUDE_HOOK_BLOCKED_LEAD: &str = "UserPromptSubmit operation blocked by hook:";
/// What comes before the prompt itself in that row.
const ORIGINAL_PROMPT_LEAD: &str = "Original prompt:";
/// The suffix of the card's row once its prompt took the row's own id.
const CARD_ID_SUFFIX: &str = ":hook-blocked";
/// The card's words, before the reason.
pub const HOOK_BLOCKED_LABEL: &str = "A hook blocked this message";

/// A prompt a hook refused: why, and what the user had sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookBlockedPrompt {
    pub reason: String,
    pub prompt: String,
}

/// The blocked prompt a transcript system row records, or `None` for any other row.
pub fn parse_hook_blocked_prompt(message: &ChatMessage) -> Option<HookBlockedPrompt> {
    if message.role != ChatRole::System || message.source != ChatSource::Transcript {
        return None;
    }
    parse_hook_blocked_text(&joined_text(&message.blocks))
}

fn parse_hook_blocked_text(text: &str) -> Option<HookBlockedPrompt> {
    let body = text.trim_start().strip_prefix(CLAUDE_HOOK_BLOCKED_LEAD)?;
    let (reason, prompt) = match body.rfind(&format!("\n{ORIGINAL_PROMPT_LEAD}")) {
        Some(at) => (&body[..at], &body[at + 1 + ORIGINAL_PROMPT_LEAD.len()..]),
        None => (body, ""),
    };
    Some(HookBlockedPrompt {
        reason: without_hook_command(reason.trim()).to_string(),
        prompt: prompt.trim().to_string(),
    })
}

/// An exit-code-2 hook's reason is "[<command>]: <stderr>"; the command is plumbing.
fn without_hook_command(reason: &str) -> &str {
    reason
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("]: "))
        .map_or(reason, |(_, stderr)| stderr.trim())
}

/// The card's one line: what happened, then the hook's reason when it gave one.
pub fn hook_blocked_label(blocked: &HookBlockedPrompt) -> String {
    let reason = blocked
        .reason
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if reason.is_empty() {
        HOOK_BLOCKED_LABEL.to_string()
    } else {
        format!("{HOOK_BLOCKED_LABEL}: {reason}")
    }
}

/// Whether a list holds a blocked prompt at all, so the common case copies nothing.
pub fn has_hook_blocked_prompt(messages: &[ChatMessage]) -> bool {
    messages
        .iter()
        .any(|message| parse_hook_blocked_prompt(message).is_some())
}

/// Puts the user's bubble with the blocked prompt right before each blocked row, which stays to
/// draw the card. The bubble takes the row's id and the card a suffixed one: rows at the same time
/// and offset are ordered by id (`order_cmp` in foreign.rs), so the card stays under the bubble. A
/// row whose prompt Claude did not print keeps only its card.
pub fn split_hook_blocked_prompts(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(messages.len() + 1);
    for message in messages {
        match parse_hook_blocked_prompt(&message) {
            Some(blocked) if !blocked.prompt.is_empty() => {
                out.push(ChatMessage {
                    role: ChatRole::User,
                    blocks: vec![ChatBlock::Text {
                        text: blocked.prompt,
                    }],
                    ..message.clone()
                });
                out.push(ChatMessage {
                    id: format!("{}{CARD_ID_SUFFIX}", message.id),
                    ..message
                });
            }
            _ => out.push(message),
        }
    }
    out
}
