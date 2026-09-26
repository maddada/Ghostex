//! Small rules family b reads that belong to another family's directory.
//!
//! Each one is the smallest faithful port of a function the transcript projection calls, kept here
//! so this family can land without waiting on its neighbours
//! (`docs/2026-09-21/rust-chat/FAMILY-BRIEF.md`). `PROGRESS.md` lists them under "to fold into"
//! their owner; when that family lands the real thing, this module shrinks to nothing.

use ghostex_gx_protocol::{ChatMessage, ChatRole};

use crate::transcript::jsstr::{ascii_lower, js_trim, js_trim_start};

/// Family a, `packages/core-ui/chat/session-chat-streaming.ts`.
pub const STREAMING_ID: &str = "streaming";

/// Family a, `packages/core-ui/chat/session-chat-pending.ts`.
pub const PENDING_ID_PREFIX: &str = "pending:";

/// Family a, `packages/core-ui/chat/session-chat-pending.ts`.
pub const CODEX_GOAL_ID_PREFIX: &str = "app-command-goal:";

/// Shared contract, `packages/shared/session-chat.ts`.
pub const FORK_BOUNDARY_ID_PREFIX: &str = "fork-boundary:";

/// Family a, `packages/core-ui/chat/session-chat-terminal-status.ts`.
pub const TERMINAL_TOOL_ID_PREFIX: &str = "terminal-tool:";

const LAUNCH_PENDING_ID_PREFIX: &str = "launch-pending:";
const CLAUDE_TERMINAL_TOOL_KIND: &str = "claude-tool";

/// Family a: the optimistic local echo has no transcript row for the daemon to rewind to yet.
pub fn is_pending_message_id(id: &str) -> bool {
    id.starts_with(PENDING_ID_PREFIX)
}

/// Family a, `sessionChatMessageSortRank`: three tiers, because the streaming preview has no
/// timestamp (it would sort to the front) and an optimistic echo carries a real one (it would sort
/// past the preview).
fn sort_rank(message: &ChatMessage) -> u8 {
    if message.id == STREAMING_ID {
        return 1;
    }
    /*
    CDXC:SessionChatTerminalActivity 2026-09-04 WHY:
    The pending tool row is stamped with the time gxserver first saw it painted, which falls between
    the tool call's own timestamp and its result's. Sorted by time it landed between the two, and the
    result, cut off from its call, rendered as a bare "Result" row under the card. The row is the
    transcript's tail by definition, so it ranks after every transcript row instead.
    */
    if message.id.starts_with(TERMINAL_TOOL_ID_PREFIX) {
        return 2;
    }
    if message.id.starts_with(PENDING_ID_PREFIX) || message.id.starts_with(LAUNCH_PENDING_ID_PREFIX)
    {
        return 3;
    }
    0
}

