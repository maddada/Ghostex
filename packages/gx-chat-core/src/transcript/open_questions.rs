//! The questions the agent asked and where they sit: every ```questions form, every one-click
//! question written with answer links (`inline_answers.rs`, a one-question form), and every question
//! page card (`ghostex show` of a page with `<gx-questions>`, whose block carries `"questions"`),
//! oldest first, with the tags they ask, whether their answers came back, and whether a later
//! question made them obsolete. Also the Q-tag links in the agent's replies.
//!
//! An asked question is unanswered while the transcript carries no answers for it and nothing
//! later made it obsolete: a form or page that asks every one of its tags again (it supersedes
//! the earlier one), or a ```questions block whose `"withdraw"` names them. A question without
//! tags can only be answered, since nothing can name it.
//!
//! CDXC:SessionChat 2026-10-14 DECISION:
//! User (Q81 = C, Q82): a "Previous question" pill beside Scroll to bottom steps through the unanswered question forms and question pages, and Q-tags (Q80, Q81…) in the agent's messages are links to the form or page that asked that tag (the latest one, answered or not). Questions the agent marks obsolete drop off the count: a later form or page asking the same tags supersedes the earlier one, and a later ```questions block can withdraw tags with `"withdraw": ["Q40"]`.
//!
//! CDXC:SessionChat 2026-10-14 SEE-ALSO:
//! questions/open_questions.rs steps the pill and lands the jumps (the document's `openQuestions`); apps/desktop/src/app/native_chat/open_questions.rs and apps/mobile/app/src/chat/native/transcript/OpenQuestions.tsx draw the pill and the highlight; server/src/ghostex_cli/show.rs writes a page block's `"questions"`; skills/ghostex-visuals/SKILL.md teaches tags and `withdraw`. Change them together.

use ghostex_gx_protocol::{ChatMessage, ChatRole};
use serde_json::Value;

use crate::transcript::jsstr::{js_trim, split_lf};
use crate::transcript::native_markdown::{fence_close, fence_open};
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_forms::{finished_fences, is_questions_language, parse_form};

/// The href a Q-tag link carries; the chat jumps to the question instead of opening anything.
/// The tag follows it, then `;` and its [`TagState`] (`#gx-question=Q80;open`).
pub const QUESTION_LINK_PREFIX: &str = "#gx-question=";

/// What became of the question a Q-tag names, which the renderers draw the tag's pill from.
///
/// CDXC:SessionChat 2026-10-15 DECISION:
/// User: "i want them to show different pill color when not answered and greenish one when answered so i dont keep clicking on them thinking they need answering and finding i already replied", then picked option B, outline tags (docs/2026-10-15/chat-visual-options/question-tags.html, Q137): a Q-tag in an agent's reply is an outlined pill, amber with a ? while its question waits for an answer, green with a check once answered, grey and struck through once withdrawn. Then: "the colors of the ? here and the "Previous question" color should be calmer please", so the tones are muted, and the "Previous question" ? pill is neutral like Threads (chat_pills.rs), superseding its amber outline. The state is the latest form or page that asked the tag, as the link's jump is; answers that are sending or queued count as answered, as the pill counts them.
/// SEE-ALSO: packages/gx-chat-core/visual/question-tags.json (the colours), apps/desktop/src/app/native_chat/open_questions.rs and markdown_links.rs, apps/mobile/app/src/chat/native/transcript/markdown/Markdown.tsx and OpenQuestions.tsx, gpui-component's `InlineLinkFrame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagState {
    Open,
    Answered,
    Withdrawn,
    /// Answered with Skip (`"skipped"`): drawn in the withdrawn grey, without the strike-through.
    Skipped,
}

impl TagState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
            Self::Withdrawn => "withdrawn",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "open" => Some(Self::Open),
            "answered" => Some(Self::Answered),
            "withdrawn" => Some(Self::Withdrawn),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// A tag a reply's words can link to, uppercased, with the state of its question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionTag {
    pub tag: String,
    pub state: TagState,
}

/// What asked a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskerKind {
    /// A ```questions block, drawn as a form.
    Form,
    /// A question page's card.
    Page,
}

