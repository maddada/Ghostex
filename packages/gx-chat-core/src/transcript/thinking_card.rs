//! The one line a thought is drawn as. Every reasoning row carries a `thinkingRow` holding its first
//! plain line (`preview`) and whether opening it shows more (`expandable`). Every reasoning row of
//! the turn that is still running also gets a `thinkingCard` holding whether it is the row being
//! written right now (`live`), its newest finished line (`preview`) and, when the thought owns the
//! calls it made, their count label (`calls`, "3 tool calls"), so a renderer draws a closed row the
//! reader opens with one click while the agent is still thinking, with its calls folded behind it.
//! Once the turn settles the rows lose the mark and fold into the turn's "Previous turn" card as
//! before.
//!
//! CDXC:SessionChat 2026-10-24 DECISION: User, on Codex's thinking in Simple mode: "thinking has to be switched to a simple expandible area, it can't be this big card like it is now for codex. do same for all the agents we support chat for please". A thought is one slim row shaped like a tool row (brain tile, "Thinking…" while it is written or "Thought" after, then its line), closed by default, one click opening the full text below it; no bordered card. The running turn's thought is drawn that way in every mode, and in Simple mode every thought is, so a run of short Codex reasoning summaries reads as quiet lines between the calls instead of bold paragraphs. Supersedes the 2026-10-23 card look (ferpart's #272 request 1); what that decision chose stays: it starts closed while it streams, shows the newest finished line with no word count, and there is no setting to hide thinking. Drawn by the desktop's `native_chat/thinking_card.rs` and the phone's `ThinkingLine` in `MessageRow.tsx`.
//!
//! CDXC:SessionChat 2026-11-02 DECISION: User, asked whether the calls under a running thought stay in view or fold the way Claude's steps fold under its sentences: "Fold them like Claude, for every agent". While a turn runs, a thought that owns calls folds them behind its line the way an agent's sentence folds its calls behind its chevron: the line ends with "N tool calls" and a chevron that opens them, on the same default as a sentence's calls (open in detailed or verbose mode, closed in Simple mode, and open while one of them waits on an approval through `toolsOpen`), separate from the click on the line that opens the thought's text. Supersedes the 2026-10-24 rule that "the calls a thought made stay in view below it". Inside a turn's work fold the calls stay listed flat (`toolsFlat`, the 2026-10-21 decision in `tool_rows.rs`), and with Tool call cards on they stay the card after it.
use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::turns::opens_turn;

/// The reasoning rows under the last prompt while the agent works, and the id of the one that
/// is the newest row of the transcript (the thought being written).
pub struct RunningThinking {
    ids: Vec<String>,
    live: Option<String>,
}

impl RunningThinking {
    pub fn new(messages: &[ChatRow], working: bool) -> Self {
        if !working {
            return Self {
                ids: Vec::new(),
                live: None,
            };
        }
        let start = messages
            .iter()
            .rposition(|message| opens_turn(message))
            .map_or(0, |at| at + 1);
        let tail = &messages[start.min(messages.len())..];
        let ids = tail
            .iter()
            .filter(|message| message.role == ChatRole::Reasoning)
            .map(|message| message.id.clone())
            .collect();
        let live = tail
            .last()
            .filter(|message| message.role == ChatRole::Reasoning)
            .map(|message| message.id.clone());
        Self { ids, live }
    }

    /// The reasoning rows that get a card.
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    /// The card a reasoning row gets: `Some(live)`, or `None` when it gets none.
    pub fn card(&self, id: &str) -> Option<bool> {
        self.ids
            .iter()
            .any(|known| known == id)
            .then(|| self.live.as_deref() == Some(id))
    }

    /// Adds `thinkingCard` to a projected reasoning row of the running turn. Any other row keeps
    /// the model it shares with the projection cache.
    pub fn mark(&self, message: &ChatMessage, row: &mut std::sync::Arc<Value>) {
        if !self.ids.iter().any(|id| id == &message.id) || !row.is_object() {
            return;
        }
        let text: String = message
            .blocks
            .iter()
            .filter_map(|block| match block {
                ChatBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let calls = folded_calls(row);
        if let Value::Object(fields) = std::sync::Arc::make_mut(row) {
            let mut card = json!({
                "live": self.live.as_deref() == Some(message.id.as_str()),
                "preview": preview(&text),
            });
            if let Some(calls) = calls {
                card["calls"] = Value::String(calls);
            }
            fields.insert("thinkingCard".to_string(), card);
        }
    }
}

/// The label of the calls a running thought folds behind its line ("3 tool calls"), or `None` when
/// it owns none: a turn's work fold lists them flat after it (`toolsFlat`), Tool call cards draw
/// them as one card after it (`toolCard`), and an answered question or a message to another agent
/// is its own card, not one of the calls (as `simpleToolLabel` counts them in `presentation.rs`).
fn folded_calls(row: &Value) -> Option<String> {
    if row["toolsFlat"] == true || row["toolCard"].is_object() {
        return None;
    }
    let calls = row["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| {
            tool["hasCall"] == true && tool["exchange"] != true && tool["sentMessage"] != true
        })
        .count();
    (calls > 0).then(|| crate::transcript::simple::tool_count_label(calls))
}

/// The newest finished line of a thought, without its Markdown marks. The line still being
/// written is cut mid-word, so it only counts once its newline arrives.
fn preview(text: &str) -> String {
    let finished = if text.ends_with('\n') {
        text
    } else {
        text.rsplit_once('\n').map_or(text, |(done, _)| done)
    };
    finished
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '*', '-', '>', ' '])
                .trim()
        })
        .rfind(|line| !line.is_empty() && !line.starts_with("```"))
        .unwrap_or_default()
        .replace("**", "")
        .replace('`', "")
}

