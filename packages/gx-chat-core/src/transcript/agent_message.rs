//! The two headers another agent's message can arrive with.
//!
//! Ported from `packages/shared/session-chat-presentation/agent-message.ts`.

use crate::transcript::jsstr::{is_js_space, js_trim, split_newlines};

/// A `Message from <sender>` header and the body under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMessage {
    pub sender: String,
    pub body: String,
}

/// `^Message from (\S+)\n\n([\s\S]*)$`.
pub fn parse_agent_message(text: &str) -> Option<AgentMessage> {
    let rest = text.strip_prefix("Message from ")?;
    let sender_len: usize = rest
        .chars()
        .take_while(|character| !is_js_space(*character))
        .map(char::len_utf8)
        .sum();
    if sender_len == 0 || !rest[sender_len..].starts_with("\n\n") {
        return None;
    }
    Some(AgentMessage {
        sender: rest[..sender_len].to_string(),
        body: js_trim(&rest[sender_len + 2..]).to_string(),
    })
}

/// A message another Ghostex agent session sent with `ghostex agents send` or
/// `agents create --task`, or one another Claude session sent over Claude's own cross-session
/// channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterAgentMessage {
    pub agent_name: String,
    pub session_title: String,
    pub session_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub reply_to: String,
    pub body: String,
    /// Delivered by Claude's cross-session channel instead of typed into the session's terminal,
    /// so it is no prompt a rewind can return to.
    pub cross_session: bool,
}

/// `^([A-Za-z ]+): (.*)$` against one line.
fn header_field(line: &str) -> Option<(&str, &str)> {
    let name_len = line
        .bytes()
        .take_while(|byte| byte.is_ascii_alphabetic() || *byte == b' ')
        .count();
    if name_len == 0 || !line[name_len..].starts_with(": ") {
        return None;
    }
    Some((&line[..name_len], &line[name_len + 2..]))
}

/// CDXC:SessionChat 2026-09-18 DECISION:
/// User: messages between agents render as a message from the sending agent, not as the user's own
/// prompt bubble, and the sender header must never show as a heading. Accepts the current header (a
/// blank line before the body) and the pre-2026-09-18 `MESSAGE FROM` header that ended in a dashed
/// line, so transcripts recorded before the format change render the same way.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs writes the header.
pub fn parse_inter_agent_message(text: &str) -> Option<InterAgentMessage> {
    if let Some(message) = parse_cross_session_message(text) {
        return Some(message);
    }
    let lines = split_newlines(text);
    let opener = js_trim(lines.first().copied().unwrap_or_default());
    if opener != "Message from another agent" && opener != "MESSAGE FROM" {
        return None;
    }
    let mut message = InterAgentMessage::default();
    let mut seen_agent_name = false;
    let mut seen_reply_to = false;
    let mut index = 1;
    while index < lines.len() {
        let Some((name, raw)) = header_field(lines[index]) else {
            break;
        };
        // The CLI writes `unavailable` for an identifier it could not resolve.
        let value = js_trim(raw);
        let value = if value == "unavailable" { "" } else { value };
        match name {
            "Agent" => {
                seen_agent_name = true;
                message.agent_name = value.to_string();
            }
            "Session" => message.session_title = value.to_string(),
            "Session ID" => message.session_id = value.to_string(),
            "Agent ID" => message.agent_id = value.to_string(),
            "Agent Session ID" => message.agent_session_id = value.to_string(),
            "Reply to" => {
                seen_reply_to = true;
                message.reply_to = value.to_string();
            }
            _ => break,
        }
        index += 1;
    }
    let separator = js_trim(lines.get(index).copied().unwrap_or_default());
    let dashed = separator.len() >= 3 && separator.bytes().all(|byte| byte == b'-');
    if !seen_agent_name || !seen_reply_to || (!separator.is_empty() && !dashed) {
        return None;
    }
    message.body = js_trim(&lines[(index + 1).min(lines.len())..].join("\n")).to_string();
    Some(message)
}

const CROSS_SESSION_OPEN: &str = "<cross-session-message";
const CROSS_SESSION_CLOSE: &str = "</cross-session-message>";

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: a message another Claude session sent renders with the chat's message-from-another-agent card, like one sent with `ghostex agents send`. Claude wraps it as `<cross-session-message from="…" from-name="…">`, so the card names Claude as the agent and the sender's session name as its session. A row holding several envelopes is left to the harness marker, since one card cannot show two senders.
/// SEE-ALSO: server/src/session_chat_decode_claude.rs keeps only this envelope from the row Claude writes when the message reaches an idle session.
pub fn parse_cross_session_message(text: &str) -> Option<InterAgentMessage> {
    let rest = js_trim(text).strip_prefix(CROSS_SESSION_OPEN)?;
    let (attributes, rest) = rest.split_once('>')?;
    if !attributes.is_empty() && !attributes.starts_with(' ') {
        return None;
    }
    let body = rest.strip_suffix(CROSS_SESSION_CLOSE)?;
    if body.contains(CROSS_SESSION_CLOSE) {
        return None;
    }
    Some(InterAgentMessage {
        agent_name: "Claude".to_string(),
        session_title: envelope_attribute(attributes, "from-name").to_string(),
        reply_to: envelope_attribute(attributes, "from").to_string(),
        body: js_trim(body).to_string(),
        cross_session: true,
        ..InterAgentMessage::default()
    })
}

/// ` name="value"`; Claude strips quotes and angle brackets from every value it writes.
fn envelope_attribute<'a>(attributes: &'a str, name: &str) -> &'a str {
    let marker = format!(" {name}=\"");
    let Some(start) = attributes.find(&marker).map(|at| at + marker.len()) else {
        return "";
    };
    let value = &attributes[start..];
    js_trim(&value[..value.find('"').unwrap_or(value.len())])
}

/// `/root/windows_support` is addressed as `windows_support` by the agents themselves.
pub fn agent_display_name(sender: &str) -> String {
    sender
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .unwrap_or(sender)
        .to_string()
}