/// One form or question page, as the transcript holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Asker {
    /// The form's id or the page's id: the key `questionForms` and `answeredPages` use.
    pub id: String,
    pub kind: AskerKind,
    /// The reply that holds it.
    pub message_id: String,
    pub tags: Vec<String>,
    /// The transcript carries its answers, or the session's resource index records them when the
    /// loaded rows do not (`questions::resource_index::settle_loaded`).
    pub answered: bool,
    /// A later form or page asks all of its tags again, or a later block withdrew them (also when
    /// only the resource index holds that later block).
    pub obsolete: bool,
    /// Its tags a later ```questions block withdrew.
    pub withdrawn: Vec<String>,
    /// A form's tags with the answer key each one's question answers under (a page's are not known
    /// to the chat).
    pub keys: Vec<(String, String)>,
    /// Its tags whose answers came back as Skip (`mark_skipped`).
    pub skipped: Vec<String>,
    /// Its tags a later form or page asks again or a later block withdrew: those questions wait on
    /// the reader there, or nowhere, not here.
    pub superseded: Vec<String>,
    /// Asked before this session was forked from another (`mark_inherited`): it waits in the
    /// parent, not here.
    pub inherited: bool,
    /// What the questions list (`questions/open_questions.rs`) calls it: a page's title, or a
    /// form's first question (its title is often just "Open questions").
    pub title: String,
    /// When the reply that holds it arrived (epoch ms), if the transcript says.
    pub at: Option<i64>,
}

impl Asker {
    /// How many of its questions still wait on the reader here while it is unanswered: a form's
    /// questions (given `form`, the form it is) and a page's tags, less those superseded, at least
    /// one. A form without its parsed questions counts its tags.
    ///
    /// CDXC:SessionStatus 2026-10-22 WHY:
    /// Agents ask the questions still open again in a new "Open questions" form that leaves out the ones settled meanwhile (Q24 to Q29, then Q24 and Q26 to Q33, then Q26 to Q33). Counting every question of each earlier form, because one of its tags was never asked again, showed `answer requested (24)` on a session where 7 questions were left unanswered, the rest answered in the later forms. Each tag counts once, where it was asked last, as the 2026-10-14 decision on superseding reads.
    pub fn waiting(&self, form: Option<&crate::transcript::question_forms::QuestionForm>) -> usize {
        let superseded = |tag: &str| self.superseded.iter().any(|known| same_tag(known, tag));
        let waiting = match (self.kind, form) {
            (AskerKind::Form, Some(form)) => form
                .questions
                .iter()
                .filter(|question| question.tag.is_empty() || !superseded(&question.tag))
                .count(),
            _ => self.tags.iter().filter(|tag| !superseded(tag)).count(),
        };
        waiting.max(1)
    }
}

/// One thing a reply's fences say about the questions: a form or page asking, or a withdrawal.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Asker(Asker),
    Withdraw(Vec<String>),
}

