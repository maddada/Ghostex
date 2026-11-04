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
    /// Sent by one of this session's own background agents; `reply_to` is its agent id, and the
    /// transcript names it (`sent_message::resolve_recipients`).
    pub subagent: bool,
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
/// CDXC:SessionChat 2026-10-05 DECISION:
/// User: the sender block moved BELOW the message body so Claude and Codex title a session from the task text; the chat card must look exactly as before. The block is recognised at the end of the text as well as at the start, because transcripts recorded earlier have it first.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs and server/src/coordinators/brief.rs write the block; server/src/coordinators/brief.rs `split_agent_message` reads it on the server.
pub fn parse_inter_agent_message(text: &str) -> Option<InterAgentMessage> {
    if let Some(message) = parse_cross_session_message(text) {
        return Some(message);
    }
    let lines = split_newlines(text);
    if let Some(message) = parse_header_first(&lines) {
        return Some(message);
    }
    parse_footer(&lines)
}

/// The block above the body: the opener (or the old `MESSAGE FROM`), the labelled lines, then a
/// blank or dashed separator line.
fn parse_header_first(lines: &[&str]) -> Option<InterAgentMessage> {
    let opener = js_trim(lines.first().copied().unwrap_or_default());
    if opener != "Message from another agent" && opener != "MESSAGE FROM" {
        return None;
    }
    let (mut message, index) = parse_header_fields(lines, 1)?;
    let separator = js_trim(lines.get(index).copied().unwrap_or_default());
    let dashed = separator.len() >= 3 && separator.bytes().all(|byte| byte == b'-');
    if !separator.is_empty() && !dashed {
        return None;
    }
    message.body = js_trim(&lines[(index + 1).min(lines.len())..].join("\n")).to_string();
    Some(message)
}

/// The block below the body: the last `Message from another agent` line that follows a blank line,
/// then labelled lines up to the end of the text (trailing blank lines allowed).
fn parse_footer(lines: &[&str]) -> Option<InterAgentMessage> {
    let at = (1..lines.len()).rev().find(|&at| {
        js_trim(lines[at]) == "Message from another agent" && js_trim(lines[at - 1]).is_empty()
    })?;
    let (mut message, index) = parse_header_fields(lines, at + 1)?;
    if lines[index.min(lines.len())..]
        .iter()
        .any(|line| !js_trim(line).is_empty())
    {
        return None;
    }
    message.body = js_trim(&lines[..at].join("\n")).to_string();
    Some(message)
}

/// The labelled lines from `start`; the message they describe and the index of the first line that
/// is not one of them. `None` unless they name an agent and a `Reply to`.
fn parse_header_fields(lines: &[&str], start: usize) -> Option<(InterAgentMessage, usize)> {
    let mut message = InterAgentMessage::default();
    let mut seen_agent_name = false;
    let mut seen_reply_to = false;
    let mut index = start;
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
    (seen_agent_name && seen_reply_to).then_some((message, index))
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: a message another Claude session sent renders with the chat's message-from-another-agent card, like one sent with `ghostex agents send`. Claude wraps it as `<cross-session-message from="…" from-name="…">`, so the card names Claude as the agent and the sender's session name as its session. A row holding several envelopes is left to the harness marker, since one card cannot show two senders.
/// CDXC:SessionChat 2026-09-30 DECISION:
/// User: the `<agent-message from="…">` a session's own background agent sends it must "look correct and use our component", the same card, instead of a harness fold around a raw code block. Its `from` is only an agent id, so the card is named from the call that started that agent or the Subagents roster (`sent_message::resolve_recipients`).
/// SEE-ALSO: server/src/session_chat_decode_claude.rs keeps only the envelope from the row Claude writes when the message reaches an idle session.
pub fn parse_cross_session_message(text: &str) -> Option<InterAgentMessage> {
    let text = js_trim(text);
    let (open, close, subagent) = if text.starts_with("<agent-message") {
        ("<agent-message", "</agent-message>", true)
    } else {
        ("<cross-session-message", "</cross-session-message>", false)
    };
    let rest = text.strip_prefix(open)?;
    let (attributes, rest) = rest.split_once('>')?;
    if !attributes.is_empty() && !attributes.starts_with(' ') {
        return None;
    }
    let body = rest.strip_suffix(close)?;
    if body.contains(close) {
        return None;
    }
    Some(InterAgentMessage {
        agent_name: if subagent { "subagent" } else { "Claude" }.to_string(),
        session_title: envelope_attribute(attributes, "from-name").to_string(),
        reply_to: envelope_attribute(attributes, "from").to_string(),
        body: js_trim(body).to_string(),
        cross_session: true,
        subagent,
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
        .rfind(|segment| !segment.is_empty())
        .unwrap_or(sender)
        .to_string()
}

#[cfg(test)]
mod pasted_report_tests {
    use super::*;

    /// A thread report as gxserver hands it over once it has unwrapped Claude Code's
    /// `<pasted_content>` envelope (`session_chat_decode_claude.rs`): the sender block at the end is
    /// read, so the row is a "Message from" card, not the user's own bubble.
    #[test]
    fn a_thread_report_with_its_sender_block_last_is_an_agent_message() {
        let text = "Ghostex thread report: finished its turn. Its final message:\n\n\
            Claude now wraps pastes in <pasted_content id=\"x\"> tags.\n\n\
            Message from another agent\nAgent: Claude\nSession: Tool call look behind a toggle\n\
            Session ID: G14l1\nAgent ID: claude\n\
            Agent Session ID: a0ef8d63-2dea-42c4-9e36-600089110889\nReply to: S90:P3lv0:G14l1";
        let message = parse_inter_agent_message(text).expect("an agent message");
        assert_eq!(message.session_title, "Tool call look behind a toggle");
        assert_eq!(message.reply_to, "S90:P3lv0:G14l1");
        assert!(message.body.starts_with("Ghostex thread report"));
        assert!(message.body.ends_with("tags."));
    }
}