/// The longest one-line thought that stays closed: a short headline fits the row on a phone, a
/// longer line is cut with an ellipsis and has to open to be read.
const CLOSED_LINE_CHARS: usize = 48;

/// A thought's one line: its first plain line, and whether its full text says more than that.
///
/// CDXC:SessionChat 2026-10-25 WHY: Renderers draw the line truncated to one row, so a one-line thought longer than a headline (OpenCode writes a whole sentence of reasoning on one line) must open too, or its end is never readable.
pub fn thinking_row(body: &str) -> Value {
    let preview = crate::transcript::message_text::plain_reasoning_teaser(body);
    let plain = crate::transcript::message_text::plain_reasoning_text(body);
    let lines = plain
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .count();
    json!({
        "expandable": lines > 1
            || body.contains("```")
            || preview.chars().count() > CLOSED_LINE_CHARS,
        "preview": preview,
    })
}

#[cfg(test)]
mod tests {
    use super::{thinking_row, RunningThinking};
    use crate::state::ChatRow;
    use ghostex_gx_protocol::ChatMessage;
    use serde_json::{json, Value};
    use std::sync::Arc;

    fn message_message(id: &str, role: &str, blocks: Value) -> ChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": role, "blocks": blocks, "timestamp": 1, "source": "transcript"
        }))
        .unwrap()
    }

    fn message(id: &str, role: &str, blocks: Value) -> crate::state::ChatRow {
        std::sync::Arc::new(message_message(id, role, blocks))
    }

    fn call(id: &str) -> Value {
        json!({"type": "tool-call", "name": "Bash", "input": {"command": format!("cat {id}.txt")}, "callId": id})
    }

    fn result(id: &str) -> Value {
        json!({"type": "tool-result", "output": "7", "callId": id})
    }

    /// A running turn: the prompt, a thought that owns the two calls it made, and a newer thought
    /// still being written. The rows are folded the way the transcript folds them.
    fn running_turn() -> Vec<ChatRow> {
        vec![
            message(
                "u1",
                "user",
                json!([{"type": "text", "text": "read the files"}]),
            ),
            message(
                "r1",
                "reasoning",
                json!([
                    {"type": "text", "text": "Reading the first two files."},
                    call("c1"), result("c1"), call("c2"), result("c2")
                ]),
            ),
            message(
                "r2",
                "reasoning",
                json!([{"type": "text", "text": "Now the third.\n"}]),
            ),
        ]
    }

    fn row(message: &ChatMessage, messages: &[ChatRow]) -> Arc<Value> {
        Arc::new(crate::transcript::presentation::project_message(
            message,
            &std::cell::OnceCell::new(),
            messages,
            "",
            None,
            Default::default(),
            &Default::default(),
            &Default::default(),
            &[],
            false,
            true,
        ))
    }

    #[test]
    fn a_running_thought_folds_the_calls_it_made() {
        let messages = running_turn();
        let running = RunningThinking::new(&messages, true);
        let mut owner = row(&messages[1], &messages);
        running.mark(&messages[1], &mut owner);
        assert_eq!(
            owner["thinkingCard"],
            json!({"live": false, "preview": "Reading the first two files.", "calls": "2 tool calls"})
        );
        // The thought being written owns no calls yet, so it has nothing to fold.
        let mut live = row(&messages[2], &messages);
        running.mark(&messages[2], &mut live);
        assert_eq!(
            live["thinkingCard"],
            json!({"live": true, "preview": "Now the third."})
        );
    }

    #[test]
    fn calls_listed_flat_or_drawn_as_a_card_are_not_folded() {
        let messages = running_turn();
        let running = RunningThinking::new(&messages, true);
        // Inside a turn's work fold the calls follow the thought at the same level.
        let mut flat = row(&messages[1], &messages);
        crate::transcript::tool_rows::list_tools_in_work_fold(Arc::make_mut(&mut flat));
        running.mark(&messages[1], &mut flat);
        assert!(flat["thinkingCard"].get("calls").is_none());
        // Settings > Chat > Tool call cards: the calls are one card after the thought.
        let mut carded = row(&messages[1], &messages);
        Arc::make_mut(&mut carded)["toolCard"] = json!({"count": 2});
        running.mark(&messages[1], &mut carded);
        assert!(carded["thinkingCard"].get("calls").is_none());
    }

    #[test]
    fn a_settled_turn_marks_no_thought() {
        let messages = running_turn();
        let mut owner = row(&messages[1], &messages);
        RunningThinking::new(&messages, false).mark(&messages[1], &mut owner);
        assert!(owner.get("thinkingCard").is_none());
    }

    #[test]
    fn codex_summary_is_one_closed_line() {
        // Codex 0.163 records only a reasoning summary's bold headline.
        assert_eq!(
            thinking_row("**Reading the main instructions**"),
            json!({"preview": "Reading the main instructions", "expandable": false})
        );
    }

    #[test]
    fn claude_thinking_opens_to_its_full_text() {
        let thought = "The user wants me to fix average().\n\nLikely cause: it adds 1 after dividing, and an empty list divides by zero.";
        assert_eq!(
            thinking_row(thought),
            json!({"preview": "The user wants me to fix average().", "expandable": true})
        );
    }

    #[test]
    fn grok_thinking_with_a_heading() {
        let thought = "**Planning the fix**\n\nRead the files first, then run the tests.";
        assert_eq!(thinking_row(thought)["preview"], "Planning the fix");
        assert_eq!(thinking_row(thought)["expandable"], true);
    }
}