fn same_tag(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Every form and question page the agent's replies hold, oldest first, once each.
/// `answered` lists the ids whose answers the transcript carries (`page_answers.rs`).
pub fn askers(messages: &[ChatMessage], answered: &[String]) -> Vec<Asker> {
    settle_askers(&asker_entries(messages, &[]), answered)
}

/// What the replies in `messages` ask and withdraw, in order: the reading half of [`askers`].
/// `prior` is what the messages before these already held, so a form or page they asked first is
/// not counted again here.
pub fn asker_entries(messages: &[ChatMessage], prior: &[Found]) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    for message in messages
        .iter()
        .filter(|message| message.role == ChatRole::Assistant)
    {
        let markdown = prose_markdown(&message.blocks);
        // One-click answers in the reply's prose ask as one-question forms (`inline_answers.rs`).
        for question in crate::transcript::inline_answers::inline_questions(&markdown) {
            let form = question.form();
            let seen = prior
                .iter()
                .chain(found.iter())
                .any(|known| matches!(known, Found::Asker(known) if known.id == form.id));
            if seen {
                continue;
            }
            found.push(Found::Asker(Asker {
                id: form.id,
                kind: AskerKind::Form,
                message_id: message.id.clone(),
                tags: vec![question.tag.clone()],
                answered: false,
                obsolete: false,
                withdrawn: Vec::new(),
                keys: vec![(question.tag.clone(), question.tag.clone())],
                skipped: Vec::new(),
                superseded: Vec::new(),
                inherited: false,
                title: question.question.clone(),
                at: message.timestamp,
            }));
        }
        if !markdown.contains("```") && !markdown.contains("~~~") {
            continue;
        }
        for (language, body) in finished_fences(&markdown) {
            let asker = if is_questions_language(&language) {
                let form = parse_form(&body);
                if !form.error.is_empty() {
                    continue;
                }
                // A block's withdrawals apply to what came before it, not to its own questions.
                if !form.withdraw.is_empty() {
                    found.push(Found::Withdraw(form.withdraw.clone()));
                }
                if form.questions.is_empty() {
                    continue;
                }
                let mut tags: Vec<String> = Vec::new();
                for tag in form.questions.iter().map(|question| question.tag.as_str()) {
                    if !tag.is_empty() && !tags.iter().any(|known| same_tag(known, tag)) {
                        tags.push(tag.to_string());
                    }
                }
                let keys = form
                    .questions
                    .iter()
                    .filter(|question| !question.tag.is_empty())
                    .map(|question| (question.tag.clone(), question.key.clone()))
                    .collect();
                let title = form
                    .questions
                    .iter()
                    .map(|question| question.title.trim())
                    .find(|title| !title.is_empty())
                    .unwrap_or(form.title.trim())
                    .to_string();
                Asker {
                    id: form.id,
                    kind: AskerKind::Form,
                    message_id: message.id.clone(),
                    tags,
                    answered: false,
                    obsolete: false,
                    withdrawn: Vec::new(),
                    keys,
                    skipped: Vec::new(),
                    superseded: Vec::new(),
                    inherited: false,
                    title,
                    at: message.timestamp,
                }
            } else if ghostex_gx_visual::is_visual_language(&language) {
                let Some(page) = ghostex_gx_visual::page_reference(&body) else {
                    continue;
                };
                let (Some(id), Some(tags)) = (page.page_id(), page.questions.clone()) else {
                    continue;
                };
                Asker {
                    id: id.to_string(),
                    kind: AskerKind::Page,
                    message_id: message.id.clone(),
                    tags,
                    answered: false,
                    obsolete: false,
                    withdrawn: Vec::new(),
                    keys: Vec::new(),
                    skipped: Vec::new(),
                    superseded: Vec::new(),
                    inherited: false,
                    title: page.title.trim().to_string(),
                    at: message.timestamp,
                }
            } else {
                continue;
            };
            let seen = prior
                .iter()
                .chain(found.iter())
                .any(|known| matches!(known, Found::Asker(known) if known.id == asker.id));
            if !seen {
                found.push(Found::Asker(asker));
            }
        }
    }
    found
}

/// Whether an agent's reply asks the reader something: it holds a ```questions form or a question
/// page's card. A finished turn keeps such a reply in view (`transcript/turns.rs`
/// `partition_completed_work`), and gxserver never defers it into a collapsed turn's work
/// (server/src/session_chat_history.rs), so the ? pill, a Q-tag link and Resources reach its card.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// The ? pill counted a question its click could not show: an agent that asks in a form and then keeps working (tool calls, then a last "done") ended its turn with the form among the folded work, so gxserver left it out of history pages and the snapshot until the fold opened, and the chat drew it inside the closed "Previous turn". The pill's jump and "Go to chat" then landed on a fold or on nothing. Such a reply now stays like an AskUserQuestion call and an image do, as summary mode already draws it in full (`turns::content_rows`).
/// SEE-ALSO: server/src/session_chat_history.rs (`Turn::push`), transcript/turns.rs (`partition_completed_work`).
pub fn asks_questions(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant
        && asker_entries(std::slice::from_ref(message), &[])
            .iter()
            .any(|found| matches!(found, Found::Asker(_)))
}

