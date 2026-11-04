//! Finds the ```questions blocks in an agent's replies. The grammar itself (what a block may say,
//! its id, the answers message) is `ghostex_gx_visual::questions`, which `ghostex visual check`
//! reads too.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! native_markdown.rs marks the fence with the form's id; questions/forms.rs keeps the reader's picks and sends them; apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx draw it; skills/ghostex-visuals/SKILL.md teaches the grammar. Change them together.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

pub use ghostex_gx_visual::questions::{
    answers_message, form_id, is_questions_language, notes_key, parse_form, resend_form_id,
    takes_notes, FormChoice, FormQuestion, QuestionForm, NOTE_SUFFIX, OTHER_CHOICE, SKIPPED,
};

use crate::transcript::jsstr::{js_trim, split_lf};
use crate::transcript::native_markdown::{fence_close, fence_open};
use crate::transcript::prose::prose_markdown;

/// The finished fences in one message's Markdown, as `(language, body)`: a fence still streaming
/// is not one yet.
pub fn finished_fences(markdown: &str) -> Vec<(String, String)> {
    fence_spans(markdown)
        .into_iter()
        .map(|(_, _, language, body)| (language, body))
        .collect()
}

/// The finished fences in one message's Markdown, as `(open line, close line, language, body)`.
fn fence_spans(markdown: &str) -> Vec<(usize, usize, String, String)> {
    let lines = split_lf(markdown);
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((character, run, info)) = fence_open(lines[index]) else {
            index += 1;
            continue;
        };
        let close = lines[index + 1..]
            .iter()
            .position(|line| {
                fence_close(line)
                    .is_some_and(|(closing, length)| closing == character && length >= run)
            })
            .map(|offset| index + 1 + offset);
        let Some(close) = close else {
            break;
        };
        let language = js_trim(info)
            .split(char::is_whitespace)
            .next()
            .unwrap_or_default();
        found.push((
            index,
            close,
            language.to_string(),
            lines[index + 1..close].join("\n"),
        ));
        index = close + 1;
    }
    found
}

/// What a generated-content fence draws (`content_fences`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    /// A ```questions block that asks something, drawn as a form.
    Form,
    /// A published page's card (```visual with a `page`, from `ghostex show`).
    Page,
    /// A drawing: a chart, stats or a table (```visual or ```vega-lite).
    Chart,
}

/// One fence of a message that draws generated content, with its body.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentFence {
    pub kind: ContentKind,
    pub body: String,
}

/// The fences in one message's Markdown that draw generated content: question forms that ask
/// something, published pages and drawings. Summary mode keeps the messages that hold any in view
/// (`turns::content_rows`), and the ? pill's list names them (`questions/content.rs`).
pub fn content_fences(markdown: &str) -> Vec<ContentFence> {
    if !markdown.contains("```") && !markdown.contains("~~~") {
        return Vec::new();
    }
    fence_spans(markdown)
        .into_iter()
        .filter_map(|(_, _, language, body)| {
            let kind = if is_questions_language(&language) {
                let form = parse_form(&body);
                (form.error.is_empty() && !form.questions.is_empty())
                    .then_some(ContentKind::Form)?
            } else if ghostex_gx_visual::is_visual_language(&language) {
                if ghostex_gx_visual::page_reference(&body).is_some() {
                    ContentKind::Page
                } else {
                    ContentKind::Chart
                }
            } else {
                return None;
            };
            Some(ContentFence { kind, body })
        })
        .collect()
}

/// The finished ```questions fences in one message's Markdown, as `(id, body)`.
pub fn form_fences(markdown: &str) -> Vec<(String, String)> {
    finished_fences(markdown)
        .into_iter()
        .filter(|(language, _)| is_questions_language(language))
        .map(|(_, body)| (form_id(&body), body))
        .collect()
}

/// Every form the agent's replies hold, oldest first, once each. A block that only withdraws
/// earlier questions (`open_questions.rs`) is not a form. A block that could not be read is
/// marked `resent` once gxserver's message asking for it again is in the transcript.
pub fn question_forms(messages: &[ChatMessage]) -> Vec<QuestionForm> {
    let (mut forms, resent) = form_entries(messages, &[]);
    mark_resent(&mut forms, &resent);
    forms
}

