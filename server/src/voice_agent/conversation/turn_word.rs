//! "Answer when I say "reply"" (`voiceAgentWaitForReply`, keys.rs): in a Regular call the agent
//! answers a spoken turn only once the user ends it with one of the call's turn words (the
//! language's words, `call_language.rs`), and every call opens with a short spoken intro.
//!
//! CDXC:VoiceMode 2026-11-12 DECISION:
//! User (Q2750): a turn ends when the user says "reply", "over" or "go ahead" at the end of what they said; the word is taken off before the agent's caption and memory see the text. Q2751: while the setting is on, the agent never answers a spoken turn without one of them (typed turns, a released hold and a recorded turn still end the turn by themselves). Q2752: every call opens with a short intro that says the agent is connected and what it can do, and when the setting is on, to say "reply" when finished, in the user's own words: "Hey, I hear you. Please say reply after you finish speaking so I can reply to you."
//!
//! CDXC:VoiceMode 2026-11-25 DECISION:
//! User: the turn words are set per language, several for each ("mehrere Wörter festlegen können pro Sprache"); English defaults to "reply", "over" and "go ahead", German to "fertig" and "antworte", and a language without its own words uses the English ones (`call_language.rs`). The intro now names the call's own words, so its request reads "Please say "reply", "over" or "go ahead" after you finish speaking so I can reply to you." with the defaults, and a call in another language says the intro in that language, keeping the words as they are.
//!
//! CDXC:VoiceMode 2026-11-25 WHY: the intro's fixed text stays English and the model says it in the call's language, rather than a translation table here: a table would need a reviewed translation for each of the 15 languages, and the model already answers in the call's language.

use std::time::Instant;

use serde_json::{json, Value};
use unicode_width::UnicodeWidthChar;

use super::super::call_language;
use super::super::focus::FocusScope;
use super::{Conversation, Ended, TURN_WORD_HOLD, UNANSWERED_TURN_GRACE};

/// `transcript` without the turn word it ends with, or `None` when it ends with none of `words`.
/// A word counts only as the last word (or words, for "go ahead"), whole, whatever its case
/// (Unicode, so "FERTIG" and "SPÄTER" match "fertig" and "später"; in a Turkish call, `language`
/// "tr", "İŞ BİTTİ" matches "iş bitti" and "IRMAK" matches "ırmak") and whatever punctuation or
/// symbols stand around and between its words (also "؟", "。", "、" and full-width marks), with
/// the apostrophes ', ’ and ʼ counted as one. What is left keeps its own words and the symbols
/// that end them ("C++", "C#", "100%"), and loses the space and punctuation before the word
/// (`is_pause`). A transcript that is only the word gives an empty string.
///
/// CDXC:VoiceMode 2026-11-26 WHY: providers spell transcripts with the language's own punctuation
/// and curly apostrophes ("los geht’s", "أجب؟", "以上です。"), which the ASCII marks of the first
/// version missed, so the turn never ended. Words are split on whitespace and on every character
/// that is neither a letter, a digit, an apostrophe nor a zero-width combining mark (punctuation
/// and symbols). Japanese and Chinese put no spaces between words, so there a turn word matches
/// only as the last stretch after punctuation or a space ("…して、以上です。"), not when it is
/// written straight onto the word before it.
pub(super) fn strip_turn_word(
    transcript: &str,
    words: &[String],
    language: &str,
) -> Option<String> {
    let turkish = language == "tr";
    let heard = words_of(transcript, turkish);
    let mut wanted: Vec<Vec<String>> = words
        .iter()
        .map(|word| {
            words_of(word, turkish)
                .into_iter()
                .map(|(_, word)| word)
                .collect()
        })
        .filter(|word: &Vec<String>| !word.is_empty())
        .collect();
    // The longest first, so a phrase is taken off whole before a shorter word it ends with.
    wanted.sort_by_key(|word| std::cmp::Reverse(word.len()));
    for word in &wanted {
        let Some(first) = heard.len().checked_sub(word.len()) else {
            continue;
        };
        if heard[first..]
            .iter()
            .map(|(_, heard)| heard)
            .eq(word.iter())
        {
            let kept = transcript[..heard[first].0].trim_end_matches(is_pause);
            return Some(kept.to_string());
        }
    }
    None
}