/// The askers `found` holds, each marked answered, superseded, obsolete and withdrawn by what came
/// after it: the deciding half of [`askers`].
pub fn settle_askers(found: &[Found], answered: &[String]) -> Vec<Asker> {
    let mut askers = Vec::new();
    for (index, entry) in found.iter().enumerate() {
        let Found::Asker(asker) = entry else {
            continue;
        };
        let asked_again = |tag: &str| {
            found[index + 1..].iter().any(|later| match later {
                Found::Asker(later) => later.tags.iter().any(|known| same_tag(known, tag)),
                Found::Withdraw(tags) => tags.iter().any(|known| same_tag(known, tag)),
            })
        };
        let withdrawn_later = |tag: &str| {
            found[index + 1..].iter().any(|later| {
                matches!(later, Found::Withdraw(tags) if tags.iter().any(|known| same_tag(known, tag)))
            })
        };
        let mut asker = asker.clone();
        asker.answered = answered.contains(&asker.id);
        asker.superseded = asker
            .tags
            .iter()
            .filter(|tag| asked_again(tag))
            .cloned()
            .collect();
        asker.obsolete = !asker.tags.is_empty() && asker.superseded.len() == asker.tags.len();
        asker.withdrawn = asker
            .tags
            .iter()
            .filter(|tag| withdrawn_later(tag))
            .cloned()
            .collect();
        askers.push(asker);
    }
    askers
}

/// The form or page a Q-tag link goes to: the latest one that asked the tag, answered or not.
pub fn tag_target<'a>(askers: &'a [Asker], tag: &str) -> Option<&'a Asker> {
    askers
        .iter()
        .rev()
        .find(|asker| asker.tags.iter().any(|known| same_tag(known, tag)))
}

/// Marks the tags whose questions came back skipped (see the SessionChat 2026-10-16 DECISION on
/// `SKIPPED` in packages/gx-visual/src/questions.rs): a form's by each tag's answer key, and a
/// page's (whose keys the chat does not know) only when every answer it sent is a skip.
pub fn mark_skipped(
    askers: &mut [Asker],
    answers: &[crate::transcript::page_answers::PageAnswers],
) {
    use crate::transcript::question_forms::{NOTE_SUFFIX, SKIPPED};
    for asker in askers.iter_mut() {
        let Some(sent) = answers.iter().find(|sent| sent.page_id == asker.id) else {
            continue;
        };
        let Some(values) = sent.answers.as_object() else {
            continue;
        };
        asker.skipped = match asker.kind {
            AskerKind::Form => asker
                .keys
                .iter()
                .filter(|(_, key)| values.get(key).and_then(Value::as_str) == Some(SKIPPED))
                .map(|(tag, _)| tag.clone())
                .collect(),
            AskerKind::Page => {
                let mut asked = values
                    .iter()
                    .filter(|(key, _)| !key.ends_with(NOTE_SUFFIX))
                    .peekable();
                let all_skipped = asked.peek().is_some()
                    && asked.all(|(_, value)| value.as_str() == Some(SKIPPED));
                if all_skipped {
                    asker.tags.clone()
                } else {
                    Vec::new()
                }
            }
        };
    }
}

/// Marks the forms and pages a forked session's transcript carries from before the fork: the replies
/// that hold them are older than `forked_at` (the session's fork time, epoch ms). A reply without a
/// time is the fork's own.
///
/// CDXC:SessionStatus 2026-10-23 DECISION:
/// User, asked whether a fork should count the open questions it inherits from the conversation it was forked from: "No, only questions asked after the fork". The pink question dot, `answer requested (n)` and the Previous question pill count only questions the fork itself asked; the parent still counts its own. The fork point is the time gxserver created the forked session (`forkedAt`, from its `createdAt`): a Claude fork copies the parent's lines with their original times, all older than that, and every line the fork writes is newer.
pub fn mark_inherited(askers: &mut [Asker], messages: &[ChatMessage], forked_at: Option<i64>) {
    let Some(forked_at) = forked_at else {
        return;
    };
    for asker in askers.iter_mut() {
        asker.inherited = messages
            .iter()
            .find(|message| message.id == asker.message_id)
            .and_then(|message| message.timestamp)
            .is_some_and(|at| at < forked_at);
    }
}

