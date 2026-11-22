//! "Answer when I say "reply"" (`voiceAgentWaitForReply`, keys.rs): in a Regular call the agent
//! answers a spoken turn only once the user ends it with a turn word, and every call opens with a
//! short spoken intro.
//!
//! CDXC:VoiceMode 2026-11-12 DECISION:
//! User (Q2750): a turn ends when the user says "reply", "over" or "go ahead" at the end of what they said; the word is taken off before the agent's caption and memory see the text. Q2751: while the setting is on, the agent never answers a spoken turn without one of them (typed turns, a released hold and a recorded turn still end the turn by themselves). Q2752: every call opens with a short intro that says the agent is connected and what it can do, and when the setting is on, to say "reply" when finished, in the user's own words: "Hey, I hear you. Please say reply after you finish speaking so I can reply to you."

use std::time::Instant;

use serde_json::{json, Value};

use super::super::focus::FocusScope;
use super::{Conversation, Ended, TURN_WORD_HOLD, UNANSWERED_TURN_GRACE};

/// The words that end a turn, as the transcript spells them (lowercase, without punctuation).
const TURN_WORDS: [&str; 3] = ["reply", "over", "go ahead"];

/// The intro's request when the setting is on, in the user's words.
const SAY_REPLY: &str = "Please say reply after you finish speaking so I can reply to you.";

/// `transcript` without the turn word it ends with, or `None` when it ends with none. The word
/// counts only as the last word (or two, for "go ahead"), whatever its case and the punctuation
/// around it; what is left keeps its own words and loses the comma or space before the word. A
/// transcript that is only the word gives an empty string.
pub(super) fn strip_turn_word(transcript: &str) -> Option<String> {
    let trimmed = transcript.trim_end_matches(|c: char| c.is_whitespace() || is_mark(c));
    for word in TURN_WORDS {
        let Some(start) = trimmed.len().checked_sub(word.len()) else {
            continue;
        };
        if !trimmed.is_char_boundary(start) || !trimmed[start..].eq_ignore_ascii_case(word) {
            continue;
        }
        let kept = &trimmed[..start];
        // A whole word only: "moreover" or "replay" does not end the turn.
        if kept.chars().last().is_some_and(char::is_alphanumeric) {
            continue;
        }
        let kept = kept.trim_end_matches(|c: char| c.is_whitespace() || is_mark(c));
        return Some(kept.to_string());
    }
    None
}

fn is_mark(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\'' | '\u{201c}' | '\u{201d}' | '\u{2026}' | '-'
    )
}

/// The line the agent opens the call with: that it is connected, what it can do in this call, and
/// with `wait_for_reply` (a Regular call with the setting on), to say "reply" when finished.
pub(super) fn intro_line(focus: Option<&FocusScope>, wait_for_reply: bool) -> String {
    let can_do = match focus {
        None => "I can tell you what your sessions are doing, read you their replies, send them prompts and answer their questions.".to_string(),
        Some(scope) if scope.gbot.is_some() => String::new(),
        Some(scope) => format!(
            "I can tell you what {} is doing, read you its replies, send it prompts and answer its questions.",
            scope.title
        ),
    };
    let mut line = String::from("Hey, I hear you.");
    if !can_do.is_empty() {
        line.push(' ');
        line.push_str(&can_do);
    }
    if wait_for_reply {
        line.push(' ');
        line.push_str(SAY_REPLY);
    }
    line
}

/// The message that asks the model to open the call with `line`, word for word.
pub(super) fn intro_message(line: &str) -> String {
    format!(
        "[Ghostex call start] Open the call by saying exactly this, and nothing else: \"{line}\""
    )
}

/// What the model is told about the turn word while the setting is on (added to its instructions).
pub(crate) const TURN_WORD_INSTRUCTIONS: &str = "

## Turn Word
- The user ends each spoken turn by saying \"reply\", \"over\" or \"go ahead\". That word only hands the turn to you: it is not part of their message, so never answer it, repeat it or mention it. They may speak in several parts before it; answer all of them together.";

impl Conversation {
    /// A Regular call with the setting on: spoken turns wait for the turn word.
    pub(super) fn waits_for_word(&self) -> bool {
        self.wait_for_reply && !self.push_to_talk
    }

    /// The user said something without the turn word a moment ago: an announcement waits.
    pub(super) fn words_waiting(&self, now: Instant) -> bool {
        self.waits_for_word()
            && self
                .words_waiting_since
                .is_some_and(|since| now.duration_since(since) < TURN_WORD_HOLD)
    }

    /// The user ended their turn with the turn word: the agent answers everything said since its
    /// last reply, now, or once the reply still running (or being dropped) has ended.
    pub(super) async fn heard_turn_word(&mut self) -> Result<(), Ended> {
        self.words_waiting_since = None;
        self.last_talk = Instant::now();
        if self.response_active {
            // Asked for when the running reply ends (`response.done`).
            self.turn_unanswered = true;
            return Ok(());
        }
        if !self.dropped_responses.is_empty() {
            // The provider still runs the reply it started by itself; ask once it is cancelled.
            self.turn_unanswered = true;
            self.create_at = Some(Instant::now() + UNANSWERED_TURN_GRACE);
            return Ok(());
        }
        self.create_response_now().await
    }

    /// A reply the provider started by itself while the call waits for the turn word (xAI may
    /// ignore `create_response: false`, provider.rs): it is cancelled, and nothing of it reaches
    /// the client, the tools or the conversation (`drops_event`).
    pub(super) async fn unrequested_response(&mut self, event: &Value) -> Result<(), Ended> {
        if let Some(id) = event.pointer("/response/id").and_then(Value::as_str) {
            self.dropped_responses.insert(id.to_string());
        }
        self.provider_send(json!({"type": "response.cancel"})).await
    }

    /// Whether `event` belongs to a dropped reply (and is handled here). Its items are deleted
    /// from the conversation, so the model never sees an answer the user did not ask for.
    pub(super) async fn drops_event(&mut self, kind: &str, event: &Value) -> Result<bool, Ended> {
        if self.dropped_responses.is_empty() && self.dropped_items.is_empty() {
            return Ok(false);
        }
        let response_id = event
            .get("response_id")
            .or_else(|| event.pointer("/response/id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let item_id = event
            .get("item_id")
            .or_else(|| event.pointer("/item/id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let dropped = (!response_id.is_empty() && self.dropped_responses.contains(response_id))
            || (!item_id.is_empty() && self.dropped_items.contains(item_id));
        if !dropped {
            return Ok(false);
        }
        match kind {
            "response.output_item.added" if !item_id.is_empty() => {
                if self.dropped_items.insert(item_id.to_string()) {
                    self.provider_send(
                        json!({"type": "conversation.item.delete", "item_id": item_id}),
                    )
                    .await?;
                }
            }
            "response.done" => {
                self.dropped_responses.remove(response_id);
            }
            _ => {}
        }
        Ok(true)
    }

    /// Opens the call: the model says the intro line (`intro_line`) word for word.
    pub(super) async fn say_intro(&mut self) -> Result<(), Ended> {
        let line = intro_line(self.focus.as_ref(), self.waits_for_word());
        self.provider_send(json!({
            "type": "conversation.item.create",
            "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": intro_message(&line)}]},
        }))
        .await?;
        self.create_response_now().await
    }
}