/// The words of `text` with where each starts: lowercase (the Turkish way with `turkish`,
/// `lowercase`), apostrophes as ', and without an apostrophe at either end (a quote mark there is
/// not part of the word).
fn words_of(text: &str, turkish: bool) -> Vec<(usize, String)> {
    let mut words = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, c) in text.char_indices() {
        if is_separator(c) {
            words.extend(current.take());
            continue;
        }
        let word = current.get_or_insert_with(|| (index, String::new()));
        if is_apostrophe(c) {
            word.1.push('\'');
        } else {
            lowercase(c, turkish, &mut word.1);
        }
    }
    words.extend(current);
    words
        .into_iter()
        .filter_map(|(start, word)| {
            let word = word.trim_matches('\'');
            (!word.is_empty()).then(|| (start, word.to_string()))
        })
        .collect()
}

/// Appends `c` in lowercase to `word`: Unicode's language-independent mapping, except that with
/// `turkish` the dotted "İ" becomes "i" and the dotless "I" becomes "ı".
///
/// CDXC:VoiceMode 2026-11-26 WHY: Unicode's language-independent lowercase turns "İ" into "i" plus
/// a combining dot and "I" into "i", so in a Turkish call the sentence-case "İş bitti." never
/// matched the saved "iş bitti" and "IRMAK" never matched "ırmak". Turkish calls fold case the
/// Turkish way on both sides, the saved words and the transcript; every other language keeps the
/// language-independent mapping, where "I" is "i".
fn lowercase(c: char, turkish: bool, word: &mut String) {
    match c {
        'İ' if turkish => word.push('i'),
        'I' if turkish => word.push('ı'),
        _ => word.extend(c.to_lowercase()),
    }
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{02bc}')
}

/// Whitespace, apostrophes and the punctuation that ends a sentence or clause, opens one (Spanish
/// "¡" and "¿"), sets off a quote or marks a pause in the 15 call languages: what stands between the
/// content and the turn word.
///
/// CDXC:VoiceMode 2026-11-26 WHY: the caption is cut back only over these, not over every
/// separator `words_of` splits on, because symbols such as "+", "#", "%", "$" or "→" belong to
/// the content ("Check C++, reply." keeps "Check C++"). A character missing from this list stays
/// in the caption rather than taking content with it.
fn is_pause(c: char) -> bool {
    const PAUSES: &str = ".,!?¡¿;:\"-–—…“”„‘«»、。「」『』！，．：；？،؛؟۔।॥";
    c.is_whitespace() || is_apostrophe(c) || PAUSES.contains(c)
}

/// Whitespace, punctuation and symbols: what is neither a letter, a digit, an apostrophe nor a
/// zero-width combining mark (which belongs to the letter before it).
fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || !(c.is_alphanumeric() || is_apostrophe(c) || UnicodeWidthChar::width(c) == Some(0))
}

/// `words` for a spoken sentence: "reply", "reply" or "go ahead", "a", "b" or "c".
fn said_words(words: &[String]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("\"{word}\"")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

/// The line the agent opens the call with: that it is connected, what it can do in this call, and
/// with `wait_for_reply` (a Regular call with the setting on), to say one of `words` when finished.
pub(super) fn intro_line(
    focus: Option<&FocusScope>,
    wait_for_reply: bool,
    words: &[String],
) -> String {
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
        line.push_str(&format!(
            " Please say {} after you finish speaking so I can reply to you.",
            said_words(words)
        ));
    }
    line
}

/// The message that asks the model to open the call with `line`: word for word in English, and
/// said in the call's language otherwise, with names and the quoted turn words kept as they are.
pub(super) fn intro_message(line: &str, language: &str) -> String {
    if language == "en" {
        return format!(
            "[Ghostex call start] Open the call by saying exactly this, and nothing else: \"{line}\""
        );
    }
    format!(
        "[Ghostex call start] Open the call by saying this in {}, and nothing else. Keep names and the words in double quotes exactly as they are: \"{line}\"",
        call_language::label(language)
    )
}

/// What the model is told about the turn words while the setting is on (added to its instructions).
pub(crate) fn turn_word_instructions(words: &[String]) -> String {
    format!(
        "

## Turn Word
- The user ends each spoken turn by saying {}. That word only hands the turn to you: it is not part of their message, so never answer it, repeat it or mention it. They may speak in several parts before it; answer all of them together.",
        said_words(words)
    )
}

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

    /// Opens the call: the model says the intro line (`intro_line`), in the call's language.
    pub(super) async fn say_intro(&mut self) -> Result<(), Ended> {
        let line = intro_line(
            self.focus.as_ref(),
            self.waits_for_word(),
            &self.call_language.turn_words,
        );
        let message = intro_message(&line, self.call_language.code);
        self.provider_send(json!({
            "type": "conversation.item.create",
            "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": message}]},
        }))
        .await?;
        self.create_response_now().await
    }
}