/// A question page some or all of whose questions were withdrawn before it was answered: the tags
/// withdrawn, and how many tags it asked.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawnPage {
    pub tags: Vec<String>,
    pub total: usize,
}

/// The question pages whose cards say what was withdrawn, by page id: a page not answered whose
/// tags a later block withdrew, or that a later form or page asked again (the same grey state the
/// Q-tag pills take, `tag_state`).
///
/// CDXC:SessionChat 2026-10-20 DECISION: User, of a page card whose questions were all withdrawn: "You show "withdrawn, no answer needed" here but not here ... do u think we can show it in Image #2 also?". The card carries a grey Withdrawn pill where an answered page's Answered pill sits, saying "Withdrawn, no answer needed"; when only some of its questions were withdrawn the pill says how many ("2 of 4 withdrawn"), so the reader knows the rest still wait. An answered page keeps its Answered pill. Drawn by apps/desktop/src/app/native_chat/page_card.rs and the phone's PageCard.tsx.
pub fn withdrawn_pages(askers: &[Asker]) -> std::collections::BTreeMap<String, WithdrawnPage> {
    askers
        .iter()
        .filter(|asker| asker.kind == AskerKind::Page && !asker.answered && !asker.tags.is_empty())
        .filter_map(|asker| {
            let tags: Vec<String> = if asker.obsolete {
                asker.tags.clone()
            } else {
                asker
                    .tags
                    .iter()
                    .filter(|tag| asker.withdrawn.iter().any(|known| same_tag(known, tag)))
                    .cloned()
                    .collect()
            };
            (!tags.is_empty()).then(|| {
                (
                    asker.id.clone(),
                    WithdrawnPage {
                        tags,
                        total: asker.tags.len(),
                    },
                )
            })
        })
        .collect()
}

/// The state of the question a tag names: answered (or skipped) once the latest form or page that
/// asked it has its answers, withdrawn once a later block withdrew it there, open otherwise.
pub fn tag_state(askers: &[Asker], tag: &str) -> Option<TagState> {
    let asker = tag_target(askers, tag)?;
    Some(
        if asker.answered && asker.skipped.iter().any(|known| same_tag(known, tag)) {
            TagState::Skipped
        } else if asker.answered {
            TagState::Answered
        } else if asker.obsolete || asker.withdrawn.iter().any(|known| same_tag(known, tag)) {
            TagState::Withdrawn
        } else {
            TagState::Open
        },
    )
}

/// The tags a reply's words can link to, uppercased and sorted, with their states: the tags some
/// form or page asked that look like a tag (`Q80`, `GX-12`), so a tag that is an ordinary word
/// never turns words into links.
pub fn linkable_tags(askers: &[Asker]) -> Vec<QuestionTag> {
    let mut tags: Vec<String> = askers
        .iter()
        .flat_map(|asker| asker.tags.iter())
        .filter(|tag| tag_length(tag, 0) == Some(tag.len()))
        .map(|tag| tag.to_ascii_uppercase())
        .collect();
    tags.sort();
    tags.dedup();
    tags.into_iter()
        .filter_map(|tag| {
            let state = tag_state(askers, &tag)?;
            Some(QuestionTag { tag, state })
        })
        .collect()
}

/// The entry of `tags` (sorted) for a word, whatever its case.
fn find_tag<'a>(tags: &'a [QuestionTag], word: &str) -> Option<&'a QuestionTag> {
    let word = word.to_ascii_uppercase();
    tags.binary_search_by(|known| known.tag.as_str().cmp(&word))
        .ok()
        .map(|at| &tags[at])
}

/// A Q-tag link's destination: the tag as written and its question's state.
fn question_href(word: &str, state: TagState) -> String {
    format!("{QUESTION_LINK_PREFIX}{word};{}", state.as_str())
}

