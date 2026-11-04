//! CDXC:SessionChat 2026-09-14 WHY:
//! A quoted ordinary chat send reaches the model but bypasses Codex's local accept_answer(), leaving its terminal questions pending.
//! Active questions use Codex's editor inside one serialized terminal job. Retained cards use Codex's identified reply envelope after that editor expires.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::session_chat::*;
use crate::session_chat_send::{
    capture_session_terminal_text, capture_session_terminal_text_vt, write_session_chat_payload,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncAnswer {
    pub question_id: String,
    pub transcript_path: PathBuf,
    pub repeated_title: bool,
    pub title: String,
    pub options: usize,
    pub text: Option<String>,
    /// Where the asking message starts in the transcript; an answer is recorded after it.
    pub question_offset: Option<u64>,
}

pub(crate) fn resolve(
    session: &Value,
    id: &str,
    text: Option<String>,
) -> Result<Option<AsyncAnswer>, String> {
    if crate::session_chat_async_questions::retired_question_ids(session)
        .iter()
        .any(|key| key == id)
    {
        return if text.is_none() {
            Ok(None)
        } else {
            Err("This Codex question has already been answered or skipped.".into())
        };
    }
    let path = resolve_session_chat_transcript_path(
        SessionChatTranscriptAgent::Codex,
        crate::server::read_runtime_text(session, "agentSessionId").as_deref(),
        crate::server::read_runtime_text(session, "agentSessionPath").as_deref(),
    )
    .ok_or("Codex's question transcript is unavailable.")?;
    let started_at = crate::session_chat_async_questions::async_questions_since(session);
    let mut questions = Vec::new();
    let mut collect = |messages: Vec<SessionChatMessage>| {
        for message in messages {
            if message
                .timestamp
                .zip(started_at)
                .is_some_and(|(at, start)| at < start)
            {
                continue;
            }
            for (index, question) in message
                .async_questions
                .unwrap_or_default()
                .into_iter()
                .enumerate()
            {
                questions.push((
                    format!("{}:{index}", message.id),
                    question,
                    message.byte_offset,
                ));
            }
        }
    };
    let messages = read_incremental_transcript_messages(
        &path,
        &mut SessionChatIncrementalState::default(),
        decode_codex_transcript_line,
        Some(&mut collect),
        None,
        None,
        None,
    )
    .map_err(|error| error.to_string())?;
    collect(messages);
    let question = questions
        .iter()
        .find(|(key, ..)| key == id)
        .map(|(_, question, offset)| (question, *offset));
    // CDXC:SessionChat 2026-09-22 WHY:
    // A cached card can outlive the question in Codex's current transcript after compaction or resume. Skip must persist its retirement even when there is no terminal question left to dismiss.
    let Some((question, question_offset)) = question else {
        return if text.is_none() {
            Ok(None)
        } else {
            Err(
                "This Codex question is no longer available. Skip it to dismiss the old card."
                    .into(),
            )
        };
    };
    let repeated_title = questions.iter().any(|(key, other, _)| {
        key != id && normalized(&other.title) == normalized(&question.title)
    });
    let (message_id, index) = id
        .rsplit_once(':')
        .ok_or("Codex's question ID is invalid.")?;
    let index = index
        .parse::<usize>()
        .map_err(|_| "Codex's question index is invalid.")?;
    Ok(Some(AsyncAnswer {
        question_id: serde_json::json!(["request_user_input_async", message_id, index]).to_string(),
        transcript_path: path,
        repeated_title,
        title: question.title.clone(),
        options: question
            .options
            .as_ref()
            .map(|options| options.iter().take(32).filter(|s| s.len() <= 512).count())
            .unwrap_or(0),
        text,
        question_offset,
    }))
}

fn normalized(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn lines(screen: &str) -> Vec<String> {
    screen
        .lines()
        .map(|line| {
            crate::session_chat_options::strip_ansi_sgr(line)
                .trim()
                .to_string()
        })
        .collect()
}

/// Read the displayed binding, including user remaps, instead of assuming Alt+Up or Ctrl+].
///
/// CDXC:SessionChat 2026-10-27 WHY: Codex 0.161 (openai/codex#49804) writes modifiers as glyphs joined to the key on macOS (`⌃] skip`, `⇧← to answer`), `^` for Control on Linux (`^]`) and `ctrl+`/`shift+` on Windows, where every platform used to show `ctrl + ]`. Reading only the spaced spelling left every question shortcut unreadable on Codex 0.164: a chat answer typed into an open question editor was taken by Codex but reported as failed ("Codex's main-prompt shortcut is unavailable"), the card stayed up with the editor still holding the terminal's focus, and Skip could not work at all.
fn key_bytes(label: &str) -> Option<String> {
    let label = label.trim().to_lowercase();
    let mut parts: Vec<_> = label.split('+').map(str::trim).collect();
    let mut key = parts.pop()?;
    let mut modifier = 1;
    for value in parts {
        modifier += match value {
            "shift" | "⇧" => 1,
            "alt" | "option" | "⌥" => 2,
            "ctrl" | "control" | "⌃" | "^" => 4,
            "super" | "cmd" | "⌘" => 8,
            _ => return None,
        };
    }
    // Glyph modifiers sit directly in front of the key; a lone `^` is the key itself.
    while let Some(first) = key.chars().next() {
        let rest = &key[first.len_utf8()..];
        let bit = match first {
            '⇧' => 1,
            '⌥' => 2,
            '⌃' | '^' => 4,
            '⌘' => 8,
            _ => break,
        };
        if rest.is_empty() {
            break;
        }
        modifier += bit;
        key = rest;
    }
    let arrow = match key {
        "↑" | "up" => Some('A'),
        "↓" | "down" => Some('B'),
        "→" | "right" => Some('C'),
        "←" | "left" => Some('D'),
        _ => None,
    };
    if let Some(arrow) = arrow {
        return Some(format!("\x1b[1;{modifier}{arrow}"));
    }
    let code = match key {
        "enter" | "return" => 13,
        "tab" => 9,
        "space" => 32,
        "esc" | "escape" => 27,
        value if value.chars().count() == 1 => value.chars().next()? as u32,
        _ => return None,
    };
    Some(format!("\x1b[{code};{modifier}u"))
}

fn binding(footer: &str, action: &str) -> Option<String> {
    footer
        .split("   ")
        .map(str::trim)
        .find_map(|tip| key_bytes(tip.strip_suffix(action)?.trim()))
}

#[derive(Debug, Clone)]
struct Editor {
    body: String,
    footer: String,
    position: usize,
    count: usize,
}

fn editor(screen: &str) -> Option<Editor> {
    let rows = lines(screen);
    let end = rows
        .iter()
        .rposition(|row| row.contains(" skip") && row.contains(" submit"))?;
    // A main input after this footer makes it historical output, not the active editor.
    if rows[end + 1..]
        .iter()
        .any(|row| row.starts_with('›') || row.starts_with('»') || row.contains(" to answer"))
    {
        return None;
    }
    let start = rows[..end]
        .iter()
        .rposition(|row| row.contains("Queued follow-up inputs"))?
        + 1;
    let mut body: Vec<_> = rows[start..end]
        .iter()
        .filter(|row| !row.is_empty() && !row.starts_with('↳'))
        .cloned()
        .collect();
    let mut position = 1;
    let mut count = 1;
    // CDXC:SessionChat 2026-09-15 WHY: Codex omits the counter for a single question. "rest of compaction" in its title was mistaken for that counter and made Enter fail before the answer was sent.
    if let Some((current, total)) = body.first().and_then(|row| {
        let (left, right) = row.split_once(" of ")?;
        let (current, total) = (left.parse::<usize>().ok()?, right.parse::<usize>().ok()?);
        (current > 0 && total > 1 && current <= total).then_some((current, total))
    }) {
        position = current;
        count = total;
        body.remove(0);
    }
    // Hints can wrap independently of question and answer rows.
    let mut footer = rows[end].clone();
    for row in &rows[end + 1..] {
        if row.contains("main prompt")
            || row.contains("prev question")
            || row.contains("next question")
            || row.contains("queued messages")
            || row.starts_with("option ")
        {
            footer.push_str("   ");
            footer.push_str(row);
        } else if !row.is_empty() {
            return None;
        }
    }
    Some(Editor {
        body: body.join("\n"),
        footer,
        position,
        count,
    })
}

fn question_input<'a>(editor: &'a Editor, title: &str) -> Option<&'a str> {
    let mut expected = title.chars().filter(|c| !c.is_whitespace());
    let mut next = expected.next()?;
    for (index, character) in editor.body.char_indices() {
        if character.is_whitespace() {
            continue;
        }
        if character != next {
            return None;
        }
        match expected.next() {
            Some(character) => next = character,
            None => return Some(&editor.body[index + character.len_utf8()..]),
        }
    }
    None
}