/// Family a, `orderSessionChatMessages`.
///
/// The TypeScript's last tie-break before the id was a `WeakMap` of arrival order, which only the
/// incremental assembler filled. A list that reaches the projection through a read has no entries
/// in it, so the id decides, which is what this reproduces.
pub fn order_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut ordered = messages.to_vec();
    ordered.sort_by(|left, right| {
        sort_rank(left)
            .cmp(&sort_rank(right))
            .then_with(|| match (left.timestamp, right.timestamp) {
                (Some(a), Some(b)) => a.cmp(&b),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| match (left.byte_offset, right.byte_offset) {
                (Some(a), Some(b)) => a.cmp(&b),
                _ => std::cmp::Ordering::Equal,
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered
}

/// Family a, `mergeSessionChatMessagesWith`, in the one shape family b uses it: an id already in
/// the list is replaced in place, a new id is appended.
///
/// The source-priority argument the TypeScript took is its default here, because the deferred rows
/// this merges carry the same source as the turn's own work.
pub fn merge_messages_with(existing: &[ChatMessage], incoming: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut list = existing.to_vec();
    for message in incoming {
        match list.iter().position(|entry| entry.id == message.id) {
            Some(at) => list[at] = message.clone(),
            None => list.push(message.clone()),
        }
    }
    list
}

/// Family a, `packages/core-ui/chat/session-chat-command-envelope.ts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub name: String,
    pub args: String,
}

const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped=\"html\"";

/// Family b's own file, re-exported here so the envelope parser can reach it without a cycle.
pub fn decode_escaped_markup(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// `<tag( attributes)?>body</tag>`, the first match, returning the attributes and the body.
fn tagged_body<'a>(text: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let after_name = start + open.len();
    let rest = &text[after_name..];
    // `(\s[^>]*)?>`: either an immediate `>` or a whitespace-led attribute list.
    let attributes_end = rest.find('>')?;
    let attributes = &rest[..attributes_end];
    if !attributes.is_empty() && !attributes.starts_with(char::is_whitespace) {
        return None;
    }
    let body_start = after_name + attributes_end + 1;
    let body_end = text[body_start..].find(&close)? + body_start;
    Some((attributes, &text[body_start..body_end]))
}

/// Family a: the slash-command envelope a Claude-family harness records a user turn as.
pub fn parse_command_envelope(text: &str) -> Option<CommandEnvelope> {
    let trimmed = js_trim_start(text);
    // Ordinary prompts, XML pastes.
    if !ascii_lower(trimmed).starts_with("<command-") {
        return None;
    }
    let (name_attributes, name_body) = tagged_body(trimmed, "command-name")?;
    let name = js_trim(name_body);
    if name.is_empty() {
        return None;
    }
    let decode = |value: &str, attributes: &str| {
        if attributes.contains(ESCAPED_MARKUP_ATTRIBUTE) {
            decode_escaped_markup(value)
        } else {
            value.to_string()
        }
    };
    let args = tagged_body(trimmed, "command-args")
        .map(|(attributes, body)| decode(js_trim(body), attributes))
        .unwrap_or_default();
    Some(CommandEnvelope {
        name: decode(name, name_attributes),
        args,
    })
}

/// Family e, `agentModelCatalogEffortLabel`, with an empty catalog.
///
/// The real function looks the effort up in the shared model catalog first, which lives in family
/// e's state. Every catalog shipped so far leaves the efforts family b sees unmapped, so this is
/// the same answer; a catalog that does map one would need family e's table passed in.
pub fn effort_label(effort: &str) -> String {
    if effort == "xhigh" {
        return "xHigh".to_string();
    }
    effort
        .split(' ')
        .map(|word| match word.chars().next() {
            None => String::new(),
            Some(first) => format!("{}{}", first.to_uppercase(), &word[first.len_utf8()..]),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Family a: the painted tool row gxserver reads off the agent's terminal.
pub fn is_terminal_tool_message(message: &ChatMessage) -> bool {
    message.id.starts_with(TERMINAL_TOOL_ID_PREFIX)
}

/// Family a, `sessionChatTerminalToolActivity`.
pub fn terminal_tool_activity(message: &ChatMessage) -> serde_json::Value {
    let detail = message
        .blocks
        .iter()
        .find_map(|block| match block {
            ghostex_gx_protocol::ChatBlock::ToolResult { output, .. } => Some(output.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let label = js_trim(
        &message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ghostex_gx_protocol::ChatBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
    .to_string();
    let mut activity = serde_json::Map::new();
    activity.insert("kind".to_string(), CLAUDE_TERMINAL_TOOL_KIND.into());
    activity.insert("label".to_string(), label.into());
    activity.insert(
        "detectedAt".to_string(),
        message.id[TERMINAL_TOOL_ID_PREFIX.len()..]
            .to_string()
            .into(),
    );
    if !detail.is_empty() {
        activity.insert("detail".to_string(), detail.into());
    }
    serde_json::Value::Object(activity)
}

/// `message.role === 'user'`, spelled once because the wire role is an open enum.
pub fn is_role(message: &ChatMessage, role: ChatRole) -> bool {
    message.role == role
}

/// Family a: whether the agent is working, which decides the turn boundaries.
///
/// `presentation.update(state.messages, state.transcriptWorking, …)` in `native-host.ts`, after
/// `NativeChatPresentation.update` has applied the sticky fold: the live signal until the turn
/// lifecycle ends the run ([`crate::session::working::transcript_working`]), held settled while the
/// newest row is the one the transcript settled on. [`crate::transcript::rows::refresh`] records
/// that settle; this only reads it, so pure readers get the same answer.
pub fn is_working(state: &crate::state::ChatState) -> bool {
    crate::session::working::transcript_working(state)
        && !crate::transcript::turns::sticky_fold_holds(
            &state.messages.composed,
            state.transcript_view.fold_settled_at.as_deref(),
        )
}