/// The length of a tag-shaped word starting at `at` (`[A-Za-z]{1,8}-?[0-9]{1,6}`), when the word
/// ends there too.
fn tag_length(text: &str, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut end = at;
    while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
        end += 1;
    }
    let letters = end - at;
    if letters == 0 || letters > 8 {
        return None;
    }
    if bytes.get(end) == Some(&b'-') {
        end += 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let digits = end - digits_start;
    if digits == 0 || digits > 6 {
        return None;
    }
    if bytes.get(end).is_some_and(|byte| is_word_byte(*byte)) {
        return None;
    }
    Some(end - at)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// The tag-shaped words in a reply's prose outside code and links, uppercased, sorted and once
/// each: what the projection cache compares against the session's tags.
pub fn tag_words(markdown: &str) -> Vec<String> {
    let mut words = Vec::new();
    walk_prose(markdown, &mut |line, out| {
        for_each_word(line, out, &mut |word, out| {
            words.push(word.to_ascii_uppercase());
            out.push_str(word);
        });
    });
    words.sort();
    words.dedup();
    words
}

/// The words of `words` (uppercased, sorted) that are among `tags`, each with its state
/// (`Q80;open`), so a cached reply re-projects when one of its tags is answered or withdrawn.
pub fn linked_words(words: &[String], tags: &[QuestionTag]) -> Vec<String> {
    words
        .iter()
        .filter_map(|word| {
            let found = find_tag(tags, word)?;
            Some(format!("{word};{}", found.state.as_str()))
        })
        .collect()
}

/// The reply with each word that names one of `tags` (uppercased, sorted) turned into a link to its
/// question, and each ```questions block that only withdraws questions turned into a line naming
/// them. Code, links and addresses are left alone.
pub fn link_question_tags(markdown: &str, tags: &[QuestionTag]) -> String {
    let withdrawals = markdown.contains("withdraw");
    if tags.is_empty() && !withdrawals {
        return markdown.to_string();
    }
    let mut result = String::with_capacity(markdown.len() + 64);
    let lines = split_lf(markdown);
    let mut index = 0;
    let mut first = true;
    let mut push_line = |result: &mut String, line: &str| {
        if !std::mem::take(&mut first) {
            result.push('\n');
        }
        result.push_str(line);
    };
    while index < lines.len() {
        let line = lines[index];
        let Some((character, run, info)) = fence_open(line) else {
            let mut linked = String::with_capacity(line.len());
            link_line(line, tags, &mut linked);
            push_line(&mut result, &linked);
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
        let language = js_trim(info)
            .split(char::is_whitespace)
            .next()
            .unwrap_or_default();
        if let Some(close) = close.filter(|_| withdrawals && is_questions_language(language)) {
            let body = lines[index + 1..close].join("\n");
            let form = parse_form(&body);
            if form.is_withdrawal() {
                push_line(&mut result, &withdrawal_line(&form.withdraw, tags));
                index = close + 1;
                continue;
            }
        }
        // The fence and its body stay as written; an unclosed fence runs to the end.
        let end = close.unwrap_or(lines.len() - 1);
        for line in &lines[index..=end] {
            push_line(&mut result, line);
        }
        index = end + 1;
    }
    result
}

/// What a withdraw-only block reads as: the tags it withdrew, each a link when its question is in
/// the transcript.
fn withdrawal_line(withdrawn: &[String], tags: &[QuestionTag]) -> String {
    let named: Vec<String> = withdrawn
        .iter()
        .map(|tag| match find_tag(tags, tag) {
            Some(found) => format!("[{tag}]({})", question_href(tag, found.state)),
            None => tag.clone(),
        })
        .collect();
    format!("_Withdrawn, no answer needed: {}_", named.join(", "))
}

/// Links the tags in one line of prose.
fn link_line(line: &str, tags: &[QuestionTag], out: &mut String) {
    if tags.is_empty() {
        out.push_str(line);
        return;
    }
    for_each_word(line, out, &mut |word, out| match find_tag(tags, word) {
        Some(found) => out.push_str(&format!("[{word}]({})", question_href(word, found.state))),
        None => out.push_str(word),
    });
}

/// Calls `line_fn` with each line of prose outside fenced code.
fn walk_prose(markdown: &str, line_fn: &mut dyn FnMut(&str, &mut String)) {
    let lines = split_lf(markdown);
    let mut scratch = String::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some((character, run, _)) = fence_open(lines[index]) {
            let close = lines[index + 1..].iter().position(|line| {
                fence_close(line)
                    .is_some_and(|(closing, length)| closing == character && length >= run)
            });
            index = close.map_or(lines.len(), |offset| index + 1 + offset + 1);
            continue;
        }
        scratch.clear();
        line_fn(lines[index], &mut scratch);
        index += 1;
    }
}

/// Copies `line` into `out`, handing every tag-shaped word outside inline code, links, autolinks
/// and addresses to `word_fn`, which writes it (or its link) itself.
fn for_each_word(line: &str, out: &mut String, word_fn: &mut dyn FnMut(&str, &mut String)) {
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut copied = 0;
    while at < bytes.len() {
        let skip_to = match bytes[at] {
            // Inline code: up to the closing run of the same length.
            b'`' => {
                let run = bytes[at..].iter().take_while(|byte| **byte == b'`').count();
                let fence = &line[at..at + run];
                line[at + run..]
                    .find(fence)
                    .map_or(at + run, |offset| at + run + offset + run)
            }
            // A link or picture: its label and destination stay as written.
            b'[' => line[at..]
                .find("](")
                .and_then(|label_end| {
                    let destination = at + label_end + 2;
                    line[destination..]
                        .find(')')
                        .map(|close| destination + close + 1)
                })
                .unwrap_or(at + 1),
            // An autolink or a tag-like `<…>`.
            b'<' => line[at..]
                .find('>')
                .filter(|close| !line[at + 1..at + close].contains(' '))
                .map_or(at + 1, |close| at + close + 1),
            byte if byte.is_ascii_alphabetic()
                && (at == 0 || !is_word_byte(bytes[at - 1]) && bytes[at - 1] != b'/') =>
            {
                let rest = &line[at..];
                if rest.starts_with("http://")
                    || rest.starts_with("https://")
                    || rest.starts_with("www.")
                {
                    at + rest.find(char::is_whitespace).unwrap_or(rest.len())
                } else if let Some(length) = tag_length(line, at) {
                    out.push_str(&line[copied..at]);
                    word_fn(&line[at..at + length], out);
                    at += length;
                    copied = at;
                    continue;
                } else {
                    at + rest
                        .bytes()
                        .take_while(|byte| is_word_byte(*byte))
                        .count()
                        .max(1)
                }
            }
            _ => at + 1,
        };
        at = skip_to.min(bytes.len());
        // Never stop inside a character.
        while !line.is_char_boundary(at) {
            at += 1;
        }
    }
    out.push_str(&line[copied..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, text: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "role": "assistant",
            "source": "transcript",
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    fn form(tags: &[&str]) -> String {
        let questions: Vec<_> = tags
            .iter()
            .map(|tag| serde_json::json!({"tag": tag, "title": format!("About {tag}?"), "choices": ["Yes", "No"]}))
            .collect();
        format!(
            "```questions\n{}\n```",
            serde_json::json!({"title": "Open questions", "questions": questions})
        )
    }

    fn page(id: &str, tags: &[&str]) -> String {
        format!(
            "```visual\n{}\n```",
            serde_json::json!({"page": {"title": "Mockups", "url": format!("http://127.0.0.1:1/visual/{id}"), "questions": tags}})
        )
    }

    const PAGE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn forms_and_question_pages_are_found_oldest_first() {
        let messages = vec![
            reply("a", &format!("First.\n\n{}", form(&["Q1", "Q2"]))),
            reply("b", &page(PAGE, &["Q3"])),
            reply("c", "```visual\n{\"page\":{\"title\":\"Plain\",\"url\":\"http://127.0.0.1:1/visual/ffffffffffffffffffffffffffffffff\"}}\n```"),
        ];
        let found = askers(&messages, &[]);
        assert_eq!(found.len(), 2, "a page that asks nothing is not a question");
        assert_eq!(found[0].kind, AskerKind::Form);
        assert_eq!(found[0].tags, vec!["Q1", "Q2"]);
        assert_eq!(found[0].message_id, "a");
        assert_eq!(found[1].kind, AskerKind::Page);
        assert_eq!(found[1].id, PAGE);
        assert!(found.iter().all(|asker| !asker.answered && !asker.obsolete));
    }

    #[test]
    fn answers_mark_the_question_answered() {
        let messages = vec![reply("b", &page(PAGE, &["Q3"]))];
        assert!(askers(&messages, &[PAGE.to_string()])[0].answered);
    }

    #[test]
    fn asking_every_tag_again_supersedes_and_a_partial_repeat_does_not() {
        let messages = vec![
            reply("a", &form(&["Q1", "Q2"])),
            reply("b", &form(&["Q2"])),
            reply("c", &form(&["Q5"])),
            reply("d", &page(PAGE, &["q5"])),
        ];
        let found = askers(&messages, &[]);
        assert!(
            !found[0].obsolete,
            "Q1 is still only asked by the first form"
        );
        assert!(!found[1].obsolete);
        assert!(
            found[2].obsolete,
            "the page asks Q5 again (tags match whatever their case)"
        );
        assert!(!found[3].obsolete);
    }

    #[test]
    fn a_withdrawal_drops_earlier_questions_only() {
        let withdraw = "```questions\n{\"withdraw\": [\"Q1\", \"Q2\"]}\n```";
        let messages = vec![
            reply("a", &form(&["Q1"])),
            reply("b", withdraw),
            reply("c", &form(&["Q2"])),
        ];
        let found = askers(&messages, &[]);
        assert_eq!(found.len(), 2, "a withdrawal asks nothing");
        assert!(found[0].obsolete);
        assert!(!found[1].obsolete, "Q2 was asked after it was withdrawn");
    }

    #[test]
    fn untagged_questions_never_go_obsolete() {
        let untagged =
            "```questions\n{\"questions\":[{\"title\":\"Anything else?\",\"type\":\"text\"}]}\n```";
        let messages = vec![
            reply("a", untagged),
            reply("b", "```questions\n{\"withdraw\": [\"Q1\"]}\n```"),
        ];
        assert!(!askers(&messages, &[])[0].obsolete);
    }

    #[test]
    fn a_tag_links_to_the_latest_question_that_asked_it() {
        let messages = vec![reply("a", &form(&["Q1"])), reply("b", &page(PAGE, &["Q1"]))];
        let found = askers(&messages, &[]);
        assert_eq!(
            tag_target(&found, "q1").map(|asker| asker.id.as_str()),
            Some(PAGE)
        );
        assert_eq!(
            linkable_tags(&found),
            vec![QuestionTag {
                tag: "Q1".to_string(),
                state: TagState::Open
            }]
        );
    }

    #[test]
    fn tag_words_are_linked_outside_code_and_links() {
        let tags = vec![
            QuestionTag {
                tag: "Q80".to_string(),
                state: TagState::Open,
            },
            QuestionTag {
                tag: "Q81".to_string(),
                state: TagState::Answered,
            },
        ];
        let text = "See Q80 and (Q81). Not Q800, `Q80`, [Q80](https://x/Q80), https://x/Q81 or AQ80.\n```\nQ80\n```\nq80!";
        assert_eq!(
            link_question_tags(text, &tags),
            "See [Q80](#gx-question=Q80;open) and ([Q81](#gx-question=Q81;answered)). Not Q800, `Q80`, [Q80](https://x/Q80), https://x/Q81 or AQ80.\n```\nQ80\n```\n[q80](#gx-question=q80;open)!"
        );
        assert_eq!(tag_words(text), vec!["AQ80", "Q80", "Q800", "Q81"]);
    }

    #[test]
    fn a_withdraw_only_block_reads_as_a_line() {
        let tags = vec![QuestionTag {
            tag: "Q40".to_string(),
            state: TagState::Withdrawn,
        }];
        let text = "Done.\n\n```questions\n{\"withdraw\": [\"Q40\", \"Q41\"]}\n```";
        assert_eq!(
            link_question_tags(text, &tags),
            "Done.\n\n_Withdrawn, no answer needed: [Q40](#gx-question=Q40;withdrawn), Q41_"
        );
    }
}