fn matches_question(editor: &Editor, title: &str) -> bool {
    question_input(editor, title)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn collapsed_binding(screen: &str) -> Option<String> {
    let rows = lines(screen);
    let index = rows
        .iter()
        .rposition(|row| row.starts_with("? ") && row.contains(" question"))?;
    binding(rows.get(index + 1)?, "to answer")
}

struct Driver<'a> {
    project_id: &'a str,
    session_id: &'a str,
    zmx_name: &'a str,
    source: &'a str,
    cancelled: &'a (dyn Fn() -> bool + Send + Sync),
}

impl Driver<'_> {
    async fn write(&self, bytes: &str) -> Result<(), String> {
        if (self.cancelled)() {
            return Err("The question action was cancelled.".into());
        }
        write_session_chat_payload(
            self.project_id,
            self.session_id,
            self.zmx_name,
            self.source,
            bytes,
        )
        .await
    }

    async fn wait<T>(
        &self,
        description: &str,
        mut accept: impl FnMut(&str) -> Option<T>,
    ) -> Result<T, String> {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if (self.cancelled)() {
                return Err("The question action was cancelled.".into());
            }
            if let Some(screen) = capture_session_terminal_text(self.zmx_name).await {
                if let Some(value) = accept(&screen) {
                    return Ok(value);
                }
            }
            if Instant::now() >= deadline {
                return Err(format!("Could not verify {description} in Codex. Your answer is kept here; check the terminal before retrying."));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// `None` means Codex's terminal holds no pending question with this title.
    async fn open(&self, title: &str) -> Result<Option<Editor>, String> {
        let screen = capture_session_terminal_text(self.zmx_name)
            .await
            .ok_or("Could not read Codex's terminal.")?;
        if editor(&screen).is_none() {
            let Some(key) = collapsed_binding(&screen) else {
                return Ok(None);
            };
            self.write(&key).await?;
        }
        let mut current = self.wait("the question editor opening", editor).await?;
        // Always search from the first live question. Previously skipped or answered terminal questions may differ from the transcript list.
        for _ in 0..100 {
            if current.position == 1 {
                break;
            }
            let key = binding(&current.footer, "prev question")
                .ok_or("Codex's previous-question shortcut is unavailable.")?;
            let before = current.position;
            self.write(&key).await?;
            current = self
                .wait("the previous question", |screen| {
                    editor(screen).filter(|e| e.position + 1 == before)
                })
                .await?;
        }
        for _ in 0..100 {
            if matches_question(&current, title) {
                return Ok(Some(current));
            }
            if current.position >= current.count {
                break;
            }
            let key = binding(&current.footer, "next question")
                .ok_or("Codex's next-question shortcut is unavailable.")?;
            let before = current.position;
            self.write(&key).await?;
            current = self
                .wait("the next question", |screen| {
                    editor(screen).filter(|e| e.position == before + 1)
                })
                .await?;
        }
        Ok(None)
    }

    async fn run(&self, answer: &AsyncAnswer) -> Result<(), String> {
        let Some(current) = self.open(&answer.title).await? else {
            return match answer.text {
                None => Ok(()),
                // open() may have expanded Codex's editor for other live questions; the reply goes through the main composer.
                Some(_) => {
                    self.return_to_main_prompt().await?;
                    self.send_retained_answer(answer).await
                }
            };
        };
        // The live editor exposes titles rather than IDs; the retained-reply envelope does not have this ambiguity.
        if answer.repeated_title {
            return Err("Codex has repeated this question. Answer it in the terminal so the correct question is selected.".into());
        }
        if let Some(text) = &answer.text {
            // Bracketed paste switches named choices to Other without a digit shortcut accidentally submitting a different answer.
            if answer.options > 0 {
                self.write("\x1b[200~ \x1b[201~").await?;
                self.wait("the question's text input", |screen| {
                    let e = editor(screen)?;
                    (matches_question(&e, &answer.title)
                        && e.body
                            .lines()
                            .any(|line| line.starts_with(&format!("› {}.", answer.options + 1))))
                    .then_some(())
                })
                .await?;
            }
            // These kill keys act only inside the verified question editor, never the main composer.
            self.write(&"\x1b[117;5u\x1b[107;5u".repeat(80)).await?;
            self.wait("the empty question input", |screen| {
                let e = editor(screen)?;
                if !matches_question(&e, &answer.title) {
                    return None;
                }
                let tail = if answer.options > 0 {
                    e.body
                        .split(&format!("› {}.", answer.options + 1))
                        .nth(1)?
                        .trim()
                } else {
                    question_input(&e, &answer.title)?.trim()
                };
                matches!(
                    tail,
                    "" | "Type your answer" | "Other" | "Other (write an answer)"
                )
                .then_some(())
            })
            .await?;
            self.write(&crate::session_chat_send::wrap_terminal_bracketed_paste_text(text))
                .await?;
            let staged = self
                .wait("your answer in the question input", |screen| {
                    let e = editor(screen)?;
                    if !matches_question(&e, &answer.title) {
                        return None;
                    }
                    let input = if answer.options > 0 {
                        e.body.split(&format!("› {}.", answer.options + 1)).nth(1)?
                    } else {
                        question_input(&e, &answer.title)?
                    };
                    let input = normalized(input);
                    // Codex collapses large pastes into an atomic placeholder. The input was verified empty before this paste.
                    let placeholder = format!("[PastedContent{}chars]", text.chars().count());
                    let pasted = input == placeholder
                        || input.strip_prefix(&placeholder).is_some_and(|suffix| {
                            suffix.strip_prefix('#').is_some_and(|n| {
                                !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())
                            })
                        });
                    (input == normalized(text) || pasted).then_some(e)
                })
                .await?;
            let key = binding(&staged.footer, "submit")
                .ok_or("Codex's question-submit shortcut is unavailable.")?;
            self.write(&key).await?;
        } else {
            let key = binding(&current.footer, "skip")
                .ok_or("Codex's question-skip shortcut is unavailable.")?;
            self.write(&key).await?;
        }
        let framed_answer = answer.text.as_ref().map(|text| {
            format!(
                "{}{text}",
                crate::session_chat_async_questions::answer_prefix(&answer.title)
            )
        });
        self.wait("Codex accepting the question action", |screen| {
            if editor(screen).is_none()
                && framed_answer.as_ref().is_some_and(|text| {
                    crate::session_chat_notice::session_chat_screen_shows_queued_input(
                        Some("codex"),
                        screen,
                        text,
                    )
                })
            {
                return Some(());
            }
            if let Some(e) = editor(screen) {
                (e.count < current.count && !matches_question(&e, &answer.title)).then_some(())
            } else {
                // Closing an unrelated dialog is not an acknowledgement. The final question must return to the real composer.
                (current.count == 1
                    && crate::session_chat_composer::detect_session_chat_composer_readiness(
                        Some("codex"),
                        screen,
                        None,
                    )
                    .state
                        == crate::session_chat_composer::SessionChatComposerState::Ready)
                    .then_some(())
            }
        })
        .await
    }

    /// CDXC:SessionChat 2026-10-04 WHY:
    /// Codex 0.159/0.160 clears its async question editor when a turn finishes or a collapsed question expires. The transcript-backed Ghostex card still has a valid identity; deliver Codex's canonical reply through its verified main composer and retire the card only after the transcript records it.
    async fn send_retained_answer(&self, answer: &AsyncAnswer) -> Result<(), String> {
        if answer.question_id.len() > 512 {
            return Err("Codex's question ID is too long to send safely.".into());
        }
        let prefix = crate::session_chat_async_questions::answer_prefix(&answer.title);
        let text = answer
            .text
            .as_deref()
            .ok_or("A question answer is required.")?;
        let payload = format!(
            "<send_user_message_question_reply>\n{}\n</send_user_message_question_reply>",
            serde_json::json!([{
                "questionItemId": answer.question_id,
                "question": &prefix[2..prefix.len() - 2],
                "answer": text,
            }])
        );
        let expected = format!("{prefix}{text}");
        if answer_already_recorded(answer, &expected).await {
            return Ok(());
        }
        let screen = capture_session_terminal_text_vt(self.zmx_name)
            .await
            .ok_or("Could not read Codex's input box. Nothing was submitted.")?;
        let input = crate::session_chat_composer::session_chat_composer_input("codex", &screen)
            .ok_or("Codex's main input box is not ready. Nothing was submitted.")?;
        if !input.is_empty() || input.shell_mode {
            return Err("Codex's input box contains a draft. Save or clear it before answering this question.".into());
        }
        let path = answer.transcript_path.clone();
        let baseline = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        self.write(&crate::session_chat_send::wrap_terminal_bracketed_paste_text(&payload))
            .await?;
        let generation = std::sync::atomic::AtomicU64::new(0);
        let pasted = crate::session_chat_send::verify_session_chat_paste_landed(
            self.zmx_name,
            Some("codex"),
            &payload,
            100,
            crate::session_chat_send::session_chat_verify_timeout_ms(payload.len()),
            &generation,
            0,
        )
        .await;
        if !matches!(
            pasted,
            crate::session_chat_send::SessionChatPasteVerification::Landed
        ) {
            return Err("Could not verify the question answer in Codex's input box. Check the terminal before retrying.".into());
        }
        self.write(crate::session_chat_send::SESSION_CHAT_SUBMIT)
            .await?;
        crate::session_chat_send_submit::confirm_submitted(
            "codex",
            self.project_id,
            self.session_id,
            self.zmx_name,
            self.source,
            &payload,
            crate::session_chat_send::SESSION_CHAT_SUBMIT,
            self.cancelled,
            None,
        )
        .await
        .map_err(|error| error.message)?;
        let mut cursor = SessionChatIncrementalState::default();
        cursor.rebase(baseline);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if (self.cancelled)() {
                return Err(
                    "The question action was cancelled. Check the terminal before retrying.".into(),
                );
            }
            let path = path.clone();
            let expected = expected.clone();
            let (next_cursor, recorded) = tokio::task::spawn_blocking(move || {
                let messages = read_incremental_transcript_messages(
                    &path,
                    &mut cursor,
                    decode_codex_transcript_line,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap_or_default();
                let recorded = messages.iter().any(|message| {
                    message.role == SessionChatRole::User
                        && message.byte_offset.is_some_and(|offset| offset >= baseline)
                        && message.blocks.iter().any(|block| {
                            matches!(block, SessionChatBlock::Text { text } if text == &expected)
                        })
                });
                (cursor, recorded)
            })
            .await
            .map_err(|_| {
                "Could not verify Codex's recorded answer. Check the terminal before retrying."
            })?;
            cursor = next_cursor;
            if recorded {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Codex has not recorded the question answer yet. Check the terminal before retrying.".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // Codex advances after accepting a question, and a failed lookup may also leave one focused. Restore the main prompt on either outcome so an ordinary chat send cannot answer another question accidentally.
    async fn return_to_main_prompt(&self) -> Result<(), String> {
        for _ in 0..100 {
            let screen = capture_session_terminal_text(self.zmx_name)
                .await
                .ok_or("Could not read Codex's terminal after answering.")?;
            let Some(current) = editor(&screen) else {
                return Ok(());
            };
            let action = if current.position == 1 {
                "main prompt"
            } else {
                "prev question"
            };
            let key = binding(&current.footer, action)
                .ok_or("Codex's main-prompt shortcut is unavailable.")?;
            self.write(&key).await?;
            self.wait("returning to the main prompt", |screen| {
                if current.position == 1 {
                    (editor(screen).is_none() && collapsed_binding(screen).is_some()).then_some(())
                } else {
                    editor(screen)
                        .filter(|e| e.position + 1 == current.position)
                        .map(|_| ())
                }
            })
            .await?;
        }
        Err("Codex could not return to its main prompt. Check the terminal before sending another message.".into())
    }
}

/// CDXC:SessionChat 2026-10-05 WHY:
/// A retained answer whose transcript check timed out ("Codex has not recorded the question answer yet") left its card up, and answering again typed the reply a second time once Codex had in fact taken the first. Before typing, the retained reply looks for a user turn after the question that already carries this exact answer and, when there is one, settles as answered without writing anything.
async fn answer_already_recorded(answer: &AsyncAnswer, expected: &str) -> bool {
    let path = answer.transcript_path.clone();
    let after = answer.question_offset;
    let expected = expected.to_string();
    tokio::task::spawn_blocking(move || {
        read_incremental_transcript_messages(
            &path,
            &mut SessionChatIncrementalState::default(),
            decode_codex_transcript_line,
            None,
            None,
            None,
            None,
        )
        .unwrap_or_default()
        .iter()
        .any(|message| {
            message.role == SessionChatRole::User
                && message
                    .byte_offset
                    .zip(after)
                    .is_none_or(|(offset, after)| offset > after)
                && message.blocks.iter().any(
                    |block| matches!(block, SessionChatBlock::Text { text } if text == &expected),
                )
        })
    })
    .await
    .unwrap_or(false)
}

pub(crate) async fn run(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    answer: &AsyncAnswer,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    let driver = Driver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = driver.run(answer).await;
    let restored = driver.return_to_main_prompt().await;
    outcome.and(restored)
}

/// CDXC:SessionChat 2026-10-27 WHY: Codex opens its async question editor only when the user presses its key in the terminal, and it then holds the keyboard until they leave it. Read as a footerless menu, it showed in chat as a blocking terminal dialog that disabled the question's own card and the composer, so a question looked at once in the terminal could no longer be answered or talked past from chat. The card owns the editor, so the dialog detectors skip it, and a chat message leaves it first (`leave_question_editor`), which keeps every unanswered question and its draft in Codex's collapsed list.
pub(crate) fn question_editor_on_screen(screen: &str) -> bool {
    editor(screen).is_some()
}

/// Returns Codex from its async question editor to the main input box before a chat message.
pub(crate) async fn leave_question_editor(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    Driver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    }
    .return_to_main_prompt()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUESTIONS: &str = "• Working (0s • esc to interrupt)\n\n• Queued follow-up inputs\n  ↳ another message\n\n  2 of 3\n\n  Which layout?\n\n  › 1. Compact\n    2. Spacious\n    3. Other\n\n  enter submit   ctrl + ] skip   ⌥ + ↓ prev question   ⌥ + ↑ next question\n";

    #[test]
    fn targets_the_active_question_not_queued_or_historical_text() {
        let e = editor(QUESTIONS).unwrap();
        assert_eq!((e.position, e.count), (2, 3));
        assert!(matches_question(&e, "Which layout?"));
        assert!(!matches_question(&e, "Compact"));
        assert!(!matches_question(&e, "Which layout"));
        assert!(editor(&(QUESTIONS.to_string() + "\n› Main composer draft\n")).is_none());
        assert!(
            editor(&(QUESTIONS.to_string() + "\n? 3 questions\n  ⌥ + ↑ to answer\n")).is_none()
        );
    }

    #[test]
    fn question_titles_can_wrap_without_matching_answer_text() {
        let screen = QUESTIONS.replace("Which layout?", "Which very long\n  layout?");
        let e = editor(&screen).unwrap();
        assert!(matches_question(&e, "Which very long layout?"));
        assert!(!matches_question(&e, "Which very long layout? Compact"));
    }

    #[test]
    fn single_idle_question_containing_of_is_not_a_progress_counter() {
        let title = "Did the chat percentage stay at 66% for the rest of compaction, or did it catch up when you switched back from the terminal?";
        let screen = format!("• Queued follow-up inputs\n\n  {title}\n\n  Type your answer\n\n  enter submit   ctrl + ] skip   ⌥ + ↓ main prompt");
        let e = editor(&screen).expect("single question editor remains answerable");
        assert_eq!((e.position, e.count), (1, 1));
        assert!(matches_question(&e, title));
        assert_eq!(
            question_input(&e, title).unwrap().trim(),
            "Type your answer"
        );

        let staged = screen.replace("Type your answer", "yes catches up when i switch back");
        let e = editor(&staged).expect("staged single answer remains detectable");
        assert_eq!(
            question_input(&e, title).unwrap().trim(),
            "yes catches up when i switch back"
        );
    }

    #[test]
    fn uses_displayed_question_shortcuts_and_rejects_unknown_bindings() {
        let e = editor(QUESTIONS).unwrap();
        assert_eq!(
            binding(&e.footer, "next question").as_deref(),
            Some("\x1b[1;3A")
        );
        assert_eq!(binding(&e.footer, "skip").as_deref(), Some("\x1b[93;5u"));
        assert_eq!(
            binding("shift + ← to answer", "to answer").as_deref(),
            Some("\x1b[1;2D")
        );
        assert_eq!(key_bytes("unknown + x"), None);
    }

    /// Codex 0.164.0 on macOS, captured live: the collapsed summary, then the editor `⇧←` opens.
    const COLLAPSED_0164: &str = "• Requesting user input (30s • esc to interrupt)\n\n• Queued follow-up inputs\n  ? 2 questions · 8s\n    ⇧← to answer\n\n\n› Ask Codex to do anything\n\n  GPT-6-Sol high · scratch · main · Context 6% used\n  ? for shortcuts";
    const EXPANDED_0164: &str = "• Requesting user input (41s • esc to interrupt)\n\n• Queued follow-up inputs\n\n\n  1 of 2\n  Which fruit do you prefer?\n\n  › 1. Apple\n    2. Pear\n    3. Other\n\n  enter submit   ⌃] skip   ⇧→ main prompt   ⇧← next question";

    #[test]
    fn reads_codex_0164_glyph_shortcuts() {
        assert!(editor(COLLAPSED_0164).is_none());
        assert_eq!(
            collapsed_binding(COLLAPSED_0164).as_deref(),
            Some("\x1b[1;2D")
        );

        let e = editor(EXPANDED_0164).expect("the 0.164 editor is recognized");
        assert_eq!((e.position, e.count), (1, 2));
        assert!(matches_question(&e, "Which fruit do you prefer?"));
        assert_eq!(binding(&e.footer, "submit").as_deref(), Some("\x1b[13;1u"));
        assert_eq!(binding(&e.footer, "skip").as_deref(), Some("\x1b[93;5u"));
        assert_eq!(
            binding(&e.footer, "main prompt").as_deref(),
            Some("\x1b[1;2C")
        );
        assert_eq!(
            binding(&e.footer, "next question").as_deref(),
            Some("\x1b[1;2D")
        );

        // After the first answer Codex shows the last question alone, without a counter.
        let last = "• Queued follow-up inputs\n\n\n  Which drink do you prefer?\n\n  › 1. Tea\n    2. Coffee\n    3. Other\n\n  enter submit   ⌃] skip   ⇧→ main prompt";
        let e = editor(last).expect("the remaining question is recognized");
        assert_eq!((e.position, e.count), (1, 1));
        assert!(matches_question(&e, "Which drink do you prefer?"));
        assert_eq!(
            binding(&e.footer, "main prompt").as_deref(),
            Some("\x1b[1;2C")
        );
    }

    #[test]
    fn an_open_question_editor_is_the_cards_not_a_blocking_dialog() {
        assert!(question_editor_on_screen(EXPANDED_0164));
        assert!(question_editor_on_screen(QUESTIONS));
        assert!(!question_editor_on_screen(COLLAPSED_0164));
        assert!(crate::session_chat_codex_dialog::detect_codex_dialog(EXPANDED_0164).is_none());
        assert!(
            crate::session_chat_codex_blocking::detect_codex_blocking_screen(EXPANDED_0164)
                .is_none()
        );
        // A chat message leaves the editor first instead of typing into it or being refused.
        let readiness = crate::session_chat_composer::detect_session_chat_composer_ready(
            Some("codex"),
            EXPANDED_0164,
        );
        assert!(readiness.should_dismiss());
        // Collapsed, the main input box is live as usual.
        assert!(
            !crate::session_chat_composer::detect_session_chat_composer_ready(
                Some("codex"),
                COLLAPSED_0164
            )
            .should_dismiss()
        );
    }

    #[test]
    fn reads_every_platforms_modifier_spelling() {
        for (label, bytes) in [
            ("⌃]", "\x1b[93;5u"),
            ("^]", "\x1b[93;5u"),
            ("ctrl+]", "\x1b[93;5u"),
            ("ctrl + ]", "\x1b[93;5u"),
            ("⇧←", "\x1b[1;2D"),
            ("shift+←", "\x1b[1;2D"),
            ("⌥↑", "\x1b[1;3A"),
            ("⌥ + ↑", "\x1b[1;3A"),
            ("⌃⇧↓", "\x1b[1;6B"),
            ("alt+shift+→", "\x1b[1;4C"),
            ("^", "\x1b[94;1u"),
            ("esc", "\x1b[27;1u"),
        ] {
            assert_eq!(key_bytes(label).as_deref(), Some(bytes), "{label}");
        }
        assert_eq!(key_bytes("⌃x ⌃y"), None);
    }
}
