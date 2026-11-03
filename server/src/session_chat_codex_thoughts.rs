//! One Thought row per stretch of a Codex turn. Codex records a reasoning summary before almost
//! every step, so its rollout decodes to Thought and Ran rows taking turns; these helpers join a
//! stretch's summaries into the stretch's first Thought row, which then owns the calls under it in
//! the chat core, the way OpenCode's decoder does (`session_chat_opencode/decode.rs`).
//!
//! CDXC:SessionChat 2026-11-02 DECISION: User, asked whether Codex turns, which still alternated Thought and Ran rows, get the one-Thought-row treatment OpenCode got: "codex too". A Codex turn's consecutive reasoning summaries join into its first Thought row until a reply sentence, a prompt or any other row that is not a tool call, a tool result or a `<system-reminder>` closes the stretch; a summary written after that starts a new row below it. The tail window joins the stretches inside it, and the append stream keeps the window's open stretch and sends that row again, grown, under its first id, so the chat replaces it in place. A stretch cut by a page boundary can show as two rows at the seam.
//! SEE-ALSO: server/src/session_chat_opencode/decode.rs (OpenCode's same rule), packages/gx-chat-core/src/transcript/thinking_card.rs (a running thought folds the calls it owns).

use crate::session_chat::{SessionChatBlock, SessionChatMessage, SessionChatRole};

/// The Thought row a Codex append stream is still adding to. `None` inside means the stretch is
/// closed; the state itself exists only for a Codex rollout, which the tail window recognises.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodexThoughtStretch {
    open: Option<SessionChatMessage>,
}

impl CodexThoughtStretch {
    /// Feeds one decoded row in file order. A summary joining the open stretch hands back the
    /// stretch's row, grown, to send in its place; every other row comes back as it is.
    pub fn feed(&mut self, message: SessionChatMessage) -> SessionChatMessage {
        if message.role == SessionChatRole::Reasoning {
            if let Some(open) = self.open.as_mut() {
                join(open, &message);
                return open.clone();
            }
            self.open = Some(message.clone());
            return message;
        }
        if closes_stretch(&message) {
            self.open = None;
        }
        message
    }
}

/// Joins the stretches inside one window of rows, in file order, and returns the stretch still
/// open at its end for the append stream that continues after it.
pub fn join_codex_thoughts(
    messages: Vec<SessionChatMessage>,
) -> (Vec<SessionChatMessage>, CodexThoughtStretch) {
    let mut joined: Vec<SessionChatMessage> = Vec::with_capacity(messages.len());
    let mut open: Option<usize> = None;
    for message in messages {
        if message.role == SessionChatRole::Reasoning {
            if let Some(at) = open {
                join(&mut joined[at], &message);
                continue;
            }
            open = Some(joined.len());
        } else if closes_stretch(&message) {
            open = None;
        }
        joined.push(message);
    }
    let stretch = CodexThoughtStretch {
        open: open.map(|at| joined[at].clone()),
    };
    (joined, stretch)
}

/// Adds `next`'s text to the stretch's row as a paragraph of its own. Each Codex summary is whole
/// when it is written, so the row ends on a newline: the chat core shows a running thought's newest
/// finished line, and a summary counts as finished.
fn join(open: &mut SessionChatMessage, next: &SessionChatMessage) {
    let text = text_of(next);
    if text.trim().is_empty() {
        return;
    }
    let combined = text_of(open);
    let combined = combined.trim_end();
    let joined = if combined.is_empty() {
        format!("{}\n", text.trim())
    } else {
        format!("{combined}\n\n{}\n", text.trim())
    };
    open.blocks = vec![SessionChatBlock::Text { text: joined }];
}