/// The forms the replies in `messages` hold that `prior` (the forms of the messages before them)
/// does not, and the ids gxserver's messages among them asked to have sent again: the reading half
/// of [`question_forms`], before [`mark_resent`].
pub fn form_entries(
    messages: &[ChatMessage],
    prior: &[QuestionForm],
) -> (Vec<QuestionForm>, Vec<String>) {
    let mut forms: Vec<QuestionForm> = Vec::new();
    let mut resent: Vec<String> = Vec::new();
    for message in messages {
        if message.role == ChatRole::User {
            if let Some(id) = resend_form_id(&prose_markdown(&message.blocks)) {
                resent.push(id.to_string());
            }
            continue;
        }
        if message.role != ChatRole::Assistant {
            continue;
        }
        for (id, body) in form_fences(&prose_markdown(&message.blocks)) {
            if !prior.iter().chain(forms.iter()).any(|form| form.id == id) {
                let form = parse_form(&body);
                if !form.is_withdrawal() {
                    forms.push(form);
                }
            }
        }
    }
    (forms, resent)
}

/// Marks a form that could not be read once gxserver's message asking for it again is in the
/// transcript.
pub fn mark_resent(forms: &mut [QuestionForm], resent: &[String]) {
    for form in forms.iter_mut().filter(|form| !form.error.is_empty()) {
        form.resent = resent.contains(&form.id);
    }
}

/// Drops a ```questions block that could not be read once the agent's next reply holds a form that
/// reads, and returns the ids of gxserver's messages that asked for those blocks again, which the
/// list draws as nothing. The messages stay in `messages` so the reply before each still ends its
/// own turn. While the agent has not answered the resend yet, or its answer could not be read
/// either, the block and the message both stay.
///
/// CDXC:SessionChat 2026-10-21 DECISION:
/// User (Q366): "Hide it entirely": once the questions were sent again successfully, the failed card disappears from the transcript and only the working form shows; the resend row (Ghostex's own message to the agent) goes with it. The re-ask is any readable form in the agent's replies between the resend row and the next user message.
pub fn hide_redone_forms(messages: &mut Vec<ChatMessage>) -> Vec<String> {
    let redone = redone_forms(messages);
    if redone.is_empty() {
        return Vec::new();
    }
    strip_redone_forms(messages, &redone);
    redone.into_iter().map(|(_, resend)| resend).collect()
}

/// The forms [`hide_redone_forms`] drops, as `(form id, id of the resend message)`: each resend
/// message the agent answered with a form that reads, before the next user message.
pub fn redone_forms(messages: &[ChatMessage]) -> Vec<(String, String)> {
    let mut redone: Vec<(String, String)> = Vec::new();
    for (at, message) in messages.iter().enumerate() {
        if message.role != ChatRole::User {
            continue;
        }
        let prose = prose_markdown(&message.blocks);
        let Some(id) = resend_form_id(&prose) else {
            continue;
        };
        let answered = messages[at + 1..]
            .iter()
            .take_while(|reply| reply.role != ChatRole::User)
            .filter(|reply| reply.role == ChatRole::Assistant)
            .flat_map(|reply| form_fences(&prose_markdown(&reply.blocks)))
            .any(|(form, body)| {
                form != id && {
                    let form = parse_form(&body);
                    form.error.is_empty() && !form.questions.is_empty()
                }
            });
        if answered {
            redone.push((id.to_string(), message.id.clone()));
        }
    }
    redone
}

/// Takes the ```questions blocks of the `redone` forms out of every reply, and a reply left with
/// nothing out of the list.
pub fn strip_redone_forms(messages: &mut Vec<ChatMessage>, redone: &[(String, String)]) {
    if redone.is_empty() {
        return;
    }
    messages.retain_mut(|message| {
        if message.role != ChatRole::Assistant {
            return true;
        }
        let mut stripped = false;
        for block in &mut message.blocks {
            let ChatBlock::Text { text } = block else {
                continue;
            };
            let spans: Vec<(usize, usize)> = fence_spans(text)
                .into_iter()
                .filter(|(_, _, language, body)| {
                    is_questions_language(language)
                        && redone.iter().any(|(id, _)| *id == form_id(body))
                })
                .map(|(open, close, _, _)| (open, close))
                .collect();
            if spans.is_empty() {
                continue;
            }
            let kept: Vec<&str> = split_lf(text)
                .into_iter()
                .enumerate()
                .filter(|(line, _)| {
                    !spans
                        .iter()
                        .any(|(open, close)| (*open..=*close).contains(line))
                })
                .map(|(_, line)| line)
                .collect();
            *text = js_trim(&kept.join("\n")).to_string();
            stripped = true;
        }
        if stripped {
            message
                .blocks
                .retain(|block| !matches!(block, ChatBlock::Text { text } if text.is_empty()));
        }
        !(stripped && message.blocks.is_empty())
    });
}