fn text_of(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a row ends the stretch: anything but the calls the thought made, their results and
/// the `<system-reminder>` rows Codex writes between them (a failed command's exit code).
fn closes_stretch(message: &SessionChatMessage) -> bool {
    let tool_only = !message.blocks.is_empty()
        && message.blocks.iter().all(|block| {
            matches!(
                block,
                SessionChatBlock::ToolCall { .. } | SessionChatBlock::ToolResult { .. }
            )
        });
    let reminder = message.role == SessionChatRole::System
        && !message.blocks.is_empty()
        && message.blocks.iter().all(|block| {
            matches!(block, SessionChatBlock::Text { text } if text.trim_start().starts_with("<system-reminder>"))
        });
    !tool_only && !reminder
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat::SessionChatSource;
    use serde_json::json;

    fn row(id: &str, role: SessionChatRole, blocks: Vec<SessionChatBlock>) -> SessionChatMessage {
        SessionChatMessage {
            id: id.into(),
            role,
            blocks,
            timestamp: Some(1),
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            async_questions: None,
            queued: false,
            api_error: None,
            custom_message: None,
        }
    }

    fn text(value: &str) -> Vec<SessionChatBlock> {
        vec![SessionChatBlock::Text { text: value.into() }]
    }

    fn thought(id: &str, value: &str) -> SessionChatMessage {
        row(id, SessionChatRole::Reasoning, text(value))
    }

    fn call(id: &str) -> SessionChatMessage {
        row(
            id,
            SessionChatRole::Assistant,
            vec![SessionChatBlock::ToolCall {
                name: "exec_command".into(),
                input: json!({"cmd": "ls"}),
                call_id: Some(id.into()),
            }],
        )
    }

    fn output(id: &str) -> SessionChatMessage {
        row(
            &format!("{id}:out"),
            SessionChatRole::Tool,
            vec![SessionChatBlock::ToolResult {
                output: "ok".into(),
                is_error: None,
                call_id: Some(id.into()),
                patch: None,
            }],
        )
    }

    fn texts(messages: &[SessionChatMessage]) -> Vec<(String, String)> {
        messages
            .iter()
            .map(|message| (message.id.clone(), text_of(message)))
            .collect()
    }

    #[test]
    fn a_window_joins_each_stretch_until_a_reply_sentence() {
        let (joined, stretch) = join_codex_thoughts(vec![
            row("u1", SessionChatRole::User, text("check the settings")),
            thought("r1", "**Checking agent configuration**"),
            call("c1"),
            output("c1"),
            thought("r2", "**Checking permission configuration**"),
            call("c2"),
            row(
                "x1",
                SessionChatRole::System,
                text("<system-reminder>Command exited with code 1: jq .</system-reminder>"),
            ),
            output("c2"),
            row("a1", SessionChatRole::Assistant, text("Both look fine.")),
            thought("r3", "**Checking session model options**"),
            call("c3"),
        ]);
        assert_eq!(
            texts(&joined)
                .into_iter()
                .filter(|(id, _)| id.starts_with('r'))
                .collect::<Vec<_>>(),
            [
                (
                    "r1".to_string(),
                    "**Checking agent configuration**\n\n**Checking permission configuration**\n"
                        .to_string()
                ),
                (
                    "r3".to_string(),
                    "**Checking session model options**".to_string()
                ),
            ]
        );
        let ids: Vec<&str> = joined.iter().map(|message| message.id.as_str()).collect();
        assert_eq!(
            ids,
            ["u1", "r1", "c1", "c1:out", "c2", "x1", "c2:out", "a1", "r3", "c3"]
        );
        // The stretch after the reply is still open for the append stream.
        assert_eq!(stretch.open.map(|open| open.id).as_deref(), Some("r3"));
    }

    #[test]
    fn the_append_stream_sends_the_grown_row_under_its_first_id() {
        let (_, mut stretch) = join_codex_thoughts(vec![
            row("u1", SessionChatRole::User, text("go")),
            thought("r1", "**Reading files**"),
            call("c1"),
        ]);
        let sent = stretch.feed(thought("r2", "**Running tests**"));
        assert_eq!(sent.id, "r1");
        assert_eq!(text_of(&sent), "**Reading files**\n\n**Running tests**\n");
        assert_eq!(stretch.feed(call("c2")).id, "c2");
        // A reply sentence closes it; the next summary is a row of its own.
        stretch.feed(row("a1", SessionChatRole::Assistant, text("Done.")));
        let next = stretch.feed(thought("r3", "**Summarising**"));
        assert_eq!(
            (next.id.as_str(), text_of(&next).as_str()),
            ("r3", "**Summarising**")
        );
        // A new prompt closes it too.
        stretch.feed(row("u2", SessionChatRole::User, text("again")));
        assert_eq!(stretch.feed(thought("r4", "**Again**")).id, "r4");
    }
}
